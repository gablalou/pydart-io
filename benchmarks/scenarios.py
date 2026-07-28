"""BENCH-01/BENCH-02 (D-39: synthetic-only, programmatically generated data -- no downloaded
public/real-world dataset) benchmark scenario definitions.

The full six-scenario matrix (Plan 02), broadened from Plan 01's single `numeric_dense` tracer:

- `numeric_dense`, `numeric_nullable`, `chunked_multi_batch` -- the project's true zero-copy
  scenarios, all built on the `[pyarrow]` ArrowDtype path (`int64[pyarrow]`/`float64[pyarrow]`),
  mirroring `tests/python/test_nulls.py` (nullable idiom) and
  `tests/python/test_multi_chunk_diagnostics.py` (`_multi_chunk_int64_arrow_frame`, a genuine
  multi-`ChunkedArray` frame produced via `pd.concat` of two Arrow-backed frames -- NOTE: this
  scenario is measured, not assumed, to be zero-copy; `pydart.Table.from_pandas` runs
  `arrow::compute::concat` on multi-chunk columns per CR-01/CONV-08, a real copy that
  `copy_report()` honestly reports as `zero_copy=False`. BENCHMARKS.md's zero-copy/copy label
  column is driven by the actual `copy_report()` result captured at run time, not by this
  docstring's grouping -- see BENCHMARKS.md Known Limitations for the resolution of this
  tension).
- `mixed_object_string`, `categorical_ordered`, `categorical_unordered` -- copy-fallback
  scenarios: a legacy numpy `object`-dtype string column (mirroring
  `tests/python/test_object_string.py`'s `test_numpy_object_string_round_trips_via_copy`) and
  real `pd.Categorical` columns (mirroring `tests/python/test_categorical.py`, including the
  >255-category / int16-code-width case at lines 82-96) respectively. Both go through pydart's
  documented copy paths (D-10 object-dtype copy, OQ1 categorical-reconstruction copy).

Plain numpy-backed numeric columns are never used for the "true zero-copy" scenarios above --
that would silently benchmark the copy-fallback code path instead (RESEARCH.md Pitfall 1's
"blended claim" trap).
"""

import numpy as np
import pandas as pd

SCENARIOS = [
    "numeric_dense",
    "numeric_nullable",
    "mixed_object_string",
    "chunked_multi_batch",
    "categorical_ordered",
    "categorical_unordered",
]

# Row-count default for a "dense" scenario run at full scale. The harness must also tolerate much
# smaller row counts (down to 1 row) without crashing -- see this plan's must_haves backstop truth
# ("scenario row counts are documented ... harness does not assume more than one row").
DEFAULT_N_ROWS = 1_000_000

# Large-cardinality categorical fixture width, mirroring test_categorical.py's >255-category
# int16-code-width case (lines 82-96) -- pandas widens category codes past int8 once the category
# count exceeds 255.
_LARGE_CARDINALITY = 300


