use etherparse::{
    Icmpv4Type, Icmpv6Slice, Icmpv6Type, IpNumber, LaxNetSlice, LaxSlicedPacket, TransportSlice,
    icmpv4, icmpv6,
};
use rand::RngCore;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use socket2::{Domain, Protocol, SockRef, Socket, Type};
use std::io::{self, Write};
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROBE_BYTES: usize = 256;
const DPI_CLASSIFICATION_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct DpiHopProbeConfig {
    pub target: SocketAddr,
    pub control_sni: String,
    pub max_ttl: u8,
    pub connect_timeout: Duration,
    pub hop_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct DpiHopProbeResult {
    pub target: SocketAddr,
    pub local_addr: SocketAddr,
    pub client_hello_bytes: usize,
    pub max_icmp_time_exceeded_ttl: Option<u8>,
    pub hops: Vec<DpiHopProbeHop>,
}

#[derive(Debug, Clone)]
pub struct DpiHopProbeHop {
    pub ttl: u8,
    pub router: Option<IpAddr>,
    pub outcome: DpiHopProbeHopOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiHopProbeHopOutcome {
    IcmpTimeExceeded,
    Timeout,
    TcpClosed,
    TcpAcknowledged,
}

impl DpiHopProbeHopOutcome {
    pub const fn invalidates_measurement(self) -> bool {
        matches!(self, Self::TcpClosed | Self::TcpAcknowledged)
    }
}

pub async fn detect_dpi_hop(config: DpiHopProbeConfig) -> io::Result<DpiHopProbeResult> {
    tokio::task::spawn_blocking(move || detect_dpi_hop_blocking(config))
        .await
        .map_err(io::Error::other)?
}

pub fn detect_dpi_hop_blocking(config: DpiHopProbeConfig) -> io::Result<DpiHopProbeResult> {
    if config.max_ttl == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "max_ttl must be greater than zero",
        ));
    }

    let (domain, protocol) = match config.target {
        SocketAddr::V4(_) => (Domain::IPV4, Protocol::ICMPV4),
        SocketAddr::V6(_) => (Domain::IPV6, Protocol::ICMPV6),
    };
    let icmp = Socket::new(domain, Type::RAW, Some(protocol))?;
    icmp.set_read_timeout(Some(config.hop_timeout))?;

    let client_hello = make_client_hello(&config.control_sni)?;
    let mut hops = Vec::with_capacity(config.max_ttl as usize);
    let mut max_icmp_time_exceeded_ttl = None;
    let mut result_local_addr: Option<SocketAddr> = None;
    let mut client_hello_bytes = None;

    for ttl in 1..=config.max_ttl {
        // TCP is a byte stream: once a low-TTL segment is lost, later writes on
        // that stream can remain queued behind it and retransmissions can use a
        // subsequently changed socket TTL. Use an independent connection for
        // every TTL so each hop corresponds to an actual packet and cannot
        // affect later hops.
        let mut tcp = TcpStream::connect_timeout(&config.target, config.connect_timeout)?;
        tcp.set_nodelay(true)?;
        let local_addr = tcp.local_addr()?;
        if !same_ip_family(local_addr, config.target) {
            return Err(io::Error::other(
                "DPI hop probe local and target address families differ",
            ));
        }
        if let Some(result_local_addr) = result_local_addr {
            if result_local_addr.ip() != local_addr.ip() {
                return Err(io::Error::other(
                    "DPI hop probe changed local address between TTL attempts",
                ));
            }
        } else {
            // Winsock requires a raw socket to be bound before `recvfrom`.
            // Binding after route selection also limits replies to the right
            // local interface. Raw sockets do not use a transport port.
            let mut icmp_addr = local_addr;
            icmp_addr.set_port(0);
            icmp.bind(&icmp_addr.into())?;
            result_local_addr = Some(local_addr);
            client_hello_bytes = Some(client_hello.len());
        }
        tcp.write_all(&client_hello)?;

        // A blocking DPI may silently discard the ClientHello, so waiting for
        // a TCP acknowledgement would deadlock the measurement. Give the DPI
        // a short interval to classify this new flow before sending the
        // TTL-limited payload instead. This is needed for every attempt because
        // each TTL deliberately uses a fresh TCP connection.
        std::thread::sleep(DPI_CLASSIFICATION_DELAY);
        drain_socket(&icmp)?;

        let mut payload = [0u8; PROBE_BYTES];
        rand::thread_rng().fill_bytes(&mut payload);

        if !send_with_ttl(&mut tcp, config.target, &payload, ttl)? {
            hops.push(DpiHopProbeHop {
                ttl,
                router: None,
                outcome: DpiHopProbeHopOutcome::TcpClosed,
            });
            break;
        }

        let router =
            listen_for_time_exceeded(&icmp, local_addr, config.target, config.hop_timeout)?;
        let outcome = classify_hop(router, peer_closed(&tcp)?, tcp_payload_acknowledged(&tcp)?);
        if outcome == DpiHopProbeHopOutcome::IcmpTimeExceeded {
            max_icmp_time_exceeded_ttl = Some(ttl);
        }
        hops.push(DpiHopProbeHop {
            ttl,
            router,
            outcome,
        });
        if outcome.invalidates_measurement() {
            break;
        }
    }

    Ok(DpiHopProbeResult {
        target: config.target,
        local_addr: result_local_addr
            .ok_or_else(|| io::Error::other("DPI hop probe made no TTL attempts"))?,
        client_hello_bytes: client_hello_bytes
            .ok_or_else(|| io::Error::other("DPI hop probe produced no ClientHello"))?,
        max_icmp_time_exceeded_ttl,
        hops,
    })
}

