---
status: complete
phase: 01-core-zero-copy-round-trip-interop
source: [01-01-SUMMARY.md, 01-02-SUMMARY.md, 01-03-SUMMARY.md, 01-04-SUMMARY.md, 01-05-SUMMARY.md]
started: 2026-07-15T13:47:12.000Z
updated: 2026-07-15T13:52:00.000Z
---

## Current Test

[testing complete]

## Tests

### 1. from_pandas/to_pandas round-trip preserves values and dtypes (ArrowDtype)
expected: A non-null int64/float64 ArrowDtype DataFrame round-trips through from_pandas().to_pandas() with values and dtypes preserved
result: pass
source: automated
coverage_id: 01-01/D1

### 2. Round-trip exact equality (pandas.testing.assert_frame_equal)
expected: Values and dtypes are exactly equal after from_pandas/to_pandas
result: pass
source: automated
coverage_id: 01-01/D2

### 3. Table exports via PyCapsule Interface, accepted by pyarrow
expected: A flint.Table exports via the Arrow PyCapsule Interface and pyarrow.table(...) accepts it with matching schema and row count
result: pass
source: automated
coverage_id: 01-01/D3

### 4. buffer_address returns nonzero for populated Table
expected: Table.buffer_address(index) returns a nonzero buffer address for a populated Table
result: pass
source: automated
coverage_id: 01-01/D4

### 5. from_pandas rejects unsupported column with clear error
expected: from_pandas rejects an unsupported column (non-ArrowDtype) with an error naming the offending column, rather than silently copying
result: pass
source: automated
coverage_id: 01-01/D5

### 6. plan_column single source of truth for conversion + diagnostics
expected: plan_column is the single per-column decision function, driving both from_pandas/to_pandas and strict mode/copy_report
result: pass
source: automated
coverage_id: 01-02/D1

### 7. Strict zero-copy mode succeeds on numeric + ArrowDtype-bool DataFrame
expected: Strict mode succeeds (no error) on a non-null numeric + ArrowDtype-bool DataFrame, proving it's functional, not a no-op
result: pass
source: automated
coverage_id: 01-02/D2

### 8. Strict mode rejects numpy-backed bool with named error
expected: Strict mode rejects a numpy-backed bool column with a clear exception naming the column and dtype, catchable as flint.ZeroCopyRequiredError/flint.FlintError
result: pass
source: automated
coverage_id: 01-02/D3

### 9. copy_report() returns per-column status agreeing with strict mode
expected: copy_report() returns one ColumnCopyStatus per column, agreeing column-for-column with strict-mode rejection
result: pass
source: automated
coverage_id: 01-02/D4

### 10. Full numeric+bool conversion matrix round-trips correctly
expected: ArrowDtype int64/float64/bool, numpy int64/float64/int32 (zero-copy borrow), non-contiguous numpy and numpy bool (copy fallback) all round-trip correctly
result: pass
source: automated
coverage_id: 01-02/D5

### 11. Forward zero-copy pointer-identity proof (numpy + ArrowDtype)
expected: from_pandas shares the same physical data buffer (pointer identity) for both a numpy-numeric and an ArrowDtype column
result: pass
source: automated
coverage_id: 01-03/D1

### 12. Reverse zero-copy pointer-identity proof (to_pandas)
expected: The confirmed to_pandas mechanism shares the Table's physical buffer
result: pass
source: automated
coverage_id: 01-03/D2

### 13. Pointer-identity proof is discriminating (fails on real copy)
expected: The proof would fail on a real copy, proven by asserting buffer_address differs from an unrelated DataFrame's source buffer
result: pass
source: automated
coverage_id: 01-03/D3

### 14. Rust allocation-counter proof: no heap allocation for data buffer
expected: The flint-core borrow-conversion entry point makes no heap allocation for the data buffer (guarded against optimizer elision)
result: pass
source: automated
coverage_id: 01-03/D4

### 15. Allocation proof detects a deliberately-copying path
expected: The allocation proof is sanity-checked to detect a deliberately-copying path
result: pass
source: automated
coverage_id: 01-03/D5

### 16. from_arrow imports a pyarrow Table zero-copy
expected: flint.from_arrow(obj) imports a pyarrow Table into a flint Table, zero-copy, via pyo3_arrow::PyTable's FromPyObject (no hand-rolled FFI)
result: pass
source: automated
coverage_id: 01-04/D1

### 17. from_arrow imports a Polars DataFrame
expected: flint.from_arrow(obj) imports a Polars DataFrame into a flint Table
result: pass
source: automated
coverage_id: 01-04/D2

### 18. from_arrow rejects malformed foreign objects safely
expected: A malformed/inconsistent foreign object surfaces flint.FlintError, never a panic/segfault
result: pass
source: automated
coverage_id: 01-04/D3

### 19. from_arrow consumes foreign stream dunder exactly once
expected: A foreign object's __arrow_c_stream__ is consumed exactly once, never invoked twice
result: pass
source: automated
coverage_id: 01-04/D4

### 20. Table export accepted by pyarrow, Polars, AND DuckDB
expected: A flint Table exported via the PyCapsule Interface is accepted by pyarrow.table(), polars.from_arrow(), and DuckDB, each with matching schema/row count
result: pass
source: automated
coverage_id: 01-04/D5

### 21. DuckDB native-PyCapsule-consumption resolved empirically
expected: DuckDB's native-PyCapsule-consumption status resolved empirically against pinned duckdb 1.5.4, never silently skipped
result: pass
source: automated
coverage_id: 01-04/D6

### 22. Multi-chunk from_pandas round-trip preserves all rows (CR-01 fix)
expected: A non-null numeric/bool pandas DataFrame whose Arrow-backed column spans more than one RecordBatch (e.g. pd.concat) round-trips through Table.from_pandas(df).to_pandas() preserving all rows — no silent truncation
result: pass
source: automated
coverage_id: 01-05/D1

### 23. Single-chunk zero-copy path unchanged after CR-01 fix
expected: The single-chunk zero-copy pointer-identity proof still passes bit-for-bit after the CR-01 fix
result: pass
source: automated
coverage_id: 01-05/D2

## Summary

total: 23
passed: 23
issues: 0
pending: 0
skipped: 0

## Gaps

[none — all deliverables deterministically covered by passing automated tests]
