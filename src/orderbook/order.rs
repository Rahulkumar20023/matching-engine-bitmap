#[derive(Debug,Copy,Clone)]
pub struct  Order{
    pub client_order_id :ClientOrderId,
    pub client_id: ClientId,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Price,
    pub quantity: Qty,
    pub filled_qty: Qty,
    pub tif: TimeInForce
}

pub type Price= u64;

pub type ClientId=u64;
pub type ClientOrderId=u64;
#[derive(Debug,Copy,Clone)]
pub enum Symbol{
    BTC,
    ETH
}

#[derive(Debug,Copy,Clone)]
pub enum Side{
    BUY,
    SELL
}

#[derive(Debug,Copy,Clone)]
pub enum OrderType{
    LIMIT,
    MARKET,
}

pub type Qty=u64;



#[derive(Debug,Copy,Clone)]
pub enum TimeInForce{
    GTC,
    IOC,
    FOK
}

// New file: src/orderbook/order_id.rs
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct OrderId {
    pub index:      u32,  // slot index in arena
    pub generation: u32,  // generation counter — detects stale handles
}

