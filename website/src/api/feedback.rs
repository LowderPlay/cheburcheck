use crate::analytics::Analytics;
use rocket::State;
use rocket::http::Status;
use rocket_client_addr::ClientRealAddr;
use uuid::Uuid;

#[post("/feedback/<uuid>/<works>")]
pub async fn feedback(
    uuid: &str,
    works: bool,
    analytics: &State<Analytics>,
    addr: &ClientRealAddr,
) -> Result<(), Status> {
    let id = Uuid::try_parse(uuid).map_err(|_| Status::BadRequest)?;
    let query = analytics
        .query_by_id(id)
        .await
        .map_err(|error| {
            log::warn!("api: failed to load feedback query {id}: {error}");
            Status::InternalServerError
        })?
        .ok_or(Status::NotFound)?;
    analytics
        .feedback(id, &query.query, &addr.ip.to_string(), works)
        .await
        .map_err(|error| {
            log::warn!("api: failed to save feedback for query {id}: {error}");
            Status::InternalServerError
        })?;
    Ok(())
}
