---
phase: 01-core-zero-copy-round-trip-interop
verified: 2026-07-15T00:00:00Z
status: passed
score: 5/5 must-haves verified # includes 2 overrides
behavior_unverified: 0
overrides_applied: 2
overrides:
  - must_have: "User can request strict zero-copy mode and it succeeds (no error) on a non-null numeric/bool DataFrame, proving the mode is functional rather than a no-op (Success Criterion 2, DIAG-01)"
    reason: >-
      Consciously accepted: strict=True does not yet detect the multi-chunk-concat copy
      introduced by the CR-01 fix, because plan_column's ColumnPlan is computed before batch
      count is known. This is a diagnostics-honesty gap on an edge case (multi-chunk
      ArrowDtype columns), not a data-loss or single-chunk regression -- the phase's core
      zero-copy guarantee (single-chunk, the primary supported path) is unaffected and
      independently re-verified. Deferred to Phase 2 alongside CONV-08's broader multi-chunk
      handling, where plan_column/ColumnConversionRecord can be made chunk-count-aware as part
      of that same body of work rather than as an isolated patch here.
    accepted_by: "John Columna"
    accepted_at: "2026-07-15T00:00:00Z"
  - must_have: "User can query per-column diagnostics and see a report confirming zero_copy=true for each numeric/bool column, honestly reflecting whether a copy occurred (Success Criterion 3, DIAG-02)"
    reason: >-
      Same root cause and same deferral as the DIAG-01 override above: copy_report() reads the
      same pre-computed ColumnConversionRecord, so it inherits the identical multi-chunk
      blind spot. Single-chunk columns (the primary supported path) are honestly reported and
      unaffected. Deferred to Phase 2 (CONV-08) as one fix alongside the DIAG-01 gap, since both
      are resolved by the same chunk-count-aware plan_column change.
    accepted_by: "John Columna"
    accepted_at: "2026-07-15T00:00:00Z"
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Truth 1 (CONV-01/CONV-02): from_pandas silently truncated multi-batch Arrow-backed pandas columns to their first RecordBatch (CR-01). Independently reproduced fixed: a pd.concat of two 3-row int64[pyarrow] frames (6 rows, 2 chunks) now round-trips through Table.from_pandas(df).to_pandas() returning all 6 rows, confirmed both via the new automated regression test and a fresh, independent manual reproduction run during this verification."
  gaps_remaining:
    - "DIAG-01/DIAG-02 multi-chunk diagnostics-honesty gap (below) — consciously accepted via recorded override, deferred to Phase 2 (CONV-08)."
  regressions:
    - "Truth 2 (DIAG-01) and Truth 3 (DIAG-02): the CR-01 fix itself introduces a new, previously-unobservable diagnostics-honesty defect on the identical in-scope input (an ordinary pd.concat of two ArrowDtype-backed numeric frames). Before the fix, this input silently truncated data but coincidentally reported zero_copy=True truthfully (no copy had actually occurred -- only truncation). After the fix, a genuine arrow::compute::concat COPY now occurs for this input, but from_pandas(df, strict=True) still SUCCEEDS without error, and table.copy_report() still reports zero_copy=True, reason=None for the copied column -- independently reproduced live during this verification. This is the exact DIAG-01/DIAG-02 invariant the project's own diagnostics exist to prevent (silently mislabeling a copying path as zero-copy), on an input class this phase's own prior verification already ruled in-scope. Root cause: ColumnConversionRecord is populated from plan_column's a-priori decision (dtype backend + arrow kind + contiguity), computed BEFORE import_column_via_pandas_stream runs and before batch count is known -- plan_column has no visibility into chunk count, so it cannot detect that a copy actually occurred in the multi-batch branch."
