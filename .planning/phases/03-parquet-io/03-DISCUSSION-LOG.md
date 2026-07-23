# Phase 3: Parquet IO - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-23
**Phase:** 3-parquet-io
**Areas discussed:** API surface — read/write entry points, Predicate pushdown & projection API shape, Compression & row-group defaults, WR-01 nullability bug — fix now or defer

---

## API surface — read/write entry points

| Option | Description | Selected |
|--------|-------------|----------|
| Table.from_parquet / to_parquet | Matches from_pandas/to_pandas convention already locked in Phase 1 (D-01/D-02) | ✓ |
| flint.read_parquet / write_parquet | Matches pyarrow's pq.read_table/write_table naming exactly | |
| Both (module fns delegate to Table methods) | Table methods as implementation, module fns as pyarrow-familiar aliases | |

**User's choice:** Table.from_parquet / Table.to_parquet
**Notes:** Chosen for consistency with the existing Table API naming pattern established in Phase 1.

| Option | Description | Selected |
|--------|-------------|----------|
| Path/str only | Simplest, matches PARQ-01 wording, keeps v1 scope tight | ✓ |
| Path + file-like objects | Also accept io.BytesIO/open file handles | |

**User's choice:** Path/str only

| Option | Description | Selected |
|--------|-------------|----------|
| Single file only | Matches PARQ-01's exact wording | |
| Also support multi-file / directory read | Read a list of paths or a directory, concatenated into one Table | ✓ |

**User's choice:** Also support multi-file / directory read
**Notes:** Exact mechanism (explicit list vs directory auto-discovery) and schema-mismatch policy left to Claude's discretion.

| Option | Description | Selected |
|--------|-------------|----------|
| Overwrite silently | Simplest, matches typical write-file semantics and pyarrow's own default | ✓ |
| Error unless overwrite=True | Safer default, requires explicit opt-in flag | |

**User's choice:** Overwrite silently

---

## Predicate pushdown & projection API shape

| Option | Description | Selected |
|--------|-------------|----------|
| pyarrow-style tuple filters | filters=[("col", ">", 5), ...] — no expression-evaluator engine needed | ✓ |
| Column projection only, no filters in v1 | Row-group stat pruning stays fully internal/automatic | |

**User's choice:** pyarrow-style tuple filters

| Option | Description | Selected |
|--------|-------------|----------|
| AND only (flat list) | All conditions must match, matches pyarrow's original pre-2.0 filters kwarg | ✓ |
| AND-of-OR (nested list-of-lists) | Matches pyarrow's current DNF filters kwarg, more expressive | |

**User's choice:** AND only (flat list)

| Option | Description | Selected |
|--------|-------------|----------|
| Row-level filtering (exact results) | Row-group stat pruning + row-level filtering within surviving row groups, no false positives | ✓ |
| Row-group-level pruning only (coarse) | Only skip row groups, surviving groups returned whole/unfiltered | |

**User's choice:** Row-level filtering (exact results)

| Option | Description | Selected |
|--------|-------------|----------|
| Core set: ==, !=, <, <=, >, >= | Covers the majority of real-world filter needs | ✓ |
| Core set + "in" (membership) | Adds membership check for categorical/dictionary filtering | |

**User's choice:** Core set: ==, !=, <, <=, >, >=

---

## Compression & row-group defaults

| Option | Description | Selected |
|--------|-------------|----------|
| Snappy | Matches pyarrow's own default | ✓ |
| Zstd | Better compression ratio, diverges from pyarrow's default | |

**User's choice:** Snappy

| Option | Description | Selected |
|--------|-------------|----------|
| Exactly the four named in PARQ-02 | snappy, zstd, gzip, uncompressed | ✓ |
| Those four plus lz4 | Adds lz4, not in the locked requirement text | |

**User's choice:** Exactly the four named in PARQ-02

| Option | Description | Selected |
|--------|-------------|----------|
| Row-count threshold (e.g. ~1M rows/group) | Matches pyarrow's own default behavior | ✓ |
| Byte-size threshold (e.g. ~64-128MB/group) | Sizes row groups by memory/disk footprint | |

**User's choice:** Row-count threshold (~1M rows/group)

---

## WR-01 nullability bug — fix now or defer

| Option | Description | Selected |
|--------|-------------|----------|
| Fix as part of Phase 3 | Directly load-bearing for PARQ-06's schema-fidelity requirement | ✓ |
| Fix first as a separate quick task | Fix before Phase 3 planning begins, via /gsd-quick | |
| Defer, note as a known gap | Leave open, note the gap in CONTEXT.md | |

**User's choice:** Fix as part of Phase 3
**Notes:** `build_field` in `crates/flint-python/src/pandas.rs` derives nullability from `null_count() > 0` instead of the source pandas dtype's declared nullability — threatens PARQ-06 (Parquet schema round-trip fidelity) directly, so bundling the fix avoids testing PARQ-06 against a known-broken signal.

---

## Claude's Discretion

- Exact multi-file/directory read mechanism (explicit list vs directory auto-discovery) and cross-file schema-mismatch policy.
- Exact WR-01 fix mechanism (where declared nullability gets threaded through the from_pandas pipeline).
- Exact row-group statistics written beyond what's needed for predicate pushdown to function.
- Whether categorical/dictionary columns get forced dictionary-encoding in the Parquet writer.
- Internal implementation of row-level exact filtering (e.g. ArrowPredicateFn closures per operator).

## Deferred Ideas

- WR-02 (numpy Copy-on-Write zero-copy-borrow guarantee gap) — out of this phase's domain, remains an open blocker.
- `in`/membership filter operator — deferred from v1's fixed operator set.
- OR / disjunctive-normal-form filter combination — deferred; AND-only locked for v1.
- File-like object / in-memory buffer support for Parquet read/write — deferred; path/str-only locked for v1.
