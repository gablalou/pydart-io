# Project Research Summary

**Project:** Flint (placeholder name) — Rust-backed zero-copy pandas <-> Arrow interop + Parquet IO
**Domain:** Rust-backed Python native-extension library (data interop / IO bridge, not a query engine)
**Researched:** 2026-07-13
**Confidence:** MEDIUM

## Executive Summary

Flint is a Rust-backed Python extension whose entire value proposition rests on being a leaner, measurably faster, zero-copy(-when-possible) alternative to pyarrow for two narrow jobs: converting between pandas DataFrames and Arrow Tables, and reading/writing Parquet. Experts build this class of library on top of `arrow-rs` (the official Rust Arrow implementation) plus `pyo3`/`pyo3-arrow` for the Python binding layer, using the Arrow C Data Interface / PyCapsule protocol (`__arrow_c_array__`, `__arrow_c_stream__`, `__arrow_c_schema__`) as the primary, dependency-free interop mechanism with the wider Arrow ecosystem (pyarrow, Polars, DuckDB). The closest real-world prior art is `arro3` (Kyle Barron), which should be studied as a reference architecture (two-crate shape: pure-Rust core + thin PyO3 binding crate) rather than depended on directly.

The recommended approach is to treat this project as having two structurally different boundaries, not one undifferentiated binding layer: (1) Arrow-to-Arrow via PyCapsule, which is cleanly, mechanically zero-copy, and (2) pandas-to-Arrow, which is copy-sometimes and depends entirely on column dtype, null presence, contiguity, and chunking. True zero-copy in both directions only holds for `pandas.ArrowDtype`-backed columns and non-null contiguous numeric columns; everything else (object/string dtype, columns with nulls in legacy numpy backing, booleans, multi-chunk tables) requires an explicit, honestly-labeled copy. The single most important cross-cutting decision from all four research files is to make this distinction a first-class, tested, benchmarked, and user-visible property of the API, not an implementation detail glossed over in documentation, because it is both the project's core value claim and its biggest credibility risk.

The key risks are concentrated in Rust/Python FFI memory-safety (buffer lifetime mismatches between Rust ownership and Python's refcounting GC, GIL deadlocks under concurrency), API-design honesty (pyarrow's own `zero_copy_only` flag is a documented cautionary tale of a strict mode that's nearly useless because the underlying implementation wasn't built to support it), and benchmark integrity (a "faster than pyarrow" claim that only tests the easy numeric/non-null case will be credibly challenged by the exact data-engineer audience this library targets). Mitigation is well-understood and documented in PITFALLS.md: reference-counted buffers with capsule-based ownership bridging, GIL-release discipline validated by concurrency tests, and a benchmark suite covering a realistic dtype/nullability/chunking matrix with both speed and memory reported.

## Key Findings

### Recommended Stack

The stack is anchored on `arrow-rs` (`arrow` + `parquet` crates, v59.1.0, released in lockstep) as the sole Arrow implementation, `pyo3` (0.29.0) as the FFI layer, `pyo3-arrow` (0.19.0) as the purpose-built Arrow<->PyO3 conversion layer that implements PyCapsule export/import for free, and `maturin` (1.14.1) as the build/packaging backend. This combination is what modern Rust-in-Python Arrow libraries (Polars for interop, `arro3`) converge on, and avoids known dead ends (arrow2/polars-arrow are unmaintained or internal-only; raw `_export_to_c`/`_import_from_c` pointer-passing is a legacy, unsafe pattern superseded by PyCapsule since ~2024).

**Core technologies:**
- Rust (2021 edition, MSRV ~1.75+) — core implementation language, non-negotiable per PROJECT.md
- PyO3 0.29.0 — Rust<->Python FFI bindings with safe ownership/refcounting model (`Py<T>`, `Bound<'py, T>`)
- pyo3-arrow 0.19.0 — Arrow<->PyO3 conversion layer implementing the PyCapsule Interface; single most important stack decision, eliminates a whole class of hand-rolled-FFI memory-safety bugs
- arrow-rs (`arrow` + `parquet` crates) 59.1.0 — official Rust Arrow columnar format + native Parquet reader/writer, versioned in lockstep to avoid schema-mismatch risk
- maturin 1.14.1 — PEP 517 build backend, standard for PyO3 projects, handles abi3/manylinux wheel packaging

