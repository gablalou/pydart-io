# Technology Stack

**Project:** Flint (placeholder name) — Rust-backed zero-copy pandas <-> Arrow interop + Parquet IO
**Researched:** 2026-07-13
**Confidence:** MEDIUM (core version numbers verified directly against crates.io/PyPI registry APIs; interop-design claims cross-corroborated across multiple independent sources but not Context7/curated-doc backed — see Sources)

## Recommended Stack

### Core Framework

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Rust | 2021 edition, stable toolchain (MSRV ~1.75+) | Core implementation language | Non-negotiable per PROJECT.md; also required by arrow-rs/pyo3 minimum versions |
| PyO3 | 0.29.0 (verified crates.io, released 2026-06-11) | Rust <-> Python bindings (the FFI layer, `#[pymodule]`, `#[pyclass]`, GIL handling) | The de facto standard binding layer for Rust-Python extensions; what Polars, arro3, and most modern Rust-in-Python libraries use. Actively maintained, tracks new CPython releases within weeks (already supports 3.14, adding 3.15 free-threaded abi3t). Avoid rolling your own FFI via raw `pyo3-ffi` or `cffi` — PyO3's ownership/refcounting model (`Py<T>`, `Bound<'py, T>`) is what prevents the use-after-free and GIL bugs that hand-rolled FFI is prone to. |
| pyo3-arrow | 0.19.0 (verified crates.io) | Reusable Arrow<->PyO3 conversion layer (wraps arrow-rs types for `IntoPyObject`/`FromPyObject`, implements the Arrow C Data Interface + PyCapsule export/import on the Rust side) | This is the single most important stack decision for this project. It is purpose-built prior art for exactly this problem (arrow-rs objects in, Python-capsule-protocol-compliant objects out) and is maintained by the author of arro3. Building on it — rather than hand-writing FFI_ArrowArray/FFI_ArrowSchema marshalling — eliminates an entire class of memory-safety bugs and gets PyCapsule Interface compliance (interop with pyarrow, polars, duckdb, nanoarrow) for free. Treat `arro3` (below) as the reference implementation to study, and `pyo3-arrow` as a library to depend on directly where its abstractions fit. |
| arrow (apache/arrow-rs) | 59.1.0 (verified crates.io, monthly release cadence) | Rust in-memory Arrow columnar format implementation, array builders/readers, IPC | This is *the* official Rust Arrow implementation and the only one still actively maintained for general use. Used internally by DataFusion, Polars (for interop, not compute), InfluxDB IOx, and is what `pyo3-arrow`/`arro3` build on. It is the correct choice per PROJECT.md's "must interoperate with the existing Arrow ecosystem" constraint. |
| parquet (apache/arrow-rs) | 59.1.0 (verified crates.io, released in lockstep with `arrow`) | Rust native Parquet reader/writer, with the `arrow` feature enabling direct RecordBatch <-> Parquet round-trips | Same monorepo as `arrow`, so versions are always compatible by construction — no cross-crate version-matrix risk. As of v57 it ships a custom Thrift metadata parser that is 3-9x faster than the previous implementation, directly relevant to a "faster than pyarrow" performance claim on file IO. |
| maturin | 1.14.1 (verified crates.io) | Build backend: compiles the Rust extension and packages it as a Python wheel via PEP 517 | The standard build tool for PyO3 projects (built and maintained by the PyO3 org itself). Handles abi3 wheel tagging, manylinux/musllinux compliance (its own auditwheel-equivalent), and sdist generation with zero extra config beyond `pyproject.toml`. |

### Data Interop / Zero-Copy Layer

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Arrow C Data Interface | Arrow format spec (stable since Arrow 2.0, no separate version) | The underlying ABI-stable C struct layout (`ArrowArray`, `ArrowSchema`) that allows two processes/libraries sharing the same memory space to exchange Arrow arrays without copying, by passing pointers + a release callback | This is the actual mechanism of zero-copy — everything else (PyCapsule Interface, `pyo3-arrow`) is a safe wrapper around it. Understanding it is required even when using higher-level helpers, because it defines what is/isn't actually zero-copy (contiguous, must own/keep-alive the underlying buffer). |
| Arrow PyCapsule Interface | Protocol methods `__arrow_c_schema__`, `__arrow_c_array__`, `__arrow_c_stream__` (stable since pyarrow ~14/pandas 2.2, 2023-2024) | Python-level dunder-method protocol that wraps C Data Interface structs in `PyCapsule` objects for safe, GC-integrated, library-agnostic zero-copy exchange | This is what makes the library interoperate with pyarrow, Polars, DuckDB, and pandas *without* a hard dependency on pyarrow's own Python API. Implementing `__arrow_c_array__`/`__arrow_c_stream__` on your Table/Array/RecordBatch Python classes (which `pyo3-arrow` gives you for free) is what lets `pa.table(your_object)`, `pl.from_arrow(your_object)`, and `duckdb.sql(...).df()` all "just work" without copies. This is the modern (2024+) replacement for the older pattern of passing raw `_export_to_c`/`_import_from_c` pointer integers — prefer it over the legacy pattern for new code. |
| numpy buffer protocol (PEP 3118) | via `rust-numpy` crate if numpy-array-level access is needed | Fallback zero-copy path for the small set of primitive numeric numpy arrays (not the general pandas story) | Only needed if you want to accept/produce numpy arrays directly (not just pandas ArrowDtype columns) at the boundary. Contiguous numeric numpy arrays can be borrowed zero-copy into Arrow buffers; non-numeric (object dtype) arrays cannot. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `rust-numpy` (pyo3 org) | latest tracking PyO3 0.29 | Zero-copy access to numpy `ndarray` buffers from Rust | Needed only for the "legacy numpy-backed pandas column" conversion path (see Critical Caveat below) — not needed for the ArrowDtype/pyarrow-backed fast path. |
| `arrow-flight` (apache/arrow-rs) | matches `arrow`/`parquet` version | Arrow Flight RPC | Out of scope for v1 (no network/distributed IO per PROJECT.md) — do not add. |
| `object_store` (apache/arrow-rs) | latest | Pluggable local/S3/GCS/Azure storage backend for Parquet IO | Only pull in if/when remote (non-local-filesystem) Parquet reads are wanted; local filesystem IO doesn't need it. Defer for v1 given PROJECT.md scopes this to local/in-memory. |
| `thiserror` / `anyhow` | latest | Rust error handling, converted to Python exceptions at the PyO3 boundary via `impl From<MyError> for PyErr` | Standard pattern for any PyO3 extension — needed from day one so Rust panics/errors surface as clean Python exceptions rather than aborting the interpreter. |
| `pytest`, `hypothesis` | latest | Python-side test suite, including property-based tests for round-trip (pandas -> your Table -> pandas) correctness | Round-trip correctness under property-based testing is the right way to catch subtle zero-copy/dtype-mapping bugs (e.g. nullable int edge cases) before they reach benchmarks. |
| `pytest-benchmark` or `codspeed` | latest | Benchmark harness for the "measurably faster than pyarrow" requirement | PROJECT.md explicitly requires a benchmark suite vs pyarrow — pick one of these rather than hand-rolling timing code, for statistically defensible, regression-trackable numbers. |
| `criterion` | latest | Rust-side micro-benchmarks (pure conversion/IO hot loops, no Python overhead) | Use alongside the Python-level benchmark suite to isolate whether slowness is in the Rust core or at the PyO3/GIL boundary. |

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Binding tooling | PyO3 + maturin | `cffi` + manual C ABI, or `setuptools-rust` | `cffi` requires hand-writing the marshalling and GIL-safety code PyO3 gives you for free — directly counter to a project whose whole value is a *safer, leaner* interop layer. `setuptools-rust` is an older/lower-level alternative to maturin with more manual `pyproject.toml`/`setup.py` wiring and weaker out-of-box manylinux/abi3 ergonomics; maturin is the more current, PyO3-org-blessed default. |
| Arrow implementation | apache/arrow-rs (`arrow` + `parquet` crates) | arrow2 / polars-arrow | arrow2 is an unmaintained experimental fork (now archived under `apache/arrow-experimental-rs-arrow2`); polars-arrow is a further fork maintained only for Polars' internal engine needs and not designed/supported as a general external dependency. Building v1 on either would mean depending on a shrinking or non-public-facing ecosystem — the opposite of "interoperates with the existing Arrow ecosystem" from PROJECT.md. |
| Zero-copy handoff | Arrow PyCapsule Interface (`__arrow_c_array__`/`__arrow_c_stream__`) via `pyo3-arrow` | Raw `_export_to_c`/`_import_from_c` pointer-integer passing (the pre-2024 pattern still used in some older pyarrow-interop code and blog posts) | The raw pattern passes bare pointers as Python ints with no lifetime/ownership safety net — a bug in either process's cleanup path is a use-after-free or leak. PyCapsule wraps the same underlying C Data Interface structs but ties their lifetime to Python's own GC via the capsule destructor, and is now the interoperability convention pandas/pyarrow/polars/duckdb have converged on since ~2024. |
| Wheel-building CI | maturin-action | cibuildwheel | cibuildwheel is the more general-purpose PEP 517 wheel builder (works with any backend, more platforms incl. WASM/iOS), but for a pure-Rust-extension project maturin-action is purpose-built: it builds the compiled Rust artifact once and cross-compiles for other targets (faster CI), and handles manylinux/musllinux compliance through maturin's own auditwheel-equivalent without an extra container-matrix setup. Prefer cibuildwheel only if this project later grows a mixed Rust+other-native-dependency build. |
| Reference architecture | Study `arro3` (kylebarron) as prior art; do not depend on it as a runtime dependency | Fork/vendor arro3 directly, or depend on `arro3-core` as a library | arro3 is a full sibling product (a minimal pyarrow replacement), not a library meant to be embedded inside someone else's package — its value here is as a working example of "arrow-rs + pyo3-arrow + PyCapsule Interface, packaged with maturin," not as a dependency. Read its source (MIT/Apache-2.0 dual-licensed) for concrete patterns instead. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| arrow2 / polars-arrow as your Arrow implementation | Unmaintained (arrow2) or internal-only, not intended as a general dependency (polars-arrow); depending on either creates ecosystem-interop risk that directly undermines PROJECT.md's Arrow-compatibility requirement | apache/arrow-rs (`arrow` crate) |
| Hand-rolled `_export_to_c`/`_import_from_c` integer-pointer FFI | No automatic lifetime management; a common source of segfaults/use-after-free in early (pre-2024) Rust-Arrow-Python glue code found in blog posts and older library code | Arrow PyCapsule Interface via `pyo3-arrow` |
| Treating all pandas DataFrames as zero-copy convertible | **Critical caveat, see below** — only Arrow-backed (`dtype_backend="pyarrow"` / `ArrowDtype`) columns are genuinely zero-copy convertible; default numpy-backed columns (especially `object`, `category`, and nullable-int/string extension dtypes) require an actual copy or type conversion during Arrow conversion regardless of tooling | Detect the column's dtype backend and document/benchmark the numpy-backed path as "minimal-copy," not "zero-copy" — matches PROJECT.md's own hedge ("zero-copy (or minimal-copy)") |
| Building a Rust core with `pyo3-ffi` raw bindings instead of PyO3's high-level API | Loses PyO3's safe wrappers (`Bound<'py,T>`, `Py<T>`) for GIL-tied reference counting; reinventing this is wasted effort and a safety liability for a library whose pitch is being a *safer* alternative | PyO3 high-level API (`#[pyclass]`, `#[pyfunction]`, `#[pymodule]`) |
| Distributing via manual `pip install`-able sdist-only packages without prebuilt wheels | Forces every user to have a Rust toolchain installed to install your library — a major adoption blocker for an open-source interop library targeting general Python/data users | maturin + maturin-action building manylinux/musllinux/macOS/Windows wheels in CI, published to PyPI |