gaps:
  - truth: "User can request strict zero-copy mode and it succeeds (no error) on a non-null numeric/bool DataFrame, proving the mode is functional rather than a no-op (Success Criterion 2, DIAG-01)"
    status: overridden
    reason: >-
      DIAG-01 (REQUIREMENTS.md): "User can request a strict zero-copy mode that errors instead of
      silently falling back to a copy." Empirically reproduced during this verification: for the
      identical pd.concat-of-two-ArrowDtype-frames input whose data-loss half (CR-01) this
      re-verification confirms is now fixed, calling `flint.Table.from_pandas(df, strict=True)`
      SUCCEEDS with no exception, even though `import_column_via_pandas_stream` performs a genuine
      `arrow::compute::concat` COPY internally for this exact column. Strict mode's entire purpose
      is to catch precisely this case (a column that requires a copy) and refuse it; instead it
      silently passes. This is a structural consequence of the CR-01 fix itself: the per-column
      `ColumnPlan` (`ZeroCopyBorrow` vs `RequiresCopy`) is computed by `plan_column` from dtype
      backend + arrow kind + contiguity alone, BEFORE `import_column_via_pandas_stream` runs and
      before the actual batch count (and therefore whether a copy occurs) is known. `plan_column`
      has no chunk-count input, so a multi-chunk Arrow-backed column is unconditionally classified
      `ZeroCopyBorrow` regardless of what actually happens at import time.
    artifacts:
      - path: "crates/flint-python/src/pandas.rs"
        issue: "from_pandas computes plan_column's ColumnPlan (and therefore ColumnConversionRecord.zero_copy) before calling import_column_via_pandas_stream, so the record cannot reflect the multi-batch concat copy that function may perform. check_strict (diagnostics.rs) reads only this pre-computed, now-stale record."
    missing:
      - "Either: (a) make plan_column/ColumnConversionRecord chunk-count-aware so a multi-batch Arrow-backed column is correctly classified RequiresCopy (causing strict=True to raise and copy_report to report zero_copy=False with a naming reason), or (b) explicitly document and consciously accept this as a known limitation via a recorded VERIFICATION.md override, rather than leaving it as a silent, undetected gap."
      - "A regression test asserting that strict=True RAISES (or copy_report reports zero_copy=False) for a multi-chunk Arrow-backed column -- the current suite has no such test in either test_strict_mode.py or test_copy_report.py."
  - truth: "User can query per-column diagnostics and see a report confirming zero_copy=true for each numeric/bool column, honestly reflecting whether a copy occurred (Success Criterion 3, DIAG-02)"
    status: overridden
    reason: >-
      DIAG-02 (REQUIREMENTS.md): "User can query per-column diagnostics explaining whether a copy
      occurred and why." Empirically reproduced during this verification: for the same multi-chunk
      pd.concat input, `table.copy_report()` returns `[ColumnCopyStatus(column='a', zero_copy=True,
      reason=None)]` for a column that, per the CR-01 fix, DID undergo an `arrow::compute::concat`
      copy. The diagnostics module's own doc comment (crates/flint-python/src/diagnostics.rs:1-6)
      states records reflect "the SAME per-column ColumnConversionRecords... this reflects the
      actual conversion that occurred, not a re-derived (possibly-diverging) decision" -- this
      invariant (T-01-05) is now violated for any multi-chunk Arrow-backed column, because the
      record was computed from plan_column's a-priori prediction, not from what
      import_column_via_pandas_stream actually did.
    artifacts:
      - path: "crates/flint-python/src/diagnostics.rs"
        issue: "build_copy_report forwards ColumnConversionRecord.zero_copy/reason unchanged; the underlying record is stale for the multi-batch-concat case (see Truth 2 gap above for root cause)."
    missing:
      - "Same fix as the DIAG-01 gap above -- both features consume the same ColumnConversionRecord, so a single fix (chunk-count-aware plan_column, or a post-hoc correction recorded after import_column_via_pandas_stream determines batch count) resolves both."
      - "A regression test in test_copy_report.py asserting zero_copy=False (with a non-None reason) for a multi-chunk Arrow-backed column."
---

# Phase 1: Core Zero-Copy Round-Trip & Interop Verification Report (Re-verification)

**Phase Goal:** A user can take a simple non-null numeric/bool pandas DataFrame, convert it to an Arrow Table with true zero-copy and back, verify the copy status via a first-class diagnostics/strict-mode API, and hand the Table off to pyarrow/Polars/DuckDB via the Arrow PyCapsule Interface (and accept one back) — all zero-copy.
**Verified:** 2026-07-15
**Status:** passed (2 overrides)
**Re-verification:** Yes — after CR-01 gap closure (commits `7d0bc52`, `b5df2da`, reconciled in `01-05-SUMMARY.md`/`1604a68`)
**Overrides:** DIAG-01/DIAG-02 multi-chunk diagnostics-honesty gap consciously accepted and deferred to Phase 2 (CONV-08) — see frontmatter `overrides:`, accepted by John Columna 2026-07-15.

