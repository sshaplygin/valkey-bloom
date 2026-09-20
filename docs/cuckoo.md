# Cuckoo filters

`CF.ADD` stores a fingerprint on every successful call, including duplicates.
`CF.ADDNX` and `CF.INSERTNX` skip a fingerprint that is already present. Membership
and multiplicity are approximate: collisions may match a different original item.
`CF.DEL` removes one matching fingerprint, returns 1 on success and 0 otherwise.
Only delete items known to have been inserted, since deleting a false positive can
remove another item's fingerprint.

## CF.INFO

`CF.INFO key [field]` returns all fields as name/value pairs, or a single integer
for the requested field name (case insensitive; quote names containing spaces).

| Field | Meaning |
| --- | --- |
| Size | Accounted object memory in bytes, including allocated pointer-vector capacity |
| Number of buckets | Total buckets across subfilters |
| Number of items inserted | Currently stored fingerprints, including duplicates, minus successful deletions |
| Number of items deleted | Lifetime successful fingerprint deletions; saturates at 9,223,372,036,854,775,807 |
| Number of filters | Number of subfilters |
| Bucket size | Fingerprint slots per bucket |
| Max iterations | Maximum eviction attempts per insertion |
| Expansion rate | Growth factor; 0 disables scaling |

Failed deletions leave the deletion counter unchanged. Successful deletions of a
colliding fingerprint count as deletions. The counter survives COPY, RDB, AOF and
replication, and contributes to the object digest.

## Persistence and upgrades

RDB and CF.LOAD snapshots use format 5. Only format 5 is supported; formats 3 and 4
are rejected by both loaders. To migrate incompatible data, rebuild filters from
the original items; changing the format identifier does not convert the data.
Command AOF replay also requires a compatible placement algorithm: its commands
do not identify that algorithm. Deploy matching module builds on primary and
replica to preserve deterministic placement and RNG continuation.

CF.LOAD uses a byte version followed by five little-endian u64 fields: expansion,
bucket size, max iterations, subfilter count and successful deletions. Each
subfilter then has six u64 fields: requested capacity, fingerprint count, low and
high RNG word position, sealed flag and bucket-byte length, followed by bucket
bytes. The decoder checks the full accounted allocation against the local memory
limit before allocating buckets. RDB and mandatory replication/AOF replay retain
the intentional local-limit bypass. All RESTORE calls, including ordinary client
calls and replicated RESTORE, also bypass this limit: the RDB callback does not
provide a client context that reliably distinguishes these callers. The limit
therefore bounds local creation and CF.LOAD, not restoration of existing objects.

RDB uses the same logical metadata through the server's unsigned-integer API.
Each subfilter's bucket data is stored as string buffers of at most 1 MiB; the
last buffer may be shorter. The loader validates each buffer's length before
growing Rust storage; a missing first chunk allocates no bucket storage. Storage
grows geometrically up to half the declared final size, then reserves the full
size after more than half the bytes have arrived. It stays below twice the bytes
accepted so far. Each buffer is released by its own allocator;
ownership of server allocations is never converted into a Rust Box.

One temporary server buffer is live at a time. During growth, the Rust allocator
may hold both the previous and next bucket allocations. For a 512 MiB subfilter,
the largest such pair is 256 + 512 MiB, plus the 1 MiB chunk. Intermediate
allocations are capped at half the final size, so the same 1.5-times bound applies
to non-power-of-two sizes. This is the deliberate memory tradeoff
for rejecting truncated streams without allocating the full declared size first.
The final conversion to boxed storage does not request another allocation.

These bounds describe module bucket storage for correctly sized chunks, not a
server-wide memory limit. The server allocates LoadStringBuffer's result before
the module can reject an oversized chunk. Checked sizes and try_reserve_exact
handle capacity errors and a fallible allocator's refusal; the production Valkey
allocator can still abort on actual out-of-memory, as for other module allocations.

## Insertion cost and determinism

An unsuccessful eviction is not proof that the filter is full for other items.
Each insertion first attempts direct placement in all unsealed subfilters, newest
first. If none has a free candidate slot, it may attempt up to MAXITERATIONS
evictions in the newest unsealed subfilter, unless all its slots are occupied.
Deleting from an old subfilter makes its free slots reusable without first paying
for eviction in the newest filter. This order does not depend on local memory
limits, so mandatory replication/AOF replay follows the same placement choices.
Failures preserve buckets, counts and RNG; COPY and snapshots preserve future
insertion behavior.

For an item of length L, F subfilters, bucket size B and I = MAXITERATIONS, an
unsuccessful insertion takes O(L + (F + I) * B), with O(I) eviction bookkeeping.
A completely full subfilter is rejected without entering the eviction loop.
Successful scaling additionally allocates and initializes the new bucket storage.

Hashing remains SipHash-1-3 with fixed keys and the existing valkey-cuckoo 0.2.0
implementation. The std slice Hash dependency is deliberately retained. Tests
check lengths 0–1024 against the existing eight-byte little-endian length prefix
and content, and retain the fixed bucket/RNG fixture using the server's slice
input type. A toolchain upgrade that changes this representation must fail tests;
do not regenerate fixtures to bypass that failure.

## Defragmentation metrics

INFO modules reports `bf_cuckoo_defrag_hits` and `bf_cuckoo_defrag_misses` for all
allocations visited by the callback. `bf_cuckoo_defrag_bucket_attempts` counts
bucket-buffer visits specifically, whether or not the allocator moved the buffer.

## Command metadata

COMMAND INFO reports the arity defined in `src/commands/cf.*.json` for all Cuckoo
commands. Positive arity is exact, negative arity is the minimum argument count
including the command name. Variable commands still validate option structure
and upper argument limits in their handlers. Registration preserves existing key
positions, command flags and ACL categories.