Critical caveat carried through the whole stack: "zero-copy" is only fully true for `pandas.ArrowDtype`-backed columns; default numpy-backed DataFrames require a real copy for `object`, categorical, and nullable-int/string columns regardless of tooling choice — this must shape both the benchmark design and the public API.

### Expected Features

**Must have (table stakes):**
- DataFrame -> Table and Table -> DataFrame conversion with full realistic dtype coverage (int/uint/float variants, bool, object/string, categorical, datetime incl. tz, timedelta) and correct null handling
- ChunkedArray/multi-chunk Table support and schema/metadata round-trip fidelity (pandas_metadata equivalent)
- Explicit zero-copy-or-error mode + per-column "did this copy?" diagnostics
- Arrow PyCapsule Interface support (export and accept), so this is a drop-in intermediary with pyarrow/Polars/DuckDB without a hard pyarrow dependency
- Parquet read/write: snappy/zstd/gzip/uncompressed codecs, configurable row-group size, row-group statistics, predicate pushdown (row-group pruning), column projection

**Should have (competitive):**
- Small install/import footprint vs pyarrow's large wheel — a concrete, benchmarkable differentiator
- First-class zero-copy-by-default path for `ArrowDtype` columns
- Per-column copy diagnostics as a queryable API (not just a flag)
- Accepting foreign PyCapsule objects (not just producing them) — genuine pyarrow-free interop

**Defer (v2+):**
- Compute kernels (filter/groupby/join/sort) — explicitly excluded, would turn this into a weaker Polars
- Distributed/out-of-core execution, multi-language bindings, CSV/JSON IO, automatic multi-file schema merging — all explicitly out of scope per PROJECT.md's scope discipline

### Architecture Approach

The system splits cleanly into a pure-Rust core (`flint-core`: Arrow arrays/tables/schema, Parquet IO via arrow-rs, zero PyO3 dependency, unit-testable without Python) and a thin PyO3 binding crate (`flint-python`: PyArray/PyTable/PySchema wrappers implementing the PyCapsule dunder methods, a dedicated `pandas.rs` module owning the copy-vs-zero-copy decision logic, and `#[pyfunction]` Parquet glue). This mirrors `arro3`'s two-crate shape rather than Polars' 20+-crate workspace, which is scale-appropriate for a query engine, not a v1 interop library.

**Major components:**
1. Rust core (Arrow) — owns Arrow memory format, arrays, buffers, validity bitmaps, RecordBatch/Table, via `arrow-rs`
2. Rust core (Parquet) — read/write against the same Arrow representation via arrow-rs's `parquet` crate, kept independent of pandas so it can be built/tested in parallel
3. PyO3 Arrow wrappers — implement PyCapsule protocol dunder methods for zero-copy exchange with pyarrow/Polars/DuckDB
4. PyO3 pandas-interop module — the hardest, most bespoke component: walks pandas' BlockManager column-by-column and decides buffer-protocol view vs. validity-bitmap construction vs. full materialize-and-copy
5. Python-facing public API — thin ergonomic surface (`Table`, `from_pandas`, `to_pandas`, `read_parquet`, `write_parquet`) matching pyarrow's shape closely enough to minimize adoption friction

Suggested build order (dependency-forced, not arbitrary): (1) toolchain + single primitive-array round-trip via PyCapsule with a proof-of-zero-copy test, (2) type coverage expansion (nulls, strings, booleans), (3) Schema/Field/DataType wrappers, (4) RecordBatch -> chunked Table -> stream support, (5) two parallel tracks — pandas-interop and Parquet IO — once the Arrow core is stable, (6) benchmark suite vs pyarrow, which requires both tracks to exist to be meaningful.

### Critical Pitfalls

