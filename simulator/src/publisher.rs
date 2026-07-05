use anyhow::Result;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use rustrack_shared::vda5050::v2_0::state::AgvState;

pub struct Publisher {
    client: AsyncClient,
    topic_prefix: String,
}

impl Publisher {
    pub async fn connect(host: &str, port: u16, topic_prefix: String) -> Result<(Self, EventLoop)> {
        let mut opts = MqttOptions::new("rustrack-sim-publisher", host, port);
        opts.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, eventloop) = AsyncClient::new(opts, 128);
        Ok((
            Publisher {
                client,
                topic_prefix,
            },
            eventloop,
        ))
    }

    pub async fn publish_state(&self, serial: &str, state: &AgvState) -> Result<()> {
        let topic = format!("{}/v2/robot/{serial}/state", self.topic_prefix);
        let payload = serde_json::to_vec(state)?;
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload)
            .await?;
        Ok(())
    }
}
