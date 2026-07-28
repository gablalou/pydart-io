# pydart Benchmarks

This file is committed to the repo and regenerated per release (D-38) -- it is the canonical,
in-repo record of pydart's speed and memory claims against pyarrow. No external CI-service
benchmark dashboard is used for v1.

## Methodology

- **Data:** All benchmark scenarios use synthetic, programmatically generated data (D-39) -- no
  downloaded public/real-world dataset. Scenario shapes and row counts are documented in
  `benchmarks/scenarios.py`.
- **Throughput (Python-level, user-visible call path):** [`pytest-benchmark`](https://pypi.org/project/pytest-benchmark/)
  times the actual public API call (`pydart.Table.from_pandas`/`to_pandas`/`to_parquet`/
  `pydart.Table.from_parquet`, vs the pyarrow equivalents), including FFI/GIL overhead -- this is
  the number a real caller experiences. Every scenario x axis cell is timed for both
  implementations (`benchmarks/test_bench_from_pandas.py`, `test_bench_to_pandas.py`,
  `test_bench_parquet_io.py`).
- **Peak memory (RSS):** [`psutil`](https://pypi.org/project/psutil/), measured in a **fresh
  subprocess per scenario/axis/implementation cell** (`benchmarks/memory/measure_rss.py` +
  `benchmarks/memory/scenarios_memory.py`). A whole-process OS-level metric is required because
  pydart's data lives in Rust-owned Arrow buffers outside the Python heap -- an in-process
  Python-heap allocation profiler would report near-zero for exactly the memory this measures
  (RESEARCH.md Pitfall 2). A fresh subprocess per cell also avoids a prior run's retained
  allocator arena contaminating the next reading. `scenarios_memory.py` accepts both an `impl`
  (`pydart`/`pyarrow`) and an `axis` (`from_pandas`/`to_pandas`/`write_parquet`/`read_parquet`)
  argument so every matrix cell -- not just the pandas<->Arrow round trip -- reports peak RSS for
  both implementations (BENCH-02's per-cell requirement).
- **Rust-kernel time (pure conversion, no PyO3/GIL):** [`criterion`](https://crates.io/crates/criterion)
  micro-benchmarks (`crates/pydart-core/benches/conversion_bench.rs`) time the pyo3-free
  `pydart_core::from_numpy_buffer` entry point directly. This isolates whether any slowness lives
  in the Rust core itself versus at the PyO3/GIL boundary. Unchanged from Plan 01 -- no Rust-core
  code was modified in this plan.
- **Zero-copy / copy-fallback labels:** driven by pydart's own `Table.copy_report()` API, captured
  empirically for every scenario -- not assumed from the scenario's name or intent. See "Scenario
  Shapes & Empirical Zero-Copy Status" below; one scenario's empirical status **disagrees** with
  this plan's original grouping (see Known Limitations).
- **Reporting convention:** every scenario x axis cell is reported as its own row -- never blended
  into a single aggregate "N times faster" headline number (RESEARCH.md Pitfall 1).

## Scenario Shapes & Empirical Zero-Copy Status

| Scenario | Shape (see `benchmarks/scenarios.py`) | `copy_report()` result (`from_pandas`) | Plan's bar group | Status |
|----------|----------------------------------------|------------------------------------------|-------------------|--------|
| `numeric_dense` | `int64[pyarrow]` + `float64[pyarrow]`, no nulls, 1,000,000 rows | `zero_copy=True` (both columns) | true zero-copy | Agrees |
| `numeric_nullable` | `int64[pyarrow]` + `float64[pyarrow]`, 1-in-10 rows null | `zero_copy=True` (both columns) | true zero-copy | Agrees |
| `mixed_object_string` | `int64[pyarrow]` + legacy numpy `object` string column (1-in-7 null) | `zero_copy=True` (numeric col), `zero_copy=False` (object col, D-10 copy) | copy-fallback | Agrees (mixed frame is not fully zero-copy) |
| `chunked_multi_batch` | `int64[pyarrow]` built via `pd.concat` of two Arrow-backed frames (genuine 2-chunk `ChunkedArray`) | **`zero_copy=False`** -- `from_pandas` runs `arrow::compute::concat` on multi-chunk columns (CR-01/CONV-08), a real copy | true zero-copy | **Disagrees -- see Known Limitations** |
| `categorical_ordered` | Ordered `pd.Categorical`, 50 categories, 1,000,000 rows | `zero_copy=False` (categorical reconstruction copy, OQ1) | copy-fallback | Agrees |
| `categorical_unordered` | Unordered `pd.Categorical`, 300 categories (int16 code width), 1,000,000 rows | `zero_copy=False` (categorical reconstruction copy, OQ1) | copy-fallback | Agrees |

## Results Matrix

All times are pytest-benchmark's Min/Mean in milliseconds (5 rounds minimum, calibrated). All
peak RSS values are psutil-measured, subprocess-isolated per implementation and axis, reported in
megabytes (bytes / 1,000,000). "Ratio" is pydart-time / pyarrow-time -- values > 1 mean pydart is
that many times **slower**; values shown as "Nx faster" mean pydart is faster.

### Axis: `from_pandas`

| Scenario | pyarrow Min/Mean (ms) | pydart Min/Mean (ms) | Ratio | pyarrow Peak RSS (MB) | pydart Peak RSS (MB) | RSS Diff |
|----------|------------------------|------------------------|-------|--------------------------|--------------------------|----------|
| `numeric_dense` | 0.2830 / 0.3993 | 1.0781 / 1.2943 | 3.24x slower | 137.28 | 147.01 | pydart +9.73 MB |
| `numeric_nullable` | 0.3044 / 0.4163 | 1.0595 / 1.3019 | 3.13x slower | 206.30 | 206.19 | pydart -0.11 MB |
| `mixed_object_string` | 25.1827 / 27.3533 | 332.9569 / 348.1594 | 12.73x slower | 221.13 | 230.76 | pydart +9.63 MB |
| `chunked_multi_batch` | 0.0398 / 0.0498 | 0.7542 / 0.9539 | 19.16x slower | 123.77 | 142.66 | pydart +18.89 MB |
| `categorical_ordered` | 1.0174 / 1.2148 | 1.4532 / 1.7852 | 1.47x slower | 211.13 | 211.22 | pydart +0.09 MB |
| `categorical_unordered` | 1.0727 / 1.2853 | 1.4726 / 1.7949 | 1.40x slower | 212.36 | 212.16 | pydart -0.19 MB |

### Axis: `to_pandas`

| Scenario | pyarrow Min/Mean (ms) | pydart Min/Mean (ms) | Ratio | pyarrow Peak RSS (MB) | pydart Peak RSS (MB) | RSS Diff |
|----------|------------------------|------------------------|-------|--------------------------|--------------------------|----------|
| `numeric_dense` | 0.1708 / 0.2496 | 0.2271 / 0.3265 | 1.31x slower | 137.89 | 147.15 | pydart +9.26 MB |
| `numeric_nullable` | 0.1684 / 0.2897 | 0.2141 / 0.3083 | 1.06x slower | 206.11 | 206.20 | pydart +0.09 MB |
| `mixed_object_string` | 0.5017 / 0.6611 | 0.2383 / 0.3386 | **1.95x faster** | 223.55 | 231.18 | pydart +7.63 MB |
| `chunked_multi_batch` | 0.1593 / 0.2412 | 0.2217 / 0.3214 | 1.33x slower | 124.38 | 143.40 | pydart +19.02 MB |
| `categorical_ordered` | 0.6710 / 0.8468 | 0.7274 / 0.9339 | 1.10x slower | 211.41 | 211.24 | pydart -0.17 MB |
| `categorical_unordered` | 0.7581 / 0.9507 | 0.8409 / 1.0469 | 1.10x slower | 212.00 | 212.02 | pydart +0.02 MB |

### Axis: `write_parquet` (`table.to_parquet`)

| Scenario | pyarrow Min/Mean (ms) | pydart Min/Mean (ms) | Ratio | pyarrow Peak RSS (MB) | pydart Peak RSS (MB) | RSS Diff |
|----------|------------------------|------------------------|-------|--------------------------|--------------------------|----------|
| `numeric_dense` | 38.4229 / 41.3891 | 539.3104 / 555.2970 | 13.42x slower | 194.95 | 168.19 | **pydart -26.76 MB** |
| `numeric_nullable` | 43.8915 / 53.1805 | 558.5700 / 583.9608 | 10.98x slower | 207.14 | 205.99 | pydart -1.15 MB |
| `mixed_object_string` | 50.2560 / 56.3757 | 734.4986 / 747.6849 | 13.26x slower | 258.63 | 254.15 | pydart -4.47 MB |
| `chunked_multi_batch` | 17.6106 / 22.5622 | 282.4205 / 290.6570 | 12.88x slower | 164.51 | 160.60 | pydart -3.92 MB |
| `categorical_ordered` | 7.6009 / 9.1467 | 381.9095 / 390.4270 | 42.68x slower | 211.24 | 211.24 | ~equal |
| `categorical_unordered` | 14.7496 / 17.5471 | 373.5495 / 387.4822 | 22.08x slower | 212.34 | 212.40 | pydart +0.06 MB |

### Axis: `read_parquet` (`Table.from_parquet`)

| Scenario | pyarrow Min/Mean (ms) | pydart Min/Mean (ms) | Ratio | pyarrow Peak RSS (MB) | pydart Peak RSS (MB) | RSS Diff |
|----------|------------------------|------------------------|-------|--------------------------|--------------------------|----------|
| `numeric_dense` | 10.6455 / 11.9013 | 193.1126 / 201.0819 | 16.90x slower | 205.34 | 182.63 | **pydart -22.71 MB** |
| `numeric_nullable` | 10.4418 / 11.8492 | 227.2098 / 235.9004 | 19.91x slower | 213.69 | 206.10 | pydart -7.59 MB |
| `mixed_object_string` | 17.4877 / 20.1618 | 325.2959 / 328.2674 | 16.28x slower | 292.42 | 279.67 | pydart -12.75 MB |
| `chunked_multi_batch` | 9.7792 / 10.8149 | 94.5805 / 99.1488 | 9.17x slower | 171.95 | 160.53 | pydart -11.42 MB |
| `categorical_ordered` | 3.3980 / 3.7673 | 91.1468 / 97.1785 | 25.80x slower | 211.10 | 211.26 | pydart +0.16 MB |
| `categorical_unordered` | 3.0186 / 3.6104 | 107.7870 / 115.5180 | 32.00x slower | 212.11 | 212.16 | pydart +0.05 MB |

### Rust-kernel time: `from_numpy_buffer`, numeric_1M_conversion (criterion, unchanged from Plan 01)

| Benchmark | Time |
|-----------|------|
| `numeric_1M_conversion` (1,000,000 `i64` values) | ~75 ns |

This ~75ns figure is effectively O(1) regardless of the 1,000,000-element input -- the Rust
conversion itself is a genuine pointer/metadata-only borrow. Every throughput number above times
the full Python-level call path (FFI/GIL/PyO3 overhead included); the gap between this figure and
the `from_pandas`/`to_parquet`/`from_parquet` numbers above lives entirely at that boundary, not
in the Rust core.

## Pass Bar Evaluation

**Stated, falsifiable pass bar (agreed at plan time):**
- **True-zero-copy scenarios** (`numeric_dense`, `numeric_nullable`, `chunked_multi_batch`) must
  show pydart throughput **>= 2x** pyarrow throughput on `from_pandas`, AND pydart peak RSS
  **<=** pyarrow peak RSS (evaluated on the `from_pandas` axis, the axis the zero-copy claim
  actually describes).
- **Copy-fallback scenarios** (`mixed_object_string`, `categorical_ordered`,
  `categorical_unordered`) must be within **+/-20%** of pyarrow throughput, reported either way
  (win or loss), on every axis.

**Evaluation against the numbers above:**

| Scenario | Group | Throughput vs bar | RSS vs bar | Verdict |
|----------|-------|---------------------|------------|---------|
| `numeric_dense` | true zero-copy | 3.24x **slower** (bar requires >= 2x faster) | pydart +9.73 MB (bar requires <=) | **FAIL** |
| `numeric_nullable` | true zero-copy | 3.13x **slower** | pydart -0.11 MB (passes, marginal) | **FAIL** (throughput) |
| `chunked_multi_batch` | true zero-copy (label disputed, see below) | 19.16x **slower** | pydart +18.89 MB (fails) | **FAIL** |
| `mixed_object_string` | copy-fallback | 12.73x slower on `from_pandas` (outside +/-20%); 1.95x **faster** on `to_pandas` (outside window, favorably); 13.26x/16.28x slower on Parquet axes | n/a (informational only) | **FAIL** on 3 of 4 axes; `to_pandas` is an honest pydart win |
| `categorical_ordered` | copy-fallback | 1.47x slower on `from_pandas` (outside +/-20%); 1.10x slower on `to_pandas` (within +/-20%, passes); 42.68x/25.80x slower on Parquet axes | n/a | **FAIL** on 3 of 4 axes |
| `categorical_unordered` | copy-fallback | 1.40x slower on `from_pandas` (outside +/-20%); 1.10x slower on `to_pandas` (within +/-20%, passes); 22.08x/32.00x slower on Parquet axes | n/a | **FAIL** on 3 of 4 axes |

**Overall verdict: the stated pass bar is NOT met.** Every true-zero-copy scenario fails on
throughput -- not narrowly (under the 2x bar), but in the *opposite direction*: pydart is
currently 3-19x **slower** than pyarrow at the full Python-level call path across every axis
except `to_pandas`, where it is roughly at parity (or, for `mixed_object_string`, faster).
Peak RSS tells a different, more favorable story: pydart's memory footprint is competitive with
or better than pyarrow's on the Parquet read/write axes (up to ~27 MB lower), even though
throughput lags badly on those same axes. This is reported honestly, per Pitfall 1/6 -- it is not
adjusted, softened, or averaged away. Whether this constitutes a blocker to sealing Phase 4 is a
human decision, not an implicit pass -- see the Task 3 checkpoint.

## Known Limitations

### `chunked_multi_batch`'s zero-copy label conflicts with its measured behavior

This plan's task specification and pass bar list `chunked_multi_batch` under "true zero-copy"
scenarios. Empirically, it is **not**: `pydart.Table.from_pandas` concatenates multi-chunk
Arrow-backed columns via `arrow::compute::concat` (the CR-01/CONV-08 fix), which is a real copy.
`Table.copy_report()` reports `zero_copy=False` for this scenario's column, and `strict=True`
correctly raises `pydart.ZeroCopyRequiredError` for it (see
`tests/python/test_multi_chunk_diagnostics.py`). Presenting this scenario as "true zero-copy" in a
public BENCHMARKS.md would contradict the project's own diagnostics-honesty posture (D-03/D-04,
D-26) and its own `copy_report()` output -- a credibility problem for a project whose entire pitch
is honesty about what is and isn't zero-copy (Pitfall 1). This file reports the scenario's
empirical `copy_report()` status plainly in the table above rather than silently accepting the
plan's original label or silently relabeling it without flagging the conflict. **Resolution
(reclassify `chunked_multi_batch` as copy-fallback, or keep it as a labeled true-zero-copy
*candidate* that currently fails the concat-copy check) is a human decision, raised explicitly at
the Task 3 checkpoint.**

### Categorical Parquet round-trip fidelity gap (D-40/T-03-09)

Categorical/dictionary columns are included in this benchmark matrix (both `categorical_ordered`
and `categorical_unordered`, including a >255-category / int16-code-width case), with an explicit
caveat: **a categorical's `.cat.categories` order and any unused categories do NOT survive a
Parquet round-trip.** This is a confirmed `arrow-rs` `ArrowWriter`/`DictEncoder` limitation --
dictionary keys are reassigned in first-occurrence-during-encoding row order, not the original
`.cat.categories` definition order, and categories with zero occurrences in the data are dropped
entirely. Per-row **values** and the **`dict_is_ordered`** flag DO survive the round-trip
correctly. `pyarrow` does not share this limitation. This is a real correctness concern for
*ordered* categoricals specifically, since the `<` relationship between categories can silently
change across a Parquet write/read cycle; it is cosmetic for unordered categoricals. No
`WriterProperties` fix exists in the pinned `parquet` 59.1.0. See
`tests/python/test_parquet_fidelity.py::test_ordered_categorical_category_order_not_guaranteed_known_gap`
for the pinned, regression-tested behavior, and `03-04-SUMMARY.md`'s Known Gap section for the
original finding (T-03-09).

### Throughput bottleneck lives at the PyO3/GIL/pandas-interop boundary, not the Rust core

Carried forward from Plan 01 and confirmed at full matrix scale in this plan: the ~75ns
criterion-measured Rust conversion kernel is effectively free regardless of row count, yet the
full Python-level `from_pandas`/`to_parquet`/`from_parquet` call paths are consistently 3-43x
slower than pyarrow's equivalents (the sole exceptions being the `to_pandas` axis, which is at
rough parity or a pydart win). This means the "measurably faster than pyarrow" core value claim
(PROJECT.md) is **not currently substantiated** by this benchmark matrix for any scenario/axis
except `to_pandas`. This is a real, honestly-reported finding, not a benchmark-harness bug -- the
harness itself works correctly (as proven by Plan 01 and re-confirmed here). Investigating and
closing this gap (likely FFI/GIL-boundary overhead in the pandas-interop layer, per Plan 01's own
carried-forward concern) is out of scope for this plan (a Rule 4 architectural change) and is
flagged for a follow-up plan/phase decision at the Task 3 checkpoint.