## Stack Patterns by Variant

**If targeting maximum Python version compatibility with minimal wheel count (recommended for v1):**
- Build `abi3` wheels (PyO3's `abi3-py39` or similar minimum-version feature) so one wheel per platform covers all newer CPython 3.x minor versions.
- Because: reduces the CI build matrix from (Python versions x platforms) to just (platforms), and is standard practice for interop/utility libraries (arro3 does this).

**If later supporting free-threaded Python (3.13t/3.14t and beyond) as an explicit goal:**
- Use PyO3's new `abi3t` feature (paired with PEP 803), available from PyO3 0.28+.
- Because: a project whose entire value proposition is performance should not ignore the free-threaded build track long-term, but this is reasonably deferred past v1 given PROJECT.md's tight scope.

**If the pandas input DataFrame has `dtype_backend="pyarrow"` columns:**
- Route through the PyCapsule Interface / Arrow C Data Interface directly — this is the true zero-copy path and the one worth benchmarking as the headline "faster than pyarrow" number.

**If the pandas input DataFrame has default numpy-backed columns:**
- Route through `rust-numpy`-borrowed buffers where the column is a contiguous primitive numpy array (int/float), and accept an explicit conversion cost for `object`/`category`/nullable extension columns.
- Because: PROJECT.md explicitly allows "minimal-copy" as a fallback — be honest in docs/benchmarks about which path is which, since silently mislabeling a copying path as "zero-copy" would undermine the project's core credibility claim.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|------------------|-------|
| `arrow` 59.1.0 | `parquet` 59.1.0 | Released from the same apache/arrow-rs monorepo in lockstep — always pin them to the same version to avoid ArrayData/schema mismatches. |
| `pyo3` 0.29.0 | `pyo3-arrow` 0.19.0 | Check `pyo3-arrow`'s Cargo.toml on release — as a smaller, faster-moving crate it sometimes trails the newest PyO3 minor by a few weeks; pin both explicitly and re-test on PyO3 bumps rather than using `^` ranges blindly. |
| `maturin` 1.14.1 | `pyo3` 0.29.0 | maturin is bindings-version-agnostic (it just invokes cargo + wheel packaging), but its abi3 wheel-tagging logic should be validated against whatever minimum Python version your `abi3-pyXY` feature targets. |
| pandas >= 2.2 | Arrow PyCapsule Interface (export-only) | pandas only *exports* via `__arrow_c_*__` (no import support as of current pandas 3.0.x) — round-trip *into* pandas still goes through `pyarrow`-style `to_pandas()`/`ArrowDtype` construction, not the capsule protocol in reverse. Confirm current pandas import-side support before assuming full bidirectional capsule interop. |
| pyarrow 25.0.0 (current PyPI) | Used only as a comparison/benchmark target and for cross-ecosystem compatibility testing, not a runtime dependency of this library | Keep pyarrow as a dev/test dependency for round-trip and benchmark validation; do not make it a runtime dependency of the shipped package (that would defeat the "leaner than pyarrow" positioning). |

## Critical Caveat for This Project (read before roadmap planning)

The single most decision-relevant technical fact from this research: **"zero-copy pandas <-> Arrow" is only fully true for Arrow-backed pandas columns** (`dtype_backend="pyarrow"` / `pd.ArrowDtype`). For the far more common default numpy-backed DataFrame (the one most existing pandas users actually have), converting to Arrow requires an actual copy or dtype conversion for `object` columns, categoricals, and nullable extension dtypes — no tooling choice (PyO3, arrow-rs, pyo3-arrow) changes this, because the underlying pandas memory layout for those columns simply isn't Arrow-compatible in place.

This has two implications for the roadmap:
1. The "measurably faster than pyarrow" benchmark claim should be split into two scenarios — Arrow-backed-column round-trip (where true zero-copy is achievable and the win should be about GIL/allocation overhead vs pyarrow's own C++ boundary) and legacy-numpy-backed-column round-trip (where the win, if any, is about conversion efficiency, not copy elimination).
2. Phase planning should treat "detect dtype backend and choose zero-copy vs minimal-copy path" as a first-class design decision, not an implementation detail — it directly affects the public API shape (e.g. whether the library nudges/requires callers to pass Arrow-backed pandas DataFrames for the fast path).

## Sources

- crates.io registry API (`https://crates.io/api/v1/crates/{pyo3,maturin,pyo3-arrow,arrow,parquet}`) — HIGH-confidence primary-source version verification, fetched 2026-07-13
- PyPI registry API (`https://pypi.org/pypi/{arro3-core,pandas,pyarrow}/json`) — HIGH-confidence primary-source version verification, fetched 2026-07-13
- `arrow.apache.org/docs/format/CDataInterface/PyCapsuleInterface.html` and related apache/arrow GitHub issues (#35531, #39195, #39689) — protocol design and pandas/pyarrow adoption status, MEDIUM confidence (official docs, general web search)
- `pandas.pydata.org/docs/user_guide/pyarrow.html`, `pandas.ArrowDtype` reference docs — pandas Arrow-backed dtype behavior, MEDIUM confidence
- `github.com/kylebarron/arro3`, `crates.io/crates/pyo3-arrow`, `docs.rs/pyo3-arrow` — reference architecture for this project's core problem, MEDIUM confidence (general web search, cross-corroborated across multiple independent results)
- `www.maturin.rs`, `github.com/PyO3/maturin-action`, `github.com/pypa/cibuildwheel` — packaging/CI tooling comparison, MEDIUM confidence
- `arrow.apache.org/blog/2025/10/30/arrow-rs-57.0.0` and `arrow.apache.org/blog/2025/10/23/rust-parquet-metadata` — arrow-rs/parquet performance and release cadence, MEDIUM confidence (official project blog)
- General web search (no Context7/curated-doc MCP available in this environment) — LOW confidence baseline for any claim not independently registry-verified; flagged inline above where applicable

---
*Stack research for: Rust-backed Python Arrow/pandas interop + Parquet IO library*
*Researched: 2026-07-13*
