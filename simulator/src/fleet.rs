use rustrack_shared::vda5050::v2_0::state::AgvState;

use crate::{agv::AgvSimulator, map::NodeMap};

pub struct FleetController {
    agvs: Vec<AgvSimulator>,
}

impl FleetController {
    pub fn new(agvs: Vec<AgvSimulator>) -> Self {
        Self { agvs }
    }

    /// Advance all AGVs by `dt` seconds. Returns (serial, state) pairs.
    pub fn tick(&mut self, dt: f64, map: &NodeMap) -> Vec<(String, AgvState)> {
        self.agvs
            .iter_mut()
            .map(|agv| {
                let serial = agv.serial.clone();
                let state = agv.tick(dt, map);
                (serial, state)
            })
            .collect()
    }
}
