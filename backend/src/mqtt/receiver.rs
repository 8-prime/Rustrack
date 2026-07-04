use crate::configuration::configuration::Configuration;

pub struct MqttReceiver {
    pub system_id: String,
    pub mqtt_url: String,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub topic_prefix: String,
    pub tls_skip_verify: bool,
}

impl MqttReceiver {
    // TODO: this only carries connection settings for now; actually
    // establishing/spawning the MQTT connection is separate follow-up work.
    pub fn new(config: &Configuration) -> Self {
        Self {
            system_id: config.id.clone(),
            mqtt_url: config.mqtt_url.clone(),
            mqtt_username: config.mqtt_username.clone(),
            mqtt_password: config.mqtt_password.clone(),
            topic_prefix: config.vda5050_topic_prefix.clone(),
            tls_skip_verify: !config.tls_skip_verify,
        }
    }
}
