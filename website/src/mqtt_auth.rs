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
    let _ = (request.clientid, request.protocol);

    if request.username == "admin"
        && std::env::var("MQTT_ADMIN_TOKEN")
            .map(|token| token == request.password)
            .unwrap_or(false)
    {
        return Json(MqttAuthResponse::allow_superuser());
    }

    if request.username != "probe" || request.password.is_empty() {
        return Json(MqttAuthResponse::deny());
    }

    let token_exists =
        sqlx::query_scalar::<_, i32>("SELECT id FROM reporters WHERE token = $1 LIMIT 1")
            .bind(request.password)
            .fetch_optional(&**pool)
            .await
            .ok()
            .flatten()
            .is_some();

    if token_exists {
        Json(MqttAuthResponse::allow())
    } else {
        Json(MqttAuthResponse::deny())
    }
}

#[post("/acl", data = "<request>")]
pub async fn acl(request: Form<MqttAclRequest<'_>>) -> Json<MqttAuthResponse> {
    let request = request.into_inner();
    let _ = request.protocol;

    if request.username == "admin" {
        return Json(MqttAuthResponse::allow());
    }

    if request.username != "probe" || request.clientid.is_empty() {
        return Json(MqttAuthResponse::deny());
    }

    match request.access {
        1 if can_probe_subscribe(request.topic) => Json(MqttAuthResponse::allow()),
        2 if can_probe_publish(request.clientid, request.topic) => Json(MqttAuthResponse::allow()),
        _ => Json(MqttAuthResponse::deny()),
    }
}

fn can_probe_subscribe(topic: &str) -> bool {
    matches!(
        topic,
        "probe/config/v1" | "probe/tasks/v1/+" | "probe/tasks/v1/#"
    )
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
