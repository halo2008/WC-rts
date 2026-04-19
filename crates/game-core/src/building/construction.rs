use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::hex::Hex;
use super::types::{BuildingType, Building};

/// Status of a construction order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ConstructionStatus {
    /// Waiting in queue — not started yet.
    Queued,
    /// Currently being built. `progress` is 0.0..1.0.
    InProgress,
    /// Construction complete.
    Completed,
    /// Cancelled by player or destroyed.
    Cancelled,
}

/// A single construction order in the queue.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConstructionOrder {
    pub id: u64,
    pub building_type: BuildingType,
    pub hex: Hex,
    pub nation_id: Option<String>,
    pub status: ConstructionStatus,
    /// Progress fraction 0.0..1.0.
    pub progress: f32,
    /// Total build time in seconds.
    pub total_time: f32,
    /// Elapsed build time in seconds.
    pub elapsed: f32,
}

impl ConstructionOrder {
    /// Create a new construction order.
    pub fn new(id: u64, building_type: BuildingType, hex: Hex) -> Self {
        let total_time = building_type.build_time();
        Self {
            id,
            building_type,
            hex,
            nation_id: None,
            status: ConstructionStatus::Queued,
            progress: 0.0,
            total_time,
            elapsed: 0.0,
        }
    }

    /// Advance construction by `dt` seconds. Returns true if just completed.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.status != ConstructionStatus::InProgress {
            return false;
        }
        self.elapsed += dt;
        self.progress = (self.elapsed / self.total_time).min(1.0);
        if self.progress >= 1.0 {
            self.status = ConstructionStatus::Completed;
            return true;
        }
        false
    }

    /// Cancel this order.
    pub fn cancel(&mut self) {
        self.status = ConstructionStatus::Cancelled;
    }

    /// Start construction (move from Queued to InProgress).
    pub fn start(&mut self) {
        if self.status == ConstructionStatus::Queued {
            self.status = ConstructionStatus::InProgress;
        }
    }
}

/// Manages a queue of construction orders for a battlefield instance.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConstructionQueue {
    orders: Vec<ConstructionOrder>,
    /// Maximum number of concurrent builds.
    max_concurrent: usize,
    next_id: u64,
}

impl ConstructionQueue {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            orders: Vec::new(),
            max_concurrent,
            next_id: 1,
        }
    }

    /// Enqueue a new construction order. Returns the order ID.
    pub fn enqueue(&mut self, building_type: BuildingType, hex: Hex) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let mut order = ConstructionOrder::new(id, building_type, hex);
        // Auto-start if we have capacity
        let active = self.active_count();
        if active < self.max_concurrent {
            order.start();
        }
        self.orders.push(order);
        id
    }

    /// Cancel a construction order by ID.
    pub fn cancel(&mut self, order_id: u64) -> bool {
        if let Some(order) = self.orders.iter_mut().find(|o| o.id == order_id) {
            order.cancel();
            true
        } else {
            false
        }
    }

    /// Tick all active construction orders. Returns completed orders as Building instances.
    pub fn tick(&mut self, dt: f32) -> Vec<Building> {
        let mut completed = Vec::new();

        for order in &mut self.orders {
            if order.tick(dt) {
                let mut building = Building::new(order.id, order.building_type, order.hex);
                building.nation_id = order.nation_id.clone();
                completed.push(building);
            }
        }

        // Start queued orders if capacity available
        let active = self.active_count();
        if active < self.max_concurrent {
            let to_start = self.max_concurrent - active;
            let mut started = 0;
            for order in &mut self.orders {
                if started >= to_start {
                    break;
                }
                if order.status == ConstructionStatus::Queued {
                    order.start();
                    started += 1;
                }
            }
        }

        // Remove completed and cancelled orders
        self.orders.retain(|o| {
            o.status != ConstructionStatus::Completed && o.status != ConstructionStatus::Cancelled
        });

        completed
    }

    /// Number of currently in-progress orders.
    fn active_count(&self) -> usize {
        self.orders
            .iter()
            .filter(|o| o.status == ConstructionStatus::InProgress)
            .count()
    }

    /// Get all pending orders (queued + in-progress).
    pub fn pending_orders(&self) -> Vec<&ConstructionOrder> {
        self.orders
            .iter()
            .filter(|o| o.status == ConstructionStatus::Queued || o.status == ConstructionStatus::InProgress)
            .collect()
    }

    /// Get all orders (including completed/cancelled that haven't been cleaned).
    pub fn all_orders(&self) -> &[ConstructionOrder] {
        &self.orders
    }
}
