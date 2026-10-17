//! Reset-aware calibration is confined to the calibration destination. Ordinary
//! target tasks still send only the existing TTL-limited TCP probes.
use super::*;
use reports::probe::{DpiCalibrationAttempt, DpiCalibrationDebug};

const BUDGET: Duration = Duration::from_secs(45);
const REPEATS: usize = 3;

pub(super) fn calibrate(config: &DpiHopProbeConfig, reason: String) -> DpiCalibrationDebug {
    // Keep the negative control different even for a manual trace of example.com.
    let control_sni = if config
        .control_sni
        .trim_end_matches('.')
        .eq_ignore_ascii_case("example.com")
    {
        "example.net"
    } else {
        "example.com"
    };
    let deadline = Instant::now() + BUDGET;
    let mut debug = scan(config.max_ttl, |ttl, control| {
        attempt(
            config,
            if control {
                control_sni
            } else {
                &config.control_sni
            },
            ttl,
            control,
            deadline,
        )
    });
    debug.control_sni = Some(control_sni.into());
    debug.fallback_reason = Some(reason);
    debug
}

fn scan(
    max_ttl: u8,
    mut observe: impl FnMut(u8, bool) -> io::Result<DpiCalibrationAttempt>,
) -> DpiCalibrationDebug {
    let mut debug = DpiCalibrationDebug {
        method: "rst_client_hello".into(),
        ..Default::default()
    };
    let result = (|| -> Result<u8, String> {
        let mut last_clear = None;
        for ttl in 1..=max_ttl {
            let observation = pair(&mut debug, &mut observe, ttl)?;
            if observation.clear() {
                last_clear = Some(ttl);
            }
            if observation.differential_reset() {
                let mut witness = observation.control_expired().then_some(ttl);
                // Routers can suppress ICMP. Recheck the last positive lower
                // observation, which need not be adjacent to the reset TTL.
                if let Some(lower) = last_clear {
                    for _ in 0..REPEATS {
                        if !pair(&mut debug, &mut observe, lower)?.clear() {
                            return Err("unstable_lower_boundary".into());
                        }
                    }
                }
                for _ in 1..REPEATS {
                    let confirmation = pair(&mut debug, &mut observe, ttl)?;
                    if !confirmation.differential_reset() {
                        return Err("unstable_reset_boundary".into());
                    }
                    witness = witness.or(confirmation.control_expired().then_some(ttl));
                }
                // A positive control expiry at/above the reset is required:
                // silence alone cannot exclude a reset from the destination.
                // Also check that the reset persists above the chosen bound.
                for higher in ttl.saturating_add(1)..=max_ttl {
                    if higher == ttl {
                        break;
                    }
                    let confirmation = pair(&mut debug, &mut observe, higher)?;
                    if !confirmation.differential_reset() {
                        return Err("non_monotonic_reset_boundary".into());
                    }
                    witness = witness.or(confirmation.control_expired().then_some(higher));
                    if witness.is_some() {
                        break;
                    }
                }
                if witness.is_none() {
                    return Err("no_control_expiry_beyond_reset".into());
                }
                debug.last_clear_ttl = last_clear;
                debug.first_reset_ttl = Some(ttl);
                debug.control_witness_ttl = witness;
                // This is an upper bound on the trigger, not the last ICMP hop.
                // The existing task path starts at dpi_hop + 1, beyond that bound.
                return Ok(ttl);
            }
            if observation.test == "tcp_reset" {
                return Err("control_not_clear_at_reset".into());
            }
        }
        Err("no_repeatable_reset_boundary".into())
    })();
    match result {
        Ok(ttl) => debug.dpi_hop = Some(ttl),
        Err(reason) => debug.failure_reason = Some(reason),
    }
    debug
}

struct PairObservation {
    control: String,
    test: String,
}

impl PairObservation {
    fn clear(&self) -> bool {
        self.control_expired() && self.test == "icmp_time_exceeded"
    }

    fn control_expired(&self) -> bool {
        self.control == "icmp_time_exceeded"
    }

    fn differential_reset(&self) -> bool {
        matches!(self.control.as_str(), "icmp_time_exceeded" | "timeout")
            && self.test == "tcp_reset"
    }
}

fn pair(
    debug: &mut DpiCalibrationDebug,
    observe: &mut impl FnMut(u8, bool) -> io::Result<DpiCalibrationAttempt>,
    ttl: u8,
) -> Result<PairObservation, String> {
    let mut outcomes = Vec::with_capacity(2);
    for control in [true, false] {
        let attempt = observe(ttl, control).map_err(|error| error.to_string())?;
        let error = attempt.error.clone();
        outcomes.push(attempt.outcome.clone());
        debug.attempts.push(attempt);
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(PairObservation {
        control: outcomes.remove(0),
        test: outcomes.remove(0),
    })
}

fn remaining(deadline: Instant, limit: Duration) -> io::Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "rst_calibration_budget_exhausted",
        ))
    } else {
        Ok(remaining.min(limit))
    }
}

