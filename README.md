# Matching Engine — Bitmap Implementation

A single-symbol limit order book and matching engine built in Rust using a
**bitmap price index + arena allocator** as the core data structure.

This is the **optimized variant** in the `matching-engine-rs` series.  
For the baseline, see → [matching-engine-btreemap](../matching-engine-btreemap)

> ⚠️ **In-Memory Only** — all order state lives in process memory.  
> There is no persistence, no WAL, no crash recovery.  
> A process restart loses all open orders and book state.

> ⚠️ **Cancel Not Yet Implemented** — generation-ID based cancel is
> designed and in progress. The 1M simulation reflects 100% add/match workload.

---

## Architecture

```
OrderBook
├── bitmap/                    ← price presence index (BSR/BSF = O(1) best bid/ask)
│   └── u64 words[N]
├── orderbook/
│   ├── PriceLevel[MAX_PRICE]  ← flat array, one slot per price tick
│   │   └── head / tail / count / total_qty
│   ├── Arena
│   │   ├── Slot[]             ← slab: Occupied { order, generation, prev, next }
│   │   ├── generations[]      ← survives free/alloc cycles for ABA safety
│   │   └── free_head          ← intrusive free list
│   └── OrderBook              ← matching logic
└── main.rs                    ← benchmark suite + 1M simulation
```

### How It Works

Orders are stored in a flat arena slab indexed by slot number. Each price
level is a doubly-linked list threaded through the arena using slot indices —
no heap pointers, no indirection. The bitmap tracks which price ticks have
resting orders; finding the best bid or ask is a single `LZCNT`/`TZCNT` CPU
instruction.

- **Insert**: `O(1)` — bitmap bit-set + arena slot claim
- **Best bid/ask**: `O(1)` — `LZCNT`/`TZCNT` on bitmap word
- **Match**: `O(1)` per order consumed — follow arena linked list
- **Cancel**: ⏳ in progress — generation ID design complete in `arena.rs`

### Why Bitmap + Arena?

BTreeMap's `O(log n)` insert and heap allocation per price level creates
measurable latency under sustained load. A bitmap reduces best-price lookup
to a single CPU instruction. An arena eliminates per-order heap allocation
entirely — all memory is claimed at startup, freed to a slab, never returned
to the OS.

---

## Benchmark Results

> **Environment**: Windows 11, Intel Core i5-12450H, Rust release mode (`--release`)  
> **Measurement**: Per-operation `Instant::now()` with `std::hint::black_box`  
> **Unit**: nanoseconds (ns)

### Latency Percentiles

| Workload       | mean  | p50   | p90   | p99   | p999    | max   |
|----------------|-------|-------|-------|-------|---------|-------|
| passive_add    | 86ns  | 100ns | 100ns | 400ns | 1,600ns | 294µs |
| mixed_workload | 105ns | 100ns | 100ns | 300ns | 1,400ns | 207µs |
| 1M simulation  | 71ns  | 100ns | 100ns | 200ns | 1,000ns | 353µs |

### 1M Order Simulation

```
── ORDER FLOW ──────────────────────────────────────
  Total submitted  :    1,000,000
  Buy orders       :      499,875  (50.0%)
  Sell orders      :      500,125  (50.0%)
  Matched          :      445,223  (44.5%)
  Passive          :      554,777  (55.5%)
  Rejected         :            0  (0.0%)
  ⚠ Cancel ops    :            0  (generation ID cancel in progress)

── THROUGHPUT ──────────────────────────────────────
  Wall time        :      125.9ms
  Throughput       :    7,943,499  ops/sec

── LATENCY PERCENTILES ─────────────────────────────
  mean :    71ns
  p50  :   100ns
  p99  :   200ns
  p999 : 1,000ns
  max  :   353µs  ← OS scheduler spike
```

### Burst Load (5 × 100k orders)

```
  burst  1: p50=100ns  p99=400ns  max=114µs
  burst  2: p50=100ns  p99=300ns  max=401µs
  burst  3: p50=100ns  p99=400ns  max=1,022µs
  burst  4: p50=100ns  p99=400ns  max=591µs
  burst  5: p50=100ns  p99=400ns  max=159µs
```

p99 stays flat at 300–400ns across all 5 bursts — no BTreeMap
rebalancing accumulation under sustained load.

### Latency Distribution (1M orders)

```
  <100ns   38.80%  ███████████████████
  <500ns   60.92%  ██████████████████████████████
   <1µs     0.17%
   <2µs     0.07%
   <5µs     0.02%
  <10µs     0.01%
  <50µs     0.01%
  ≥100µs    0.00%
```

