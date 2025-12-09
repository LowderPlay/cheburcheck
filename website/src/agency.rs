use crate::Db;
use reports::AgencyReport;
use rocket::http::Status;
use rocket::serde::json::serde_json::json;
use rocket::serde::json::{Json, Value};
use rocket::serde::msgpack::MsgPack;
use rocket_client_addr::ClientRealAddr;
use rocket_db_pools::Connection;
use sqlx::Acquire;

pub struct Agency {
    pub id: i32,
    pub name: String,
}

#[rocket::post("/report", format = "application/msgpack", data = "<report>")]
pub async fn upload_report(
    report: MsgPack<AgencyReport>,
    addr: &ClientRealAddr,
    agency: Agency,
    mut db: Connection<Db>,
) -> Result<Json<Value>, (Status, String)> {
    let mut tx = db
        .begin()
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;
    let report = report.into_inner();

    let report_id: i32 = sqlx::query_scalar(
        "INSERT INTO reports (
                    reporter,
                    reporter_ip,
                    version,
                    http,
                    tx_junk,
                    ip,
                    path,
                    retry_count,
                    timeout_secs,
                    probe_count
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(agency.id)
    .bind(addr.ip.to_string())
    .bind(report.version)
    .bind(report.config.http)
    .bind(report.config.tx_junk)
    .bind(report.config.ip.to_string())
    .bind(report.config.path)
    .bind(report.config.retry_count as i32)
    .bind(report.config.timeout_secs as i64)
    .bind(report.config.probe_count as i32)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    let mut copy_in = tx
        .copy_in_raw("COPY report_row (report_id, evidence, domain) FROM STDIN (FORMAT CSV)")
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    for (domain, evidence) in report.data {
        let line = format!("{},{},{}\n", report_id, evidence, domain);
        copy_in
            .send(line.as_bytes())
            .await
            .map_err(|e| (Status::InternalServerError, e.to_string()))?;
    }

    copy_in
        .finish()
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    sqlx::query!("REFRESH MATERIALIZED VIEW whitelist")
        .execute(&mut *tx)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    Ok(Json(json!({ "ok": true, "id": report_id })))
}
