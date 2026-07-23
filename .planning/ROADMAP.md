# Roadmap: Flint (placeholder name)

## Overview

Flint proves its reason to exist in four vertical slices. Phase 1 delivers the whole pipeline for the narrowest possible case — a non-null numeric/bool pandas DataFrame round-tripped zero-copy through Arrow, with the strict-mode/diagnostics API and PyCapsule interop built in from day one (not bolted on later). Phase 2 broadens that same pipeline to every realistic pandas dtype shape (nulls, strings, categoricals, datetime/timezone, timedelta, chunked arrays) so the conversion story is actually complete, not a numeric-only demo. Phase 3 adds Parquet IO as its own end-to-end, user-facing capability against the now-complete Arrow core. Phase 4 closes the loop: benchmark the core value claim against pyarrow across a realistic matrix, and package/ship it so it's actually installable and trustworthy for the open-source audience it targets.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Core Zero-Copy Round-Trip & Interop** - Round-trip a simple numeric/bool DataFrame through Arrow zero-copy, with strict mode, diagnostics, and PyCapsule interop working end-to-end (completed 2026-07-14)
- [x] **Phase 2: Full Dtype & Structural Coverage** - Extend the same conversion pipeline to nulls, strings, categoricals, datetime/timezone, timedelta, and multi-chunk tables (completed 2026-07-23)
- [ ] **Phase 3: Parquet IO** - Read and write Parquet files with compression, row-group configuration, statistics, pushdown, and correct logical-type round-trip
- [ ] **Phase 4: Benchmark & Release Readiness** - Prove the speed/memory claim against pyarrow and ship installable, cross-platform, uv-compatible wheels

## Phase Details

### Phase 1: Core Zero-Copy Round-Trip & Interop

**Goal**: A user can take a simple non-null numeric/bool pandas DataFrame, convert it to an Arrow Table with true zero-copy and back, verify the copy status via a first-class diagnostics/strict-mode API, and hand the Table off to pyarrow/Polars/DuckDB via the Arrow PyCapsule Interface (and accept one back) — all zero-copy.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: CONV-01, CONV-02, DIAG-01, DIAG-02, CAP-01, CAP-02
**Success Criteria** (what must be TRUE):

  1. User can convert a non-null numeric/bool pandas DataFrame to an Arrow Table with zero-copy (no data duplication) and back to pandas
  2. User can request strict zero-copy mode and it succeeds (no error) on a non-null numeric/bool DataFrame, proving the mode is functional rather than a no-op
  3. User can query per-column diagnostics and see a report confirming zero-copy=true for each numeric/bool column
  4. User can export a Table via the Arrow PyCapsule Interface (`__arrow_c_array__`/`__arrow_c_stream__`/`__arrow_c_schema__`) and have it accepted zero-copy by pyarrow, Polars, or DuckDB
  5. User can import a foreign Arrow object (pyarrow Table, Polars DataFrame) via the PyCapsule Interface into a Table with zero-copy

**Plans**: 5/5 plans complete
**Wave 1**

- [x] 01-01-PLAN.md — Walking Skeleton: two-crate workspace + Table shell + thin numeric round-trip + one pyarrow PyCapsule export

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02-PLAN.md — Full per-column decision matrix (plan_column) + strict mode + copy_report diagnostics

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — Dual zero-copy proofs: Python pointer-identity + Rust allocation-counter (D-06)
- [x] 01-04-PLAN.md — PyCapsule interop: from_arrow import + export/import validated against pyarrow, Polars, DuckDB

**Gap closure** *(from 01-VERIFICATION.md CR-01)*

- [x] 01-05-PLAN.md — Fix multi-batch truncation in from_pandas (import_column_via_pandas_stream): concatenate all chunks so multi-chunk ArrowDtype columns round-trip all rows (CONV-01/CONV-02)

### Phase 2: Full Dtype & Structural Coverage

**Goal**: The conversion pipeline from Phase 1 correctly handles every realistic pandas column shape — nulls, object/string, categorical, datetime/timezone, timedelta, and multi-chunk tables — so the conversion story is complete rather than numeric-only.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: CONV-03, CONV-04, CONV-05, CONV-06, CONV-07, CONV-08
**Success Criteria** (what must be TRUE):

  1. User can convert pandas numeric columns containing nulls to/from an Arrow Table with correct null positions preserved
  2. User can convert object/string dtype columns to/from an Arrow Table with correct values and null handling
  3. User can convert categorical dtype columns to/from Arrow dictionary-encoded columns
  4. User can convert datetime, timezone-aware timestamp, and timedelta columns to/from an Arrow Table correctly
  5. User can convert a Table with multiple chunks per column (ChunkedArray) to/from pandas correctly

