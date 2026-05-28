use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn sample_ipv4_subnet(net: Ipv4Net) -> Vec<IpAddr> {
    if net.prefix_len() >= 24 {
        net.hosts().map(IpAddr::V4).collect()
    } else {
        let mut ips = Vec::with_capacity(12);
        ips.push(IpAddr::V4(net.network()));

        let base = u32::from(net.network());
        let max = (1u64 << (32 - net.prefix_len())) - 1;

        for i in 1..=10 {
            let offset = (max * i) / 10;
            ips.push(IpAddr::V4(Ipv4Addr::from(base + offset as u32)));
        }

        ips.push(IpAddr::V4(net.broadcast()));
        ips
    }
}

pub fn sample_ipv6_subnet(net: Ipv6Net) -> Vec<IpAddr> {
    if net.prefix_len() >= 124 {
        net.hosts().map(IpAddr::V6).collect()
    } else {
        let mut ips = Vec::with_capacity(12);
        ips.push(IpAddr::V6(net.network()));

        let base = u128::from(net.network());
        let bits = 128 - net.prefix_len();
        let max = if bits >= 64 {
            u64::MAX as u128
        } else {
            (1u128 << bits) - 1
        };

        for i in 1..=10 {
            let offset = (max * i) / 10;
            ips.push(IpAddr::V6(Ipv6Addr::from(base + offset)));
        }

        ips.push(IpAddr::V6(net.broadcast()));
        ips
    }
}
