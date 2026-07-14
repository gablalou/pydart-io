---
phase: 01-core-zero-copy-round-trip-interop
plan: 01
subsystem: interop
tags: [rust, pyo3, pyo3-arrow, arrow, maturin, uv, pandas, pyarrow, walking-skeleton]

# Dependency graph
requires: []
provides:
  - Two-crate Cargo workspace (flint-core pure Rust, flint-python pyo3 extension)
  - flint.Table pyclass composing pyo3_arrow::PyTable (never hand-rolls FFI)
  - Table.from_pandas/to_pandas numeric happy path (non-null int64/double ArrowDtype)
  - Table.__arrow_c_schema__/__arrow_c_stream__ PyCapsule export (CAP-01, consumed by pyarrow.table)
  - Table.column(name)/Table.buffer_address(index) accessors for the Plan 03 pointer-identity proof
  - maturin + uv local dev workflow (uv run maturin develop && uv run pytest)
affects: [01-02, 01-03, 01-04]

# Tech tracking
tech-stack:
  added:
    - "rustup stable toolchain (rustc/cargo 1.97.0, pinned via rust-toolchain.toml channel=stable)"
    - "pyo3 0.29.0 (features extension-module, abi3-py311 -- see Deviations)"
    - "pyo3-arrow 0.19.0"
    - "arrow (apache/arrow-rs) 59.1.0"
    - "maturin 1.14.1 build backend"
    - "thiserror 2.0.18"
    - "allocation-counter 0.8.1 (flint-core dev-dependency, cfg(test), for Plan 03)"
    - "uv dev deps: pandas 3.0.3, pyarrow 25.0.0, polars 1.42.1, duckdb 1.5.4, numpy 2.5.1, pytest 9.1.1, hypothesis 6.156.6"
  patterns:
    - "Compose pyo3_arrow::PyTable as an inner Py<PyTable> field; delegate PyCapsule dunders via Bound::call_method* (Python dispatch) since pyo3-arrow's own dunder methods are crate-private Rust items"
    - "Single FlintError enum (thiserror) + one impl From<FlintError> for PyErr boundary"
    - "from_pandas: validate every column is a supported ArrowDtype before touching Arrow C Data Interface; import via pandas' own DataFrame.__arrow_c_stream__ export + PyTable::from_arrow_pycapsule"
    - "to_pandas: PyTable::into_pyarrow (existing pyo3-arrow API) + pyarrow's own Table.to_pandas(types_mapper=pandas.ArrowDtype) -- no hand-written buffer marshalling"

key-files:
  created:
    - Cargo.toml
    - rust-toolchain.toml
    - .gitignore
    - crates/flint-core/Cargo.toml
    - crates/flint-core/src/lib.rs
    - crates/flint-core/src/table.rs
    - crates/flint-python/Cargo.toml
    - crates/flint-python/src/lib.rs
    - crates/flint-python/src/table.rs
    - crates/flint-python/src/error.rs
    - pyproject.toml
    - python/flint/__init__.py
    - tests/python/test_round_trip.py
    - tests/python/test_export_smoke.py
  modified: []

key-decisions:
  - "Bumped PyO3's abi3 floor from abi3-py39 (CLAUDE.md's stated default) to abi3-py311, because pyo3-arrow 0.19.0's buffer-protocol #[pymethods] require CPython's stable-ABI buffer support, which PyO3 only compiles in when the abi3 floor is >=3.11 -- abi3-py39 fails to compile against pyo3-arrow with 'releasebufferproc'/'Py_buffer' not found errors."
  - "Set pyproject.toml requires-python to >=3.12 (not >=3.9) because RESEARCH.md's pinned numpy==2.5.1 dev dependency requires Python>=3.12, and uv's resolver requires requires-python to cover every declared dependency including dev deps."
  - "from_pandas delegates entirely to pandas' own DataFrame.__arrow_c_stream__ PyCapsule export (rather than reaching into pandas' private _pa_array attribute as RESEARCH.md's D-06a sketch suggested) -- confirmed at runtime that pandas 3.0.3's DataFrame implements __arrow_c_stream__ for ArrowDtype-backed columns, which pyo3_arrow::PyTable::from_arrow_pycapsule already knows how to import zero-copy."
  - "to_pandas goes through pyarrow's own Table.to_pandas(types_mapper=pandas.ArrowDtype) rather than manually constructing pandas ArrowExtensionArray columns -- this is the officially documented zero-copy path and pandas' own ArrowDtype feature already requires pyarrow to be installed, so this does not add a new runtime dependency beyond what pandas' ArrowDtype support already implies."

patterns-established:
  - "PyCapsule dunder delegation via Bound::call_method* (Python dispatch), not direct Rust method calls, when composing a foreign #[pyclass] whose #[pymethods] are not `pub` Rust items"
  - "Single Rust error enum -> one From<...> for PyErr boundary, extended per-variant as new error cases are needed (column errors -> PyTypeError, not-implemented -> PyNotImplementedError)"

