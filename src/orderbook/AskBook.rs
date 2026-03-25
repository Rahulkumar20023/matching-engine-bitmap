use crate::bitmap::bitmap::BitMap;
use crate::orderbook::pricelevel::PriceLevel;
use crate::orderbook::order::Order;
use crate::orderbook::arena::Arena;

pub struct AskBook {
    pub bitmap:       BitMap,
    pub price_levels: Box<[Option<PriceLevel>]>,
}

impl AskBook {
    pub fn new() -> Self {
        let price_levels = (0..64 * 64 * 64)
            .map(|_| None)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { bitmap: BitMap::new(), price_levels }
    }

    pub fn add_order(&mut self, tick_idx: usize, order: Order, arena: &mut Arena) -> Result<usize, Order> {
        if self.price_levels[tick_idx].is_none() {
            self.price_levels[tick_idx] = Some(PriceLevel::new());
        }

        let price_level = self.price_levels[tick_idx].as_mut().unwrap();
        let result = arena.alloc_order(order, price_level);

        if result.is_ok() {
            self.bitmap.set_bit(tick_idx as u64);
        } else if price_level.order_count == 0 {
            // ✅ clean up empty price level if alloc failed
            self.price_levels[tick_idx] = None;
        }

        result
    }

    pub fn remove_order(&mut self, tick_idx: usize, slot_idx: usize, arena: &mut Arena) {
        if let Some(price_level) = self.price_levels[tick_idx].as_mut() {
            arena.free_order(slot_idx, price_level);
            if price_level.order_count == 0 {
                self.price_levels[tick_idx] = None;
                self.bitmap.clear_bit(tick_idx as u64);
            }
        }
    }

    pub fn best_ask(&self) -> Option<usize> { self.bitmap.best_ask() }

    pub fn get_price_level(&self, tick_idx: usize) -> Option<&PriceLevel> {
        self.price_levels[tick_idx].as_ref()
    }
}