def make_df(scenario: str, n_rows: int = DEFAULT_N_ROWS) -> pd.DataFrame:
    """Build a synthetic pandas DataFrame for `scenario` with `n_rows` rows.

    Tolerates `n_rows` as small as 1 -- the benchmark harness must not assume more than one row.
    """
    if scenario == "numeric_dense":
        return pd.DataFrame(
            {
                "a": pd.array(np.arange(n_rows, dtype="int64"), dtype="int64[pyarrow]"),
                "b": pd.array(
                    np.arange(n_rows, dtype="float64") * 0.5, dtype="float64[pyarrow]"
                ),
            }
        )

    if scenario == "numeric_nullable":
        # D-07 nullable idiom (tests/python/test_nulls.py lines 27-34): real pd.NA nulls on the
        # ArrowDtype path, every 10th row null so the null bitmap is non-trivial at scale.
        int_values = [None if i % 10 == 0 else i for i in range(n_rows)]
        float_values = [None if i % 10 == 0 else i * 0.5 for i in range(n_rows)]
        return pd.DataFrame(
            {
                "a": pd.array(int_values, dtype="int64[pyarrow]"),
                "b": pd.array(float_values, dtype="float64[pyarrow]"),
            }
        )

    if scenario == "mixed_object_string":
        # Copy-fallback scenario: an ArrowDtype numeric column alongside a legacy numpy
        # object-dtype string column (D-10), mirroring
        # tests/python/test_object_string.py::test_numpy_object_string_round_trips_via_copy.
        # The object column has no Arrow-compatible physical layout, so this whole frame goes
        # through pydart's honest copy path, not the zero-copy path.
        strings = [f"row_{i}" if i % 7 != 0 else None for i in range(n_rows)]
        return pd.DataFrame(
            {
                "a": pd.array(np.arange(n_rows, dtype="int64"), dtype="int64[pyarrow]"),
                "s": pd.Series(strings, dtype=object),
            }
        )

    if scenario == "chunked_multi_batch":
        # True-zero-copy-eligible shape (per the plan's grouping) built via pd.concat of two
        # Arrow-backed frames, mirroring
        # tests/python/test_multi_chunk_diagnostics.py::_multi_chunk_int64_arrow_frame -- pandas/
        # pyarrow never auto-rechunk on concat, so this produces a genuine multi-chunk
        # ChunkedArray column. NOTE: from_pandas concatenates multi-chunk columns via
        # arrow::compute::concat (CR-01/CONV-08), a real copy -- copy_report() is the source of
        # truth for this scenario's actual zero-copy status, not this comment.
        first_half = n_rows // 2
        second_half = n_rows - first_half
        df1 = pd.DataFrame(
            {"a": pd.array(np.arange(first_half, dtype="int64"), dtype="int64[pyarrow]")}
        )
        df2 = pd.DataFrame(
            {
                "a": pd.array(
                    np.arange(first_half, first_half + second_half, dtype="int64"),
                    dtype="int64[pyarrow]",
                )
            }
        )
        return pd.concat([df1, df2], ignore_index=True)

    if scenario == "categorical_ordered":
        # Copy-fallback scenario (OQ1): an ordered pd.Categorical, mirroring
        # tests/python/test_categorical.py's ordered-categorical fixture (lines 34-48).
        # D-40/T-03-09: when this scenario feeds a Parquet-IO benchmark case, its
        # `.cat.categories` ORDER and any unused categories do NOT survive a Parquet round-trip
        # (a confirmed arrow-rs ArrowWriter/DictEncoder limitation -- values and
        # `dict_is_ordered` DO survive correctly). See BENCHMARKS.md Known Limitations.
        categories = [f"cat_{i}" for i in range(50)]
        codes = np.arange(n_rows) % len(categories)
        values = [categories[c] for c in codes]
        return pd.DataFrame(
            {"cat": pd.Categorical(values, categories=categories, ordered=True)}
        )

    if scenario == "categorical_unordered":
        # Copy-fallback scenario (OQ1), large-cardinality (>255-category) case mirroring
        # tests/python/test_categorical.py lines 82-96 -- pandas widens category codes to int16
        # once the category count exceeds 255.
        # D-40/T-03-09: when this scenario feeds a Parquet-IO benchmark case, its
        # `.cat.categories` ORDER and any unused categories do NOT survive a Parquet round-trip
        # (a confirmed arrow-rs ArrowWriter/DictEncoder limitation -- values and
        # `dict_is_ordered` DO survive correctly). See BENCHMARKS.md Known Limitations.
        categories = [f"cat_{i}" for i in range(_LARGE_CARDINALITY)]
        codes = np.arange(n_rows) % len(categories)
        values = [categories[c] for c in codes]
        return pd.DataFrame(
            {"cat": pd.Categorical(values, categories=categories, ordered=False)}
        )

    raise ValueError(f"Unknown scenario: {scenario!r} (known: {SCENARIOS})")
