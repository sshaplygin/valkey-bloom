# Cuckoo filters

This implementation stores one-byte fingerprints, not item strings or an occurrence map. `CF.ADD` is idempotent across all subfilters: repeated adds return 1 without adding another fingerprint. `CF.ADDNX` and `CF.INSERTNX` return 0 when membership is already reported. `CF.COUNT` returns a membership estimate of 0 or 1, **not RedisBloom's multiplicity estimate**. One `CF.DEL` removes the matching fingerprint. As with other probabilistic filters, false positives are possible, and deleting an item that was not actually inserted can remove a colliding fingerprint.

## Parameters

`CF.RESERVE key capacity [BUCKETSIZE n] [MAXITERATIONS n] [EXPANSION n]`

- Capacity: 1 through 2^32 items per subfilter. Each subfilter allocates a power-of-two number of buckets, rounding up to cover the requested capacity.
- Bucket size: 1 through 255 one-byte fingerprints per bucket (default 4).
- Maximum iterations: 1 through 65535 eviction attempts (default 512).
- Expansion: 0 disables scaling; 1 through 32768 scales the next subfilter by that factor (default 1). An object has at most 1024 subfilters.
- The configured memory limit applies to actual allocated fingerprint storage and wrapper/vector sizes. Allocated slots are used before scaling. Saturated subfilters are retried after a deletion frees capacity; this avoids repeating futile eviction work on every insert.

`BUCKETSIZE` and `MAXITERATIONS` configure the underlying filter, rather than only its metadata. `CF.INFO` reports the actual number of allocated buckets.

## Replication and persistence

RNG and hasher dependency versions are pinned to keep their behavior consistent across builds. All filters use fixed-key SipHash-1-3 with canonical little-endian length encoding, and ChaCha8 seeded with 42. Creation is replicated as `CF.RESERVE` with every property explicitly specified. Multi-item commands replicate only the processed successful prefix using `CF.INSERT ... NOCREATE ITEMS ...`; replica configuration differences cannot cause extra insertions after a primary-side memory-limit failure.

Failed insertions restore the exact fingerprint data and RNG state. They never drop an existing fingerprint. Copies, RDB snapshots, AOF rewrites, and full replica synchronization preserve raw fingerprint data, filter metadata, and the ChaCha stream position. The digest includes these values so it can detect equal-sized filters with different contents or RNG positions.

The persistence version is **2**. Version 1 from the unmerged PR #87 is rejected: it used an item map and a different hashing scheme. No stable Cuckoo release uses that old format. Bloom persistence is unchanged.

The dependency is `valkey-cuckoo`, imported as `cuckoofilter` and patched to a fixed Git revision. The fork adds transactional insertion, configurable bucket/eviction parameters, and restoration with an explicit RNG. The previously published `valkey-cuckoo 0.1.0` alone does not provide those APIs.

## Comparison with RedisBloom

See [the measured comparison and reproduction environment](../benchmarks/README.md).

Run `python benchmarks/cuckoo_comparison.py --help` for the benchmark driver. Use two dedicated, idle servers. It creates and removes only `cuckoo-bench:*` keys.

The driver varies item count (10k, 100k, 1M), bucket size, maximum iterations, and expansion. Each pair receives identical `item:<integer>` values and parameters. Scaling configurations start at one quarter of the requested item count; expansion 0 starts at the requested count. It records `MEMORY USAGE`, insertion errors, membership hits, pipeline throughput, and sampled single-command ADD/EXISTS latency across the loading range. It alternates implementation order between repeats.

Latency includes the Python client and local network round trip. The Markdown table uses median measurements across repeats; raw JSON also contains p95 latency, throughput, server versions, and exact filter metadata. This compares two implementations with different duplicate/count semantics; it is not a claim of full RedisBloom API equivalence.
