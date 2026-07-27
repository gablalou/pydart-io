# Phase 4: Benchmark & Release Readiness - Pattern Map

**Mapped:** 2026-07-27
**Files analyzed:** 14 (7 new benchmark/doc files, 4 new CI workflow files, 3 modified config files)
**Analogs found:** 9 / 14 (5 have no in-repo analog — greenfield CI/docs, documented below)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|----------------|
| `benchmarks/conftest.py` | test-fixture/config | batch (synthetic data generation) | `tests/python/test_categorical.py`, `test_object_string.py`, `test_nulls.py` (fixture-building helpers, e.g. `_numeric_arrow_dtype_frame()`) | role-match |
| `benchmarks/scenarios.py` | utility (data generators) | transform | Same test files above — per-scenario `pd.DataFrame({...})` construction patterns | role-match |
| `benchmarks/test_bench_from_pandas.py` | test | request-response (timed call) | `tests/python/test_round_trip.py`, `test_categorical.py` (call `pydart.Table.from_pandas`, assert on result) | role-match |
| `benchmarks/test_bench_to_pandas.py` | test | request-response (timed call) | `tests/python/test_round_trip.py` | role-match |
| `benchmarks/test_bench_parquet_io.py` | test | file-I/O | `tests/python/test_parquet_roundtrip.py` | exact (same read/write pattern, now timed not just asserted) |
| `benchmarks/memory/measure_rss.py` | utility (subprocess harness) | event-driven (subprocess poll) | No analog — new tier (OS-process measurement); no existing subprocess-orchestration code in repo | no analog |
| `benchmarks/memory/scenarios_memory.py` | utility (data generators) | transform | Same as `benchmarks/scenarios.py` — reuse pattern | role-match |
| `crates/pydart-core/benches/conversion_bench.rs` | test (Rust micro-bench) | transform | `tests/rust/zero_copy_alloc.rs` (calls `pydart_core::from_numpy_buffer` directly, no PyO3) | role-match |
| `Cargo.toml` / `crates/pydart-core/Cargo.toml` (modified — add `criterion` dev-dep + `[[bench]]`) | config | — | Existing `[dev-dependencies]` + `[[test]]` entries in `crates/pydart-core/Cargo.toml` | exact |
| `pyproject.toml` (modified — requires-python, pandas floor, numpy dev-pin, new dev deps) | config | — | Existing `[project]`/`[dependency-groups]` blocks in same file | exact |
| `.github/workflows/ci.yml` | config (CI) | event-driven | none — greenfield | no analog |
| `.github/workflows/compat-matrix.yml` | config (CI) | event-driven / batch | none — greenfield | no analog |
| `.github/workflows/wheels.yml` | config (CI) | event-driven / batch | none — greenfield | no analog |
| `.github/workflows/release.yml` | config (CI) | event-driven | none — greenfield | no analog |
| `BENCHMARKS.md` | doc | — | none — new doc genre (no existing top-level `.md` doc with a data-table format to mirror in-repo; RESEARCH.md's own table format is the best structural reference) | no analog (use RESEARCH.md's own Markdown table conventions as the closest structural precedent) |

## Pattern Assignments

### `benchmarks/conftest.py` + `benchmarks/scenarios.py` (utility/fixture, batch)

**Analog:** `tests/python/test_categorical.py`, `tests/python/test_nulls.py`, `tests/python/test_parquet_roundtrip.py`

**Scenario-frame builder pattern** (`tests/python/test_parquet_roundtrip.py` lines 16-22):
```python
def _numeric_arrow_dtype_frame() -> pd.DataFrame:
    return pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
        }
    )
```
Reuse this exact shape for `benchmarks/scenarios.py`'s `make_df(scenario, n_rows)` — each scenario should build ArrowDtype-backed frames (the project's true zero-copy path) explicitly, mirroring how existing tests distinguish `int64[pyarrow]` (zero-copy path) from masked `Int64`/plain numpy (copy/rejected paths).

**Nullable-scenario pattern** (`tests/python/test_nulls.py` lines 27-34):
```python
def test_nullable_arrow_dtype_int_round_trips_with_nulls_preserved():
    df = pd.DataFrame({"a": pd.array([1, None, 3], dtype="int64[pyarrow]")})
    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()
```
Use this exact `pd.array([...], dtype="int64[pyarrow]")` null-injection idiom for the `numeric_nullable` benchmark scenario.

