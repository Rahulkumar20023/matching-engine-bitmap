

pub struct PriceLevel{
    pub head: Option<usize>,
    pub tail: Option<usize>,
    pub order_count: u32,
    pub total_qty: u64,
}

impl PriceLevel{
    pub fn new()->Self{
        Self{
            head:None,
            tail:None,
            order_count:0,
            total_qty:0
        }
    }
}