pub mod lif;
pub mod nurbs;
pub mod protocol;
pub mod vda5050;

/// Largest MQTT packet any Rustrack client or broker will accept or send.
/// rumqttc defaults to 10 KiB, which real VDA5050 state messages can exceed.
pub const MQTT_MAX_PACKET_BYTES: usize = 1024 * 1024;