fn attempt(
    config: &DpiHopProbeConfig,
    sni: &str,
    ttl: u8,
    control: bool,
    deadline: Instant,
) -> io::Result<DpiCalibrationAttempt> {
    let timeout = remaining(deadline, config.connect_timeout)?;
    let (domain, protocol) = if config.target.is_ipv4() {
        (Domain::IPV4, Protocol::ICMPV4)
    } else {
        (Domain::IPV6, Protocol::ICMPV6)
    };
    let icmp = Socket::new(domain, Type::RAW, Some(protocol))?;
    // Capture is necessary even in automatic mode: a closed socket alone cannot
    // distinguish an injected RST from FIN, a TLS alert, or a local socket error.
    let capture = TcpCapture::start(config.target);
    let (port, connection) = connect_with_port(config.target, timeout);
    let mut router = None;
    let mut hello_ms = None;
    let mut local = None;
    let mut stream = None;
    let measurement = (|| -> io::Result<()> {
        stream = Some(connection?);
        let tcp = stream.as_mut().unwrap();
        tcp.set_nodelay(true)?;
        tcp.set_write_timeout(Some(remaining(deadline, config.connect_timeout)?))?;
        let address = tcp.local_addr()?;
        local = Some(address);
        let mut bind = address;
        bind.set_port(0);
        icmp.bind(&bind.into())?;
        // The handshake used normal TTL; only the ClientHello is TTL-limited.
        // Leave that TTL installed until drop, including any retransmissions.
        set_ttl(tcp, config.target, u32::from(ttl))?;
        let hello = make_client_hello(sni)?;
        hello_ms = Some(capture.elapsed_ms());
        tcp.write_all(&hello)?;
        router = listen_for_time_exceeded(
            &icmp,
            address,
            config.target,
            remaining(deadline, config.hop_timeout)?,
        )?;
        Ok(())
    })();
    let mut diagnostics = capture.finish(&port.into_iter().collect::<Vec<_>>());
    // Normalize raw IPv6 capture addresses in the same way as the legacy trace.
    if let Some(local) = local {
        for packet in &mut diagnostics.packets {
            if packet.destination.ip().is_unspecified() {
                packet.destination.set_ip(local.ip());
            }
        }
    }
    diagnostics.client_hello_sent_ms = hello_ms;
    let reset = diagnostics.packets.iter().any(|packet| {
        hello_ms.is_some_and(|sent| packet.observed_ms >= sent)
            && packet.flags.iter().any(|flag| flag == "RST")
    });
    let mut error = None;
    if let Err(failure) = measurement {
        let message = failure.to_string();
        if hello_ms.is_none() {
            diagnostics.connect_error = Some(message.clone());
        } else {
            diagnostics.send_error = Some(message.clone());
        }
        // An observed reset is useful even if write_all also reports the reset.
        if !reset {
            error = Some(message);
        }
    }
    if let Some(capture_error) = &diagnostics.capture_error {
        error = Some(format!("rst_capture_unavailable: {capture_error}"));
    }
    let outcome = if reset {
        "tcp_reset"
    } else if router.is_some() {
        "icmp_time_exceeded"
    } else if error.is_some() {
        "error"
    } else if diagnostics
        .packets
        .iter()
        .any(|packet| packet.flags.iter().any(|flag| flag == "FIN"))
    {
        "tcp_closed"
    } else if diagnostics
        .packets
        .iter()
        .any(|packet| packet.payload_bytes > 0)
    {
        "tcp_response"
    } else {
        "timeout"
    };
    // Keep command replies bounded even when the peer sends a large TLS flight.
    if diagnostics.packets.len() > 8 {
        diagnostics.packets.truncate(8);
        diagnostics.truncated = true;
    }
    Ok(DpiCalibrationAttempt {
        ttl,
        control,
        outcome: outcome.into(),
        router,
        error,
        tcp_diagnostics: config.collect_tcp_metadata.then_some(diagnostics),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ttl: u8, control: bool, outcome: &str) -> DpiCalibrationAttempt {
        DpiCalibrationAttempt {
            ttl,
            control,
            outcome: outcome.into(),
            router: (outcome == "icmp_time_exceeded").then(|| "192.0.2.1".parse().unwrap()),
            error: None,
            tcp_diagnostics: None,
        }
    }

    #[test]
    fn brackets_reset_injection_and_uses_upper_bound_for_target_probes() {
        let result = scan(8, |ttl, control| {
            Ok(sample(
                ttl,
                control,
                if !control && ttl >= 4 {
                    "tcp_reset"
                } else {
                    "icmp_time_exceeded"
                },
            ))
        });
        assert_eq!(result.dpi_hop, Some(4));
        assert_eq!(result.last_clear_ttl, Some(3));
        assert_eq!(result.first_reset_ttl, Some(4));
        assert_eq!(
            result
                .attempts
                .iter()
                .filter(|a| a.ttl == 4 && !a.control)
                .count(),
            REPEATS
        );
        assert!(result.attempts.iter().any(|a| a.ttl == 5));
    }

    #[test]
    fn identical_low_ttl_resets_are_not_a_boundary() {
        let result = scan(8, |ttl, control| Ok(sample(ttl, control, "tcp_reset")));
        assert_eq!(result.dpi_hop, None);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("control_not_clear_at_reset")
        );
    }

    #[test]
    fn missing_icmp_is_unknown_not_hop_zero() {
        let result = scan(4, |ttl, control| Ok(sample(ttl, control, "timeout")));
        assert_eq!(result.dpi_hop, None);
    }

    #[test]
    fn icmp_gaps_keep_a_range_with_a_positive_control_witness() {
        let result = scan(8, |ttl, control| {
            let outcome = if !control && ttl >= 4 {
                "tcp_reset"
            } else if ttl == 3 || ttl == 4 || ttl == 5 {
                "timeout"
            } else {
                "icmp_time_exceeded"
            };
            Ok(sample(ttl, control, outcome))
        });
        assert_eq!(result.dpi_hop, Some(4));
        assert_eq!(result.last_clear_ttl, Some(2));
        assert_eq!(result.control_witness_ttl, Some(6));
    }

    #[test]
    fn reset_without_any_control_expiry_is_not_trusted() {
        let result = scan(4, |ttl, control| {
            Ok(sample(
                ttl,
                control,
                if control { "timeout" } else { "tcp_reset" },
            ))
        });
        assert_eq!(result.dpi_hop, None);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("no_control_expiry_beyond_reset")
        );
    }

    #[test]
    fn transient_reset_does_not_survive_confirmation() {
        let mut injected = false;
        let result = scan(6, |ttl, control| {
            let reset = ttl == 3 && !control && !injected;
            injected |= reset;
            Ok(sample(
                ttl,
                control,
                if reset {
                    "tcp_reset"
                } else {
                    "icmp_time_exceeded"
                },
            ))
        });
        assert_eq!(result.dpi_hop, None);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("unstable_reset_boundary")
        );
    }

    #[test]
    fn non_monotonic_threshold_is_rejected() {
        let result = scan(6, |ttl, control| {
            Ok(sample(
                ttl,
                control,
                if ttl == 3 && !control {
                    "tcp_reset"
                } else {
                    "icmp_time_exceeded"
                },
            ))
        });
        assert_eq!(result.dpi_hop, None);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("non_monotonic_reset_boundary")
        );
    }

    #[test]
    fn partial_attempts_survive_a_connection_failure() {
        let result = scan(6, |ttl, control| {
            let mut attempt = sample(ttl, control, "icmp_time_exceeded");
            if ttl == 3 {
                attempt.error = Some("connection timed out".into());
            }
            Ok(attempt)
        });
        assert_eq!(result.dpi_hop, None);
        assert_eq!(result.attempts.len(), 5);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("connection timed out")
        );
    }

    #[test]
    fn expired_budget_prevents_another_connection() {
        assert!(remaining(Instant::now(), Duration::from_secs(5)).is_err());
    }

    #[test]
    #[ignore = "requires CAP_NET_RAW and local socket access"]
    fn live_clienthello_ttl_is_limited_and_fin_is_not_a_reset() {
        use std::io::Read;
        for reset in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let target = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut header = [0; 5];
                stream.read_exact(&mut header).unwrap();
                let mut hello = vec![0; u16::from_be_bytes([header[3], header[4]]) as usize];
                stream.read_exact(&mut hello).unwrap();
                if reset {
                    SockRef::from(&stream)
                        .set_linger(Some(Duration::ZERO))
                        .unwrap();
                }
            });
            let receiver = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::TCP)).unwrap();
            receiver
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let observer = std::thread::spawn(move || {
                let mut buffer = [0; 65535];
                loop {
                    let (bytes, _) = recv_socket(&receiver, &mut buffer).unwrap();
                    let packet = LaxSlicedPacket::from_ip(&buffer[..bytes]).unwrap();
                    if let (Some(LaxNetSlice::Ipv4(ip)), Some(TransportSlice::Tcp(tcp))) =
                        (packet.net, packet.transport)
                        && tcp.destination_port() == target.port()
                        && !tcp.payload().is_empty()
                    {
                        return ip.header().ttl();
                    }
                }
            });
            let config = DpiHopProbeConfig {
                target,
                control_sni: "instagram.com".into(),
                max_ttl: 4,
                connect_timeout: Duration::from_secs(1),
                hop_timeout: Duration::from_millis(100),
                accept_rst_fin_as_timeout: true,
                accept_ack_as_timeout: false,
                collect_tcp_metadata: true,
            };
            let result = attempt(
                &config,
                "instagram.com",
                3,
                false,
                Instant::now() + Duration::from_secs(3),
            )
            .unwrap();
            assert_eq!(observer.join().unwrap(), 3);
            server.join().unwrap();
            assert_eq!(
                result.outcome,
                if reset { "tcp_reset" } else { "tcp_closed" }
            );
            assert!(result.error.is_none());
            let diagnostics = result.tcp_diagnostics.unwrap();
            assert!(diagnostics.client_hello_sent_ms.is_some());
            assert_eq!(diagnostics.junk_sent_ms, None);
        }
    }
}
