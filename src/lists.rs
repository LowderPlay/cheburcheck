use std::collections::VecDeque;
use std::io;
use std::io::{BufRead, Error, Read};
use std::net::IpAddr;
use std::str::FromStr;
use ipnet::IpNet;
use ipnet_trie::IpnetTrie;
use serde::{de, Deserialize, Deserializer, Serializer};
use trie_rs::map::{Trie, TrieBuilder};
use crate::updater::{fetch_db, Updatable};

pub struct CdnList {
    trie: IpnetTrie<NetworkRecord>,
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
    pub fn new() -> CdnList{
        CdnList { trie: IpnetTrie::new() }
    }

    pub fn update<R: Read>(&mut self, list_reader: R) -> Result<(), Error>  {
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
        self.trie.longest_match(&IpNet::from(*ip)).map(|(_, net)| net.clone())
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
    domain_trie: Trie<String, String>,
    pub domain_count: usize,
}

impl RuBlacklist {
    pub fn new() -> RuBlacklist {
        RuBlacklist {
            ip_trie: Default::default(),
            domain_trie: TrieBuilder::new().build(),
            domain_count: 0
        }
    }

    pub fn update<R: BufRead>(&mut self, ip_reader: R, domain_reader: R) -> Result<(), Error>  {
        let mut ip_trie = IpnetTrie::new();
        for net in ip_reader.lines() {
            let net = net?;
            let net = IpNet::from_str(&net)
                .map_err(|e| Error::new(io::ErrorKind::InvalidData, e))?;
            ip_trie.insert(net, ());
        }
        let (v4, v6) = ip_trie.ip_count();
        info!("ip count: v4={}, v6={}", v4, v6);
        self.ip_trie = ip_trie;

        let mut domain_trie = TrieBuilder::new();
        let mut count = 0;
        for domain in domain_reader.lines() {
            let domain = domain?;
            domain_trie.insert(Self::domain_chunks(&domain), domain);
            count += 1;
        }
        info!("domain count: {}", count);
        self.domain_count = count;
        self.domain_trie = domain_trie.build();
        Ok(())
    }

    pub fn v4_count(&self) -> u32 {
        self.ip_trie.ip_count().0
    }

    fn domain_chunks(domain: &str) -> Vec<String> {
        domain.split(".").collect::<Vec<_>>()
            .into_iter().map(|s| s.to_string())
            .rev().collect()
    }

    pub fn contains_ip(&self, ip: &IpAddr) -> Option<IpNet> {
        self.ip_trie.longest_match(&IpNet::from(*ip)).map(|(ip, _)| ip)
    }

    pub fn contains_domain(&self, domain: &str) -> Option<String> {
        self.domain_trie.common_prefix_search(Self::domain_chunks(domain)).next()
            .map(|(_, b): (Vec<_>, &String)| b).cloned()
    }
}

#[async_trait]
impl Updatable for RuBlacklist {
    type Base = (VecDeque<u8>, VecDeque<u8>);

    async fn download() -> Result<Self::Base, Error> {
        Ok((VecDeque::from(
            fetch_db(Self::get_url("RKN_NETS", "https://antifilter.download/list/allyouneed.lst")).await?),
            VecDeque::from(
            fetch_db(Self::get_url("RKN_DOMAINS", "https://antifilter.download/list/domains.lst")).await?)))
    }

    async fn install(&mut self, (nets, domains): Self::Base) -> Result<(), Error> {
        self.update(nets, domains)
    }
}
