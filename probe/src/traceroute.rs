use etherparse::{
    Icmpv4Type, Icmpv6Slice, Icmpv6Type, IpNumber, LaxNetSlice, LaxSlicedPacket, TransportSlice,
    icmpv4, icmpv6,
};
use polling::{Event, Events, Poller};
use reports::probe::{TcpTracerouteOutcome, TcpTracerouteResult};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::{self, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

const HTTPS_PORT: u16 = 443;
const HOP_TIMEOUT: Duration = Duration::from_secs(1);

pub async fn tcp_traceroute(target: IpAddr, max_hops: u8) -> Option<TcpTracerouteResult> {
    tokio::task::spawn_blocking(move || trace_blocking(target, max_hops))
        .await
        .map_err(|error| log::warn!("TCP traceroute task failed for {target}: {error}"))
        .ok()?
        .map_err(|error| log::warn!("TCP traceroute failed for {target}: {error}"))
        .ok()
}

fn trace_blocking(target: IpAddr, max_hops: u8) -> io::Result<TcpTracerouteResult> {
    let (domain, icmp_protocol) = match target {
        IpAddr::V4(_) => (Domain::IPV4, Protocol::ICMPV4),
        IpAddr::V6(_) => (Domain::IPV6, Protocol::ICMPV6),
    };
    let receiver = Socket::new(domain, Type::RAW, Some(icmp_protocol))?;
    let mut last_icmp_hop = None;

    for ttl in 1..=max_hops {
        let tcp = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        tcp.set_nonblocking(true)?;
        match target {
            IpAddr::V4(_) => tcp.set_ttl_v4(ttl as u32)?,
            IpAddr::V6(_) => tcp.set_unicast_hops_v6(ttl as u32)?,
        }
        let unspecified = match target {
            IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
        };
        tcp.bind(&SockAddr::from(unspecified))?;
        let destination = SockAddr::from(SocketAddr::new(target, HTTPS_PORT));
        if let Err(error) = tcp.connect(&destination) {
            if error.kind() == io::ErrorKind::ConnectionRefused {
                return Ok(TcpTracerouteResult {
                    target,
                    result: TcpTracerouteOutcome::Rst { hop: ttl },
                });
            }
            if !is_connect_in_progress(&error) {
                return Err(error);
            }
        }
        let source_port = tcp
            .local_addr()?
            .as_socket()
            .map(|addr| addr.port())
            .unwrap_or(0);
        match wait_for_hop_response(&receiver, &tcp, target, source_port)? {
            HopResponse::IcmpTimeExceeded => last_icmp_hop = Some(ttl),
            HopResponse::Rst => {
                return Ok(TcpTracerouteResult {
                    target,
                    result: TcpTracerouteOutcome::Rst { hop: ttl },
                });
            }
            HopResponse::Connected => {
                return Ok(TcpTracerouteResult {
                    target,
                    result: TcpTracerouteOutcome::Connected { hop: ttl },
                });
            }
            HopResponse::Timeout => {}
        }
    }

    Ok(TcpTracerouteResult {
        target,
        result: last_icmp_hop
            .map(|hop| TcpTracerouteOutcome::IcmpTimeExceeded { hop })
            .unwrap_or(TcpTracerouteOutcome::Timeout),
    })
}

fn is_connect_in_progress(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
        || error.raw_os_error() == Some(rustix::io::Errno::INPROGRESS.raw_os_error())
}

enum HopResponse {
    IcmpTimeExceeded,
    Rst,
    Connected,
    Timeout,
}

fn wait_for_hop_response(
    receiver: &Socket,
    tcp: &Socket,
    target: IpAddr,
    source_port: u16,
) -> io::Result<HopResponse> {
    const ICMP_KEY: usize = 1;
    const TCP_KEY: usize = 2;

    let poller = Poller::new()?;
    // SAFETY: both sockets remain alive and are removed from the poller before this function exits.
    unsafe {
        poller.add(receiver, Event::readable(ICMP_KEY))?;
        if let Err(error) = poller.add(tcp, Event::writable(TCP_KEY)) {
            poller.delete(receiver)?;
            return Err(error);
        }
    }

    let result = wait_on_poller(&poller, receiver, tcp, target, source_port);
    let tcp_delete = poller.delete(tcp);
    let receiver_delete = poller.delete(receiver);
    let cleanup = tcp_delete.and(receiver_delete);
    match result {
        Ok(response) => cleanup.map(|()| response),
        Err(error) => Err(error),
    }
}

fn wait_on_poller(
    poller: &Poller,
    receiver: &Socket,
    tcp: &Socket,
    target: IpAddr,
    source_port: u16,
) -> io::Result<HopResponse> {
    const ICMP_KEY: usize = 1;
    const TCP_KEY: usize = 2;

    let deadline = Instant::now() + HOP_TIMEOUT;
    let mut watch_tcp = true;
    let mut buffer = [0u8; 2048];
    let mut events = Events::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(HopResponse::Timeout);
        }
        events.clear();
        match poller.wait(&mut events, Some(remaining)) {
            Ok(0) => return Ok(HopResponse::Timeout),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }

        if watch_tcp && events.iter().any(|event| event.key == TCP_KEY) {
            match tcp.take_error()? {
                Some(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    return Ok(HopResponse::Rst);
                }
                None => return Ok(HopResponse::Connected),
                Some(_) => watch_tcp = false,
            }
        }

        if events.iter().any(|event| event.key == ICMP_KEY) {
            let mut raw = receiver;
            match raw.read(&mut buffer) {
                Ok(bytes) if is_matching_time_exceeded(&buffer[..bytes], target, source_port) => {
                    return Ok(HopResponse::IcmpTimeExceeded);
                }
                Ok(_) => poller.modify(receiver, Event::readable(ICMP_KEY))?,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    poller.modify(receiver, Event::readable(ICMP_KEY))?;
                }
                Err(error) => return Err(error),
            }
        }
    }
}
fn is_matching_time_exceeded(packet: &[u8], target: IpAddr, source_port: u16) -> bool {
    match target {
        IpAddr::V4(target) => matching_ipv4_time_exceeded(packet, target, source_port),
        IpAddr::V6(target) => matching_ipv6_time_exceeded(packet, target, source_port),
    }
}

