use crate::analytics::Analytics;
use crate::check_context::CheckContext;
use rocket::State;
use rocket::http::Status;
use rocket_client_addr::ClientRealAddr;

#[post("/feedback/<id>/<works>")]
pub async fn feedback(
    id: &str,
    works: bool,
    contexts: &State<CheckContext>,
    analytics: &State<Analytics>,
    addr: &ClientRealAddr,
) -> Result<(), Status> {
    let context = contexts.verify(id)?;
    let id = context.query_id;
    analytics
        .feedback(id, &context.target, &addr.ip.to_string(), works)
        .await
        .map_err(|error| {
            log::warn!("api: failed to save feedback for query {id}: {error}");
            Status::InternalServerError
        })?;
    Ok(())
}
