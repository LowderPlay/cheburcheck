use rocket::State;
use rocket::http::Status;
use rocket_client_addr::ClientRealAddr;
use sqlx::postgres::PgPool;
use sqlx::types::Uuid;

#[post("/feedback/<uuid>/<works>")]
pub async fn feedback(
    uuid: &str,
    works: bool,
    pool: &State<PgPool>,
    addr: &ClientRealAddr,
) -> Result<(), Status> {
    sqlx::query!(
        "INSERT INTO human_reports (id, source_ip, works) VALUES ($1, $2, $3)",
        Uuid::try_parse(uuid).map_err(|_| Status::BadRequest)?,
        addr.ip.to_string(),
        works
    )
    .execute(&**pool)
    .await
    .map_err(|_| Status::InternalServerError)?;

    Ok(())
}
