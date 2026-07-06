use chrono::Utc;

use crate::runtime::state::{InterpolatedState, MobileRobotState};

pub fn interpolate(robot_state: &MobileRobotState) -> Option<InterpolatedState> {
    let Some(position) = robot_state.vda_state.position.as_ref() else {
        return None;
    };
    Some(InterpolatedState {
        x: position.x.clone(),
        y: position.y.clone(),
        theta: position.theta.clone(),
        timestamp: Utc::now(),
    })
}
