use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

pub struct RobotState {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub timestamp: DateTime<Utc>,
}

pub struct InterpolatedState {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub timestamp: DateTime<Utc>,
}

pub struct MobileRobotState {
    pub vda_state: Option<RobotState>,
    pub interpolated_state: Option<InterpolatedState>,
}

pub struct StateManager {
    pub states: RwLock<HashMap<String, MobileRobotState>>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }
}
