use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::map::NodeMap;

pub trait Scenario: Send {
    /// Called when the AGV arrives at `current_node`. Returns the next node to move to,
    /// or `None` if the route is finished (non-looping scripted scenario).
    fn next_target(&mut self, current_node: &str, map: &NodeMap) -> Option<String>;
}

pub struct ScriptedScenario {
    route: Vec<String>,
    cursor: usize,
    loop_route: bool,
}

impl ScriptedScenario {
    pub fn new(route: Vec<String>, loop_route: bool) -> Self {
        Self {
            route,
            cursor: 0,
            loop_route,
        }
    }
}

impl Scenario for ScriptedScenario {
    fn next_target(&mut self, _current_node: &str, _map: &NodeMap) -> Option<String> {
        self.cursor += 1;
        if self.cursor >= self.route.len() {
            if self.loop_route {
                self.cursor = 0;
            } else {
                return None;
            }
        }
        Some(self.route[self.cursor].clone())
    }
}

pub struct RandomWalkScenario {
    prev_node: Option<String>,
    // StdRng is Send, unlike ThreadRng
    rng: StdRng,
}

impl RandomWalkScenario {
    pub fn new() -> Self {
        Self {
            prev_node: None,
            rng: StdRng::from_entropy(),
        }
    }
}

impl Scenario for RandomWalkScenario {
    fn next_target(&mut self, current_node: &str, map: &NodeMap) -> Option<String> {
        let neighbours: Vec<&str> = map
            .adjacency
            .get(current_node)
            .map(|eids| {
                let all: Vec<&str> = eids.iter().map(|eid| map.edges[eid].to.as_str()).collect();
                // Avoid immediate reversal when there are other options
                let filtered: Vec<&str> = all
                    .iter()
                    .copied()
                    .filter(|&n| Some(n) != self.prev_node.as_deref())
                    .collect();
                if filtered.is_empty() {
                    all
                } else {
                    filtered
                }
            })
            .unwrap_or_default();

        let chosen = neighbours.choose(&mut self.rng).copied()?.to_string();
        self.prev_node = Some(current_node.to_string());
        Some(chosen)
    }
}
