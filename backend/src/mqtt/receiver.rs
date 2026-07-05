use std::sync::Arc;

use rumqttc::{
    AsyncClient, Event, MqttOptions, Packet, Publish, QoS, SubscribeFilter, Transport,
    tokio_rustls::rustls::{
        self, ClientConfig, DigitallySignedStruct, SignatureScheme,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, ServerName, UnixTime},
    },
};
use serde_json::ser;
use shared::vda5050::{
    state::State,
    visualization::{self, Visualization},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{configuration::configuration::Configuration, runtime::state::StateManager};

pub struct MqttReceiver {
    pub system_id: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub topic_prefix: String,
    pub tls_skip_verify: bool,
    state_manager: Arc<StateManager>,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl MqttReceiver {
    // TODO: this only carries connection settings for now; actually
    // establishing/spawning the MQTT connection is separate follow-up work.
    pub fn new(config: Configuration, state_manager: Arc<StateManager>) -> Self {
        Self {
            system_id: config.id.clone(),
            mqtt_url: config.mqtt_url.clone(),
            mqtt_port: config.mqtt_port.clone(),
            mqtt_username: config.mqtt_username.clone(),
            mqtt_password: config.mqtt_password.clone(),
            topic_prefix: config.vda5050_topic_prefix.clone(),
            tls_skip_verify: config.tls_skip_verify,
            state_manager: state_manager,
            cancel: CancellationToken::new(),
            handle: None,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        let mut config = ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();

        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification));

        let mut opts = MqttOptions::new("Rustrack", self.mqtt_url.clone(), self.mqtt_port);
        if let Some(user) = &self.mqtt_username
            && let Some(password) = &self.mqtt_password
        {
            opts.set_credentials(user, password);
        }
        if self.tls_skip_verify {
            opts.set_transport(Transport::tls_with_config(config.into()));
        }

        let (client, eventloop): (AsyncClient, rumqttc::EventLoop) = AsyncClient::new(opts, 128);

        let filters = vec![
            SubscribeFilter::new(format!("{}/+/state", self.topic_prefix), QoS::AtMostOnce),
            SubscribeFilter::new(
                format!("{}/+/visualization", self.topic_prefix),
                QoS::AtMostOnce,
            ),
        ];

        client.subscribe_many(filters).await?;

        self.handle = Some(tokio::spawn(Self::receive_loop(
            client,
            eventloop,
            self.state_manager.clone(),
            self.cancel.clone(),
        )));
        Ok(())
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.cancel.cancel();
        if let Some(handle) = self.handle.take() {
            handle.await?;
        }
        return Ok(());
    }

    async fn receive_loop(
        client: AsyncClient,
        mut eventloop: rumqttc::EventLoop,
        state_manager: Arc<StateManager>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = client.disconnect().await;
                    break;
                }
                event = eventloop.poll() => {
                    match event  {
                        Ok(notification) => {
                            match notification {
                                Event::Incoming(Packet::Publish(pub_msg)) => {
                                    match handle_messag(pub_msg, state_manager.clone()).await {
                                        Ok(_) => todo!(),
                                        Err(_) => todo!(),
                                    }
                                }
                                _ => {}
                            }
                            //push notification into state manager
                        },
                        Err(_) => todo!(),
                    }
                }
            }
        }
    }
}

async fn handle_messag(
    publish_message: Publish,
    state_manager: Arc<StateManager>,
) -> anyhow::Result<()> {
    let mut parts = publish_message.topic.rsplit('/');
    let kind = parts.next();
    let serial = parts.next();

    match (serial, kind) {
        (Some(serial), Some("state")) => {
            match serde_json::from_slice::<State>(&publish_message.payload) {
                Ok(state) => {
                    state_manager
                        .update_state(serial.to_string(), state)
                        .await?
                }
                Err(e) => todo!(),
            }
        }
        (Some(serial), Some("visualization")) => {
            match serde_json::from_slice::<Visualization>(&publish_message.payload) {
                Ok(visualization) => {
                    state_manager
                        .update_visualization(serial.to_string(), visualization)
                        .await?
                }
                Err(_) => todo!(),
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}