requirements-completed: [CONV-01, CONV-02, CAP-01]

coverage:
  - id: D1
    description: "flint.Table.from_pandas(df).to_pandas() round-trips a non-null int64/float64 ArrowDtype DataFrame with values and dtypes preserved"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "tests/python/test_round_trip.py#test_from_pandas_to_pandas_round_trip_preserves_values_and_dtypes"
        status: pass
    human_judgment: false
  - id: D2
    description: "Round-trip correctness: values and dtypes are exactly equal after from_pandas/to_pandas (pandas.testing.assert_frame_equal)"
    requirement: "CONV-02"
    verification:
      - kind: unit
        ref: "tests/python/test_round_trip.py#test_from_pandas_to_pandas_round_trip_preserves_values_and_dtypes"
        status: pass
    human_judgment: false
  - id: D3
    description: "A flint.Table exports via the Arrow PyCapsule Interface and pyarrow.table(...) accepts it with matching schema and row count"
    requirement: "CAP-01"
    verification:
      - kind: unit
        ref: "tests/python/test_export_smoke.py#test_pyarrow_table_accepts_flint_table_via_pycapsule"
        status: pass
    human_judgment: false
  - id: D4
    description: "Table.buffer_address(index) returns a nonzero buffer address for a populated Table (backs the Plan 03 pointer-identity proof)"
    verification:
      - kind: unit
        ref: "tests/python/test_export_smoke.py#test_buffer_address_is_nonzero_for_populated_table"
        status: pass
    human_judgment: false
  - id: D5
    description: "from_pandas rejects an unsupported column (non-ArrowDtype) with an error naming the offending column, rather than silently copying"
    verification:
      - kind: unit
        ref: "tests/python/test_export_smoke.py#test_from_pandas_rejects_unsupported_column_with_column_name_in_message"
        status: pass
    human_judgment: false

duration: 24min
completed: 2026-07-14
status: complete
---

# Phase 1 Plan 1: Walking Skeleton (Two-Crate Workspace + Numeric Round-Trip + PyCapsule Export) Summary

**Two-crate Rust/PyO3 workspace with a `flint.Table` composing `pyo3_arrow::PyTable`, a working numeric ArrowDtype pandas round-trip, and a real pyarrow PyCapsule export -- the whole stack buildable and importable via `uv run maturin develop`.**

## Performance

- **Duration:** 24 min
- **Started:** 2026-07-14T05:12:00Z
- **Completed:** 2026-07-14T05:36:34Z
- **Tasks:** 2 completed
- **Files modified:** 14 created

## Accomplishments

- Installed a stable Rust toolchain (rustup, rustc/cargo 1.97.0) and stood up a two-crate Cargo workspace: `flint-core` (pure Rust, zero pyo3 dependency) and `flint-python` (the only crate depending on `pyo3`/`pyo3-arrow`)
- `flint.Table` `#[pyclass]` composes an inner `pyo3_arrow::PyTable`; PyCapsule export dunders (`__arrow_c_schema__`, `__arrow_c_stream__`) delegate to it -- no hand-rolled `FFI_ArrowArray`/`FFI_ArrowSchema` construction anywhere
- Implemented the numeric happy-path `from_pandas`/`to_pandas`: non-null `int64[pyarrow]`/`double[pyarrow]` (`ArrowDtype`-backed) columns round-trip through pandas' own `__arrow_c_stream__` export and pyarrow's own `to_pandas(types_mapper=pandas.ArrowDtype)`, with zero hand-written buffer marshalling
- One real external-consumer PyCapsule handoff proven: `pyarrow.table(flint_table)` succeeds with matching schema and row count (CAP-01)
- `uv run maturin develop && uv run pytest` documented and exercised as the local full-stack dev workflow (uv-compatible per PROJECT.md constraint)

## Task Commits

Each task was committed atomically:

1. **Task 1: Install toolchain, scaffold the two-crate workspace, and land a failing end-to-end round-trip test** - `5d77276` (feat)
2. **Task 2: Implement the thin numeric round-trip and one pyarrow PyCapsule export (turn the skeleton GREEN)** - `2c7c3dd` (test, RED gate) then `ca81766` (feat, GREEN gate)

**Plan metadata:** pending (this commit)

_Note: Task 2 was tagged `tdd="true"` -- RED (`test_export_smoke.py` failing against the Task 1 NotImplementedError stubs) then GREEN (real implementation, all four tests passing) per the TDD execution flow._

## Files Created/Modified

