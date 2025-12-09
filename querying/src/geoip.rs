use crate::updater::{fetch_db, Updatable};
use async_trait::async_trait;
use maxminddb::geoip2::{city, country, City, Country};
use maxminddb::{geoip2, MaxMindDbError};
use serde::Serialize;
use std::io::Error;
use std::net::IpAddr;
use std::io;

pub struct GeoIp {
    asn: Option<maxminddb::Reader<Vec<u8>>>,
    city: Option<maxminddb::Reader<Vec<u8>>>,
    country: Option<maxminddb::Reader<Vec<u8>>>,
}

#[derive(Serialize, Debug)]
pub struct IpInfo {
    pub asn: Option<String>,
    pub country_code: Option<String>,
    pub organisation: Option<String>,
    pub city_geo_name_id: Option<u32>,
    location: String,
}

impl Default for IpInfo {
    fn default() -> IpInfo {
        IpInfo {
            asn: None,
            country_code: None,
            organisation: None,
            city_geo_name_id: None,
            location: "-".to_string(),
        }
    }
}

impl GeoIp {
    pub fn new() -> Self {
        GeoIp {
            asn: None,
            country: None,
            city: None
        }
    }

    pub fn update(&mut self, asn: Vec<u8>, country: Vec<u8>, city: Vec<u8>) -> Result<(), MaxMindDbError>  {
        self.asn = Some(maxminddb::Reader::from_source(asn)?);
        self.country = Some(maxminddb::Reader::from_source(country)?);
        self.city = Some(maxminddb::Reader::from_source(city)?);
        Ok(())
    }

    pub fn lookup(&self, ip: IpAddr) -> Result<IpInfo, MaxMindDbError> {
        let asn = if let Some(db) = &self.asn {
            db.lookup::<geoip2::Asn>(ip)?
        } else { None };
        let city = if let Some(db) = &self.city {
            db.lookup::<City>(ip)?
        } else { None };
        let country = if let Some(db) = &self.country {
            db.lookup::<Country>(ip)?
        } else { None };

        let country_code = country.as_ref().map(|c| c.country.as_ref()
                .map(|c| c.iso_code
                    .map(|c| c.to_string()))
                .flatten())
            .flatten();

        let city_geo_name_id = city.as_ref()
            .map(|c| c.city.as_ref()
                .map(|c| c.geoname_id)
                .flatten())
            .flatten();

        let location = match (city, country) {
            (Some(City { city: Some(city::City { names: Some(city), .. }),
                      country: Some(country::Country { names: Some(country), .. }), .. }), _) => {
                let city = city.get("ru").unwrap_or(&"-");
                let country = country.get("ru").unwrap_or(&"-");
                format!("{}, {}", city, country)
            }
            (_, Some(Country { country: Some(country::Country { names: Some(country), .. }), .. })) => {
                country.get("ru").unwrap_or(&"-").to_string()
            }
            (_, _) => "-".to_string(),
        };

        Ok(IpInfo {
            location,
            country_code,
            city_geo_name_id,
            asn: asn.clone().and_then(|asn| asn.autonomous_system_number)
                .map(|asn| format!("AS{}", asn)),
            organisation: asn.and_then(|asn| asn.autonomous_system_organization)
                .map(|org| org.to_string()),
        })
    }
}

#[async_trait]
impl Updatable for GeoIp {
    type Base = (Vec<u8>, Vec<u8>, Vec<u8>);

    async fn download() -> Result<Self::Base, Error> {
        Ok((fetch_db(Self::get_url("GEO_ASN", "https://git.io/GeoLite2-ASN.mmdb")).await?,
            fetch_db(Self::get_url("GEO_COUNTRY", "https://git.io/GeoLite2-Country.mmdb")).await?,
            fetch_db(Self::get_url("GEO_CITY", "https://git.io/GeoLite2-City.mmdb")).await?))
    }

    async fn install(&mut self, (asn, country, city): Self::Base) -> Result<(), Error> {
        self.update(asn, country, city)
            .map_err(|e| Error::new(io::ErrorKind::Other, e))
    }
}
