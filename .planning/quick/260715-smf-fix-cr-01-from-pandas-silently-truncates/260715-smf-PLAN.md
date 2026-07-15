---
phase: quick
plan: 260715-smf
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/flint-python/src/pandas.rs
  - tests/python/test_round_trip.py
autonomous: true
requirements: [CONV-01, CONV-02]

must_haves:
  truths:
    - "A pd.concat of two Arrow-backed frames (6 rows, 2 chunks) round-trips through from_pandas().to_pandas() preserving all 6 rows"
    - "Single-chunk Arrow-backed columns still take the genuinely zero-copy path (no concat, no data copy)"
    - "No silent truncation: every RecordBatch in a column's Arrow C stream is accounted for"
  artifacts:
    - "crates/flint-python/src/pandas.rs (import_column_via_pandas_stream accounts for all batches)"
    - "tests/python/test_round_trip.py (multi-chunk regression test)"
  key_links:
    - "import_column_via_pandas_stream -> arrow::compute::concat (multi-batch path only)"
    - "import_column_via_pandas_stream single-batch path -> batch.column(0).clone() (Arc clone, zero-copy)"
---

<objective>
Fix CR-01: `from_pandas` silently truncates multi-chunk Arrow-backed pandas columns to only the
first RecordBatch, causing silent data loss (documented reproduction: 6 rows in via `pd.concat`
of two 3-row `int64[pyarrow]` frames -> 3 rows out, no exception).

Purpose: Eliminate silent data loss on an ordinary, in-scope pandas construction. This is the one
place the project's "never silent copy/loss" invariant (DIAG-01/DIAG-02) is broken. Documented as
CR-01 (Critical) in 01-REVIEW.md and independently reproduced in 01-VERIFICATION.md.
Output: A corrected `import_column_via_pandas_stream` that concatenates all batches for the
multi-chunk case while preserving the certified single-chunk zero-copy fast path, plus a
regression test locking in full-row-count round-trip for multi-chunk input.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md

# The file containing the bug (read the whole file to understand from_pandas structure)
@crates/flint-python/src/pandas.rs

# CR-01 origin and independent reproduction (exact repro + suggested fix)
@.planning/phases/01-core-zero-copy-round-trip-interop/01-REVIEW.md
@.planning/phases/01-core-zero-copy-round-trip-interop/01-VERIFICATION.md

# Existing round-trip test suite — mirror its fixture/style conventions
@tests/python/test_round_trip.py
</context>

<tasks>

<task type="auto">
  <name>Task 1: Concatenate all batches in import_column_via_pandas_stream, preserving the single-chunk zero-copy fast path</name>
  <files>crates/flint-python/src/pandas.rs</files>
  <action>
Fix the silent-truncation defect in `import_column_via_pandas_stream` (currently around lines
183-199). The current body binds `py_table.batches().first()` and returns `batch.column(0).clone()`,
dropping every RecordBatch after the first.

Replace the batch-selection logic so it accounts for EVERY batch in the column's Arrow C stream:

1. Bind `let batches = py_table.batches();`.
2. If `batches.is_empty()`, keep the existing behavior: return
   `FlintError::Other("column stream produced no record batches".to_string())`.
3. If `batches.len() == 1`, return `batches[0].column(0).clone()` directly. This is a genuinely
   zero-copy `Arc<dyn Array>` clone and MUST remain the path taken for the ordinary single-chunk
   case — do NOT route single-batch columns through `concat` (that would allocate/copy and regress
   the already-certified single-chunk zero-copy behavior; see the hard constraint in this plan and
   the single-chunk pointer-identity/allocation proofs in the Phase 1 test suite).
4. If `batches.len() >= 2`, collect each batch's column-0 array and concatenate them into one array
   via `arrow::compute::concat`. Build a `Vec<ArrayRef>` of `b.column(0).clone()` over
   `batches.iter()`, then pass a `&[&dyn Array]` slice (map each `ArrayRef` through `.as_ref()`) to
   `arrow::compute::concat(...)`, mapping its error via `.map_err(FlintError::from)?`. Return the
   concatenated `ArrayRef`.