**Categorical scenario pattern** (`tests/python/test_categorical.py` lines 34-48, 82-96):
```python
source = pd.Categorical(["b", "a", "c", "a"], categories=["c", "b", "a"], ordered=True)
df = pd.DataFrame({"cat": source})
table = pydart.Table.from_pandas(df)
result = table.to_pandas()
```
For a large-cardinality `categorical` benchmark scenario, mirror the `>255-category` int16-code-width fixture (lines 82-96) — this is also the scenario the D-40 Parquet caveat applies to; benchmark generation code for the Parquet-IO categorical scenario should include an inline comment referencing D-40/T-03-09 so the benchmark file itself documents the known non-round-trip-of-category-order/unused-categories limitation at the point of use, not just in `BENCHMARKS.md`.

**Docstring/attribution convention** (every existing test file, e.g. `test_categorical.py` lines 1-26): module-level docstring names the requirement ID (`CONV-05`, `PARQ-01`) and the specific decisions it proves (`D-17`, `D-18`). New benchmark files should follow the same convention, citing `BENCH-01`/`BENCH-02` and the relevant D-38/D-39/D-40.

---

### `benchmarks/test_bench_from_pandas.py` / `test_bench_to_pandas.py` (test, request-response)

**Analog:** `tests/python/test_round_trip.py`, `tests/python/test_categorical.py`

**Core call pattern** (consistent across all `tests/python/*.py` files):
```python
import pandas as pd
import pydart

table = pydart.Table.from_pandas(df)
result = table.to_pandas()
```
Wrap this exact call (not a reimplemented conversion) inside `benchmark(...)` per Pattern 1 from RESEARCH.md:
```python
@pytest.mark.parametrize("scenario", SCENARIOS)
def test_from_pandas_pydart(benchmark, scenario, make_df):
    df = make_df(scenario, n_rows=1_000_000)
    benchmark(pydart.from_pandas, df)
```
Note: existing tests call `pydart.Table.from_pandas(df)` (classmethod on `Table`, per D-19 — no module-level `pydart.from_pandas`/`read_parquet`/`write_parquet` functions exist). RESEARCH.md's own Pattern 1 code example uses `pydart.from_pandas` — **this is stale/wrong relative to the actual codebase API**; the planner/executor must call `pydart.Table.from_pandas(df)` and `table.to_pandas()`, matching the real API in `crates/pydart-python/src/pandas.rs` line 329 and every existing test file.

---

### `benchmarks/test_bench_parquet_io.py` (test, file-I/O)

**Analog:** `tests/python/test_parquet_roundtrip.py`

**Imports pattern** (lines 1-13):
```python
import pandas as pd
import pandas.testing as pdt

import pydart
```

**Core file-I/O pattern** (lines 25-30):
```python
table = pydart.Table.from_pandas(df)
path = tmp_path / "t.parquet"
table.to_parquet(str(path))
```
Benchmark equivalent should use `tmp_path` (pytest fixture) exactly the same way, timing `table.to_parquet(str(path))` and `pydart.Table.from_parquet(str(path))` (D-19 instance/classmethod shape) separately as two benchmark cases, mirroring RESEARCH.md's from_pandas/to_pandas split — matches Architecture Pattern 1's {from_pandas, to_pandas, read_parquet, write_parquet} matrix axis.

---

### `benchmarks/memory/measure_rss.py` (utility, subprocess/event-driven)

**No in-repo analog** — this is a genuinely new capability tier (whole-process OS-level measurement via a subprocess, not a Python-heap profiler). Use RESEARCH.md's own Code Examples section verbatim as the starting skeleton (already reviewed and cited as MEDIUM-confidence standard practice):
```python
import subprocess, sys, json

def measure_peak_rss(scenario_script: str, scenario_name: str) -> int:
    import psutil
    proc = subprocess.Popen([sys.executable, scenario_script, scenario_name])
    p = psutil.Process(proc.pid)
    peak = 0
    while proc.poll() is None:
        try:
            peak = max(peak, p.memory_info().rss)
        except psutil.NoSuchProcess:
            break
    return peak
```
Each scenario needs a standalone runnable script entry point (invoked via `sys.executable`), reusing `benchmarks/scenarios.py`'s `make_df` generators so scenario definitions aren't duplicated between the throughput and memory harnesses (per Recommended Project Structure: `memory/scenarios_memory.py` re-imports/re-exports from `benchmarks/scenarios.py`, does not redefine).