1. **Buffer lifetime mismatch between Rust ownership and Python's GC** — use-after-free/segfaults when a Rust-owned buffer is exposed to Python without a capsule/`base` reference keeping it alive. Avoid via `Arc`-backed buffers and capsule-based ownership bridging; never raw pointers with manual lifetime bookkeeping.
2. **GIL deadlocks under concurrency** — long Rust work holding the GIL kills the "faster" pitch; releasing the GIL and calling back into Python incorrectly can deadlock. Avoid via disciplined `allow_threads`/`detach` usage, `PyOnceLock` for lazy statics, and mandatory multi-threaded concurrency tests.
3. **Silent copies on object/string columns marketed as "zero-copy"** — the most common real-world pandas column type (object dtype strings) can never be truly zero-copy; failing to surface this makes benchmark/correctness claims quietly false. Avoid via explicit dtype-eligibility detection as a first-class, tested code path and honest benchmark labeling.
4. **API design that makes strict zero-copy mode useless or silently falls back with no signal** — pyarrow's own `zero_copy_only=True` is a documented cautionary tale (rarely succeeds even when it should). Avoid by designing the copy/no-copy signal as a first-class return value from day one, not a bolted-on flag.
5. **Benchmark claims that don't survive scrutiny** — testing only the easy numeric/non-null case and reporting only wall-clock time (not memory) will be challenged by the target audience. Avoid via a realistic dtype/nullability/chunking benchmark matrix reporting both speed and peak RSS.
6. **Packaging/ABI failures visible only to a subset of users** — NumPy ABI forward-compat-only, manylinux/glibc tag mismatches. Avoid by building against the oldest supported NumPy ABI and testing a version matrix in CI, not just "latest."

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Toolchain, Arrow Core, and Single-Array Zero-Copy Round-Trip
**Rationale:** Everything else depends on a working Rust<->Python FFI boundary with proven zero-copy semantics; this is the serial bottleneck all subsequent work depends on.
**Delivers:** maturin/PyO3 build pipeline producing an installable wheel; one primitive type (e.g. `Int64Array`) round-tripping Rust -> Python -> Rust via `__arrow_c_array__`/PyCapsule; a test proving zero-copy (pointer identity or allocation counting) that becomes the template for all future benchmark/verification work.
**Addresses:** Arrow PyCapsule Interface support (table stakes), foundational for all P1 features.
**Avoids:** Pitfall 1 (buffer lifetime/use-after-free) and Pitfall 4 (alignment/endianness) — both must be solved at this foundational layer, not retrofitted.

### Phase 2: Type Coverage Expansion (Nulls, Strings, Booleans) + Schema/Table Wrappers
**Rationale:** Each additional type category (validity bitmaps, variable-length strings, bit-packed booleans) has a genuinely different FFI/buffer shape than the plain-primitive case from Phase 1; must be built before multi-column Table representation makes sense.
**Delivers:** Full Arrow type coverage in the Rust core + PyO3 wrappers; Schema/Field/DataType wrappers; RecordBatch -> chunked Table -> `__arrow_c_stream__` support (what Polars/DuckDB actually expect to receive).
**Uses:** arrow-rs array/schema types, pyo3-arrow patterns for wrapper structs (`Arc<dyn Array>` + `FieldRef`).
**Implements:** PyO3 Arrow wrappers component; PyCapsule Interface and newtype-wrapper patterns.

### Phase 3: Pandas Interop — Core Conversion + Zero-Copy Diagnostics API
**Rationale:** This is the hardest, most bespoke, and highest-risk component — the copy-vs-zero-copy decision tree per dtype is new code, not something inherited for free from pyo3-arrow/arrow-rs. It should be its own phase with the API surface designed correctly from the start (the strict-mode design pitfall is expensive to fix post-launch).
**Delivers:** `from_pandas`/`to_pandas`-equivalent conversion covering the full realistic dtype matrix (numeric, object/string, categorical, datetime+tz, bool) with correct null handling; explicit zero-copy-or-error mode plus per-column "why did this copy?" diagnostics API; error messages naming the specific failing column/dtype.
**Addresses:** DataFrame<->Table conversion, zero-copy guarantee mode, per-column diagnostics.
**Avoids:** Silent copies on object/string columns, useless strict-mode flag, and requires GIL-release discipline for concurrency safety.

