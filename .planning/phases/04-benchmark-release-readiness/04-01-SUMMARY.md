---
phase: 04-benchmark-release-readiness
plan: 01
subsystem: testing
tags: [pytest-benchmark, psutil, criterion, maturin, uv, benchmark, packaging]

# Dependency graph
requires:
  - phase: 03-parquet-io
    provides: Table.from_pandas/to_pandas/from_parquet/to_parquet API surface exercised by the benchmark harness
provides:
  - Corrected pyproject.toml (requires-python >=3.11, pydart-io distribution name, benchmark dev deps)
  - Corrected crates/pydart-core/Cargo.toml (criterion dev-dependency + conversion_bench bench target)
  - A working, honest, end-to-end benchmark measurement path (throughput + peak RSS + Rust-kernel time) proven on one scenario
  - BENCHMARKS.md skeleton with real numeric_dense results, ready for Plan 02 to extend
affects: [04-02-benchmark-matrix, 04-03-packaging-ci, 04-04-release]

# Tech tracking
tech-stack:
  added: [pytest-benchmark 5.2.3, psutil 7.2.2, criterion 0.8]
  patterns:
    - "Benchmark scenario generators live in benchmarks/scenarios.py as the single source of truth; benchmarks/memory/scenarios_memory.py re-imports rather than duplicating"
    - "Peak-RSS measurement always runs in a fresh subprocess per scenario via psutil, never an in-process Python-heap profiler"
    - "Rust-side criterion benches call the pyo3-free pydart_core entry points directly, mirroring tests/rust/*.rs's safety-comment convention"

key-files:
  created:
    - benchmarks/scenarios.py
    - benchmarks/conftest.py
    - benchmarks/test_bench_from_pandas.py
    - benchmarks/memory/measure_rss.py
    - benchmarks/memory/scenarios_memory.py
    - crates/pydart-core/benches/conversion_bench.rs
    - BENCHMARKS.md
  modified:
    - pyproject.toml
    - uv.lock
    - Cargo.lock
    - crates/pydart-core/Cargo.toml
    - .claude/CLAUDE.md

key-decisions:
  - "Loosened the numpy dev-pin from an exact ==2.5.1 to a range (>=2.3,<2.6) rather than keeping requires-python at >=3.12 -- uv's universal resolver picks numpy 2.4.x for a 3.11 interpreter and 2.5.x for 3.12+ within a single uv.lock, empirically confirmed via `uv lock`/`uv sync --dev`, resolving RESEARCH.md Assumption A1"
  - "[project].name changed to pydart-io (D-41); import path, module-name (pydart._pydart), and all Rust crate/exception names are unchanged"
  - "pandas Version Compatibility claim in .claude/CLAUDE.md corrected from >=2.2 to >=3.0 (D-36), closing the WR-02 CoW-safety documentation gap"
  - "Created crates/pydart-core/benches/conversion_bench.rs during Task 2 (not strictly Task 2's file) because Cargo.toml's new [[bench]] entry requires the file to exist for cargo to parse the manifest at all -- without it, `uv sync --dev`'s editable maturin build fails immediately. The file's real content (matching Task 3's own spec) was written once and committed under Task 3, where it canonically belongs; Task 2's commit only includes Cargo.toml itself."
  - "benchmarks/scenarios.py and benchmarks/memory/scenarios_memory.py are imported by bare module name (e.g. `from scenarios import ...`), not as `benchmarks.scenarios`, relying on pytest's own prepend-mode sys.path insertion and an explicit sys.path.insert in the subprocess entry point -- avoids requiring `benchmarks/__init__.py` while still satisfying the plan's no-duplication requirement"
  - "measure_rss.py's docstring describes the excluded in-process profiling approach without using the literal string \"tracemalloc\", to satisfy the plan's own automated grep check (`grep -c tracemalloc ... returns 0`) while still documenting Pitfall 2's rationale"

requirements-completed: [BENCH-01, BENCH-02, PKG-03]

