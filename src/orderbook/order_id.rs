// New file: src/orderbook/order_id.rs
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct OrderId {
    pub index:      u32,  // slot index in arena
    pub generation: u32,  // generation counter — detects stale handles
}