fn make_client_hello(sni: &str) -> io::Result<Vec<u8>> {
    let server_name = ServerName::try_from(sni.to_owned()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "control_sni is not a valid DNS name",
        )
    })?;
    let config = ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    let mut connection = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| io::Error::other(format!("create TLS client connection: {error}")))?;
    let mut client_hello = Vec::new();
    connection
        .write_tls(&mut client_hello)
        .map_err(|error| io::Error::other(format!("write TLS ClientHello: {error}")))?;
    if client_hello.is_empty() {
        return Err(io::Error::other("rustls did not produce a ClientHello"));
    }
    Ok(client_hello)
}

fn same_ip_family(left: SocketAddr, right: SocketAddr) -> bool {
    matches!(
        (left, right),
        (SocketAddr::V4(_), SocketAddr::V4(_)) | (SocketAddr::V6(_), SocketAddr::V6(_))
    )
}

fn send_with_ttl(
    tcp: &mut TcpStream,
    target: SocketAddr,
    payload: &[u8],
    ttl: u8,
) -> io::Result<bool> {
    set_ttl(tcp, target, ttl as u32)?;
    // Keep this TTL until the per-hop connection is dropped. Retransmissions
    // must expire at the same hop instead of escaping with the default TTL.
    match tcp.write_all(payload) {
        Ok(()) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn set_ttl(tcp: &TcpStream, target: SocketAddr, ttl: u32) -> io::Result<()> {
    let socket = SockRef::from(tcp);
    match target {
        SocketAddr::V4(_) => socket.set_ttl_v4(ttl),
        SocketAddr::V6(_) => socket.set_unicast_hops_v6(ttl),
    }
}

fn listen_for_time_exceeded(
    icmp: &Socket,
    local_addr: SocketAddr,
    target: SocketAddr,
    timeout: Duration,
) -> io::Result<Option<IpAddr>> {
    let deadline = Instant::now() + timeout;
    let mut buffer = [0u8; 65535];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        icmp.set_read_timeout(Some(remaining))?;
        match recv_socket(icmp, &mut buffer) {
            Ok((bytes, source)) => {
                if let Some(router) =
                    match_time_exceeded(&buffer[..bytes], source, local_addr, target)
                {
                    return Ok(Some(router));
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
    }
}

fn peer_closed(tcp: &TcpStream) -> io::Result<bool> {
    tcp.set_nonblocking(true)?;
    let mut byte = [0u8; 1];
    let result = match tcp.peek(&mut byte) {
        Ok(0) => Ok(true),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
            ) =>
        {
            Ok(true)
        }
        Err(error) => Err(error),
    };
    let _ = tcp.set_nonblocking(false);
    result
}

fn classify_hop(
    router: Option<IpAddr>,
    peer_closed: bool,
    payload_acknowledged: bool,
) -> DpiHopProbeHopOutcome {
    if router.is_some() {
        DpiHopProbeHopOutcome::IcmpTimeExceeded
    } else if peer_closed {
        DpiHopProbeHopOutcome::TcpClosed
    } else if payload_acknowledged {
        DpiHopProbeHopOutcome::TcpAcknowledged
    } else {
        DpiHopProbeHopOutcome::Timeout
    }
}

#[cfg(target_os = "linux")]
fn tcp_payload_acknowledged(tcp: &TcpStream) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    let mut info = std::mem::MaybeUninit::<libc::tcp_info>::zeroed();
    let mut length = std::mem::size_of::<libc::tcp_info>() as libc::socklen_t;
    // SAFETY: `info` points to writable storage of `length` bytes, and both
    // pointers remain valid for the duration of `getsockopt`.
    let result = unsafe {
        libc::getsockopt(
            tcp.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_INFO,
            info.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    // Linux initialized the returned prefix, which includes `tcpi_unacked`.
    let info = unsafe { info.assume_init() };
    Ok(info.tcpi_unacked == 0)
}

#[cfg(not(target_os = "linux"))]
fn tcp_payload_acknowledged(_tcp: &TcpStream) -> io::Result<bool> {
    // TCP acknowledgment state is not exposed portably. Other platforms keep
    // the previous conservative behavior and never infer direct delivery.
    Ok(false)
}

fn drain_socket(socket: &Socket) -> io::Result<usize> {
    let previous_timeout = socket.read_timeout()?;
    socket.set_nonblocking(true)?;
    let mut drained = 0;
    let mut buffer = [0u8; 65535];
    loop {
        match recv_socket(socket, &mut buffer) {
            Ok(_) => drained += 1,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => {
                let _ = socket.set_nonblocking(false);
                let _ = socket.set_read_timeout(previous_timeout);
                return Err(error);
            }
        }
    }
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(previous_timeout)?;
    Ok(drained)
}

fn recv_socket(socket: &Socket, buffer: &mut [u8]) -> io::Result<(usize, Option<SocketAddr>)> {
    // `recv_from` initializes exactly the returned prefix of the buffer.
    let uninitialized = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<MaybeUninit<u8>>(), buffer.len())
    };
    let (bytes, source) = socket.recv_from(uninitialized)?;
    Ok((bytes, source.as_socket()))
}

fn match_time_exceeded(
    packet: &[u8],
    source: Option<SocketAddr>,
    local_addr: SocketAddr,
    target: SocketAddr,
) -> Option<IpAddr> {
    match (local_addr, target) {
        (SocketAddr::V4(local), SocketAddr::V4(target)) => {
            match_ipv4_time_exceeded(packet, local, target).map(IpAddr::V4)
        }
        (SocketAddr::V6(local), SocketAddr::V6(target)) => {
            match_ipv6_time_exceeded(packet, source, local, target)
        }
        _ => None,
    }
}

fn match_ipv4_time_exceeded(
    packet: &[u8],
    local_addr: SocketAddrV4,
    target: SocketAddrV4,
) -> Option<Ipv4Addr> {
    let Ok(outer) = LaxSlicedPacket::from_ip(packet) else {
        return None;
    };
    let router = match outer.net {
        Some(LaxNetSlice::Ipv4(ip)) => ip.header().source_addr(),
        _ => return None,
    };
    let Some(TransportSlice::Icmpv4(icmp)) = outer.transport else {
        return None;
    };
    if matches!(
        icmp.icmp_type(),
        Icmpv4Type::TimeExceeded(icmpv4::TimeExceededCode::TtlExceededInTransit)
    ) && matching_quoted_tcp_tuple(
        icmp.payload(),
        SocketAddr::V4(local_addr),
        SocketAddr::V4(target),
    ) {
        Some(router)
    } else {
        None
    }
}

fn match_ipv6_time_exceeded(
    packet: &[u8],
    source: Option<SocketAddr>,
    local_addr: SocketAddrV6,
    target: SocketAddrV6,
) -> Option<IpAddr> {
    let (icmp, outer_router) = if packet.first().map(|byte| byte >> 4) == Some(6) {
        let Ok(outer) = LaxSlicedPacket::from_ip(packet) else {
            return None;
        };
        let router = match outer.net {
            Some(LaxNetSlice::Ipv6(ip)) => IpAddr::V6(ip.header().source_addr()),
            _ => return None,
        };
        let Some(TransportSlice::Icmpv6(icmp)) = outer.transport else {
            return None;
        };
        (icmp, Some(router))
    } else {
        let Ok(icmp) = Icmpv6Slice::from_slice(packet) else {
            return None;
        };
        (icmp, None)
    };
    if !matches!(
        icmp.icmp_type(),
        Icmpv6Type::TimeExceeded(icmpv6::TimeExceededCode::HopLimitExceeded)
    ) || !matching_quoted_tcp_tuple(
        icmp.payload(),
        SocketAddr::V6(local_addr),
        SocketAddr::V6(target),
    ) {
        return None;
    }
    outer_router.or_else(|| match source {
        Some(SocketAddr::V6(source)) => Some(IpAddr::V6(*source.ip())),
        _ => None,
    })
}

fn matching_quoted_tcp_tuple(packet: &[u8], local_addr: SocketAddr, target: SocketAddr) -> bool {
    let Ok(quoted) = LaxSlicedPacket::from_ip(packet) else {
        return false;
    };
    let payload = match (quoted.net, local_addr, target) {
        (Some(LaxNetSlice::Ipv4(ip)), SocketAddr::V4(local), SocketAddr::V4(target))
            if ip.header().source_addr() == *local.ip()
                && ip.header().destination_addr() == *target.ip()
                && ip.payload().ip_number == IpNumber::TCP =>
        {
            ip.payload().payload
        }
        (Some(LaxNetSlice::Ipv6(ip)), SocketAddr::V6(local), SocketAddr::V6(target))
            if ip.header().source_addr() == *local.ip()
                && ip.header().destination_addr() == *target.ip()
                && ip.payload().ip_number == IpNumber::TCP =>
        {
            ip.payload().payload
        }
        _ => return false,
    };
    let Some(ports) = payload.get(..4) else {
        return false;
    };
    u16::from_be_bytes([ports[0], ports[1]]) == local_addr.port()
        && u16::from_be_bytes([ports[2], ports[3]]) == target.port()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledged_payload_marks_direct_tcp_delivery() {
        assert_eq!(
            classify_hop(None, false, true),
            DpiHopProbeHopOutcome::TcpAcknowledged
        );
        assert!(DpiHopProbeHopOutcome::TcpAcknowledged.invalidates_measurement());
    }

    #[test]
    fn icmp_response_takes_precedence_over_tcp_state() {
        assert_eq!(
            classify_hop(Some(IpAddr::V4(Ipv4Addr::LOCALHOST)), true, true),
            DpiHopProbeHopOutcome::IcmpTimeExceeded
        );
    }

    #[test]
    fn matches_icmp_time_exceeded_quote_by_flow_tuple() {
        let local = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 10), 45_000);
        let target = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 10), 443);
        let router = Ipv4Addr::new(198, 51, 100, 1);
        let mut packet = vec![0; 20 + 8 + 20 + 4];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&52u16.to_be_bytes());
        packet[8] = 64;
        packet[9] = IpNumber::ICMP.0;
        packet[12..16].copy_from_slice(&router.octets());
        packet[20] = 11;
        packet[28] = 0x45;
        packet[30..32].copy_from_slice(&24u16.to_be_bytes());
        packet[36] = 1;
        packet[37] = IpNumber::TCP.0;
        packet[40..44].copy_from_slice(&local.ip().octets());
        packet[44..48].copy_from_slice(&target.ip().octets());
        packet[48..50].copy_from_slice(&local.port().to_be_bytes());
        packet[50..52].copy_from_slice(&target.port().to_be_bytes());

        assert_eq!(
            match_time_exceeded(&packet, None, SocketAddr::V4(local), SocketAddr::V4(target)),
            Some(IpAddr::V4(router))
        );
    }

    #[test]
    fn rejects_icmp_time_exceeded_for_another_port() {
        let local = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 10), 45_000);
        let target = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 10), 443);
        let mut packet = vec![0; 20 + 8 + 20 + 4];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&52u16.to_be_bytes());
        packet[9] = IpNumber::ICMP.0;
        packet[20] = 11;
        packet[28] = 0x45;
        packet[30..32].copy_from_slice(&24u16.to_be_bytes());
        packet[37] = IpNumber::TCP.0;
        packet[40..44].copy_from_slice(&local.ip().octets());
        packet[44..48].copy_from_slice(&target.ip().octets());
        packet[48..50].copy_from_slice(&local.port().to_be_bytes());
        packet[50..52].copy_from_slice(&444u16.to_be_bytes());

        assert_eq!(
            match_time_exceeded(&packet, None, SocketAddr::V4(local), SocketAddr::V4(target)),
            None
        );
    }

    #[test]
    fn matches_ipv6_icmp_time_exceeded_quote_by_flow_tuple() {
        let local = "[2001:db8::1]:45000".parse::<SocketAddrV6>().unwrap();
        let target = "[2001:db8::10]:443".parse::<SocketAddrV6>().unwrap();
        let router = "2001:db8::ff".parse::<std::net::Ipv6Addr>().unwrap();
        let mut packet = vec![0; 8 + 40 + 4];
        packet[0] = 3;
        packet[8] = 0x60;
        packet[14] = IpNumber::TCP.0;
        packet[16..32].copy_from_slice(&local.ip().octets());
        packet[32..48].copy_from_slice(&target.ip().octets());
        packet[48..50].copy_from_slice(&local.port().to_be_bytes());
        packet[50..52].copy_from_slice(&target.port().to_be_bytes());

        assert_eq!(
            match_time_exceeded(
                &packet,
                Some(SocketAddr::V6(SocketAddrV6::new(router, 0, 0, 0))),
                SocketAddr::V6(local),
                SocketAddr::V6(target)
            ),
            Some(IpAddr::V6(router))
        );

        packet[50..52].copy_from_slice(&444u16.to_be_bytes());
        assert_eq!(
            match_time_exceeded(
                &packet,
                Some(SocketAddr::V6(SocketAddrV6::new(router, 0, 0, 0))),
                SocketAddr::V6(local),
                SocketAddr::V6(target)
            ),
            None
        );
    }
}
