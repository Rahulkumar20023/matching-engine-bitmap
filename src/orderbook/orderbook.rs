use crate::orderbook::BuyBook::BidBook;
use crate::orderbook::AskBook::AskBook;
use crate::orderbook::arena::Arena;
use crate::orderbook::order::{Order, Side, Price, Qty};
use crate::orderbook::slot::Slot;

pub struct OrderBook {
    pub bids:       BidBook,
    pub asks:       AskBook,
    pub arena:      Arena,
    pub base_price: Price,
    pub tick_size:  Price,
}

impl OrderBook {
    pub fn new(base_price: Price, tick_size: Price, capacity: usize) -> Self {
        Self {
            bids: BidBook::new(),
            asks: AskBook::new(),
            arena: Arena::new(capacity),
            base_price,
            tick_size,
        }
    }

    // ✅ returns None if out of range
    pub fn price_to_tick(&self, price: Price) -> Option<usize> {
        if price < self.base_price { return None; }
        let tick = ((price - self.base_price) / self.tick_size) as usize;
        if tick >= 64 * 64 * 64 { return None; }
        Some(tick)
    }

    pub fn tick_to_price(&self, tick: usize) -> Price {
        self.base_price + (tick as Price * self.tick_size)
    }

    pub fn cancel_order(&mut self, tick_idx: usize, slot_idx: usize, side: &Side) {
        match side {
            Side::BUY  => self.bids.remove_order(tick_idx, slot_idx, &mut self.arena),
            Side::SELL => self.asks.remove_order(tick_idx, slot_idx, &mut self.arena),
        }
    }

    pub fn add_limit_order(&mut self, order: Order) -> Result<Option<usize>, Order> {
        let tick_idx = match self.price_to_tick(order.price) {
            Some(t) => t,
            None    => return Err(order),
        };

        let result = match order.side {
            Side::BUY  => self.bids.add_order(tick_idx, order, &mut self.arena),
            Side::SELL => self.asks.add_order(tick_idx, order, &mut self.arena),
        };

        match result {
            Err(o) => Err(o),
            Ok(idx) => {
                self.match_orders();
                // check if order is still in the book
                match &self.arena.order_store[idx] {
                    Slot::Occupied { .. } => Ok(Some(idx)),  // still resting
                    Slot::Free { .. }     => Ok(None),        // fully matched
                }
            }
        }
    }


    pub fn match_orders(&mut self) {
        loop {
            let best_bid_tick = match self.bids.best_bid() {
                Some(t) => t,
                None    => break,
            };
            let best_ask_tick = match self.asks.best_ask() {
                Some(t) => t,
                None    => break,
            };

            if best_bid_tick < best_ask_tick { break; }

            let bid_idx = match self.bids.get_price_level(best_bid_tick).and_then(|pl| pl.head) {
                Some(i) => i,
                None    => break,
            };
            let ask_idx = match self.asks.get_price_level(best_ask_tick).and_then(|pl| pl.head) {
                Some(i) => i,
                None    => break,
            };

            let bid_qty = match &self.arena.order_store[bid_idx] {
                Slot::Occupied { order, .. } => order.quantity,
                _ => break,
            };
            let ask_qty = match &self.arena.order_store[ask_idx] {
                Slot::Occupied { order, .. } => order.quantity,
                _ => break,
            };

            let filled_qty = bid_qty.min(ask_qty);

            if bid_qty == ask_qty {
                // both fully filled
                self.bids.remove_order(best_bid_tick, bid_idx, &mut self.arena);
                self.asks.remove_order(best_ask_tick, ask_idx, &mut self.arena);

            } else if bid_qty < ask_qty {
                // bid fully filled, ask partially filled
                self.bids.remove_order(best_bid_tick, bid_idx, &mut self.arena);

                if let Slot::Occupied { ref mut order, .. } = self.arena.order_store[ask_idx] {
                    order.quantity   -= filled_qty;
                    order.filled_qty += filled_qty;  // ✅ track filled
                }
                if let Some(pl) = self.asks.price_levels[best_ask_tick].as_mut() {
                    pl.total_qty -= filled_qty;
                }

            } else {
                // ask fully filled, bid partially filled
                self.asks.remove_order(best_ask_tick, ask_idx, &mut self.arena);

                if let Slot::Occupied { ref mut order, .. } = self.arena.order_store[bid_idx] {
                    order.quantity   -= filled_qty;
                    order.filled_qty += filled_qty;  // ✅ track filled
                }
                if let Some(pl) = self.bids.price_levels[best_bid_tick].as_mut() {
                    pl.total_qty -= filled_qty;
                }
            }
        }
    }

}
