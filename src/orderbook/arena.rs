use crate::orderbook::slot::Slot;
use crate::orderbook::pricelevel::PriceLevel;
use crate::orderbook::order::Order;

pub struct Arena {
    pub order_store:     Box<[Slot]>,
    pub free_order_head: Option<usize>,
    pub order_count:     usize,
}

impl Arena {
    pub fn new(capacity: usize) -> Self {
        let mut vec = Vec::new();
        for i in 0..capacity - 1 {
            vec.push(Slot::Free { next_free: Some(i + 1) });
        }
        vec.push(Slot::Free { next_free: None });

        Self {
            order_store:     vec.into_boxed_slice(),
            free_order_head: Some(0),
            order_count:     0,
        }
    }

    pub fn is_full(&self) -> bool {
        self.free_order_head.is_none()
    }

    pub fn is_empty(&self) -> bool {
        self.order_count == 0
    }

    pub fn alloc_order(&mut self, order: Order, price_level: &mut PriceLevel) -> Result<usize, Order> {
        match self.free_order_head {
            None => Err(order),
            Some(idx) => {
                let next_free = match self.order_store[idx] {
                    Slot::Free { next_free } => next_free,
                    _ => None,
                };

                self.order_store[idx] = Slot::Occupied {
                    order,
                    prev: price_level.tail,
                    next: None,
                };

                if let Some(tail_idx) = price_level.tail {
                    if let Slot::Occupied { ref mut next, .. } = self.order_store[tail_idx] {
                        *next = Some(idx);
                    }
                }

                if price_level.head.is_none() {
                    price_level.head = Some(idx);
                }

               
                if let Slot::Occupied { ref order, .. } = self.order_store[idx] {
                    price_level.total_qty += order.quantity;
                }

                price_level.tail        = Some(idx);
                price_level.order_count += 1;
                self.free_order_head    = next_free;
                self.order_count        += 1;

                Ok(idx)
            }
        }
    }

    pub fn free_order(&mut self, idx: usize, price_level: &mut PriceLevel) {
        let (prev1, next1) = match self.order_store[idx] {
            Slot::Occupied { prev, next, .. } => (prev, next),
            Slot::Free { .. } => return,
        };

        
        if let Slot::Occupied { ref order, .. } = self.order_store[idx] {
            price_level.total_qty = price_level.total_qty.saturating_sub(order.quantity);
        }

        if let Some(prev_idx) = prev1 {
            if let Slot::Occupied { ref mut next, .. } = self.order_store[prev_idx] {
                *next = next1;
            }
        }
        if let Some(next_idx) = next1 {
            if let Slot::Occupied { ref mut prev, .. } = self.order_store[next_idx] {
                *prev = prev1;
            }
        }

        if price_level.head == Some(idx) { price_level.head = next1; }
        if price_level.tail == Some(idx) { price_level.tail = prev1; }
        price_level.order_count -= 1;

        self.order_store[idx]    = Slot::Free { next_free: self.free_order_head };
        self.free_order_head     = Some(idx);
        self.order_count         -= 1;
    }
}