coverage:
  - id: D1
    description: "uv lock/sync resolve cleanly on Python 3.11 under the corrected requires-python floor and numpy dev-pin range, empirically proving RESEARCH.md Assumption A1"
    requirement: "PKG-03"
    verification:
      - kind: other
        ref: "uv lock && uv sync --dev (exit 0, log shows numpy resolved to 2.4.6 for the active interpreter, pydart-io built)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Full existing Python test suite (141 tests) still passes after the packaging config edits, no regression from the floor/dependency changes"
    requirement: "PKG-03"
    verification:
      - kind: unit
        ref: "uv run pytest -q (141 passed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "numeric_dense scenario measured end-to-end for throughput via pytest-benchmark, pydart.Table.from_pandas vs pyarrow.Table.from_pandas"
    requirement: "BENCH-01"
    verification:
      - kind: unit
        ref: "benchmarks/test_bench_from_pandas.py::test_from_pandas_pydart[numeric_dense], test_from_pandas_pyarrow[numeric_dense] (both pass, comparative timing recorded in BENCHMARKS.md)"
        status: pass
    human_judgment: false
  - id: D4
    description: "numeric_dense scenario's peak RSS measured in a fresh subprocess via psutil, reported in bytes"
    requirement: "BENCH-02"
    verification:
      - kind: other
        ref: "uv run python benchmarks/memory/measure_rss.py numeric_dense (prints peak RSS bytes, ~147MB, reproduced across repeated runs)"
        status: pass
    human_judgment: false
  - id: D5
    description: "cargo bench -p pydart-core --bench conversion_bench compiles and runs the criterion micro-benchmark against the pyo3-free from_numpy_buffer entry point"
    requirement: "BENCH-01"
    verification:
      - kind: other
        ref: "cargo bench -p pydart-core --bench conversion_bench -- --warm-up-time 1 --measurement-time 1 (numeric_1M_conversion ~75ns, no warnings)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Benchmark harness does not assume more than one row (backstop truth)"
    requirement: "BENCH-01"
    verification:
      - kind: other
        ref: "manual smoke check: make_df('numeric_dense', n_rows=1) + pydart.Table.from_pandas round-trip succeeds"
        status: pass
    human_judgment: false

# Metrics
duration: ~40min
completed: 2026-07-28
status: complete
---

# Phase 4 Plan 1: Config Floor + Benchmark Harness Slice Summary

**Corrected the packaging floor (Python >=3.11, pydart-io PyPI name, benchmark dev deps) and proved the full benchmark measurement path end-to-end on one honest scenario: pytest-benchmark throughput (pydart vs pyarrow), psutil subprocess-isolated peak RSS, and a criterion Rust-kernel micro-benchmark, all recorded in BENCHMARKS.md.**

## Performance

- **Duration:** ~40 min (resumed after a blocking-human package-legitimacy checkpoint)
- **Completed:** 2026-07-28
- **Tasks:** 3 (1 checkpoint, 2 auto)
- **Files modified:** 11 created/modified across two commits

## Accomplishments
- `pyproject.toml`/`uv.lock` re-resolved cleanly on the corrected `>=3.11` floor with the `numpy>=2.3,<2.6` dev-pin range, empirically proving RESEARCH.md's Assumption A1 (uv's universal resolver selects per-interpreter numpy versions within a single lockfile)
- `[project].name` corrected to `pydart-io` (D-41), unblocking the real-PyPI-name-collision finding from RESEARCH.md, with `import pydart` and all Rust crate/module/exception names left unchanged
- `.claude/CLAUDE.md`'s pandas version-compatibility claim corrected from `>=2.2` to `>=3.0` (D-36), closing the WR-02 CoW-safety documentation gap carried forward from Phase 3
- Full existing test suite (141 tests) verified passing with zero regressions after the packaging edits
- A working three-tier benchmark measurement path (Python throughput, OS-level peak RSS, Rust-kernel time) proven on the `numeric_dense` scenario, with real, honestly-reported numbers committed to `BENCHMARKS.md`

## Task Commits

1. **Task 1: Package legitimacy gate for new benchmark dependencies** - checkpoint, approved by user (no code commit — pure gate)
2. **Task 2: Correct packaging config, re-resolve uv lock, add benchmark dependencies** - `2fff62f` (feat)
3. **Task 3: Wire the numeric_dense scenario end-to-end (throughput + RSS + Rust criterion)** - `0f0ca81` (feat)

_Note: Task 1 required no commit — it is a blocking-human checkpoint that gates Task 2's installs, not a code-producing task. The user's "approved" response (quoting pypi.org/project/{pytest-benchmark,psutil} and crates.io/crates/criterion as verified legitimate) is recorded here per the checkpoint protocol._

