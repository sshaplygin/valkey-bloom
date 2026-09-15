# Cuckoo comparison

[Measured results](cuckoo-results.md) compare this implementation with RedisBloom
(ReBloom). [Raw measurements](cuckoo-results.json) include every repetition,
insertion errors, lookup hits, filter metadata, throughput, and p95 latency.

## Environment

- Linux ARM64 containers in Docker Desktop on macOS; VM: 4 CPUs, 8,323,530,752 bytes RAM.
- Valkey 8.1.10, source `55a542671762eaddff979f78dc6a3d1414ce1d75`, jemalloc 5.3.0.
- Valkey-Bloom source `2ff82f090b8432e3fd3c4ae6fde25d1bf2e8028d`, release build with Rust 1.98.1; pinned
  `valkey-cuckoo` source `dbaa5d02894451a1185efc141a225ace775d8d2f`.
- Redis 7.4.7 with RedisBloom 2.8.16, native ARM64 image
  `redis/redis-stack-server:7.4.0-v8`, repository digest
  `sha256:798ab84d9f266936b034ab11c4d04a2b8e4b441884c5aa7d17ac951eefdf742a`.
  Other Redis Stack modules were loaded but unused.
- Python 3.12.3 and `valkey` client 6.1.1. Client and both servers share a
  network namespace and use loopback. Persistence was disabled on both servers.

## Reproduce

Build the module with `cargo build --release`. Start two dedicated servers,
loading this module into Valkey on port 6380 and RedisBloom into Redis on port
6390. Disable RDB saves and AOF (`--save "" --appendonly no`). Then run:

```sh
python3 -m pip install valkey==6.1.1
python3 benchmarks/cuckoo_comparison.py \
  --valkey-url redis://127.0.0.1:6380 \
  --rebloom-url redis://127.0.0.1:6390 \
  --sizes 10000 100000 1000000 --repeats 3 --batch-size 512 \
  --output benchmarks/cuckoo-results.json
```

The script creates and deletes `cuckoo-bench:*` keys. Each trial starts with a
fresh filter and inserts the same distinct `item:<integer>` strings. Scaling
configurations reserve one quarter of the item count; expansion 0 reserves the
full count. Implementation order alternates between repetitions. Tests and
builds were stopped before measurement.

## Interpreting the table

Memory is the server-reported `MEMORY USAGE` after insertion. Table entries are
medians across three repetitions. The ADD and EXISTS columns are medians of
each repetition's sampled single-command round-trip p50, in microseconds.
One command per batch is timed individually: 20, 196, or 1,954 samples per
operation per repetition, respectively. Remaining commands are pipelined;
throughput in the JSON includes client and pipeline overhead.

These are local client/server measurements, not isolated server execution
times. Small latency differences should not be treated as portable performance
rankings. Allocator accounting and bucket rounding also differ between servers.

The implementations have different duplicate semantics: this branch suppresses
matching fingerprints and exposes a 0/1 `CF.COUNT`; RedisBloom tracks approximate
multiplicity. Distinct input strings can still collide. This is an equal-input
workload comparison, not a claim of equal false-positive rates or full API
compatibility. The raw `CF.INFO` values show how many fingerprints and subfilters
were allocated by each implementation.

## Results and insertion outcomes

All 72 trials completed. Valkey reported zero insertion errors and membership
hits for every input. ReBloom reported four insertion errors and 999,996 hits
in each 1M-item, bucket-4, 500-kick, expansion-0 trial; all its other trials
reported zero errors and hits for every input.

| Workload (each repetition) | Valkey ADD errors | ReBloom ADD errors | Valkey lookup hits | ReBloom lookup hits |
| --- | ---: | ---: | ---: | ---: |
| 1M, bucket 4, kicks 500, expansion 0 | 0 | 4 | 1,000,000 | 999,996 |
| All other configurations | 0 | 0 | All inputs | All inputs |

At 1M inputs, Valkey's reported memory is within 0.15% above ReBloom in every
configuration. The largest difference is the 100k-input, bucket-2, 20-kick,
expansion-1 case: 165,872 versus 131,240 bytes (26.4% higher), with five
subfilters versus four. Thus memory is close at larger sizes, but differences
in fill behavior can trigger another allocation. Timings are shown without
claiming a consistent latency advantage for either implementation.
