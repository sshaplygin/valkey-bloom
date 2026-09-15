# Cuckoo filter implementation status

This branch supersedes PR #87 and retains @pk-vungle's command registrations, callbacks, and integration-test foundation.

- Removed stored item strings, occurrence maps, and cached serialized bucket copies.
- Implemented 0/1 membership estimates for CF.COUNT and idempotent additions across subfilters.
- Added deterministic eviction, transactional failed inserts, and explicit property replication.
- Preserved raw buckets and RNG position through RDB/AOF, copying, and replication.
- Enforced bucket size and eviction limits in the filter implementation.
- Counted allocated fingerprint memory and vector capacity accurately.

See [the Cuckoo documentation](docs/cuckoo.md) for semantics and persistence compatibility, and [the benchmark driver](benchmarks/cuckoo_comparison.py) for reproducible memory and latency comparisons.