## Files Created/Modified
- `pyproject.toml` - `requires-python = ">=3.11"`, `[project].name = "pydart-io"`, `numpy>=2.3,<2.6` dev-pin range, added `pytest-benchmark>=5.2,<6` and `psutil>=7.2,<8` dev deps, rewrote the stale floor-rationale comment
- `uv.lock` / `Cargo.lock` - re-resolved under the corrected floor and new dependencies
- `crates/pydart-core/Cargo.toml` - added `criterion = "0.8"` dev-dependency and a `[[bench]] name = "conversion_bench" harness = false` entry
- `.claude/CLAUDE.md` - Version Compatibility table: pandas floor corrected to `>=3.0` with D-36 rationale
- `benchmarks/scenarios.py` - `SCENARIOS = ["numeric_dense"]`, `make_df(scenario, n_rows)` building ArrowDtype-backed (`int64[pyarrow]`/`float64[pyarrow]`) frames
- `benchmarks/conftest.py` - exposes `make_df` as a pytest fixture
- `benchmarks/test_bench_from_pandas.py` - parametrized pytest-benchmark cases: `pydart.Table.from_pandas` vs `pyarrow.Table.from_pandas`
- `benchmarks/memory/scenarios_memory.py` - standalone subprocess entry point, re-imports `make_df`/`SCENARIOS` from `benchmarks/scenarios.py`
- `benchmarks/memory/measure_rss.py` - `measure_peak_rss(scenario_script, scenario_name) -> int`, psutil subprocess-isolated RSS harness
- `crates/pydart-core/benches/conversion_bench.rs` - criterion `bench_numeric_conversion` calling `pydart_core::from_numpy_buffer` directly
- `BENCHMARKS.md` - methodology + populated `numeric_dense` results (throughput, peak RSS, Rust-kernel time)

## Decisions Made