99.72% of all operations complete under 1µs.

### Latency Over Time (per 100k batch)

```
       batch   p50(ns)   p99(ns)   max(ns)
        100k       100       200    353,100
        200k       100       300     69,000
        300k       100       200     57,700
        400k       100       200     83,000
        500k       100       200    204,600
        600k       100       200    306,800
        700k       100       200    198,300
        800k       100       200      5,900
        900k       100       200    186,500
       1000k       100       200    281,100
```

p99 is stable across all 10 batches — no degradation as book depth grows.

---

## Comparison with BTreeMap Baseline

| Metric             | BTreeMap          | This (Bitmap+Arena) | Δ          |
|--------------------|-------------------|---------------------|------------|
| Throughput         | 3,151,023 ops/sec | 7,943,499 ops/sec   | **+2.52×** |
| Wall time (1M ops) | 317.4ms           | 125.9ms             | **−60%**   |
| mean latency       | 273ns             | 71ns                | **−74%**   |
| p50                | 200ns             | 100ns               | **−50%**   |
| p99                | 900ns             | 200ns               | **−78%**   |
| p999               | 3,300ns           | 1,000ns             | **−70%**   |
| max (sched spike)  | 15,392µs          | 353µs               | **−98%**   |
| Best bid/ask       | O(log n)          | O(1) BSR/BSF        | —          |
| Alloc per order    | Heap              | Arena (zero)        | —          |
| Cancel support     | ✅ 10% of ops     | ⏳ in progress      | —          |

> The cancel gap means the BTreeMap simulation processes 10% more operation
> types than the bitmap run. The matching core numbers are directly comparable.

---

## Known Limitations

### 1. Cancel Not Implemented
Generation-ID design is complete (see `orderbook/arena.rs`). The `generations[]`
array survives free/alloc cycles, making stale `OrderId` handles always
detectable. Wire-up to `OrderBook::cancel_order` is in progress.

### 2. Windows Timer Resolution Floor
`Instant::now()` on Windows has ~100ns resolution. Operations faster than
100ns snap to 100ns. True p50 may be lower. Linux TSC gives ~1ns resolution
for accurate sub-100ns measurement.

### 3. In-Process Measurement Only
Numbers reflect pure matching logic. Not included: network I/O, TCP stack,
FIX parsing, risk checks, persistence, or market data dissemination.
A production system adds 10–50µs on top.

### 4. Single-Threaded
No contention. Real concurrent order ingestion would raise p999 depending
on queue design (SPSC, MPSC, or Disruptor pattern).

### 5. Hot Cache
1M orders over MID..MID+40 ticks keeps the working set in L2/L3
throughout the run. A multi-symbol engine with a wider price range
would see higher latency from cache misses.

### 6. Synthetic Order Distribution
Orders are generated with a uniform-spread RNG. Real order flow clusters
more heavily around mid-price with occasional large outlier orders.

---

## Folder Structure

```
matching-engine-bitmap/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs              — benchmark suite + 1M simulation
    ├── bitmap/
    │   └── ...              — bitmap price index (LZCNT/TZCNT best bid/ask)
    └── orderbook/
        └── ...              — Arena, Slot, PriceLevel, OrderBook, Order
```

---

## Running

```bash
# Clone
git clone https://github.com/YOUR_USERNAME/matching-engine-bitmap
cd matching-engine-bitmap

# Run full benchmark suite
cargo test bench --release -- --nocapture

# Run 1M simulation only
cargo test simulate_1m --release -- --nocapture
```

---

## What's Next

- [ ] Cancel order (generation-ID design complete, wire-up in progress)
- [ ] Risk engine (position limits, fat finger checks)
- [ ] Write-ahead log (WAL) for crash recovery
- [ ] WebSocket gateway (tokio + axum)
- [ ] Multi-symbol routing
- [ ] Criterion benchmarks with HTML reports

---

## References

- [Rust Arena Allocator Pattern](https://manishearth.github.io/blog/2021/03/15/arenas-in-rust/)
- [LMAX Disruptor](https://lmax-exchange.github.io/disruptor/)
- [QuantCup Winning Solution](https://gist.github.com/druska/d6ce3f2bac74db08ee9007cdf98106ef)
- [Mechanical Sympathy Blog](https://mechanical-sympathy.blogspot.com/)
- [x86 BSR/BSF Instructions](https://www.felixcloutier.com/x86/bsr)