fn matching_ipv4_time_exceeded(packet: &[u8], target: Ipv4Addr, source_port: u16) -> bool {
    let Ok(outer) = LaxSlicedPacket::from_ip(packet) else {
        return false;
    };
    let Some(TransportSlice::Icmpv4(icmp)) = outer.transport else {
        return false;
    };
    matches!(
        icmp.icmp_type(),
        Icmpv4Type::TimeExceeded(icmpv4::TimeExceededCode::TtlExceededInTransit)
    ) && matching_quoted_tcp(icmp.payload(), IpAddr::V4(target), source_port)
}

fn matching_ipv6_time_exceeded(packet: &[u8], target: Ipv6Addr, source_port: u16) -> bool {
    let icmp = if packet.first().map(|byte| byte >> 4) == Some(6) {
        let Ok(outer) = LaxSlicedPacket::from_ip(packet) else {
            return false;
        };
        let Some(TransportSlice::Icmpv6(icmp)) = outer.transport else {
            return false;
        };
        icmp
    } else {
        let Ok(icmp) = Icmpv6Slice::from_slice(packet) else {
            return false;
        };
        icmp
    };
    matches!(
        icmp.icmp_type(),
        Icmpv6Type::TimeExceeded(icmpv6::TimeExceededCode::HopLimitExceeded)
    ) && matching_quoted_tcp(icmp.payload(), IpAddr::V6(target), source_port)
}

fn matching_quoted_tcp(packet: &[u8], target: IpAddr, source_port: u16) -> bool {
    let Ok(quoted) = LaxSlicedPacket::from_ip(packet) else {
        return false;
    };
    let (ip_number, payload) = match (quoted.net, target) {
        (Some(LaxNetSlice::Ipv4(ip)), IpAddr::V4(target))
            if ip.header().destination_addr() == target =>
        {
            (ip.payload().ip_number, ip.payload().payload)
        }
        (Some(LaxNetSlice::Ipv6(ip)), IpAddr::V6(target))
            if ip.header().destination_addr() == target =>
        {
            (ip.payload().ip_number, ip.payload().payload)
        }
        _ => return false,
    };
    if ip_number != IpNumber::TCP {
        return false;
    }
    let Some(ports) = payload.get(..4) else {
        return false;
    };
    u16::from_be_bytes([ports[0], ports[1]]) == source_port
        && u16::from_be_bytes([ports[2], ports[3]]) == HTTPS_PORT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ipv4_time_exceeded_for_tcp_flow() {
        let target = Ipv4Addr::new(203, 0, 113, 10);
        let mut packet = vec![0; 20 + 8 + 20 + 8];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&56u16.to_be_bytes());
        packet[9] = IpNumber::ICMP.0;
        packet[20] = 11;
        packet[28] = 0x45;
        packet[30..32].copy_from_slice(&28u16.to_be_bytes());
        packet[37] = 6;
        packet[44..48].copy_from_slice(&target.octets());
        packet[48..50].copy_from_slice(&42_000u16.to_be_bytes());
        packet[50..52].copy_from_slice(&HTTPS_PORT.to_be_bytes());

        assert!(matching_ipv4_time_exceeded(&packet, target, 42_000));
        assert!(!matching_ipv4_time_exceeded(&packet, target, 42_001));
    }

    #[test]
    fn matches_ipv6_time_exceeded_for_tcp_flow() {
        let target = "2001:db8::10".parse::<Ipv6Addr>().unwrap();
        let mut packet = vec![0; 8 + 40 + 8];
        packet[0] = 3;
        packet[8] = 0x60;
        packet[14] = 6;
        packet[32..48].copy_from_slice(&target.octets());
        packet[48..50].copy_from_slice(&42_000u16.to_be_bytes());
        packet[50..52].copy_from_slice(&HTTPS_PORT.to_be_bytes());

        assert!(matching_ipv6_time_exceeded(&packet, target, 42_000));
        assert!(!matching_ipv6_time_exceeded(&packet, target, 42_001));
    }
}
