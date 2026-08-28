use crate::updater::{Updatable, fetch_db};
use async_trait::async_trait;
use ipnet::IpNet;
use ipnet_trie::IpnetTrie;
use log::info;
use serde::{Deserialize, Deserializer, Serializer, de};
use std::collections::VecDeque;
use std::io;
use std::io::{BufRead, Error, Read};
use std::net::IpAddr;
use std::str::FromStr;

pub struct CdnList {
    trie: IpnetTrie<NetworkRecord>,
}

impl Default for CdnList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash)]
pub struct NetworkRecord {
    pub provider: String,
    #[serde(deserialize_with = "deserialize_ip_net")]
    #[serde(serialize_with = "serialize_ip_net")]
    pub cidr: IpNet,
    pub region: Option<String>,
}

fn deserialize_ip_net<'de, D>(deserializer: D) -> Result<IpNet, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    FromStr::from_str(&s).map_err(de::Error::custom)
}

fn serialize_ip_net<S>(ip_net: &IpNet, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&ip_net.to_string())
}

impl CdnList {
    pub fn new() -> CdnList {
        CdnList {
            trie: IpnetTrie::new(),
        }
    }

    pub fn load<R: Read>(list_reader: R) -> Result<Self, Error> {
        let mut list = Self::new();
        list.update(list_reader)?;
        Ok(list)
    }

    pub fn update<R: Read>(&mut self, list_reader: R) -> Result<(), Error> {
        let mut trie = IpnetTrie::new();
        let mut rdr = csv::Reader::from_reader(list_reader);
        for result in rdr.deserialize() {
            let record: NetworkRecord = result?;
            trie.insert(record.cidr, record);
        }
        let (v4, v6) = trie.ip_count();
        info!("ip count: v4={}, v6={}", v4, v6);
        self.trie = trie;
        Ok(())
    }

    pub fn v4_count(&self) -> u32 {
        self.trie.ip_count().0
    }

    pub fn contains(&self, ip: &IpAddr) -> Option<NetworkRecord> {
        self.trie
            .longest_match(&IpNet::from(*ip))
            .map(|(_, net)| net.clone())
    }
}

#[async_trait]
impl Updatable for CdnList {
    type Base = VecDeque<u8>;

    async fn download() -> Result<Self::Base, Error> {
        Ok(VecDeque::from(fetch_db(Self::get_url(
            "CDN_SOURCE",
            "https://raw.githubusercontent.com/123jjck/cdn-ip-ranges/refs/heads/main/all/all.csv"
        )).await?))
    }

    async fn install(&mut self, base: Self::Base) -> Result<(), Error> {
        self.update(base)
    }
}

pub struct RuBlacklist {
    ip_trie: IpnetTrie<()>,
    blocked_domains: Vec<Box<str>>,
    pub domain_count: usize,
}

impl Default for RuBlacklist {
    fn default() -> Self {
        Self::new()
    }
}

impl RuBlacklist {
    pub fn new() -> RuBlacklist {
        RuBlacklist {
            ip_trie: Default::default(),
            blocked_domains: Vec::new(),
            domain_count: 0,
        }
    }

    pub fn load<R: BufRead>(
        ip_reader: R,
        domain_reader: R,
        custom_domains_reader: R,
    ) -> Result<Self, Error> {
        let mut list = Self::new();
        list.update(ip_reader, domain_reader, custom_domains_reader)?;
        Ok(list)
    }

    pub fn update<R: BufRead>(
        &mut self,
        ip_reader: R,
        domain_reader: R,
        custom_domains_reader: R,
    ) -> Result<(), Error> {
        let mut ip_trie = IpnetTrie::new();
        for net in ip_reader.lines() {
            let net = net?;
            let net =
                IpNet::from_str(&net).map_err(|e| Error::new(io::ErrorKind::InvalidData, e))?;
            ip_trie.insert(net, ());
        }
        let (v4, v6) = ip_trie.ip_count();
        info!("ip count: v4={}, v6={}", v4, v6);
        self.ip_trie = ip_trie;

        let mut blocked_domains = Vec::new();
        let mut count = 0;
        for domain in domain_reader.lines().chain(custom_domains_reader.lines()) {
            let domain = domain?;
            blocked_domains.push(domain.into_boxed_str());
            count += 1;
        }
        blocked_domains.sort_unstable();
        blocked_domains.dedup();
        info!("domain count: {}", count);
        self.domain_count = count;
        self.blocked_domains = blocked_domains;
        Ok(())
    }

    pub fn v4_count(&self) -> u32 {
        self.ip_trie.ip_count().0
    }

    pub fn contains_ip(&self, ip: &IpAddr) -> Option<IpNet> {
        self.ip_trie
            .longest_match(&IpNet::from(*ip))
            .map(|(ip, _)| ip)
    }

    pub fn contains_domain(&self, domain: &str) -> Option<String> {
        let mut suffix = domain;
        loop {
            if let Ok(index) = self
                .blocked_domains
                .binary_search_by(|blocked_domain| blocked_domain.as_ref().cmp(suffix))
            {
                return Some(self.blocked_domains[index].to_string());
            }

            match suffix.find('.') {
                Some(dot) => suffix = &suffix[dot + 1..],
                None => return None,
            }
        }
    }
}

#[async_trait]
impl Updatable for RuBlacklist {
    type Base = (VecDeque<u8>, VecDeque<u8>, VecDeque<u8>);

    async fn download() -> Result<Self::Base, Error> {
        Ok((
            VecDeque::from(
                fetch_db(Self::get_url(
                    "RKN_NETS",
                    "https://antifilter.network/download/ipsum.lst",
                ))
                .await?,
            ),
            VecDeque::from(
                fetch_db(Self::get_url(
                    "RKN_DOMAINS",
                    "https://antifilter.download/list/domains.lst",
                ))
                .await?,
            ),
            VecDeque::from(include_bytes!("../dist-domains.txt").to_vec()),
        ))
    }

    async fn install(&mut self, (nets, domains, custom_domains): Self::Base) -> Result<(), Error> {
        self.update(nets, domains, custom_domains)
    }
}

#[cfg(test)]
mod tests {
    use super::RuBlacklist;
    use std::io::Cursor;

    #[test]
    fn domain_lookup_matches_subdomains_on_label_boundaries() {
        let list = RuBlacklist::load(
            Cursor::new(""),
            Cursor::new("blocked.example\n"),
            Cursor::new("custom.test\n"),
        )
        .unwrap();

        assert_eq!(
            list.contains_domain("blocked.example"),
            Some("blocked.example".to_string())
        );
        assert_eq!(
            list.contains_domain("www.blocked.example"),
            Some("blocked.example".to_string())
        );
        assert_eq!(
            list.contains_domain("custom.test"),
            Some("custom.test".to_string())
        );
        assert_eq!(list.contains_domain("notblocked.example"), None);
    }
}
