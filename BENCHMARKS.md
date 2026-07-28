# pydart Benchmarks

This file is committed to the repo and regenerated per release (D-38) -- it is the canonical,
in-repo record of pydart's speed and memory claims against pyarrow. No external CI-service
benchmark dashboard is used for v1.

## Methodology

- **Data:** All benchmark scenarios use synthetic, programmatically generated data (D-39) -- no
  downloaded public/real-world dataset. Scenario shapes and row counts are documented in
  `benchmarks/scenarios.py`.
- **Throughput (Python-level, user-visible call path):** [`pytest-benchmark`](https://pypi.org/project/pytest-benchmark/)
  times the actual public API call (`pydart.Table.from_pandas` / `pyarrow.Table.from_pandas`),
  including FFI/GIL overhead -- this is the number a real caller experiences.
- **Peak memory (RSS):** [`psutil`](https://pypi.org/project/psutil/), measured in a **fresh
  subprocess per scenario** (`benchmarks/memory/measure_rss.py` + `benchmarks/memory/scenarios_memory.py`).
  A whole-process OS-level metric is required because pydart's data lives in Rust-owned Arrow
  buffers outside the Python heap -- an in-process Python-heap allocation profiler would report
  near-zero for exactly the memory this measures (RESEARCH.md Pitfall 2). A fresh subprocess per
  scenario also avoids a prior scenario's retained allocator arena contaminating the next
  scenario's peak reading.
- **Rust-kernel time (pure conversion, no PyO3/GIL):** [`criterion`](https://crates.io/crates/criterion)
  micro-benchmarks (`crates/pydart-core/benches/conversion_bench.rs`) time the pyo3-free
  `pydart_core::from_numpy_buffer` entry point directly. This isolates whether any slowness lives
  in the Rust core itself versus at the PyO3/GIL boundary.
- **Reporting convention:** every scenario is reported as its own row -- never blended into a
  single aggregate "N times faster" headline number (RESEARCH.md Pitfall 1). This phase's Plan 01
  populates one scenario (`numeric_dense`); Plan 02 broadens the matrix to
  {mixed, nullable, chunked, object-string, categorical}, and the pass/fail bar for "measurably
  faster than pyarrow" (Open Question 2 in 04-RESEARCH.md) is decided at that point, not implied
  here.

## Results

### Throughput: `from_pandas`, numeric_dense (1,000,000 rows, `int64[pyarrow]` + `float64[pyarrow]`)

| Implementation | Min (us) | Mean (us) | Max (us) |
|-----------------|---------:|----------:|---------:|
| `pyarrow.Table.from_pandas` | 295.3 | 428.6 | 811.9 |
| `pydart.Table.from_pandas` | 1,038.2 | 1,187.8 | 1,814.6 |

pydart is currently **slower** than pyarrow on this scenario at the full Python-level call path
(~2.8x mean). This is reported honestly, not hidden: `numeric_dense` is a true zero-copy-eligible
scenario (both columns are `ArrowDtype`-backed), so the gap here is FFI/GIL-boundary overhead in
`pydart`'s pandas-interop layer, not a Rust-core conversion cost -- see the Rust-kernel result
below, which isolates the two. Closing this gap (or determining it's an acceptable tradeoff) is
tracked as follow-up work for Plan 02/03, not resolved in this plan.

### Peak RSS: `numeric_dense` (1,000,000 rows), pydart, subprocess-isolated

| Scenario | Peak RSS (bytes) |
|----------|------------------:|
| `numeric_dense` (pydart `from_pandas` + `to_pandas`) | ~147,000,000 (147.0-147.3 MB across repeated runs) |

### Rust-kernel time: `from_numpy_buffer`, numeric_1M_conversion (criterion)

| Benchmark | Time |
|-----------|------|
| `numeric_1M_conversion` (1,000,000 `i64` values) | ~75 ns |

This ~75ns figure is effectively O(1) regardless of the 1,000,000-element input -- direct evidence
that the underlying Rust conversion is a genuine pointer/metadata-only borrow (no per-element copy
cost), and that the throughput gap reported above lives entirely at the PyO3/GIL/Python-object
boundary, not in the Rust core.

## Known Gaps / Placeholders

- Only the `numeric_dense` scenario is measured in this plan (04-01). The full scenario matrix
  ({mixed, nullable, chunked, object-string, categorical} x {from_pandas, to_pandas, read_parquet,
  write_parquet}) lands in Plan 02.
- The categorical/dictionary Parquet-IO scenario (when added in Plan 02) must carry the D-40/T-03-09
  caveat inline: `.cat.categories` order and unused-category retention do NOT survive a Parquet
  round-trip (values and `dict_is_ordered` DO survive correctly).
- The pass/fail bar for "measurably faster than pyarrow" (04-RESEARCH.md Open Question 2) is not
  yet decided -- this file reports raw numbers honestly per scenario until that bar is set.