- `Cargo.toml` - Workspace root, members flint-core + flint-python
- `rust-toolchain.toml` - Pins channel = stable (MSRV floor ~1.75)
- `.gitignore` - Ignores /target, .venv, __pycache__, .pytest_cache, and the maturin-develop-dropped `python/flint/*.so` build artifact
- `crates/flint-core/Cargo.toml` - Pure Rust crate; arrow 59.1.0 dependency; allocation-counter 0.8 dev-dependency (Plan 03 target)
- `crates/flint-core/src/lib.rs` / `table.rs` - Thin `RecordBatch` re-export; `from_numpy_buffer` stub placeholder for Plan 03
- `crates/flint-python/Cargo.toml` - pyo3 0.29.0 (extension-module, abi3-py311), pyo3-arrow 0.19.0, arrow 59.1.0, thiserror 2, flint-core path dep
- `crates/flint-python/src/lib.rs` - `#[pymodule] fn _flint` registering `Table`
- `crates/flint-python/src/table.rs` - `Table` pyclass: `from_pandas`/`to_pandas` (numeric happy path), PyCapsule dunders, `column`/`buffer_address` accessors
- `crates/flint-python/src/error.rs` - `FlintError` enum (thiserror) + single `impl From<FlintError> for PyErr`
- `pyproject.toml` - maturin build backend, `module-name = "flint._flint"`, `python-source = "python"`, uv-compatible dev workflow documented
- `python/flint/__init__.py` - re-exports `Table`
- `tests/python/test_round_trip.py` - end-to-end numeric round-trip test (GREEN)
- `tests/python/test_export_smoke.py` - CAP-01 pyarrow export, `buffer_address`, unsupported-column rejection tests (GREEN)

## Decisions Made