---

### `crates/pydart-core/benches/conversion_bench.rs` (test, transform)

**Analog:** `tests/rust/zero_copy_alloc.rs`

**Direct pyo3-free entry point call pattern** (lines 61-86):
```rust
let ptr = data.as_ptr() as *const u8;
let len = data.len() * std::mem::size_of::<i64>();
let array = unsafe { pydart_core::from_numpy_buffer(ptr, len) };
```
`from_numpy_buffer` (re-exported at `crates/pydart-core/src/lib.rs` line 12: `pub use table::{from_numpy_buffer, Table};`) is the correct pyo3-free entry point to benchmark — same function this existing allocation-counting test already calls, giving the criterion bench a proven-safe call shape to copy directly, replacing `allocation_counter::measure(...)` with `c.bench_function(...)`:
```rust
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn bench_numeric_conversion(c: &mut Criterion) {
    let data: Vec<i64> = (0..1_000_000).collect();
    c.bench_function("numeric_1M_conversion", |b| {
        b.iter(|| {
            let ptr = data.as_ptr() as *const u8;
            let len = data.len() * std::mem::size_of::<i64>();
            let array = unsafe { pydart_core::from_numpy_buffer(ptr, len) };
            black_box(&array);
        })
    });
}

criterion_group!(benches, bench_numeric_conversion);
criterion_main!(benches);
```
Carry over the same safety-comment convention as `zero_copy_alloc.rs` (lines 70-72) documenting why the unsafe pointer/length pair is valid for the closure's lifetime.

