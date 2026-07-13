# Requirements: Flint (placeholder name)

**Defined:** 2026-07-13
**Core Value:** Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Conversion (pandas <-> Arrow)

- [ ] **CONV-01**: User can convert a pandas DataFrame with non-null numeric/bool columns to an Arrow Table with true zero-copy (no data duplication)
- [ ] **CONV-02**: User can convert numeric/bool Arrow Table columns back to a pandas DataFrame with true zero-copy
- [ ] **CONV-03**: User can convert pandas columns with nulls (numeric) to/from an Arrow Table with correct null handling
- [ ] **CONV-04**: User can convert object/string dtype columns to/from an Arrow Table with correct value and null handling
- [ ] **CONV-05**: User can convert categorical dtype columns to/from Arrow dictionary-encoded columns
- [ ] **CONV-06**: User can convert datetime and timezone-aware timestamp columns to/from an Arrow Table
- [ ] **CONV-07**: User can convert timedelta columns to/from an Arrow Table
- [ ] **CONV-08**: User can convert a Table with multiple chunks per column (ChunkedArray) to/from pandas

### Diagnostics

- [ ] **DIAG-01**: User can request a strict zero-copy mode that errors instead of silently falling back to a copy
- [ ] **DIAG-02**: User can query per-column diagnostics explaining whether a copy occurred and why

### PyCapsule Interop

- [ ] **CAP-01**: User can export a Table via the Arrow PyCapsule Interface (`__arrow_c_array__`/`__arrow_c_stream__`/`__arrow_c_schema__`) for zero-copy handoff to pyarrow, Polars, DuckDB, etc.
- [ ] **CAP-02**: User can import a foreign Arrow object (pyarrow Table, Polars DataFrame, etc.) via the PyCapsule Interface into a Table with zero-copy

### Parquet IO

- [ ] **PARQ-01**: User can read a Parquet file into a Table
- [ ] **PARQ-02**: User can write a Table to a Parquet file with a chosen compression codec (snappy/zstd/gzip/uncompressed)
- [ ] **PARQ-03**: User can configure row-group size on write
- [ ] **PARQ-04**: Written Parquet files include row-group statistics enabling predicate pushdown on read
- [ ] **PARQ-05**: User can apply column projection and predicate pushdown when reading Parquet
- [ ] **PARQ-06**: Parquet round-trip preserves logical types correctly (tz-aware timestamps, categorical/dictionary encoding)

### Benchmarking

- [ ] **BENCH-01**: Benchmark suite compares this library vs pyarrow across a realistic matrix (numeric, mixed, nullable, chunked, object-string scenarios)
- [ ] **BENCH-02**: Benchmark suite reports both throughput and peak memory (RSS), not just speed

### Packaging

- [ ] **PKG-01**: Project builds installable wheels for manylinux, macOS, and Windows via maturin
- [ ] **PKG-02**: CI tests against a version matrix of supported numpy/pandas versions (oldest to newest supported)
- [ ] **PKG-03**: Package installs cleanly via `uv` (`uv add`/`uv pip install`), and the dev environment (lockfile, build/test commands) is uv-compatible

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### File IO

- **IO-01**: User can read/write CSV files
- **IO-02**: User can read/write JSON files

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Compute kernels (filter/groupby/join/aggregation) | Query-engine territory (Polars' job), not this library's — would dilute the project into a weaker Polars |
| Distributed / out-of-core execution | Single machine, in-memory only for the foreseeable future |
| Multi-language bindings (R, Node, etc.) | Python only — Rust core could theoretically support this later, but not a v1/v2 concern |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| CONV-01 | TBD | Pending |
| CONV-02 | TBD | Pending |
| CONV-03 | TBD | Pending |
| CONV-04 | TBD | Pending |
| CONV-05 | TBD | Pending |
| CONV-06 | TBD | Pending |
| CONV-07 | TBD | Pending |
| CONV-08 | TBD | Pending |
| DIAG-01 | TBD | Pending |
| DIAG-02 | TBD | Pending |
| CAP-01 | TBD | Pending |
| CAP-02 | TBD | Pending |
| PARQ-01 | TBD | Pending |
| PARQ-02 | TBD | Pending |
| PARQ-03 | TBD | Pending |
| PARQ-04 | TBD | Pending |
| PARQ-05 | TBD | Pending |
| PARQ-06 | TBD | Pending |
| BENCH-01 | TBD | Pending |
| BENCH-02 | TBD | Pending |
| PKG-01 | TBD | Pending |
| PKG-02 | TBD | Pending |
| PKG-03 | TBD | Pending |

**Coverage:**
- v1 requirements: 23 total
- Mapped to phases: 0
- Unmapped: 23 ⚠️ (filled by roadmap creation)

---
*Requirements defined: 2026-07-13*
*Last updated: 2026-07-13 after initial definition*
