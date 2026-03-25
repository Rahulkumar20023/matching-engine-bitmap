
use crate::orderbook::order::Order;
pub enum Slot{
    Occupied{
        order: Order,
        prev: Option<usize>,
        next: Option<usize>,
    },
    Free{
        next_free:Option<usize>
    }
}