## Summary of this re-verification

The one gap tracked from the previous verification cycle (CR-01: silent multi-batch truncation
causing data loss in `from_pandas`) **is genuinely fixed** — independently reproduced below, not
just trusted from SUMMARY.md. `import_column_via_pandas_stream` now short-circuits a single-batch
stream to a direct `Arc` clone (unchanged zero-copy behavior) and concatenates a multi-batch stream
via `arrow::compute::concat` (an honest copy). The single-chunk zero-copy pointer-identity proof
(D-06) still passes bit-for-bit — no regression to the certified zero-copy path.

**However, this re-verification surfaces a new, adjacent defect that was not visible before the
fix and is not covered by any existing test:** on the identical multi-chunk input, `from_pandas`'s
strict mode (DIAG-01) and `copy_report()` (DIAG-02) both continue to report the column as
zero-copy (`zero_copy=True`, `reason=None`), even though the CR-01 fix now performs a genuine copy
for that column via `arrow::compute::concat`. Before the fix, this coincidentally read as "true"
because no copy occurred (only silent truncation); after the fix, a copy demonstrably occurs but
is not reported. This is exactly the "silently mislabeling a copying path as zero-copy" failure
mode this project's own CLAUDE.md names as a core credibility risk, and it is the same DIAG-01/
DIAG-02 invariant whose violation made CR-01 a blocker in the first place. The 01-05-PLAN.md/
01-05-SUMMARY.md both explicitly note and defer this as a "known limitation" attributed to Phase
2's CONV-08 — but CONV-08 (REQUIREMENTS.md: "User can convert a Table with multiple chunks per
column (ChunkedArray) to/from pandas") is about ADDING multi-chunk *conversion* capability, not
about *diagnostics honesty* for a case this phase's own prior verification already ruled in-scope.
It does not defer this finding under Step 9b's later-phase-match rule. Net result: **the fix traded
a data-loss defect for a diagnostics-honesty defect on the same input** — the phase goal is closer
to achieved but not yet fully achieved.

## Goal Achievement

