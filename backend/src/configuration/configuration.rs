use serde::{Deserialize, Serialize};

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

/// The caller-supplied part of a [`Configuration`] — everything except the
/// server-assigned `id` and `created_at`. Used as the request body for both
/// creating and updating a system.
#[derive(Clone, Deserialize)]
pub struct ConfigurationFields {
    pub name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub tls_skip_verify: bool,
    pub vda5050_topic_prefix: String,
}

impl Configuration {
    pub fn new(id: String, created_at: String, fields: ConfigurationFields) -> Self {
        Self {
            id,
            created_at,
            name: fields.name,
            mqtt_url: fields.mqtt_url,
            mqtt_port: fields.mqtt_port,
            mqtt_username: fields.mqtt_username,
            mqtt_password: fields.mqtt_password,
            tls_skip_verify: fields.tls_skip_verify,
            vda5050_topic_prefix: fields.vda5050_topic_prefix,
        }
    }

    /// Apply new field values, preserving `id` and `created_at`.
    pub fn with_fields(&self, fields: ConfigurationFields) -> Self {
        Self::new(self.id.clone(), self.created_at.clone(), fields)
    }
}
