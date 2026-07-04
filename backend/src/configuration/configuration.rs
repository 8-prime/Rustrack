use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Configuration {
    pub id: String,
    pub name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub tls_skip_verify: bool,
    pub vda5050_topic_prefix: String,
    pub created_at: String,
}