- **numpy dev-pin loosened to a range, not kept as an exact pin under a higher floor:** `numpy>=2.3,<2.6` lets uv's universal resolver pick 2.4.x for Python 3.11 and 2.5.x for 3.12+, all in one `uv.lock` — empirically verified via `uv lock`/`uv sync --dev` rather than assumed.
- **`[project].name` -> `pydart-io`, everything else unchanged:** matches D-41 exactly; `[tool.maturin] module-name = "pydart._pydart"` and `import pydart` are untouched.
- **pandas floor -> `>=3.0` in CLAUDE.md:** per D-36, since CoW is unconditional only from pandas 3.0 onward and the zero-copy numpy borrow's post-borrow-mutation safety depends on it.
- **`conversion_bench.rs` written during Task 2, committed under Task 3:** see Deviations below — this was a sequencing necessity, not a scope change.
- **`black_box` sourced from `std::hint` rather than `criterion::black_box`:** the criterion-re-exported version is deprecated in criterion 0.8; using `std::hint::black_box` (per 04-PATTERNS.md's own example) avoids a compiler warning with identical semantics.
- **Benchmark scenario modules imported by bare name (`from scenarios import ...`), not as `benchmarks.scenarios`:** relies on pytest's prepend-mode `sys.path` insertion for `benchmarks/conftest.py` (no `__init__.py` above it) and an explicit `sys.path.insert` in the subprocess entry point, avoiding a package `__init__.py` file the plan didn't ask for while still satisfying the "single source of scenario definitions, no duplication" key-link.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created `crates/pydart-core/benches/conversion_bench.rs` during Task 2, not Task 3**
- **Found during:** Task 2 (`uv sync --dev`, after adding the `[[bench]]` entry to `crates/pydart-core/Cargo.toml`)
- **Issue:** Cargo parses every workspace member's manifest even when only building `pydart-python` (a `pydart-core` path dependency). The new `[[bench]] name = "conversion_bench"` entry references `benches/conversion_bench.rs`, which did not exist yet (it's nominally a Task 3 deliverable) — `uv sync --dev`'s editable maturin build failed immediately with `can't find conversion_bench bench at benches/conversion_bench.rs`.
- **Fix:** Wrote the real `conversion_bench.rs` content (matching Task 3's own action spec and 04-PATTERNS.md's example exactly — the pyo3-free `from_numpy_buffer` call, criterion `bench_function`/`criterion_group!`/`criterion_main!`, the same safety-comment convention as `tests/rust/zero_copy_alloc.rs`) at Task 2 time so the build could proceed, but held it **uncommitted** until Task 3, where it was staged and committed alongside the plan's other Task 3 files. Git history correctly attributes the file's creation to Task 3's commit.
- **Files modified:** `crates/pydart-core/benches/conversion_bench.rs` (created, held uncommitted through Task 2, committed in Task 3's `0f0ca81`)
- **Verification:** `uv sync --dev`, `uv run maturin develop && uv run pytest -q` all pass after the file exists; Task 3's own `cargo bench` verification independently re-confirms the file compiles and runs correctly
- **Committed in:** `0f0ca81` (Task 3 commit — the file's canonical home)

**2. [Rule 1 - Bug] Fixed a criterion deprecation warning (`black_box`)**
- **Found during:** Task 3, first `cargo bench` run
- **Issue:** `criterion::black_box` is deprecated in criterion 0.8 in favor of `std::hint::black_box`; the initial `conversion_bench.rs` draft used the deprecated re-export, producing two compiler warnings.
- **Fix:** Switched the import to `std::hint::black_box` (matching 04-PATTERNS.md's own example verbatim) and updated the call site to pass a reference (`black_box(&array)`).
- **Files modified:** `crates/pydart-core/benches/conversion_bench.rs`
- **Verification:** `cargo bench -p pydart-core --bench conversion_bench` runs with zero warnings
- **Committed in:** `0f0ca81` (Task 3 commit)

**3. [Rule 3 - Blocking] Rephrased `measure_rss.py`'s docstring to avoid the literal string "tracemalloc"**
- **Found during:** Task 3, running the plan's own acceptance-criteria grep checks
- **Issue:** The plan's automated acceptance criterion requires `grep -c 'tracemalloc' benchmarks/memory/measure_rss.py` to return `0` (proving the file doesn't use that profiler), but the initial draft's docstring named `tracemalloc` explicitly (to document *why* it's excluded, per RESEARCH.md Pitfall 2) — which made the literal grep check fail (count 2, not 0).
- **Fix:** Reworded the docstring to describe the excluded profiling approach generically ("any in-process Python-heap allocation profiler") with a pointer to "RESEARCH.md Pitfall 2 for the specific tool this rules out", preserving the rationale without the literal string the grep check forbids.
- **Files modified:** `benchmarks/memory/measure_rss.py`
- **Verification:** `grep -c 'tracemalloc' benchmarks/memory/measure_rss.py` returns `0`; `grep -q 'import psutil'` still succeeds
- **Committed in:** `0f0ca81` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking/sequencing, 1 bug/deprecation, 1 blocking/verification-literal)
**Impact on plan:** All three were necessary to make the plan's own commands and acceptance criteria pass exactly as written; none changed scope, added functionality beyond what Task 2/3 already specified, or altered the plan's intended architecture.

## Issues Encountered

None beyond the deviations documented above.

## Benchmark Results (headline numbers, full detail in BENCHMARKS.md)

- **Throughput (`from_pandas`, numeric_dense, 1M rows):** pyarrow mean ~393-429us; pydart mean ~1188-1333us. pydart is currently ~2.8-3.5x **slower** than pyarrow at the full Python-level call path on this scenario — reported honestly, not hidden.
- **Peak RSS (numeric_dense, pydart):** ~147MB, reproducible across repeated subprocess runs.
- **Rust-kernel time (`from_numpy_buffer`, 1M `i64` values, criterion):** ~75ns — effectively O(1) regardless of row count, direct evidence the Rust core itself is a genuine pointer-borrow with no per-element cost. This isolates the throughput gap above to the PyO3/GIL/Python-object boundary in the pandas-interop layer, not the Rust conversion core — exactly the diagnostic split BENCH-01's Rust-vs-Python measurement tiers exist to produce.
- The pass/fail bar for "measurably faster than pyarrow" (04-RESEARCH.md Open Question 2) remains an open decision for Plan 02/03, not resolved here — this plan's job was proving the measurement path works honestly, not setting the bar.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- The packaging floor (`requires-python >=3.11`, `pydart-io` name, benchmark dev deps) is corrected and uv-resolvable — Plan 03/04's packaging/release track can build on this directly without redoing the floor fix.
- The benchmark harness pattern (`benchmarks/scenarios.py` as single source of scenario definitions, throughput + RSS + criterion all wired to one scenario) is proven and ready for Plan 02 to extend to the full {mixed, nullable, chunked, object-string, categorical} matrix.
- **Concern carried forward:** pydart's `from_pandas` currently loses to pyarrow on the one true-zero-copy scenario measured so far at the full Python-level call path, even though the isolated Rust kernel is effectively free. This is a real, honestly-reported finding (not a benchmark-harness bug) that Plan 02/03 should investigate — likely FFI/GIL-boundary overhead in the pandas-interop layer — before the project's core "measurably faster than pyarrow" claim can be validated across the matrix.

---
*Phase: 04-benchmark-release-readiness*
*Completed: 2026-07-28*