- **abi3 floor raised to `abi3-py311`** (from CLAUDE.md's stated `abi3-py39`): `pyo3-arrow` 0.19.0's buffer-protocol `#[pymethods]` require CPython stable-ABI buffer support, which PyO3 only compiles when the abi3 floor is >=3.11. Building with `abi3-py39` fails with `releasebufferproc`/`Py_buffer` "not found" compile errors. This is a genuine upstream ABI-floor incompatibility discovered during the actual build, not a design preference -- raising the floor to 3.11 is the minimal fix; it does not affect this environment (Python 3.12.3) and only narrows the eventual wheel's consumer floor from 3.9 to 3.11 (a Phase 4 packaging concern).
- **`pyproject.toml requires-python` raised to `>=3.12`** (from an initial `>=3.9`): RESEARCH.md's pinned `numpy==2.5.1` dev dependency requires Python>=3.12, and `uv`'s resolver requires `requires-python` to cover every declared dependency (dev included) or resolution fails outright. This governs the uv-managed dev/test environment only, not the abi3 wheel's own consumer-facing compatibility floor.
- **`from_pandas` imports via pandas' own `DataFrame.__arrow_c_stream__` export** rather than reaching into pandas' private `_pa_array` attribute (as RESEARCH.md's D-06a code sketch illustrated): confirmed at runtime that pandas 3.0.3's `DataFrame` implements `__arrow_c_stream__` for `ArrowDtype`-backed columns, and `pyo3_arrow::PyTable::from_arrow_pycapsule` already imports it zero-copy. This is a cleaner composition (delegates to two already-correct, already-public APIs) than reaching into a private pandas attribute.
- **`to_pandas` goes through `PyTable::into_pyarrow` + pyarrow's own `Table.to_pandas(types_mapper=pandas.ArrowDtype)`**: this is pyarrow's officially documented zero-copy-when-types-match conversion path. Since pandas' own `ArrowDtype` feature already requires pyarrow to be installed (pandas doesn't reimplement Arrow itself), this does not introduce a new runtime dependency beyond what using `ArrowDtype` already implies -- `flint`'s own `pyproject.toml` `dependencies` list stays empty (CLAUDE.md "leaner than pyarrow" positioning).
- **PyCapsule dunders (`__arrow_c_schema__`/`__arrow_c_stream__`) and `column()` delegate via `Bound::call_method*` (Python method dispatch)**, not direct Rust method calls: confirmed by reading pyo3-arrow 0.19.0's actual source (`crates.io` download) that `PyTable`'s own `#[pymethods]` implementations of these are not `pub` Rust items -- calling them directly from another crate does not compile. Dispatching through Python's own method resolution (which finds the compiled, already-registered wrapper) still delegates 100% of the FFI marshalling to `pyo3-arrow`; it does not reimplement any of it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking build config] abi3 floor raised from abi3-py39 to abi3-py311**
- **Found during:** Task 1 (first `uv add --dev` build attempt, which builds the project via maturin)
- **Issue:** `pyo3-arrow` 0.19.0 fails to compile under PyO3's `abi3-py39` feature (`releasebufferproc`/`Py_buffer` not found in `pyo3::ffi`) because its buffer-protocol `#[pymethods]` require CPython's stable-ABI buffer support (added in 3.11), which PyO3 only compiles in when the declared abi3 floor is >=3.11.
- **Fix:** Changed `crates/flint-python/Cargo.toml`'s pyo3 feature from `abi3-py39` to `abi3-py311`.
- **Files modified:** `crates/flint-python/Cargo.toml`
- **Verification:** `cargo build --workspace` and `uv run maturin develop` both succeed cleanly with no warnings.
- **Committed in:** `5d77276` (Task 1 commit)

**2. [Rule 3 - Blocking version conflict] `pyproject.toml requires-python` raised to >=3.12**
- **Found during:** Task 1 (`uv add --dev` dependency resolution)
- **Issue:** RESEARCH.md's pinned `numpy==2.5.1` requires Python>=3.12; the initial `requires-python = ">=3.9"` (matching the intended abi3 wheel floor) made uv's resolver report the whole dev-dependency set as unsatisfiable.
- **Fix:** Raised `requires-python` to `>=3.12` in `pyproject.toml`, with an inline comment noting this governs the dev/test environment only, separate from the shipped wheel's abi3 consumer floor.
- **Files modified:** `pyproject.toml`
- **Verification:** `uv add --dev ...` with all seven RESEARCH.md-pinned versions resolves and installs successfully.
- **Committed in:** `5d77276` (Task 1 commit)

**3. [Rule 1 - Bug] Fixed PyCapsule dunder delegation compile errors (private methods, PyObject removal, signature mismatches)**
- **Found during:** Task 1 build attempts
- **Issue:** The RESEARCH.md Code Example sketch (`self.inner.__arrow_c_stream__(py, requested_schema)`) does not compile: `PyTable`'s `#[pymethods]` implementations of `__arrow_c_schema__`/`__arrow_c_stream__` are not `pub` Rust items (confirmed by reading the actual pyo3-arrow 0.19.0 source). Separately, `PyObject` is not a type alias in pyo3 0.29.0 (removed), and `#[pyo3(signature = ...)]` requires exact Rust parameter name matches.
- **Fix:** Delegate via `Bound::call_method0`/`call_method1` (Python method dispatch) instead of direct Rust calls; use `Py<PyAny>` instead of the removed `PyObject` alias; renamed Rust parameters to match declared `#[pyo3(signature = ...)]` names.
- **Files modified:** `crates/flint-python/src/table.rs`
- **Verification:** `cargo build -p flint-python` compiles with zero errors/warnings.
- **Committed in:** `5d77276` (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (2 blocking build-config/version-resolution issues, 1 blocking compile-error fix). All were required just to make `cargo build`/`maturin develop`/`uv add --dev` succeed at all -- no architectural changes, no scope creep, no package substitutions.

## Issues Encountered

- `uv add --dev` initially auto-downloaded and used its own bundled Rust toolchain (via maturin's `puccinialin` fallback installer) because the pre-installed `rustup`-provided cargo was only on `PATH` within the interactive shell that installed it, not inside `uv`'s build-isolation subprocess. Both toolchains are the same rustc/cargo 1.97.0 stable release, so this did not cause any version skew; noted here only because it explains why the pyo3-arrow source was inspected from `~/.cache/puccinialin/cargo/registry/...` rather than `~/.cargo/registry/...` while investigating the abi3/PyObject compile errors above.

## User Setup Required

None - no external service configuration required. (Toolchain and dependency installation described above was performed as part of this plan's Task 1, not left as manual setup.)

## Next Phase Readiness

- The two-crate workspace, `Table` composition pattern, and `uv run maturin develop && uv run pytest` dev workflow are locked and match SKELETON.md's Architectural Decisions -- Plan 02 (full per-column decision matrix, strict mode, `copy_report()`), Plan 03 (zero-copy proofs), and Plan 04 (Polars/DuckDB interop) can build directly on this.
- `flint-core`'s `from_numpy_buffer` stub and `allocation-counter` dev-dependency are in place and untouched, ready for Plan 03 to fill in without any `Cargo.toml` changes.
- `Table.buffer_address(index)` is implemented and tested (nonzero on a populated Table) -- ready for Plan 03's pointer-identity proof to consume directly.
- No blockers. One item worth flagging forward: `to_pandas` currently calls `py.import("pyarrow")` internally (via `PyTable::into_pyarrow` + `Table.to_pandas(types_mapper=...)`); this is consistent with pandas' own `ArrowDtype` feature already requiring pyarrow, but Plan 02 (which generalizes `to_pandas` beyond the numeric happy path) should keep this dependency in mind when it extends the conversion surface.

---
*Phase: 01-core-zero-copy-round-trip-interop*
*Completed: 2026-07-14*

## Self-Check: PASSED

All 13 created files verified present on disk; all 4 commits (`5d77276`, `2c7c3dd`, `ca81766`, `ddf1fea`) verified present in `git log --all`.