### Observable Truths (Roadmap Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can convert a non-null numeric/bool pandas DataFrame to an Arrow Table with zero-copy and back to pandas | ✓ VERIFIED | Single-chunk case: pointer-identity proof passes both directions (unchanged). Multi-chunk case (CR-01): independently reproduced — a fresh `pd.concat` of two 3-row `int64[pyarrow]` frames (6 rows, 2 chunks) round-trips through `flint.Table.from_pandas(df).to_pandas()` returning all 6 rows with correct values, confirmed via both the new automated regression test (`test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column`) and a separate ad hoc script run during this verification. |
| 2 | User can request strict zero-copy mode and it succeeds (no error) on a non-null numeric/bool DataFrame, proving the mode is functional rather than a no-op | ⚠️ PASSED (override) | Strict mode correctly succeeds on the phase's core single-chunk happy path and correctly REJECTS numpy-backed bool (existing tests pass). On the multi-chunk edge case, `from_pandas(df, strict=True)` SUCCEEDS despite the fix performing an actual `concat` copy for that column — a diagnostics-honesty gap, not a single-chunk regression. Override: consciously accepted, deferred to Phase 2 (CONV-08) — accepted by John Columna 2026-07-15. |
| 3 | User can query per-column diagnostics and see a report confirming zero_copy=true for each numeric/bool column | ⚠️ PASSED (override) | For ordinary single-chunk columns, `copy_report()` correctly and honestly reports `zero_copy=True`/`reason=None` (existing tests pass). On the multi-chunk edge case, `copy_report()` still reports `zero_copy=True, reason=None` even though a genuine copy occurred. Override: consciously accepted, deferred to Phase 2 (CONV-08) — accepted by John Columna 2026-07-15. |
| 4 | User can export a Table via the Arrow PyCapsule Interface and have it accepted zero-copy by pyarrow, Polars, or DuckDB | ✓ VERIFIED | `tests/python/test_interop.py` — `test_pyarrow_accepts_flint_table_export`, `test_polars_accepts_flint_table_export`, `test_duckdb_accepts_flint_table_export` all pass, unaffected by the CR-01 fix (export path never touches `pandas.rs`'s import helper). |
| 5 | User can import a foreign Arrow object (pyarrow Table, Polars DataFrame) via the PyCapsule Interface into a Table with zero-copy | ✓ VERIFIED | `tests/python/test_interop.py::test_from_arrow_imports_pyarrow_table` / `test_from_arrow_imports_polars_dataframe` pass; `flint.from_arrow` (CAP-02, `crates/flint-python/src/import.rs`) never calls `import_column_via_pandas_stream` — it delegates entirely to `pyo3_arrow::PyTable`'s own `FromPyObject`, so this fix and its side effects do not touch CAP-02. Unaffected, confirmed again this cycle. |

**Score:** 5/5 truths verified (3 direct, 2 via recorded override; 0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/flint-python/src/pandas.rs` — `import_column_via_pandas_stream` | Accounts for every RecordBatch (CR-01 fix) | ✓ VERIFIED | Read in full this cycle: `batches.is_empty()` → explicit `FlintError` (unchanged); `batches.len() == 1` → direct `Arc` clone (unchanged zero-copy short-circuit, exactly matching the plan's Task 1 refinement); `batches.len() > 1` → `arrow::compute::concat` over all batches' column-0 arrays. Matches `01-05-PLAN.md`'s `must_haves.artifacts` exactly (`contains: "concat"` — confirmed present). |
| `tests/python/test_round_trip.py` — multi-chunk regression test | Proves 6-in/6-out | ✓ VERIFIED | `test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column` present, passes; reproduces the exact original CR-01 scenario (`pd.concat` of two 3-row `int64[pyarrow]` frames). |
| `crates/flint-python/src/diagnostics.rs` | Reports honest, actual copy status per column (DIAG-01/DIAG-02) | ✗ STALE FOR MULTI-CHUNK | `check_strict`/`build_copy_report` both consume `ColumnConversionRecord`s computed by `plan_column` BEFORE `import_column_via_pandas_stream` runs — for multi-batch Arrow-backed columns, the record is now provably wrong (says `zero_copy=True` for a column that was just copied via `concat`). This artifact's core claim ("reflects the actual conversion that occurred, not a re-derived decision" — its own doc comment) is falsified for this input class. |
| `tests/python/test_strict_mode.py` | Strict-mode success + rejection tests, including multi-chunk | ✗ GAP | Existing 4 tests pass but none exercises a multi-chunk Arrow-backed column; the gap above is entirely untested by this file. |
| `tests/python/test_copy_report.py` | copy_report shape + agreement tests, including multi-chunk | ✗ GAP | Existing 4 tests pass but none exercises a multi-chunk Arrow-backed column; the gap above is entirely untested by this file. |
| All other Phase 1 artifacts (Table, PyCapsule dunders, import.rs, pyproject.toml, etc.) | Unchanged from prior verification | ✓ VERIFIED (carried forward) | Not touched by this fix; re-confirmed via passing `test_interop.py` (8/8) and `test_zero_copy_pointer.py` (4/4) this cycle. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `crates/flint-python/src/pandas.rs` (`import_column_via_pandas_stream`) | `arrow::compute::concat` | Multi-batch branch concatenates every batch's column-0 array | ✓ WIRED | Confirmed by reading source (lines 215-217) and by the `grep concat` pattern from the plan's `must_haves.key_links`. |
| `crates/flint-python/src/pandas.rs` (`from_pandas`) | `crates/flint-core/src/pandas_plan.rs` (`plan_column`) | Per-column plan computed BEFORE import | ⚠️ WIRED BUT STALE | Confirmed wired (line 148), but this ordering is exactly why the plan cannot know the actual batch count / copy outcome — this is the mechanical root cause of the new DIAG-01/DIAG-02 gap above. |
| `crates/flint-python/src/diagnostics.rs` | `crates/flint-python/src/pandas.rs` (`ColumnConversionRecord`) | Both consume the same records | ⚠️ WIRED BUT NOW DIVERGENT FROM REALITY | Wiring itself is sound (same record read by both, so strict mode and `copy_report` never disagree WITH EACH OTHER) — but the record itself now disagrees with the ACTUAL conversion behavior for multi-chunk columns. |
| `crates/flint-python/src/import.rs` (`from_arrow`, CAP-02) | `pyo3_arrow::PyTable` `FromPyObject` | Direct `.extract()`, no `import_column_via_pandas_stream` involvement | ✓ WIRED, UNAFFECTED | Re-confirmed: `from_arrow` never calls the fixed function; CAP-02 is untouched by CR-01 or this fix. |

### Behavioral Spot-Checks / Empirical Reproductions (run directly during this verification, not taken from SUMMARY claims)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full Rust test suite | `cargo test --workspace` | `flint-core`: 5/5 `plan_column` unit tests pass; `zero_copy_alloc`: 2/2 pass | ✓ PASS |
| Build extension | `uv run maturin develop` | Wheel built and installed, no new warnings | ✓ PASS |
| Full Python test suite | `uv run pytest tests/python/ -q` | `29 passed in 0.94s` (was 28 in the prior verification cycle — +1 new regression test) | ✓ PASS |
| Single-chunk pointer-identity proof, isolated | `uv run pytest tests/python/test_zero_copy_pointer.py -v` | 4/4 pass, including the reverse-direction proof and the negative-control sanity test | ✓ PASS — no regression to D-06 |
| Multi-chunk `from_pandas` round-trip (independent reproduction of the original CR-01 scenario, NOT via the committed test file) | Ad hoc script: `pd.concat` of two 3-row `int64[pyarrow]` frames → `flint.Table.from_pandas(df).to_pandas()` | Source: 6 rows, 2 chunks. Result: **6 rows**, values `[1,2,3,4,5,6]` — correct | ✓ PASS — confirms CR-01 genuinely closed |
| Strict mode + copy_report on the SAME multi-chunk input | Ad hoc script: `flint.Table.from_pandas(df, strict=True)` then `.copy_report()` | `strict=True` succeeds (no exception); `copy_report()` returns `zero_copy=True, reason=None` for column `a`, despite an internal `concat` copy having just occurred | ✗ FAIL — confirms new DIAG-01/DIAG-02 gap, not previously visible before the CR-01 fix |
| Truths 4-5 (interop) regression check | `uv run pytest tests/python/test_interop.py tests/python/test_strict_mode.py tests/python/test_copy_report.py -v` | 16/16 pass | ✓ PASS — existing suite unaffected; the new gap above is NOT caught by any existing assertion |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|--------------|-------------|--------------|--------|----------|
| CONV-01 | 01-01, 01-02, 01-03, 01-05 | Convert non-null numeric/bool DataFrame to Arrow Table, true zero-copy | ✓ SATISFIED | Both single-chunk (pointer-identity + allocation proofs) and multi-chunk (CR-01 fix, independently reproduced) cases now correct. |
| CONV-02 | 01-01, 01-02, 01-03, 01-05 | Convert numeric/bool Arrow Table columns back to pandas, true zero-copy | ✓ SATISFIED | Reverse-direction pointer-identity proof passes; forward-side data is now correct for both single- and multi-chunk inputs. |
| DIAG-01 | 01-02 | Strict zero-copy mode that errors instead of silently falling back to a copy | ⚠️ SATISFIED (override) | Newly surfaced this cycle: `strict=True` does not error for a multi-chunk Arrow-backed column that the CR-01 fix now genuinely copies via `concat`. Consciously accepted and deferred to Phase 2 (CONV-08) — see frontmatter `overrides:`. |
| DIAG-02 | 01-02 | Per-column diagnostics explaining copy status/reason | ⚠️ SATISFIED (override) | Newly surfaced this cycle: `copy_report()` reports `zero_copy=True, reason=None` for the same column that was just copied. Consciously accepted and deferred to Phase 2 (CONV-08) — see frontmatter `overrides:`. |
| CAP-01 | 01-01, 01-04 | Export Table via PyCapsule Interface, accepted by pyarrow/Polars/DuckDB | ✓ SATISFIED | Re-confirmed this cycle, unaffected by the fix. |
| CAP-02 | 01-04 | Import a foreign Arrow object via PyCapsule Interface, zero-copy | ✓ SATISFIED | Re-confirmed this cycle; `from_arrow` never calls the fixed function. |

No orphaned requirements: all 6 requirement IDs mapped to this phase in REQUIREMENTS.md are claimed by at least one plan and appear above.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/flint-python/src/diagnostics.rs` / `crates/flint-python/src/pandas.rs` | n/a (structural, not a single line) | Stale/predicted diagnostics record no longer matching actual per-column behavior for multi-chunk Arrow-backed columns | 🛑 Blocker | See Truth 2/3 gaps above — DIAG-01/DIAG-02 both silently mislabel a copy as zero-copy for this input class. |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in either file touched by the CR-01 fix (`crates/flint-python/src/pandas.rs`, `tests/python/test_round_trip.py`) — confirmed by direct grep this cycle. The known-limitation note about this exact diagnostics gap is documented only in `01-05-PLAN.md`/`01-05-SUMMARY.md` (planning artifacts), not as an in-code marker — which is why it did not surface as a code-level debt marker, but the underlying behavior is nonetheless a live, reproducible defect against DIAG-01/DIAG-02.

## Human Verification Required

None required to confirm the defect itself — both the DIAG-01 and DIAG-02 violations were reproduced deterministically via direct script execution (no visual, real-time, or subjective element). What IS a human decision (see Escalation Gate note below) is whether to fix this now or knowingly accept it via a recorded override.

## Gaps Summary

**CR-01 (data-loss) is genuinely closed** — independently re-verified in this cycle via a fresh reproduction of the exact original failing scenario (`pd.concat` of two 3-row `int64[pyarrow]` frames, 6 rows/2 chunks in, 6 rows out, correct values), plus a clean `cargo test --workspace` and `uv run pytest tests/python/ -q` (29/29 pass). The single-chunk zero-copy pointer-identity proof (D-06) is unchanged and still passes, so the certified zero-copy path has not regressed.

**However, this re-verification surfaces one new blocking finding, adjacent to but distinct from CR-01:** the CR-01 fix converts a *silent truncation* (data loss) into a *silent, unreported copy* (diagnostics dishonesty) for the identical multi-chunk input. Concretely:

- `flint.Table.from_pandas(df, strict=True)` succeeds without error for a multi-chunk `ArrowDtype`-backed column, even though `import_column_via_pandas_stream` now performs a real `arrow::compute::concat` copy for that column. DIAG-01 exists specifically to prevent exactly this outcome ("errors instead of silently falling back to a copy").
- `table.copy_report()` reports `zero_copy=True, reason=None` for that same column, contradicting DIAG-02's promise to explain "whether a copy occurred and why."
- Root cause: `ColumnConversionRecord` (consumed identically by both `check_strict` and `build_copy_report`) is populated from `plan_column`'s a-priori decision, computed from dtype backend + arrow kind + contiguity alone — BEFORE `import_column_via_pandas_stream` runs and before the actual batch count is known. `plan_column` has no chunk-count awareness, so it cannot detect that the multi-batch branch just performed a copy.
- This was undetectable before the CR-01 fix (the pre-fix code never actually copied multi-chunk data — it silently truncated it, which coincidentally left `zero_copy=True` "accidentally true"). The fix's own plan (`01-05-PLAN.md`) and SUMMARY explicitly acknowledge this as a "known limitation" and attribute it to Phase 2's CONV-08 — but CONV-08 ("User can convert a Table with multiple chunks per column (ChunkedArray) to/from pandas", REQUIREMENTS.md) is scoped to ADDING multi-chunk conversion *capability*, not to diagnostics *honesty* for an input this phase's own prior verification cycle already ruled in-scope ("an entirely ordinary, non-exotic pandas construction... the phase's own stated scope already accepts"). Per Step 9b's deferred-item rule, deferral requires clear, specific evidence in a later phase's stated goal or success criteria — CONV-08's text does not mention diagnostics, strict mode, or copy_report at all, so this finding does not qualify as deferred and is reported as a live gap.

**Resolution:** Consciously accepted and deferred — recorded as an explicit override in this file's frontmatter (`overrides:`, accepted by John Columna, 2026-07-15). Rationale: this is a diagnostics-honesty gap on a multi-chunk edge case, not a data-loss or single-chunk regression; the phase's core zero-copy guarantee (single-chunk, the primary supported path) is independently re-verified and unaffected. Fixing `plan_column`/`ColumnConversionRecord` to be chunk-count-aware is deferred to Phase 2 alongside CONV-08's broader multi-chunk handling work, rather than as an isolated patch here.

This gap is scoped narrowly to the multi-chunk Arrow-backed column diagnostics path. It does **not** affect:
- CR-01's data-loss dimension (genuinely fixed, re-confirmed).
- Single-chunk round-trip correctness or diagnostics honesty (both proven, unchanged).
- CAP-01 (export) or CAP-02 (import) — both independently re-confirmed unaffected this cycle.

---

_Verified: 2026-07-15T00:00:00Z_
_Verifier: Claude (gsd-verifier)_
