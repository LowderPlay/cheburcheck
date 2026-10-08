use etherparse::{LaxNetSlice, LaxSlicedPacket, TcpOptionElement, TcpSlice, TransportSlice};
use reports::probe::{TcpDiagnostics, TcpPacketMetadata};
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::mem::MaybeUninit;
use std::net::{IpAddr, SocketAddr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_PACKETS: usize = 32;

/// A separate reader observes TCP headers without consuming the stream's data.
/// It runs before connect so a SYN-ACK or an immediate reset is not missed.
pub struct TcpCapture {
    started: Instant,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<TcpDiagnostics>>,
    error: Option<String>,
}

impl TcpCapture {
    pub fn start(target: SocketAddr) -> Self {
        let started = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let socket = (|| {
            let domain = if target.is_ipv4() {
                Domain::IPV4
            } else {
                Domain::IPV6
            };
            let socket = Socket::new(domain, Type::RAW, Some(Protocol::TCP))?;
            socket.set_read_timeout(Some(Duration::from_millis(20)))?;
            Ok::<_, io::Error>(socket)
        })();
        match socket {
            Ok(socket) => {
                let reader_stop = stop.clone();
                let reader = std::thread::spawn(move || {
                    let mut result = TcpDiagnostics::default();
                    let mut buffer = vec![MaybeUninit::<u8>::uninit(); 65535];
                    let mut shutdown_reads = 0;
                    loop {
                        if reader_stop.load(Ordering::Relaxed) {
                            shutdown_reads += 1;
                            if shutdown_reads > 256 {
                                result.truncated = true;
                                break;
                            }
                            if let Err(e) = socket.set_nonblocking(true) {
                                result.capture_error = Some(e.to_string());
                                break;
                            }
                        }
                        match socket.recv_from(&mut buffer) {
                            Ok((len, source)) => {
                                // SAFETY: recv_from initialized the returned prefix.
                                let bytes = unsafe {
                                    std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), len)
                                };
                                if let Some(packet) = parse_packet(
                                    bytes,
                                    source.as_socket(),
                                    target,
                                    started.elapsed().as_millis() as u64,
                                ) {
                                    // Filter to the exact local port(s) at finish. Bound the
                                    // temporary queue too, in case other traces run concurrently.
                                    if result.packets.len() < 256 {
                                        result.packets.push(packet);
                                    } else {
                                        result.truncated = true;
                                    }
                                }
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                                ) =>
                            {
                                if reader_stop.load(Ordering::Relaxed) {
                                    break;
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                            Err(e) => {
                                result.capture_error = Some(e.to_string());
                                break;
                            }
                        }
                    }
                    result
                });
                Self {
                    started,
                    stop,
                    reader: Some(reader),
                    error: None,
                }
            }
            Err(e) => Self {
                started,
                stop,
                reader: None,
                error: Some(e.to_string()),
            },
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn finish(mut self, ports: &[u16]) -> TcpDiagnostics {
        self.stop.store(true, Ordering::Relaxed);
        let mut result = match self.reader.take() {
            Some(reader) => reader.join().unwrap_or_else(|_| TcpDiagnostics {
                capture_error: Some("TCP capture reader panicked".into()),
                ..Default::default()
            }),
            None => TcpDiagnostics {
                capture_error: self.error.take(),
                ..Default::default()
            },
        };
        result
            .packets
            .retain(|p| ports.contains(&p.destination.port()));
        if result.packets.len() > MAX_PACKETS {
            result.packets.truncate(MAX_PACKETS);
            result.truncated = true;
        }
        result
    }
}

/// Binding before connecting preserves the local port even when connect fails.
pub fn connect_with_port(
    target: SocketAddr,
    timeout: Duration,
) -> (Option<u16>, io::Result<std::net::TcpStream>) {
    let mut port = None;
    let result = (|| {
        let domain = if target.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        let unspecified = if target.is_ipv4() {
            IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)
        };
        socket.bind(&SocketAddr::new(unspecified, 0).into())?;
        port = socket.local_addr()?.as_socket().map(|a| a.port());
        socket.connect_timeout(&target.into(), timeout)?;
        Ok(socket.into())
    })();
    (port, result)
}

impl Drop for TcpCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn parse_packet(
    bytes: &[u8],
    source: Option<SocketAddr>,
    target: SocketAddr,
    observed_ms: u64,
) -> Option<TcpPacketMetadata> {
    // Linux raw IPv6 sockets deliver the TCP segment without an IPv6 header.
    // recv_from supplies its source; the destination IP is unavailable there.
    let (src, dst, ttl, ip_id, tcp) =
        if target.is_ipv6() && source.is_some_and(|s| s.ip() == target.ip()) {
            match LaxSlicedPacket::from_ip(bytes) {
                Ok(packet) if matches!(packet.net, Some(LaxNetSlice::Ipv6(_))) => ip_tcp(packet)?,
                _ => (
                    target.ip(),
                    IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
                    None,
                    None,
                    TcpSlice::from_slice(bytes).ok()?,
                ),
            }
        } else {
            ip_tcp(LaxSlicedPacket::from_ip(bytes).ok()?)?
        };
    if src != target.ip() || tcp.source_port() != target.port() {
        return None;
    }
    let mut flags = Vec::new();
    for (set, name) in [
        (tcp.syn(), "SYN"),
        (tcp.ack(), "ACK"),
        (tcp.rst(), "RST"),
        (tcp.fin(), "FIN"),
        (tcp.psh(), "PSH"),
        (tcp.urg(), "URG"),
        (tcp.ece(), "ECE"),
        (tcp.cwr(), "CWR"),
    ] {
        if set {
            flags.push(name.to_owned());
        }
    }
    let mut timestamp = None;
    let mut timestamp_echo = None;
    for option in tcp.options_iterator().flatten() {
        if let TcpOptionElement::Timestamp(value, echo) = option {
            timestamp = Some(value);
            timestamp_echo = Some(echo);
        }
    }
    Some(TcpPacketMetadata {
        observed_ms,
        source: SocketAddr::new(src, tcp.source_port()),
        destination: SocketAddr::new(dst, tcp.destination_port()),
        ttl,
        ip_id,
        sequence: tcp.sequence_number(),
        acknowledgment: tcp.acknowledgment_number(),
        window: tcp.window_size(),
        flags,
        timestamp,
        timestamp_echo,
        options_hex: tcp.options().iter().map(|b| format!("{b:02x}")).collect(),
        payload_bytes: tcp.payload().len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;

    #[test]
    fn captures_reset_fingerprint_without_payload() {
        let mut bytes = Vec::new();
        PacketBuilder::ipv4([192, 0, 2, 1], [192, 0, 2, 2], 117)
            .tcp(443, 45000, 1234, 2048)
            .rst()
            .ack(5678)
            .options(&[TcpOptionElement::Timestamp(100, 200)])
            .unwrap()
            .write(&mut bytes, b"private payload")
            .unwrap();
        bytes[4..6].copy_from_slice(&42u16.to_be_bytes());
        let packet = parse_packet(&bytes, None, "192.0.2.1:443".parse().unwrap(), 12).unwrap();
        assert_eq!(packet.flags, ["ACK", "RST"]);
        assert_eq!(packet.ttl, Some(117));
        assert_eq!(packet.ip_id, Some(42));
        assert_eq!(
            (packet.sequence, packet.acknowledgment, packet.window),
            (1234, 5678, 2048)
        );
        assert_eq!(
            (packet.timestamp, packet.timestamp_echo),
            (Some(100), Some(200))
        );
        assert_eq!(packet.payload_bytes, 15);
        let json = serde_json::to_string(&packet).unwrap();
        assert!(!json.contains("private payload"));
        assert!(parse_packet(&bytes, None, "192.0.2.3:443".parse().unwrap(), 0).is_none());
        assert!(parse_packet(&bytes, None, "192.0.2.1:444".parse().unwrap(), 0).is_none());
        assert!(parse_packet(&bytes[..22], None, "192.0.2.1:443".parse().unwrap(), 0).is_none());
    }

    #[test]
    fn captures_ipv6_fin_with_or_without_ip_header() {
        let target: SocketAddr = "[2001:db8::1]:443".parse().unwrap();
        let dst: std::net::Ipv6Addr = "2001:db8::2".parse().unwrap();
        let IpAddr::V6(src) = target.ip() else {
            unreachable!()
        };
        let mut bytes = Vec::new();
        PacketBuilder::ipv6(src.octets(), dst.octets(), 55)
            .tcp(443, 45000, 9, 123)
            .fin()
            .ack(10)
            .write(&mut bytes, &[])
            .unwrap();
        let full = parse_packet(&bytes, Some(target), target, 20).unwrap();
        assert_eq!(full.ttl, Some(55));
        assert_eq!(full.destination.ip(), IpAddr::V6(dst));
        assert_eq!(full.flags, ["ACK", "FIN"]);
        let bare = parse_packet(&bytes[40..], Some(target), target, 21).unwrap();
        assert_eq!(bare.ttl, None);
        assert_eq!(bare.sequence, full.sequence);
        assert!(bare.destination.ip().is_unspecified());
    }

    #[test]
    fn legacy_hops_remain_compatible() {
        let json = r#"{"ttl":1,"src":null,"outcome":"tcp_closed"}"#;
        let hop: reports::probe::DpiProbeHop = serde_json::from_str(json).unwrap();
        assert!(hop.tcp_diagnostics.is_none());
        assert_eq!(serde_json::to_string(&hop).unwrap(), json);
        let diagnostics = TcpDiagnostics {
            packets: vec![],
            capture_error: Some("permission denied".into()),
            ..Default::default()
        };
        let hop = reports::probe::DpiProbeHop {
            tcp_diagnostics: Some(diagnostics),
            ..hop
        };
        let restored: reports::probe::DpiProbeHop =
            serde_json::from_str(&serde_json::to_string(&hop).unwrap()).unwrap();
        assert_eq!(restored.tcp_diagnostics, hop.tcp_diagnostics);
    }

    #[test]
    #[ignore = "requires CAP_NET_RAW and local socket access"]
    fn live_capture_records_syn_ack_and_reset() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"x").unwrap();
            std::thread::sleep(Duration::from_millis(50));
            socket2::SockRef::from(&stream)
                .set_linger(Some(Duration::ZERO))
                .unwrap();
        });
        let capture = TcpCapture::start(target);
        let (port, connection) = connect_with_port(target, Duration::from_secs(1));
        let mut stream = connection.unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        assert_eq!(
            stream.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
        server.join().unwrap();
        let result = capture.finish(&[port.unwrap()]);
        assert_eq!(result.capture_error, None);
        assert!(
            result
                .packets
                .iter()
                .any(|p| p.flags.contains(&"SYN".into()) && p.flags.contains(&"ACK".into()))
        );
        assert!(
            result
                .packets
                .iter()
                .any(|p| p.flags.contains(&"RST".into()))
        );
        assert!(
            result
                .packets
                .iter()
                .all(|p| p.ttl.is_some() && p.source == target)
        );
    }
}

fn ip_tcp(
    packet: LaxSlicedPacket<'_>,
) -> Option<(IpAddr, IpAddr, Option<u8>, Option<u16>, TcpSlice<'_>)> {
    let (src, dst, ttl, id) = match packet.net? {
        LaxNetSlice::Ipv4(ip) => (
            IpAddr::V4(ip.header().source_addr()),
            IpAddr::V4(ip.header().destination_addr()),
            Some(ip.header().ttl()),
            Some(ip.header().identification()),
        ),
        LaxNetSlice::Ipv6(ip) => (
            IpAddr::V6(ip.header().source_addr()),
            IpAddr::V6(ip.header().destination_addr()),
            Some(ip.header().hop_limit()),
            None,
        ),
        _ => return None,
    };
    let Some(TransportSlice::Tcp(tcp)) = packet.transport else {
        return None;
    };
    Some((src, dst, ttl, id, tcp))
}