Add the `arrow::compute::concat` reference via a path-qualified call (`arrow::compute::concat(...)`)
rather than a new top-level `use`, unless a `use` matches the file's existing import style. The
`arrow` crate (59.1.0, already a dependency per crates/flint-python/Cargo.toml) exposes
`concat` under `arrow::compute`; no new dependency or Cargo feature is required. (If a Cargo
feature turned out to be missing, Task 1's `cargo build` verify would catch it.)

Update the doc comment on `import_column_via_pandas_stream` to state that multi-chunk columns are
concatenated into a single array (an honest copy — a multi-chunk column was never one contiguous
buffer, so this does not regress the single-chunk zero-copy path), replacing any wording implying a
single batch is expected.

Do NOT change `from_pandas`, `borrow_numpy_numeric_column`, `classify_dtype`, or the
`ColumnConversionRecord`/`plan_column` wiring. Scope is limited to the batch-handling inside this
one helper. Leave the WR-01..WR-04 / IN-01 / IN-02 findings from 01-REVIEW.md out of scope for this
quick fix (they are separate, non-blocking issues).
  </action>
  <verify>
    <automated>cargo build -p flint-python 2>&1 | tail -5 && cargo test --workspace 2>&1 | tail -20</automated>
  </verify>
  <done>
`import_column_via_pandas_stream` returns a single array covering all batches: single-batch columns
return `batches[0].column(0).clone()` (zero-copy Arc clone, unchanged behavior), multi-batch columns
return `arrow::compute::concat` over all column-0 arrays, and the empty-stream error is preserved.
`cargo build -p flint-python` succeeds and the existing workspace test suite still passes (no
regression to single-chunk zero-copy proofs).
  </done>
</task>

<task type="auto">
  <name>Task 2: Add a multi-chunk round-trip regression test reproducing CR-01</name>
  <files>tests/python/test_round_trip.py</files>
  <action>
Add a regression test to `tests/python/test_round_trip.py` that reproduces the exact CR-01
scenario from 01-VERIFICATION.md and asserts it is fixed. Follow the existing module's fixture and
assertion style (`pandas`, `pandas.testing as pdt`, `import flint`).

Name the test to make the defect it guards explicit, e.g.
`test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column`.

Construct a multi-chunk Arrow-backed DataFrame by concatenating two 3-row `int64[pyarrow]` frames:
build `df1` and `df2` each as a 3-row DataFrame with an `int64[pyarrow]` column (distinct values so
truncation is unambiguous, e.g. 1,2,3 and 4,5,6), then
`df = pd.concat([df1, df2], ignore_index=True)`. This yields 6 logical rows backed by a 2-chunk
pyarrow ChunkedArray (pd.concat does not auto-rechunk).

Assert the round trip preserves the full row count and values:
- `table = flint.Table.from_pandas(df)`, `result = table.to_pandas()`.
- Assert `len(result) == 6` (this is the assertion that fails against the pre-fix truncating code —
  it returned 3).
- Assert the column's values are `[1, 2, 3, 4, 5, 6]` (use `.tolist()` for a dtype-label-agnostic
  value comparison, consistent with the numpy-backed round-trip tests already in this file, since
  `to_pandas` reconstructs ArrowDtype-backed columns).

Add a short docstring referencing CR-01 / silent multi-chunk truncation so the test's purpose is
self-documenting.

CRITICAL — rebuild before testing: `flint` is a maturin-compiled PyO3 extension (`_flint` .so).
There is NO auto-rebuild import hook or conftest in this repo (verified: no `conftest.py`, no
`maturin_import_hook`; pyproject.toml documents the workflow as `uv run maturin develop && uv run
pytest`). The Task 1 Rust edit does NOT take effect in Python until the extension is recompiled, so
the verify command below MUST run `uv run maturin develop` before pytest. If it does not, the new
test would run against the stale (pre-fix) binary and report a misleading 3-row failure.
  </action>
  <verify>
    <automated>uv run maturin develop 2>&1 | tail -3 && uv run pytest tests/python/test_round_trip.py -q 2>&1 | tail -15</automated>
  </verify>
  <done>
`tests/python/test_round_trip.py` contains a test that builds a 6-row / 2-chunk `int64[pyarrow]`
DataFrame via `pd.concat`, round-trips it through `from_pandas().to_pandas()`, and asserts both a
row count of 6 and values `[1,2,3,4,5,6]`. The test passes against the Task 1 fix and would fail
(3 rows) against the pre-fix code. The full `tests/python/` suite still passes.
  </done>
</task>

</tasks>

<verification>
- `cargo test --workspace` passes (single-chunk zero-copy pointer-identity and allocation proofs
  unaffected).
- `uv run maturin develop && uv run pytest tests/python/ -q` passes, including the new multi-chunk
  regression test (the maturin rebuild is required so pytest runs against the recompiled extension).
- Manual sanity (optional): `pd.concat` of two 3-row `int64[pyarrow]` frames round-trips to 6 rows.
</verification>

<success_criteria>
- CR-01 resolved: multi-chunk Arrow-backed columns round-trip with full row count, no silent
  truncation.
- Single-chunk zero-copy fast path preserved (single-batch columns still return an Arc clone, not a
  concat copy).
- Regression test locks in the fix so the defect cannot recur silently.
</success_criteria>

<output>
Create `.planning/quick/260715-smf-fix-cr-01-from-pandas-silently-truncates/260715-smf-SUMMARY.md` when done.
</output>
