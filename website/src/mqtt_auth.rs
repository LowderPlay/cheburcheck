use rocket::form::Form;
use rocket::serde::json::Json;
use serde::Serialize;
use sqlx::PgPool;

#[derive(FromForm)]
pub struct MqttAuthRequest<'r> {
    username: &'r str,
    clientid: &'r str,
    password: &'r str,
    protocol: Option<&'r str>,
}

#[derive(FromForm)]
pub struct MqttAclRequest<'r> {
    access: u8,
    username: &'r str,
    clientid: &'r str,
    topic: &'r str,
    protocol: Option<&'r str>,
}

#[derive(Serialize)]
pub struct MqttAuthResponse {
    result: &'static str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    superuser: bool,
}

impl MqttAuthResponse {
    fn allow() -> Self {
        Self {
            result: "allow",
            superuser: false,
        }
    }

    fn allow_superuser() -> Self {
        Self {
            result: "allow",
            superuser: true,
        }
    }

    fn deny() -> Self {
        Self {
            result: "deny",
            superuser: false,
        }
    }
}

#[post("/auth", data = "<request>")]
pub async fn auth(
    request: Form<MqttAuthRequest<'_>>,
    pool: &rocket::State<PgPool>,
) -> Json<MqttAuthResponse> {
    let request = request.into_inner();
    let _ = request.protocol;

    if request.username == "admin"
        && std::env::var("MQTT_ADMIN_TOKEN")
            .map(|token| token == request.password)
            .unwrap_or(false)
    {
        return Json(MqttAuthResponse::allow_superuser());
    }

    if request.username != "probe" || request.clientid.is_empty() || request.password.is_empty() {
        return Json(MqttAuthResponse::deny());
    }

    let Ok(reporter_id) = request.clientid.parse::<i32>() else {
        return Json(MqttAuthResponse::deny());
    };
    let authenticated = sqlx::query_scalar::<_, i32>(
        "UPDATE reporters
         SET last_connected_at = NOW()
         WHERE id = $1 AND token = $2
         RETURNING id",
    )
    .bind(reporter_id)
    .bind(request.password)
    .fetch_optional(&**pool)
    .await
    .ok()
    .flatten()
    .is_some();

    if authenticated {
        Json(MqttAuthResponse::allow())
    } else {
        Json(MqttAuthResponse::deny())
    }
}

#[post("/acl", data = "<request>")]
pub async fn acl(
    request: Form<MqttAclRequest<'_>>,
    pool: &rocket::State<PgPool>,
) -> Json<MqttAuthResponse> {
    let request = request.into_inner();
    let _ = request.protocol;

    if request.username == "admin" {
        return Json(MqttAuthResponse::allow());
    }

    if request.username != "probe" || request.clientid.is_empty() {
        return Json(MqttAuthResponse::deny());
    }

    match request.access {
        1 if can_probe_subscribe(request.clientid, request.topic, pool).await => {
            Json(MqttAuthResponse::allow())
        }
        2 if can_probe_publish(request.clientid, request.topic) => Json(MqttAuthResponse::allow()),
        _ => Json(MqttAuthResponse::deny()),
    }
}

async fn can_probe_subscribe(client_id: &str, topic: &str, pool: &PgPool) -> bool {
    if topic == "probe/config/v1" || is_own_task_subscription(client_id, topic) {
        return true;
    }
    if !is_global_task_subscription(topic) {
        return false;
    }

    let Ok(reporter_id) = client_id.parse::<i32>() else {
        return false;
    };
    sqlx::query_scalar::<_, bool>("SELECT NOT hidden FROM reporters WHERE id = $1")
        .bind(reporter_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn is_own_task_subscription(client_id: &str, topic: &str) -> bool {
    topic == format!("probe/tasks/v1/{client_id}/+")
}

fn is_global_task_subscription(topic: &str) -> bool {
    topic == "probe/tasks/v1/+"
}

fn can_probe_publish(client_id: &str, topic: &str) -> bool {
    let status_topic = format!("probe/status/v1/{client_id}");
    if topic == status_topic {
        return true;
    }

    let mut parts = topic.split('/');
    matches!(
        (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next()
        ),
        (Some("probe"), Some("results"), Some("v1"), Some(_job_id), Some(probe_id), None)
            if probe_id == client_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individual_task_subscriptions_are_node_scoped() {
        assert!(is_global_task_subscription("probe/tasks/v1/+"));
        assert!(!is_global_task_subscription("probe/tasks/v1/#"));
        assert!(is_own_task_subscription("42", "probe/tasks/v1/42/+"));
        assert!(!is_own_task_subscription("42", "probe/tasks/v1/7/+"));
        assert!(!is_own_task_subscription("42", "probe/tasks/v1/+/+"));
        assert!(!is_own_task_subscription("42", "probe/tasks/v1/#"));
    }

    #[test]
    fn results_can_only_be_published_as_the_authenticated_node() {
        assert!(can_probe_publish("42", "probe/results/v1/job/42"));
        assert!(!can_probe_publish("42", "probe/results/v1/job/7"));
    }
}
