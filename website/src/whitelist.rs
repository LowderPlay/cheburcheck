use crate::Db;
use rocket::futures::StreamExt;
use rocket::http::{ContentType, Status};
use rocket::request::FromParam;
use rocket_cache_response::CacheResponse;
use rocket_db_pools::Connection;
use std::io;
use rocket::serde::json::Json;
use crate::db::{collect_histogram, WhitelistHistogramBin};

enum ExportType {
    Full,
    Domains,
}

impl<'r> FromParam<'r> for ExportType {
    type Error = &'r str;

    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        match param {
            "full.csv" => Ok(ExportType::Full),
            "domains.csv" => Ok(ExportType::Domains),
            _ => Err(param),
        }
    }
}

#[get("/<export_type>")]
pub async fn export_csv(
    export_type: ExportType,
    mut db: Connection<Db>,
) -> Result<CacheResponse<(ContentType, Vec<u8>)>, io::Error> {
    let query = match export_type {
        ExportType::Full => {
            "COPY (SELECT domain, rank, last_ok FROM whitelist) TO STDOUT WITH (FORMAT CSV, HEADER, ENCODING 'UTF8')"
        }
        ExportType::Domains => {
            "COPY (SELECT domain FROM whitelist) TO STDOUT WITH (FORMAT CSV, ENCODING 'UTF8')"
        }
    };

    let mut stream = db
        .copy_out_raw(query)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let mut data = Vec::default();

    while let Some(bytes_result) = stream.next().await {
        let bytes = bytes_result.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        data.extend(bytes)
    }

    Ok(CacheResponse::Public {
        responder: (ContentType::CSV, data),
        max_age: 86400,
        must_revalidate: false,
    })
}

#[get("/histogram?<filter>&<limit>")]
pub async fn histogram(mut db: Connection<Db>, filter: Option<bool>, limit: Option<i32>) -> Result<Json<Vec<WhitelistHistogramBin>>, Status> {
    let limit = limit.unwrap_or(100_000).clamp(0, 1_000_000);
    Ok(Json(collect_histogram(&mut db, 50, limit, filter.is_some()).await
        .map_err(|e| Status::InternalServerError)?))
}
