use std::{collections::HashMap, sync::Arc};

use tokio::sync::broadcast;

use crate::runtime::state::{MobileRobotState, StateManager};

pub type StateSnapshot = Arc<HashMap<String, MobileRobotState>>;

pub struct Publisher {
    sender: broadcast::Sender<StateSnapshot>,
    statemanager: Arc<StateManager>,
}

impl Publisher {
    pub fn new(state_manager: Arc<StateManager>) -> Self {
        let (sender, _) = broadcast::channel(16);
        Self {
            sender,
            statemanager: state_manager,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StateSnapshot> {
        self.sender.subscribe()
    }

    pub fn publish(&self, snapshot: StateSnapshot) {
        // Err just means no subscribers are currently listening; nothing to do.
        let _ = self.sender.send(snapshot);
    }
}
