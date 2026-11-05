use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::Serialize;
use shared::vda5050::state::{ActionStatus, State};

/// How many orders are remembered per device.
pub const MAX_ORDERS_PER_DEVICE: usize = 5;

/// Where an order stands. VDA5050 has no such field, so this is derived from what
/// the device reports about the work it has left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// The device still has nodes, edges or actions to get through.
    Active,
    /// Nothing is left and every action succeeded.
    Finished,
    /// Nothing is left and at least one action failed. Cancelling an order fails
    /// its unfinished actions, so cancelled orders land here too.
    Failed,
    /// Replaced by a newer order before the device was seen to finish it, for
    /// example because the state message that said so was lost.
    Unknown,
}

impl OrderStatus {
    fn of(state: &State) -> Self {
        let action_pending = state.action_states.iter().any(|a| {
            !matches!(
                a.action_status,
                ActionStatus::Finished | ActionStatus::Failed
            )
        });

        if action_pending || !state.node_states.is_empty() || !state.edge_states.is_empty() {
            Self::Active
        } else if state
            .action_states
            .iter()
            .any(|a| matches!(a.action_status, ActionStatus::Failed))
        {
            Self::Failed
        } else {
            Self::Finished
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRecord {
    pub order_id: String,
    /// Latest revision seen. Master control extends an order by sending the same
    /// `orderId` with a higher `orderUpdateId`.
    pub order_update_id: i64,
    pub status: OrderStatus,
    /// Device timestamp of the first state message carrying this order. Later
    /// than the real start if the backend came up while the order was running.
    pub started_at: DateTime<Utc>,
    /// Device timestamp at which the order stopped being active.
    pub ended_at: Option<DateTime<Utc>>,
}

/// The most recent orders of one device, newest first.
#[derive(Clone, Default)]
pub struct OrderHistory(VecDeque<OrderRecord>);

impl OrderHistory {
    /// Fold one `state` message into the history.
    pub fn observe(&mut self, state: &State, at: DateTime<Utc>) {
        // An empty id means the device has not had an order yet.
        if state.order_id.is_empty() {
            return;
        }

        let status = OrderStatus::of(state);
        let ended_at = (status != OrderStatus::Active).then_some(at);

        match self.0.front_mut() {
            // Devices repeat their last orderId until a new order arrives, so this
            // is the common case. Status is recomputed rather than latched because
            // a finished order can still be extended.
            Some(current) if current.order_id == state.order_id => {
                current.order_update_id = state.order_update_id;
                current.status = status;
                current.ended_at = ended_at.map(|at| current.ended_at.unwrap_or(at));
            }
            previous => {
                if let Some(previous) = previous
                    && previous.status == OrderStatus::Active
                {
                    previous.status = OrderStatus::Unknown;
                    previous.ended_at = Some(at);
                }
                self.0.truncate(MAX_ORDERS_PER_DEVICE - 1);
                self.0.push_front(OrderRecord {
                    order_id: state.order_id.clone(),
                    order_update_id: state.order_update_id,
                    status,
                    started_at: at,
                    ended_at,
                });
            }
        }
    }

    pub fn records(&self) -> Vec<OrderRecord> {
        self.0.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::{Value, json};

    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    fn state(order_id: &str, update_id: i64, nodes: usize, action_statuses: &[&str]) -> State {
        let node_states: Vec<Value> = (0..nodes)
            .map(|i| json!({ "nodeId": format!("N{i}"), "sequenceId": i, "released": true }))
            .collect();
        let action_states: Vec<Value> = action_statuses
            .iter()
            .enumerate()
            .map(|(i, status)| json!({ "actionId": format!("A{i}"), "actionStatus": status }))
            .collect();

        serde_json::from_value(json!({
            "headerId": 1,
            "timestamp": "2026-01-01T00:00:00Z",
            "version": "2.1.0",
            "manufacturer": "acme",
            "serialNumber": "AGV-1",
            "orderId": order_id,
            "orderUpdateId": update_id,
            "lastNodeId": "",
            "lastNodeSequenceId": 0,
            "driving": false,
            "nodeStates": node_states,
            "edgeStates": [],
            "actionStates": action_states,
            "batteryState": { "batteryCharge": 90.0, "charging": false },
            "errors": [],
            "operatingMode": "AUTOMATIC",
            "safetyState": { "eStop": "NONE", "fieldViolation": false },
        }))
        .unwrap()
    }

    #[test]
    fn repeated_messages_for_one_order_make_one_record() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 2, &[]), at(1));
        history.observe(&state("a", 1, 1, &[]), at(2));

        let records = history.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].order_update_id, 1);
        assert_eq!(records[0].status, OrderStatus::Active);
        assert_eq!(records[0].started_at, at(1));
        assert_eq!(records[0].ended_at, None);
    }

    #[test]
    fn keeps_only_the_newest_orders_newest_first() {
        let mut history = OrderHistory::default();
        for i in 0..MAX_ORDERS_PER_DEVICE + 2 {
            history.observe(&state(&format!("order-{i}"), 0, 0, &[]), at(i as i64));
        }

        let ids: Vec<String> = history.records().into_iter().map(|r| r.order_id).collect();
        assert_eq!(ids, ["order-6", "order-5", "order-4", "order-3", "order-2"]);
    }

    #[test]
    fn ignores_the_empty_order_id() {
        let mut history = OrderHistory::default();
        history.observe(&state("", 0, 0, &[]), at(1));
        assert!(history.records().is_empty());

        history.observe(&state("a", 0, 0, &[]), at(2));
        history.observe(&state("", 0, 0, &[]), at(3));
        assert_eq!(history.records().len(), 1);
    }

    #[test]
    fn end_time_stays_at_the_first_finished_message() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 1, &[]), at(1));
        history.observe(&state("a", 0, 0, &[]), at(2));
        history.observe(&state("a", 0, 0, &[]), at(3));

        let record = &history.records()[0];
        assert_eq!(record.status, OrderStatus::Finished);
        assert_eq!(record.ended_at, Some(at(2)));
    }

    #[test]
    fn extending_a_finished_order_reactivates_it() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 0, &[]), at(1));
        history.observe(&state("a", 1, 1, &[]), at(2));

        let records = history.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, OrderStatus::Active);
        assert_eq!(records[0].ended_at, None);
    }

    #[test]
    fn running_action_keeps_the_order_active() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 0, &["RUNNING"]), at(1));
        assert_eq!(history.records()[0].status, OrderStatus::Active);
    }

    #[test]
    fn failed_action_fails_the_order_once_nothing_is_left() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 1, &["FAILED"]), at(1));
        assert_eq!(history.records()[0].status, OrderStatus::Active);

        history.observe(&state("a", 0, 0, &["FAILED", "FINISHED"]), at(2));
        assert_eq!(history.records()[0].status, OrderStatus::Failed);
    }

    #[test]
    fn replacing_an_active_order_leaves_its_outcome_unknown() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 1, &[]), at(1));
        history.observe(&state("b", 0, 1, &[]), at(2));

        let records = history.records();
        assert_eq!(records[0].status, OrderStatus::Active);
        assert_eq!(records[1].status, OrderStatus::Unknown);
        assert_eq!(records[1].ended_at, Some(at(2)));
    }

    #[test]
    fn replacing_a_finished_order_keeps_its_outcome() {
        let mut history = OrderHistory::default();
        history.observe(&state("a", 0, 0, &[]), at(1));
        history.observe(&state("b", 0, 1, &[]), at(2));

        let record = &history.records()[1];
        assert_eq!(record.status, OrderStatus::Finished);
        assert_eq!(record.ended_at, Some(at(1)));
    }
}
