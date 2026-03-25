pub mod bitmap;
pub mod orderbook;

use crate::orderbook::orderbook::OrderBook;
use crate::orderbook::order::{Order, Side, OrderType, Symbol, TimeInForce};

fn main() {}

#[cfg(test)]
mod bench {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    const WARM_UP:      usize = 10_000;
    const MID_PRICE:    u64   = 1000;

    // ── Same xorshift64 RNG as BTreeMap repo ────────────────────────
    struct Rng { state: u64 }

    impl Rng {
        fn new(seed: u64) -> Self { Self { state: seed } }
        fn next(&mut self) -> u64 {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            self.state
        }
        fn next_order(&mut self) -> Order {
            let r    = self.next();
            let side = if r & 1 == 0 { Side::BUY } else { Side::SELL };

            let spread =
                (self.next() % 10) +
                (self.next() % 10) +
                (self.next() % 10) +
                (self.next() % 10); // 0..+40

            let price = MID_PRICE + spread;

            let qty = match self.next() % 100 {
                0..=60  => 1  + self.next() % 10,
                61..=85 => 10 + self.next() % 90,
                _       => 100 + self.next() % 900,
            };

            Order {
                client_order_id: self.next() % 1_000_000,
                client_id:       self.next() % 100,
                symbol:          Symbol::BTC,
                side,
                order_type:      OrderType::LIMIT,
                price,
                quantity:        qty,
                filled_qty:      0,
                tif:             TimeInForce::GTC,
            }
        }
    }

    // ── Seed book with resting liquidity ────────────────────────────
    fn seed_book(book: &mut OrderBook, n: usize) {
        for i in 0..(n / 2) {
            let _ = book.add_limit_order(Order {
                client_order_id: 9_000_000 + i as u64,
                client_id:       1,
                symbol:          Symbol::BTC,
                side:            Side::SELL,
                order_type:      OrderType::LIMIT,
                price:           MID_PRICE + 1 + (i as u64 % 50),
                quantity:        10 + (i as u64 % 90),
                filled_qty:      0,
                tif:             TimeInForce::GTC,
            });
            let _ = book.add_limit_order(Order {
                client_order_id: 9_500_000 + i as u64,
                client_id:       1,
                symbol:          Symbol::BTC,
                side:            Side::BUY,
                order_type:      OrderType::LIMIT,
                price:           MID_PRICE,
                quantity:        10 + (i as u64 % 90),
                filled_qty:      0,
                tif:             TimeInForce::GTC,
            });
        }
    }

