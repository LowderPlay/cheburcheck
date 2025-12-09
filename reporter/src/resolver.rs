use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, SocketAddr};

pub struct Resolver {
    ip: SocketAddr,
}

impl Resolver {
    pub fn new(ip: IpAddr) -> Resolver {
        Resolver {
            ip: SocketAddr::from(SocketAddr::new(ip, 0)),
        }
    }
}

impl Resolve for Resolver {
    fn resolve(&self, _: Name) -> Resolving {
        let ip = self.ip.clone();
        Box::pin(async move {
            Ok(Addrs::from(Box::new(vec![ip].into_iter())))
        })
    }
}
