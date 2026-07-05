use anyhow::Result;
use rumqttd::{Broker, Config};
use std::net::SocketAddr;

/// Start the embedded rumqttd broker. Spawns the broker in a background thread.
/// Returns the bound address so publisher clients can connect to it.
pub fn start(bind_addr: &str, port: u16) -> Result<SocketAddr> {
    let addr: SocketAddr = format!("{bind_addr}:{port}").parse()?;

    // Build config as TOML to avoid rumqttd's private struct fields
    let config_toml = format!(
        r#"
id = 0

[router]
max_connections = 100
max_outgoing_packet_count = 200
max_segment_size = 104857600
max_segment_count = 10

[v4.mqtt]
name = "mqtt"
listen = "{bind_addr}:{port}"
next_connection_delay_ms = 1

[v4.mqtt.connections]
connection_timeout_ms = 5000
max_payload_size = 262144
max_inflight_count = 500
dynamic_filters = true
"#
    );

    let config: Config =
        toml::from_str(&config_toml).map_err(|e| anyhow::anyhow!("rumqttd config error: {e}"))?;

    let mut broker = Broker::new(config);

    std::thread::spawn(move || {
        if let Err(e) = broker.start() {
            tracing::error!("embedded MQTT broker exited: {e}");
        }
    });

    // Give the broker a moment to bind before clients try to connect
    std::thread::sleep(std::time::Duration::from_millis(300));

    tracing::info!("embedded MQTT broker listening on {addr}");
    Ok(addr)
}