    // ── Stats printer ────────────────────────────────────────────────
    fn print_stats(name: &str, latencies: &mut Vec<u64>) {
        latencies.sort_unstable();
        let n    = latencies.len();
        let mean = latencies.iter().sum::<u64>() / n as u64;
        let p50  = latencies[n * 50  / 100];
        let p90  = latencies[n * 90  / 100];
        let p99  = latencies[n * 99  / 100];
        let p999 = latencies[n * 999 / 1000];
        let max  = latencies[n - 1];

        println!("\n── {} ──", name);
        println!("  mean : {:>8}ns", mean);
        println!("  p50  : {:>8}ns", p50);
        println!("  p90  : {:>8}ns", p90);
        println!("  p99  : {:>8}ns", p99);
        println!("  p999 : {:>8}ns", p999);
        println!("  max  : {:>8}ns", max);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 1: Passive Add
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn bench_passive_add() {
        let iterations = 1_000_000;
        let mut book   = OrderBook::new(MID_PRICE, 1, iterations + 10_000);
        let mut rng    = Rng::new(0xdeadbeefcafe1234);

        seed_book(&mut book, 2000);

        // warmup
        for _ in 0..WARM_UP {
            let _ = black_box(book.add_limit_order(rng.next_order()));
        }

        let mut book = OrderBook::new(MID_PRICE, 1, iterations + 10_000);
        seed_book(&mut book, 2000);

        let mut latencies = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let o     = rng.next_order();
            let start = Instant::now();
            let _ = black_box(book.add_limit_order(black_box(o)));
            latencies.push(start.elapsed().as_nanos() as u64);
        }

        print_stats("passive_add [Bitmap]", &mut latencies);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 2: Mixed Workload — 60% passive, 30% aggressive
    // NOTE: cancel not included — generation ID cancel in progress
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn bench_mixed_workload() {
        let iterations = 100_000;
        let mut book   = OrderBook::new(MID_PRICE, 1, 500_000);
        let mut rng    = Rng::new(0xdeadbeefcafe1234);
        let mut ctrl   = Rng::new(0xabcdef1234567890);

        seed_book(&mut book, 2000);

        // warmup
        for _ in 0..1000 {
            let _ = black_box(book.add_limit_order(rng.next_order()));
        }

        let mut book = OrderBook::new(MID_PRICE, 1, 500_000);
        seed_book(&mut book, 2000);

        let mut latencies = Vec::with_capacity(iterations);

        for _ in 0..iterations {
            let roll  = ctrl.next() % 100;
            let start = Instant::now();

            if roll < 60 {
                // 60% passive
                let _ = black_box(book.add_limit_order(black_box(rng.next_order())));
            } else {
                // 40% aggressive — crosses spread to trigger match
                let aggressive = Order {
                    client_order_id: ctrl.next() % 1_000_000,
                    client_id:       2,
                    symbol:          Symbol::BTC,
                    side:            if roll % 2 == 0 { Side::BUY } else { Side::SELL },
                    order_type:      OrderType::LIMIT,
                    price:           if roll % 2 == 0 { MID_PRICE + 5 } else { MID_PRICE + 1 },
                    quantity:        10,
                    filled_qty:      0,
                    tif:             TimeInForce::GTC,
                };
                let _ = black_box(book.add_limit_order(black_box(aggressive)));
            }

            latencies.push(start.elapsed().as_nanos() as u64);
        }

        print_stats("mixed_workload [Bitmap]", &mut latencies);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 3: Burst Load — 5 bursts of 100k each
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn bench_burst_load() {
        let bursts           = 5;
        let orders_per_burst = 100_000;
        let mut book         = OrderBook::new(MID_PRICE, 1, bursts * orders_per_burst + 10_000);
        let mut rng          = Rng::new(0xdeadbeefcafe1234);

        seed_book(&mut book, 2000);

        let mut all_latencies = Vec::with_capacity(bursts * orders_per_burst);

        println!("\n── burst_load [Bitmap] ──");

        for burst in 1..=bursts {
            let mut burst_latencies = Vec::with_capacity(orders_per_burst);

            for _ in 0..orders_per_burst {
                let o     = rng.next_order();
                let start = Instant::now();
                let _ = black_box(book.add_limit_order(black_box(o)));
                let elapsed = start.elapsed().as_nanos() as u64;
                burst_latencies.push(elapsed);
                all_latencies.push(elapsed);
            }

            burst_latencies.sort_unstable();
            let n    = burst_latencies.len();
            let bp50 = burst_latencies[n * 50 / 100];
            let bp99 = burst_latencies[n * 99 / 100];
            let bmax = burst_latencies[n - 1];
            println!("  burst {:>2}: p50={:>6}ns  p99={:>6}ns  max={:>8}ns",
                burst, bp50, bp99, bmax);
        }

        print_stats("burst_load_overall [Bitmap]", &mut all_latencies);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 4: Sweep Worst Case — large order sweeps 10 price levels
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn bench_sweep_worst_case() {
        let iterations    = 10_000;
        let mut latencies = Vec::with_capacity(iterations);

        for _ in 0..iterations {
            let mut book = OrderBook::new(MID_PRICE, 1, 10_000);

            for level in 0..10u64 {
                for j in 0..5u64 {
                    let _ = book.add_limit_order(Order {
                        client_order_id: level * 5 + j,
                        client_id:       1,
                        symbol:          Symbol::BTC,
                        side:            Side::SELL,
                        order_type:      OrderType::LIMIT,
                        price:           MID_PRICE + 1 + level,
                        quantity:        10,
                        filled_qty:      0,
                        tif:             TimeInForce::GTC,
                    });
                }
            }

            let sweeper = Order {
                client_order_id: 999_999,
                client_id:       2,
                symbol:          Symbol::BTC,
                side:            Side::BUY,
                order_type:      OrderType::LIMIT,
                price:           MID_PRICE + 10,
                quantity:        500,
                filled_qty:      0,
                tif:             TimeInForce::GTC,
            };

            let start = Instant::now();
            let _ = black_box(book.add_limit_order(black_box(sweeper)));
            latencies.push(start.elapsed().as_nanos() as u64);
        }

        print_stats("sweep_worst_case [Bitmap]", &mut latencies);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 5: 1M Order Simulation
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn simulate_1m_orders() {
        const TOTAL_ORDERS: usize = 1_000_000;

        let mut book = OrderBook::new(MID_PRICE, 1, TOTAL_ORDERS + 10_000);
        let mut rng  = Rng::new(0xdeadbeefcafe1234);
        let mut ctrl = Rng::new(0xabcdef1234567890);

        seed_book(&mut book, 4000);

        let mut total_matched    = 0usize;
        let mut total_passive    = 0usize;
        let mut total_rejected   = 0usize;
        let mut buy_orders       = 0usize;
        let mut sell_orders      = 0usize;
        let mut total_qty_traded = 0u64;
        let mut latencies        = Vec::with_capacity(TOTAL_ORDERS);
        let mut bucket_latencies: Vec<Vec<u64>> = vec![Vec::new(); 10];

        let wall_start = Instant::now();

        for i in 0..TOTAL_ORDERS {
            // 70% normal orders, 30% aggressive
            let roll  = ctrl.next() % 100;
            let order = if roll < 70 {
                rng.next_order()
            } else {
                Order {
                    client_order_id: ctrl.next() % 1_000_000,
                    client_id:       2,
                    symbol:          Symbol::BTC,
                    side:            if roll % 2 == 0 { Side::BUY } else { Side::SELL },
                    order_type:      OrderType::LIMIT,
                    price:           if roll % 2 == 0 { MID_PRICE + 5 } else { MID_PRICE + 1 },
                    quantity:        10 + ctrl.next() % 50,
                    filled_qty:      0,
                    tif:             TimeInForce::GTC,
                }
            };

            match order.side {
                Side::BUY  => buy_orders  += 1,
                Side::SELL => sell_orders += 1,
            }

            let t_start = Instant::now();
            let result  = black_box(book.add_limit_order(black_box(order)));
            let elapsed = t_start.elapsed().as_nanos() as u64;

            latencies.push(elapsed);
            bucket_latencies[i / 100_000].push(elapsed);

            match result {
                Ok(Some(_)) => total_passive  += 1,  // resting in book
                Ok(None)    => total_matched  += 1,  // fully matched
                Err(_)      => total_rejected += 1,
            }
        }

        let wall_elapsed = wall_start.elapsed();

        latencies.sort_unstable();
        let n    = latencies.len();
        let mean = latencies.iter().sum::<u64>() / n as u64;
        let p50  = latencies[n * 50  / 100];
        let p90  = latencies[n * 90  / 100];
        let p99  = latencies[n * 99  / 100];
        let p999 = latencies[n * 999 / 1000];
        let max  = latencies[n - 1];

        println!("\n╔══════════════════════════════════════════════════╗");
        println!("║      1M ORDER SIMULATION — Bitmap Engine        ║");
        println!("╚══════════════════════════════════════════════════╝");

        println!("\n── ORDER FLOW ──────────────────────────────────────");
        println!("  Total submitted  : {:>10}", TOTAL_ORDERS);
        println!("  Buy orders       : {:>10} ({:.1}%)", buy_orders,
            buy_orders  as f64 / TOTAL_ORDERS as f64 * 100.0);
        println!("  Sell orders      : {:>10} ({:.1}%)", sell_orders,
            sell_orders as f64 / TOTAL_ORDERS as f64 * 100.0);
        println!("  Matched          : {:>10} ({:.1}%)", total_matched,
            total_matched  as f64 / TOTAL_ORDERS as f64 * 100.0);
        println!("  Passive          : {:>10} ({:.1}%)", total_passive,
            total_passive  as f64 / TOTAL_ORDERS as f64 * 100.0);
        println!("  Rejected         : {:>10} ({:.1}%)", total_rejected,
            total_rejected as f64 / TOTAL_ORDERS as f64 * 100.0);
        println!("  ⚠ Cancel ops     :          0 (generation ID cancel in progress)");

        println!("\n── THROUGHPUT ──────────────────────────────────────");
        println!("  Wall time        : {:>10.3}ms",
            wall_elapsed.as_secs_f64() * 1000.0);
        println!("  Throughput       : {:>10.0} ops/sec",
            TOTAL_ORDERS as f64 / wall_elapsed.as_secs_f64());

        println!("\n── LATENCY PERCENTILES ─────────────────────────────");
        println!("  mean : {:>8}ns", mean);
        println!("  p50  : {:>8}ns", p50);
        println!("  p90  : {:>8}ns", p90);
        println!("  p99  : {:>8}ns", p99);
        println!("  p999 : {:>8}ns", p999);
        println!("  max  : {:>8}ns", max);

        println!("\n── LATENCY OVER TIME (per 100k batch) ──────────────");
        println!("  {:>10}  {:>8}  {:>8}  {:>8}", "batch", "p50(ns)", "p99(ns)", "max(ns)");
        for (i, bucket) in bucket_latencies.iter_mut().enumerate() {
            bucket.sort_unstable();
            let bn   = bucket.len();
            let bp50 = bucket[bn * 50 / 100];
            let bp99 = bucket[bn * 99 / 100];
            let bmax = bucket[bn - 1];
            println!("  {:>7}00k  {:>8}  {:>8}  {:>8}", i + 1, bp50, bp99, bmax);
        }

        println!("\n── LATENCY DISTRIBUTION ────────────────────────────");
        let bounds = [0u64, 100, 500, 1_000, 2_000, 5_000,
                      10_000, 50_000, 100_000, u64::MAX];
        let labels = ["  <100ns", "  <500ns", "   <1µs", "   <2µs",
                      "   <5µs",  "  <10µs",  "  <50µs", " <100µs", "  ≥100µs"];
        let mut prev = 0u64;
        for (label, &upper) in labels.iter().zip(bounds[1..].iter()) {
            let count = latencies.iter().filter(|&&x| x >= prev && x < upper).count();
            let pct   = count as f64 / n as f64 * 100.0;
            let bar   = "█".repeat((pct / 2.0) as usize);
            println!("  {}  {:>6.2}%  {}", label, pct, bar);
            prev = upper;
        }
        println!();
    }
}
