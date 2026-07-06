use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Ok;
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::runtime::state::{MobileRobotState, StateManager};

pub type StateSnapshot = Arc<HashMap<String, MobileRobotState>>;

pub struct Publisher {
    sender: broadcast::Sender<StateSnapshot>,
    statemanager: Arc<StateManager>,
    cancellation: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl Publisher {
    pub fn new(state_manager: Arc<StateManager>) -> Self {
        let (sender, _) = broadcast::channel(16);
        Self {
            sender,
            statemanager: state_manager,
            cancellation: CancellationToken::new(),
            handle: None,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        let sender = self.sender.clone();
        let state_manager = self.statemanager.clone();
        let canellation = self.cancellation.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(50));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = canellation.cancelled() => break,
                        _ = ticker.tick() => {
                            state_manager.update_interpolation().await;
                            let snapshot = state_manager.get_snapshot().await;
                            let _ = sender.send(Arc::new(snapshot));
                        }
                }
            }
        });

        self.handle = Some(handle);

        Ok(())
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.cancellation.cancel();
        if let Some(handle) = self.handle.take() {
            handle.await?;
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StateSnapshot> {
        self.sender.subscribe()
    }

    pub fn publish(&self, snapshot: StateSnapshot) {
        // Err just means no subscribers are currently listening; nothing to do.
        let _ = self.sender.send(snapshot);
    }
}
