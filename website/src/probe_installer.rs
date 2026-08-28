use rocket::http::ContentType;

const INSTALLER: &str = include_str!("../../probe/install.sh");

#[get("/install-probe.sh")]
pub fn download() -> (ContentType, &'static str) {
    (ContentType::new("text", "x-shellscript"), INSTALLER)
}
