"""Multi-chunk diagnostics honesty (CONV-08, D-12/D-13/D-14).

Closes the DIAG-01/DIAG-02 override recorded in
`.planning/phases/01-core-zero-copy-round-trip-interop/01-VERIFICATION.md`: a multi-chunk
Arrow-backed pandas column (e.g. a `pd.concat` of two `int64[pyarrow]` frames, producing a 2-chunk
`ChunkedArray`) is concatenated into one contiguous buffer via `arrow::compute::concat` -- a real
copy, not a zero-copy borrow. Before this plan, `copy_report()`/`strict=True` both silently
reported/accepted this as zero-copy (the CR-01 fix's own diagnostics-honesty blind spot). This
file proves:

- The concat copy still preserves every row (D-12, CR-01 not regressed).
- `copy_report()` now honestly reports the column as `zero_copy=False` with a chunk/concat reason
  (D-13).
- `strict=True` now RAISES `flint.ZeroCopyRequiredError` for this column, with no bypass flag
  (D-14) -- a behavior change from the prior (accepted-as-a-gap) silent success.
- A single-chunk Arrow-backed column is unaffected by the correction: still `zero_copy=True` under
  `copy_report()`, and still succeeds under `strict=True` (the correction only fires for
  `batches.len() > 1`).
- `copy_report()` and `strict` mode agree on exactly which column is non-zero-copy (single source
  of truth, mirroring `test_copy_report.py`'s existing agreement test).
"""

import pandas as pd
import pytest

import flint


def _multi_chunk_int64_arrow_frame() -> pd.DataFrame:
    """A pd.concat of two Arrow-backed frames -- pandas/pyarrow never auto-rechunk on concat, so
    this produces a genuine 2-chunk ChunkedArray column (the same CR-01 fixture shape)."""
    df1 = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    df2 = pd.DataFrame({"a": pd.array([4, 5, 6], dtype="int64[pyarrow]")})
    return pd.concat([df1, df2], ignore_index=True)


def _single_chunk_int64_arrow_frame() -> pd.DataFrame:
    return pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})


def test_multi_chunk_column_still_round_trips_all_rows():
    """D-12 / CR-01 not regressed: all 6 rows survive the concat-copy round trip."""
    df = _multi_chunk_int64_arrow_frame()

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert len(result) == 6
    assert result["a"].tolist() == [1, 2, 3, 4, 5, 6]


def test_multi_chunk_column_reported_as_copy_in_copy_report():
    """D-13: copy_report() now honestly reports the multi-chunk concat as a copy."""
    df = _multi_chunk_int64_arrow_frame()
    table = flint.Table.from_pandas(df)

    report = {status.column: status for status in table.copy_report()}

    assert report["a"].zero_copy is False
    assert report["a"].reason is not None
    assert "chunk" in report["a"].reason or "concat" in report["a"].reason


def test_single_chunk_arrow_column_still_zero_copy():
    """The chunk-count correction only fires for batches.len() > 1 -- a single-chunk
    Arrow-backed column is unaffected."""
    df = _single_chunk_int64_arrow_frame()
    table = flint.Table.from_pandas(df)

    report = {status.column: status for status in table.copy_report()}

    assert report["a"].zero_copy is True
    assert report["a"].reason is None


def test_strict_mode_now_rejects_multi_chunk_column():
    """D-14: strict=True now RAISES for a multi-chunk column, with no bypass flag -- the
    behavior-change acknowledged by CONTEXT.md/this plan versus the prior silent-success gap."""
    df = _multi_chunk_int64_arrow_frame()

    with pytest.raises(flint.ZeroCopyRequiredError) as exc_info:
        flint.Table.from_pandas(df, strict=True)

    assert "a" in str(exc_info.value)


def test_single_chunk_arrow_column_still_succeeds_under_strict():
    """Proves the D-14 change is scoped to multi-chunk columns: an ordinary single-chunk
    Arrow-backed column still succeeds under strict=True."""
    df = _single_chunk_int64_arrow_frame()

    table = flint.Table.from_pandas(df, strict=True)

    assert table.to_pandas()["a"].tolist() == [1, 2, 3]


def test_copy_report_and_strict_agree_for_multi_chunk():
    """Single source of truth (mirrors test_copy_report.py's existing agreement test): the column
    copy_report() marks zero_copy=False is exactly the column strict mode raises on."""
    df = _multi_chunk_int64_arrow_frame()
    table = flint.Table.from_pandas(df)
    report = table.copy_report()

    non_zero_copy_columns = {status.column for status in report if not status.zero_copy}

    with pytest.raises(flint.ZeroCopyRequiredError) as exc_info:
        flint.Table.from_pandas(df, strict=True)

    assert non_zero_copy_columns == {"a"}
    assert "a" in str(exc_info.value)