### Phase 4: Parquet IO
**Rationale:** Independent of pandas-interop (both depend only on the Arrow core from Phases 1-2), so it can be built in parallel with Phase 3 if resourcing allows, but is sequenced here for a single-threaded roadmap. Explicitly required by PROJECT.md.
**Delivers:** `read_parquet`/`write_parquet` against arrow-rs's `parquet` crate; compression codecs (snappy, zstd, gzip, uncompressed); configurable row-group size; row-group statistics on write; predicate pushdown (row-group pruning) and column projection on read; correct Arrow<->Parquet logical type round-trip including timezone-aware timestamps and dictionary/categorical encoding.
**Addresses:** Parquet-specific table stakes, all P1 priority.
**Avoids:** Silent type coercion on round-trip (a "leaner but less correct than pyarrow" failure mode).

### Phase 5: Benchmark Suite vs pyarrow
**Rationale:** Requires Phases 3 and 4 to exist to be meaningful; this is where the project's stated reason to exist (the Core Value claim) gets empirically tested, and this phase's own success criteria must include realistic-shape coverage, not just a headline number.
**Delivers:** Benchmark matrix across numeric/mixed/object-dtype/nullable/chunked data shapes, reporting both throughput and peak memory (RSS), using `criterion` (Rust) and `pytest-benchmark`/`codspeed` (Python), with published methodology.
**Addresses:** "measurably faster than pyarrow" P1 requirement.
**Avoids:** Benchmark claims that don't survive scrutiny — explicitly split into Arrow-backed-column (true zero-copy) and legacy-numpy-backed-column (conversion-efficiency) scenarios.

### Phase 6: Packaging, Distribution, and Compatibility Hardening
**Rationale:** ABI/packaging failures are invisible in the maintainer's own dev environment and only surface across a diverse user base; this must be a distinct phase (or clearly-scoped release-phase component) with its own acceptance criteria, not folded into "wheel builds successfully."
**Delivers:** maturin-based wheel builds targeting an explicit, deliberate manylinux/glibc floor; built against the oldest supported NumPy ABI; CI test matrix across oldest-to-newest supported numpy/pandas/pyarrow versions; concurrency stress tests (GIL release/reacquire, exception paths) as a release gate.
**Avoids:** Packaging/ABI failures surfacing only for a subset of users; closes out GIL-deadlock verification.

### Phase Ordering Rationale

- Phases 1-2 (Arrow core + FFI) are a hard, serial dependency for everything else — this is forced by the architecture, not a preference.
- Phases 3 (pandas) and 4 (Parquet) are architecturally independent of each other (both depend only on the Arrow core) — sequenced serially here but flagged as parallelizable if capacity allows.
- Phase 5 (benchmarks) is placed after both interop paths exist because the project's core value claim cannot be honestly measured until both the easy (Arrow-to-Arrow) and hard (pandas-to-Arrow) paths are implemented.
- Phase 6 (packaging) is placed last because ABI/distribution issues are a release-gating concern, not a feature-development concern, but must happen before any public wheel ships — skipping the multi-version CI matrix is "only acceptable pre-first-release."
- Memory-safety and GIL-discipline pitfalls are deliberately front-loaded into Phases 1-3 rather than treated as later hardening, since these are foundational correctness properties, not polish.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3 (Pandas Interop):** Highest-risk, most bespoke component; dtype-by-dtype edge cases (categorical/dictionary Parquet round-trip bugs, nullable-int upcast behavior, `ArrowDtype` import-side gaps in current pandas) warrant a `--research-phase` pass before planning.
- **Phase 5 (Benchmark Suite):** Benchmarking-methodology specifics are MEDIUM confidence (task-derived best practice, not a single authoritative case study) — worth validating current best-practice tooling at plan time.
- **Phase 6 (Packaging):** ABI/manylinux specifics move quickly (Rust glibc requirements, NumPy 2.x transition edge cases); verify current guidance at plan time rather than relying solely on this research snapshot.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Toolchain + Arrow Core):** Well-documented, established pattern (PyO3 + maturin + arrow-rs), directly modeled on `arro3`/`pyo3-arrow` reference implementations.
- **Phase 2 (Type Coverage + Schema):** Standard Arrow type-system work with official arrow-rs/PyCapsule spec documentation as HIGH-confidence primary sources.
- **Phase 4 (Parquet IO):** arrow-rs's `parquet` crate is mature and well-documented; the feature set (codecs, row-group stats, pushdown, projection) is clearly scoped.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Core version numbers verified directly against crates.io/PyPI registry APIs (HIGH); interop-design claims cross-corroborated across multiple independent sources but not Context7/curated-doc backed |
| Features | MEDIUM | Cross-checked against Apache Arrow and pandas official docs plus multiple independent GitHub issues/community sources; no single-source claims presented as fact |
| Architecture | MEDIUM | Official docs for FFI/PyCapsule mechanics are HIGH-trust primary sources; project-structure conclusions synthesized from public repo structure via web search, not independently re-verified line-by-line |
| Pitfalls | HIGH/MEDIUM split | HIGH for FFI memory-safety, packaging/ABI, and API-design findings (official PyO3/Arrow/NumPy docs + primary-source GitHub issue trackers, cross-checked against `arro3`/`pyo3-arrow`); MEDIUM for benchmarking-methodology specifics (informed best practice, not a verified case study) |