**Plans**: 5/5 plans executed

**Wave 1**

- [x] 02-01-PLAN.md — Nulls + isinstance-first classify_dtype foundation + masked-extension honest rejection + A1 concat probe (CONV-03)

**Wave 2** *(blocked on Wave 1)*

- [x] 02-02-PLAN.md — Object/string columns + Flint-owned D-11 content validation (CONV-04)

**Wave 3** *(blocked on Wave 1; shares core files, runs after Wave 2)*

- [x] 02-03-PLAN.md — Categorical fidelity: ordered flag + category order + code width (Pitfall 3/4 fixes) (CONV-05)

**Wave 4** *(shares core files, runs after Wave 3)*

- [x] 02-04-PLAN.md — Datetime/tz/timedelta ns-only gating + pandas-3.0 rejection messaging (CONV-06, CONV-07)

**Wave 5** *(blocked on Waves 1-4)*

- [x] 02-05-PLAN.md — Multi-chunk diagnostics-awareness (Strategy B): closes DIAG-01/02 honesty gap (CONV-08)

### Phase 3: Parquet IO

**Goal**: A user can read and write Parquet files directly against Flint's Arrow core, with compression, row-group configuration, statistics-driven pushdown, and correct round-trip of the full dtype range established in Phases 1-2.
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: PARQ-01, PARQ-02, PARQ-03, PARQ-04, PARQ-05, PARQ-06
**Success Criteria** (what must be TRUE):

  1. User can read a Parquet file into a Table
  2. User can write a Table to a Parquet file choosing a compression codec (snappy/zstd/gzip/uncompressed) and configuring row-group size
  3. Written Parquet files carry row-group statistics that enable predicate pushdown, and the user can apply column projection plus predicate pushdown when reading
  4. A Parquet round-trip preserves logical types correctly, including tz-aware timestamps and categorical/dictionary encoding

**Plans**: 1/4 plans executed

**Wave 1**

- [x] 03-01-PLAN.md — End-to-end Parquet round-trip skeleton (from_parquet/to_parquet, snappy default) + Wave-0 arrow-rs fidelity gate (PARQ-01, PARQ-02)

**Wave 2** *(blocked on Wave 1)*

- [ ] 03-02-PLAN.md — Compression codecs (snappy/zstd/gzip/uncompressed, honest rejection) + row-count row-group sizing (PARQ-02, PARQ-03)

**Wave 3** *(blocked on Waves 1-2)*

- [ ] 03-03-PLAN.md — Predicate pushdown (row-group skip + exact row filter) + column projection (PARQ-04, PARQ-05)

**Wave 4** *(blocked on Waves 1-3)*

- [ ] 03-04-PLAN.md — Logical-type fidelity (tz + categorical/dictionary), WR-01/D-31 nullability fix, multi-file/directory read (PARQ-06, PARQ-01)

### Phase 4: Benchmark & Release Readiness

**Goal**: The project's core value claim — measurably faster than pyarrow, zero-copy where physically possible — is proven with a realistic benchmark matrix, and the package is installable and verified across platforms, Python/numpy/pandas versions, and the uv toolchain.
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: BENCH-01, BENCH-02, PKG-01, PKG-02, PKG-03
**Success Criteria** (what must be TRUE):

  1. A benchmark suite compares this library against pyarrow across a realistic matrix (numeric, mixed, nullable, chunked, object-string scenarios)
  2. The benchmark suite reports both throughput and peak memory (RSS) for each scenario, not just speed
  3. The project builds installable wheels for manylinux, macOS, and Windows via maturin, and CI passes across the supported numpy/pandas version matrix
  4. The package installs cleanly via `uv` (`uv add`/`uv pip install`), with a working uv-compatible lockfile and dev commands

**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Core Zero-Copy Round-Trip & Interop | 5/5 | Complete    | 2026-07-14 |
| 2. Full Dtype & Structural Coverage | 5/5 | Complete    | 2026-07-23 |
| 3. Parquet IO | 1/4 | In Progress|  |
| 4. Benchmark & Release Readiness | 0/TBD | Not started | - |