**Cargo.toml wiring pattern** (`crates/pydart-core/Cargo.toml`, existing `[dev-dependencies]` + `[[test]]` entries):
```toml
[dev-dependencies]
allocation-counter = "0.8"

[[test]]
name = "zero_copy_alloc"
path = "../../tests/rust/zero_copy_alloc.rs"
```
Add analogously:
```toml
[dev-dependencies]
criterion = "0.8"

[[bench]]
name = "conversion_bench"
harness = false
```
(Benches live at the crate-local `benches/` path per Cargo convention — no `path =` override needed, unlike the `[[test]]` entries which point at the repo-root `tests/rust/` per that crate's own documented deviation.)

---

### `pyproject.toml` (modified, config)

**Analog:** itself — extend existing structure, don't restructure

**Current exact-pin dev-dependency block** (lines 33-42):
```toml
[dependency-groups]
dev = [
    "duckdb==1.5.4",
    "hypothesis==6.156.6",
    "maturin==1.14.1",
    "numpy==2.5.1",
    "pandas==3.0.3",
    "polars==1.42.1",
    "pyarrow==25.0.0",
    "pytest==9.1.1",
]
```
Apply RESEARCH.md's registry-verified fix directly (Pitfall 4 / Code Examples):
```diff
-    "numpy==2.5.1",
+    "numpy>=2.3,<2.6",
     "pandas==3.0.3",
     "polars==1.42.1",
     "pyarrow==25.0.0",
     "pytest==9.1.1",
+    "pytest-benchmark>=5.2,<6",
+    "psutil>=7.2,<8",
```
Also update `requires-python = ">=3.12"` (line 10) to `>=3.11` per D-35, and update the stale comment on lines 6-9 that currently explains the >=3.12 floor (it references the exact `numpy==2.5.1` pin being removed — the comment itself is now factually wrong and must be rewritten, not left in place, since D-04-style diagnostics-honesty applies to docs/comments too, not just runtime errors).
Update `[project].name` per D-41: `name = "pydart"` -> `name = "pydart-io"` (import path/module-name/extension unaffected — `[tool.maturin] module-name = "pydart._pydart"` stays unchanged, only `[project].name` and the PyPI-facing identity change).

**Pandas floor:** no `pandas` version constraint currently exists outside the dev pin (`pandas==3.0.3`) — if a runtime-facing minimum-supported-version declaration is added anywhere (docs, a runtime check), the D-36 floor is `>=3.0`, not `>=2.2`. Cross-check `.claude/CLAUDE.md`'s "pandas >= 2.2" version-compatibility table claim, which also needs updating per D-36 (not a file this agent writes, but flagged for the planner since CONTEXT.md D-36 explicitly calls this out as in-scope).

---

## Shared Patterns

### Diagnostics-honesty / never-hide-a-limitation
**Source:** `crates/pydart-python/src/error.rs` (module-level doc comment, lines 1-6) and every test file's docstring convention (e.g. `test_categorical.py` lines 22-26 documenting the OQ1 decision explicitly rather than silently)
**Apply to:** `BENCHMARKS.md` (D-40's categorical/Parquet caveat), any benchmark scenario code touching categorical Parquet round-trips, and release docs generally
```rust
// Pattern: name the specific limitation, don't generalize it away.
/// A `from_parquet` multi-file/directory read (D-21) where two files' Arrow schemas
/// disagree. Carries the first file, the first mismatched file, and the first differing
/// column name so the raised exception is directly actionable...
```
`BENCHMARKS.md`'s categorical-scenario section must name the exact limitation (category order + unused categories don't survive Parquet round-trip; values + `dict_is_ordered` do) with the same specificity this project already applies to its Rust error variants — not a vague "known limitations may apply" footnote.

### Test/benchmark docstring attribution convention
**Source:** every file in `tests/python/*.py` (module-level docstring citing requirement IDs and decision IDs, e.g. `test_categorical.py` lines 1-26, `test_nulls.py` lines 1-16)
**Apply to:** all new `benchmarks/*.py` files and `crates/pydart-core/benches/conversion_bench.rs`
```python
"""CONV-05: categorical dtype round-trip fidelity (D-17, D-18).
...
"""
```
New benchmark files should open with a docstring citing `BENCH-01`/`BENCH-02` and the specific D-38/D-39/D-40 decisions the file's scenario matrix satisfies — keeps the same traceability convention this codebase already relies on throughout `tests/`.

### ArrowDtype-backed frame construction (the "true zero-copy" scenario shape)
**Source:** `tests/python/test_parquet_roundtrip.py` lines 16-22, `tests/python/test_nulls.py` lines 27-34, `tests/python/test_categorical.py` lines 34-48
**Apply to:** `benchmarks/scenarios.py`, `benchmarks/memory/scenarios_memory.py`
```python
pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
```
All "true zero-copy" benchmark scenarios (numeric_dense, numeric_nullable, chunked_multi_batch) must use `dtype="...[pyarrow]"` construction, exactly like every existing correctness test does — using plain numpy-backed columns instead would silently benchmark the wrong (copy-fallback) code path and produce a misleading BENCH-01 result, directly contradicting Pitfall 1's "blended claim" warning.

## No Analog Found

Files with no close match in the codebase (planner should rely on RESEARCH.md's Architecture Patterns / Code Examples instead):

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `benchmarks/memory/measure_rss.py` | utility | event-driven (subprocess) | No existing subprocess-orchestration code anywhere in the repo; use RESEARCH.md Code Examples verbatim as the base |
| `.github/workflows/ci.yml` | config | event-driven | `.github/` does not exist yet (confirmed via `ls`) — fully greenfield; use RESEARCH.md Architecture Pattern 2/3 YAML skeletons |
| `.github/workflows/compat-matrix.yml` | config | batch | Same — greenfield |
| `.github/workflows/wheels.yml` | config | batch | Same — greenfield; RESEARCH.md Pattern 2 gives the exact matrix `include:` block to copy |
| `.github/workflows/release.yml` | config | event-driven | Same — greenfield; RESEARCH.md Pattern 3 gives the exact OIDC publish job to copy |
| `BENCHMARKS.md` | doc | — | No existing committed benchmark-results doc in repo; structure per RESEARCH.md's own scenario-matrix table conventions (mirror this PATTERNS.md's / RESEARCH.md's own Markdown table style for consistency with the rest of `.planning/`) |

## Metadata

**Analog search scope:** `tests/python/`, `tests/rust/`, `crates/pydart-core/src/`, `crates/pydart-python/src/`, root `pyproject.toml`/`Cargo.toml`, `.github/` (confirmed absent)
**Files scanned:** ~20 (all `tests/python/*.py`, all `tests/rust/*.rs`, both `Cargo.toml` manifests, `pyproject.toml`, `crates/pydart-core/src/lib.rs`, `crates/pydart-python/src/error.rs`)
**Pattern extraction date:** 2026-07-27
