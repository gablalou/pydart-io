"""BENCH-01/BENCH-02 (D-39: synthetic-only, programmatically generated data -- no downloaded
public/real-world dataset) benchmark scenario definitions.

`numeric_dense` is the only scenario in this plan (04-01) -- the thin end-to-end slice proving the
full throughput + peak-RSS + Rust-criterion measurement path works on one honest scenario before
Plan 02 broadens the matrix to {mixed, nullable, chunked, object-string, categorical}.

Every scenario builds an ArrowDtype-backed frame (`int64[pyarrow]`/`float64[pyarrow]`), the
project's true zero-copy path -- see `tests/python/test_parquet_roundtrip.py`'s
`_numeric_arrow_dtype_frame()` for the same construction idiom. Plain numpy-backed columns would
silently benchmark the copy-fallback code path instead (RESEARCH.md Pitfall 1's "blended claim"
trap) and must not be used here.
"""

import numpy as np
import pandas as pd

SCENARIOS = ["numeric_dense"]

# Row-count default for a "dense" scenario run at full scale. The harness must also tolerate much
# smaller row counts (down to 1 row) without crashing -- see this plan's must_haves backstop truth
# ("scenario row counts are documented ... harness does not assume more than one row").
DEFAULT_N_ROWS = 1_000_000


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
    raise ValueError(f"Unknown scenario: {scenario!r} (known: {SCENARIOS})")