**Overall confidence:** MEDIUM

### Gaps to Address

- **pandas ArrowDtype import-side support:** pandas currently only exports via `__arrow_c_*__` (no import support as of current pandas 3.0.x) — confirm current pandas version behavior at Phase 3 planning time, since this affects whether full bidirectional PyCapsule interop with pandas itself is achievable or whether the reverse direction must go through `ArrowDtype` construction instead.
- **Benchmarking methodology specifics:** flagged as task-derived best practice rather than a verified case study — validate current tooling recommendations (criterion, pytest-benchmark, codspeed) don't need updating at Phase 5 planning time.
- **Exact manylinux/glibc floor decision:** Rust 1.64+ requires glibc >= 2.17 (manylinux2014 minimum); confirm this is still current and decide the explicit target at Phase 6 planning time rather than trusting maturin's default.
- **Categorical/dictionary Parquet round-trip edge cases:** documented pyarrow rough edges here (issue #35259, #1688) — worth deeper investigation during Phase 3/4 planning to decide whether to match or improve on pyarrow's behavior.

## Sources

### Primary (HIGH confidence)
- crates.io / PyPI registry APIs — direct version verification for pyo3, pyo3-arrow, arrow, parquet, maturin, arro3-core, pandas, pyarrow (fetched 2026-07-13)
- Apache Arrow official docs — C Data Interface, PyCapsule Interface spec, Columnar Format spec, Pandas Integration docs
- PyO3 official docs — Memory management guide, Free-threading support guide, FAQ & Troubleshooting
- NumPy official docs — ABI guidance for downstream package authors, 2.0.0 Release Notes
- Primary-source GitHub issue trackers — apache/arrow #38644, #39194, #35531, #39195, #39689, #35259, #1688, #23786; pandas-dev/pandas #23786, #63105; pola-rs/polars #12232, #12530; PyO3/rust-numpy #409; PyO3/pyo3 discussion #3089

### Secondary (MEDIUM confidence)
- `arro3` (Kyle Barron) GitHub repo and README — reference architecture, directly comparable real-world project
- `pyo3-arrow` docs.rs — FFI layer design patterns
- pandas official user guide — `dtype_backend="pyarrow"` and `ArrowDtype` behavior
- Apache Arrow project blog — arrow-rs release notes, Rust Parquet metadata performance
- Maturin/maturin-action/cibuildwheel official docs and comparisons
- pypackaging-native — ABI dependency issues (community-maintained but widely-cited)
- DeepWiki summary of Polars crate organization — secondary source, directionally consistent with public repo structure

### Tertiary (LOW confidence)
- General web search without Context7/curated-doc MCP — flagged inline in STACK.md wherever a claim isn't independently registry-verified
- Benchmarking-methodology recommendations in PITFALLS.md — task-derived best practice, not a single authoritative post-mortem

---
*Research completed: 2026-07-13*
*Ready for roadmap: yes*
