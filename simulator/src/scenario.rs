use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::map::NodeMap;

pub trait Scenario: Send {
    /// Called when the AGV arrives at `current_node`. Returns the next node to move to,
    /// or `None` if the route is finished (non-looping scripted scenario).
    fn next_target(&mut self, current_node: &str, map: &NodeMap) -> Option<String>;

    /// The nodes this scenario has already planned after its current target, in
    /// driving order. Empty when it only decides on arrival.
    fn upcoming(&self) -> Vec<String> {
        Vec::new()
    }
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
    fn next_target(&mut self, current_node: &str, _map: &NodeMap) -> Option<String> {
        self.cursor += 1;
        if self.cursor >= self.route.len() {
            if self.loop_route {
                // A closed route ends where it starts; wrapping onto that same
                // node would ask to drive to where the AGV already is.
                self.cursor = usize::from(self.route.first().is_some_and(|n| n == current_node));
            } else {
                return None;
            }
        }
        self.route.get(self.cursor).cloned()
    }

    fn upcoming(&self) -> Vec<String> {
        let len = self.route.len();
        // One lap ahead on a loop, so the reported order doesn't grow forever.
        let ahead = if self.loop_route {
            len.saturating_sub(1)
        } else {
            len.saturating_sub(self.cursor + 1)
        };

        let mut nodes: Vec<String> = Vec::with_capacity(ahead);
        let mut previous = self.route.get(self.cursor);
        for offset in 1..=ahead {
            let node = &self.route[(self.cursor + offset) % len];
            if previous != Some(node) {
                nodes.push(node.clone());
            }
            previous = Some(node);
        }
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scripted(route: &[&str], looping: bool) -> ScriptedScenario {
        ScriptedScenario::new(route.iter().map(|n| n.to_string()).collect(), looping)
    }

    fn no_map() -> NodeMap {
        NodeMap {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            adjacency: HashMap::new(),
            map_id: String::new(),
        }
    }

    #[test]
    fn a_closed_loop_keeps_moving_past_its_start() {
        let mut s = scripted(&["A", "B", "A"], true);
        let map = no_map();

        assert_eq!(s.next_target("A", &map).as_deref(), Some("B"));
        assert_eq!(s.next_target("B", &map).as_deref(), Some("A"));
        assert_eq!(s.next_target("A", &map).as_deref(), Some("B"));
    }

    #[test]
    fn upcoming_on_a_closed_loop_never_repeats_a_node_in_a_row() {
        let s = scripted(&["A", "B", "C", "A"], true);
        assert_eq!(s.upcoming(), ["B", "C", "A"]);
    }

    #[test]
    fn upcoming_stops_at_the_end_of_a_one_way_route() {
        let mut s = scripted(&["A", "B", "C"], false);
        s.next_target("A", &no_map());
        assert_eq!(s.upcoming(), ["C"]);
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
