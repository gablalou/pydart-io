---
phase: 03-parquet-io
verified: 2026-07-24T07:30:00Z
status: passed
score: 10/10 must-haves verified (behavior-dependent truths present + wired; durability/concurrency backstop items routed to human sign-off)
behavior_unverified: 0
overrides_applied: 0
human_verification:

  - test: "Kill the Python process (or fill the disk) mid-way through a large to_parquet() write, then inspect the target path."
    expected: "A partial/truncated Parquet file may be left on disk (std::fs::File::create truncates the target before writing) -- this is the documented, accepted behavior (no atomic-write/temp-then-rename guarantee), not a bug. Confirm this matches your operational expectations before shipping to users who might rely on write atomicity."
    why_human: "Cannot be exercised by an automated test without actually killing a process or filling a disk mid-write; the plan's own must_haves mark this a `verification: backstop` truth explicitly deferred to human judgment. Code inspection confirms this is the intended, disclosed design (03-01/02/03/04-PLAN.md's threat models accept this), not something the test suite can prove safe or unsafe."

  - test: "Run two writers targeting the same Parquet file path concurrently (two processes/threads both calling to_parquet(path)) and two readers concurrently reading the same file while it is being written."
    expected: "Last-writer-wins / OS-level file semantics apply; Flint provides no locking or synchronization. Concurrent reads of a file that is NOT being concurrently written are safe (each read opens its own read-only file handle; verified structurally -- no static/global/shared mutable state exists in parquet_io.rs, parquet_filter.rs, or table.rs's Parquet code paths)."
    why_human: "The read-side no-shared-state claim was verified structurally (grep for static/Mutex/OnceCell/thread_local in the Parquet modules found none, and read_parquet/read_parquet_multi only construct a Table from Ok(batch) -- never partially), so a failed read cannot yield a partially-populated Table. The write-under-concurrent-access race and the disk-full/kill-signal write path cannot be proven safe or characterized further by an automated check; this needs a human decision on whether the disclosed caller-responsibility framing is acceptable for ship."
mvp_mode_note: "ROADMAP.md marks this phase Mode: mvp, but the Phase 3 Goal text is not phrased as a User Story (\"As a ... I want to ... so that ...\") -- only 03-01-PLAN.md's <objective> section reframes the goal that way for its own narrower slice. Verification proceeded against the four ROADMAP Success Criteria (goal-backward, Option per Step 2a) rather than invoking the MVP User Flow Coverage table, since refusing to verify a fully-executed phase over a goal-phrasing technicality would not be useful. Flagging this discrepancy for the human's awareness rather than silently resolving it."
---

# Phase 3: Parquet IO Verification Report

**Phase Goal:** A user can read and write Parquet files directly against Flint's Arrow core, with compression, row-group configuration, statistics-driven pushdown, and correct round-trip of the full dtype range established in Phases 1-2.
**Verified:** 2026-07-24T07:30:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can read a Parquet file into a Table | VERIFIED | `Table.from_parquet` exists (`crates/flint-python/src/table.rs:358`), delegates to `flint_core::parquet_io::read_parquet_multi`. Independently exercised (not just via existing tests): numeric/bool round-trip confirmed live via `uv run pytest tests/python/test_parquet_roundtrip.py` (5/5 pass) and a fresh ad-hoc script reading a directory of files back into one Table. |
| 2 | User can write a Table to a Parquet file choosing a compression codec (snappy/zstd/gzip/uncompressed) and configuring row-group size | VERIFIED | `to_parquet(path, compression="snappy", row_group_size=1_048_576)` (`table.rs:430-431`); `build_writer_properties` is an exhaustive 4-arm match with an explicit error arm (`parquet_io.rs`); `tests/python/test_parquet_compression.py` (13 tests, all pass) confirms each codec round-trips and the on-disk column-chunk compression metadata actually reflects the chosen codec (via pyarrow's `ParquetFile.metadata`), not just value correctness. |
| 3 | Written Parquet files carry row-group statistics that enable predicate pushdown, and the user can apply column projection plus predicate pushdown when reading | VERIFIED | `surviving_row_groups` (row-group skip, driven by `StatisticsConverter`) proven to genuinely engage in isolation via `tests/rust/parquet_row_group_pruning.rs` (5/5 pass, real 3-row-group Parquet file, exact index-subset assertions) — not merely correct final rows. `RowFilter`/`ArrowPredicateFn` guarantees exact row-level correctness (`tests/python/test_parquet_pushdown.py`, 43 tests incl. a 36-case six-operator boundary property test against an unfiltered-then-pandas-filter baseline, all pass). CR-01 (critical review finding: cast-to-null silently drops matching rows for out-of-range literals against narrow int columns) was fixed (`df26820`) and independently re-verified by this verifier with a fresh script against an `int8[pyarrow]` column and out-of-range filter literals (`<300`, `!=300`) — both correctly returned all 5 rows, not 0. |
| 4 | A Parquet round-trip preserves logical types correctly, including tz-aware timestamps and categorical/dictionary encoding | VERIFIED (with an accepted, documented, user-approved scope carve-out) | `tests/python/test_parquet_fidelity.py` (8 tests, all pass). Read the actual assertions directly (not just the SUMMARY's description): the tz test asserts `str(result_df["ts"].dtype.pyarrow_dtype.tz) == "America/New_York"` (exact zone string, not epoch-only) plus unchanged instant values and ns precision at boundary values. The dictionary test asserts `pa.types.is_dictionary(field.type)`, `field.type.ordered is True`, and correct per-row values via Arrow-level inspection before even reconstructing pandas. Per the task's stated known-gap carve-out (checkpoint-approved 2026-07-24, STRIDE T-03-09): `.cat.categories` order and unused-category retention are NOT preserved (confirmed arrow-rs `DictEncoder` limitation, no `WriterProperties` fix in parquet 59.1.0, verified independently against pyarrow which does not share the limitation) — this is treated as accepted per the task's explicit instruction, not a verification failure. |

**Score:** 4/4 ROADMAP success criteria truths present, wired, and behaviorally verified (independently re-executed by this verifier, not just SUMMARY-trusted).

### Backstop (concurrency/durability) truths — routed to human verification

Every one of the four plans' `must_haves.truths` includes a final item marked `verification: backstop` (interrupted/concurrent read or write behavior). Per this agent's Step 3b/5b/Step 9 rules, a backstop truth abstains from VERIFIED/FAILED until discharged by explicit evidence; undischarged backstop items route to `human_needed`, not `passed`.

Disposition after code inspection:

- **Read-side backstop claims (Plans 01/03/04): DISCHARGED by code inspection.** `read_parquet`/`read_parquet_multi` (`crates/flint-core/src/parquet_io.rs`) only construct/return a `RecordBatch` on the `Ok` path — every parquet-crate error is `?`-propagated before any `Table` is built (`table.rs:from_parquet` only calls `PyTable::try_new` after `read_parquet_multi` returns `Ok`). No partial `Table` is reachable. A grep across `parquet_io.rs`, `parquet_filter.rs`, and `table.rs`'s Parquet code paths for `static`/`Mutex`/`RwLock`/`OnceCell`/`thread_local` returned zero matches — there is no shared mutable state, so concurrent reads of the same (unwritten-to) file are structurally safe.
- **Write-side backstop claims (Plans 01/02/04): NOT dischargeable by an automated check — routed to human verification.** "A write interrupted by process-kill or disk-full may leave a partial/truncated file" and "concurrent writes to the same path are unsynchronized (last-writer-wins)" are explicitly disclosed, accepted-caller-responsibility design decisions (per each plan's own threat model, e.g. T-03-02-adjacent framing), but cannot be proven or disproven by running the existing test suite or by static code inspection — they require either an actual kill-signal/disk-full experiment or a human sign-off that the disclosed behavior is acceptable to ship. See `human_verification` items below.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/flint-core/src/parquet_io.rs` | write_parquet/read_parquet/read_parquet_multi/build_writer_properties/surviving_row_groups, pyo3-free | VERIFIED | 517 lines, exists, substantive, wired (imported by `table.rs`), zero `.unwrap()`/`.expect()` on unchecked parse results (the two `.expect()` calls present are on a length-checked-above invariant, not a parse `Result`). |
| `crates/flint-core/src/parquet_filter.rs` | Op/FilterExpr/ScalarValue/could_match_range, exhaustively tested | VERIFIED | 277 lines; 24 `#[cfg(test)]` unit tests all pass; exhaustive `match` on `Op` (no wildcard arm, confirmed by reading the match). |
| `crates/flint-python/src/table.rs` | from_parquet/to_parquet #[pymethods] | VERIFIED | Signatures match plan spec exactly: `from_parquet(path, columns=None, filters=None)`, `to_parquet(path, compression="snappy", row_group_size=1_048_576)`. |
| `crates/flint-python/src/error.rs` | UnsupportedCodec/UnsupportedFilterOperator/ParquetSchemaMismatch/ParquetReadError/InvalidParquetPathArgument | VERIFIED | All 5 variants present and routed through `PyFlintError::new_err`/`PyValueError::new_err`/`PyNotImplementedError` as documented. |
| `tests/rust/parquet_dictionary_tz_roundtrip.rs` | Wave-0 A6 gate | VERIFIED | 1 test, passes (`cargo test --workspace`). |
| `tests/rust/parquet_row_group_pruning.rs` | PARQ-04 skip-engagement probe | VERIFIED | 5 tests, all pass, call the real `surviving_row_groups` (not a stub). |
| `tests/python/test_parquet_roundtrip.py`, `test_parquet_compression.py`, `test_parquet_pushdown.py`, `test_parquet_fidelity.py`, `test_parquet_multifile.py`, `test_wr01_nullability.py` | Full Python test coverage | VERIFIED | 80 Parquet-specific tests, all pass under a freshly rebuilt extension (`uv run maturin develop && uv run pytest`, run independently by this verifier, not taken from the SUMMARY). |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `table.rs::from_parquet`/`to_parquet` | `parquet_io.rs` | direct function calls (`read_parquet_multi`, `write_parquet`, `build_writer_properties`) | WIRED | Confirmed by reading the call sites; independently confirmed at runtime (ad-hoc scripts below). |
| `table.rs::from_parquet` filter parsing | `parquet_filter.rs::FilterExpr`/`Op` | `parse_filter_operator`/`parse_filter_value`, single parse point | WIRED | One `Vec<FilterExpr>` built once, passed to both `surviving_row_groups` and the `RowFilter` builder — confirmed by reading `parquet_io.rs`'s read path (single slice consumed by both). |
| write path | read path | `ARROW:schema` embedded metadata (no `with_skip_arrow_metadata()`/`with_schema()` overrides) | WIRED | Confirmed absent via source read; Wave-0 gate + fidelity tests empirically confirm the mechanism holds end-to-end. |

### Behavioral Spot-Checks (independently run by this verifier, not sourced from SUMMARY.md)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full Rust suite | `cargo test --workspace` | 24 + 3 + 1 + 5 + 2 unit/integration tests, all pass | PASS |
| Full Python suite (fresh build) | `uv run maturin develop && uv run pytest tests/python -q` | 141 passed | PASS |
| CR-01 regression (critical review fix) | ad-hoc script: `int8[pyarrow]` column, `filters=[("x","<",300)]` and `filters=[("x","!=",300)]` | 5/5 rows returned both times (not silently dropped to 0) | PASS |
| Multi-file directory read + schema mismatch | ad-hoc script: write 2 compatible files to a dir, read dir as one Table; then add a schema-divergent 3rd file, re-read | 6 rows concatenated correctly; schema mismatch raised `flint.FlintError` naming both files | PASS |
| Commit-hash provenance | `git cat-file -e <hash>` for all 16 commit hashes cited across the four SUMMARY.md files | All 16 present in `git log` | PASS |

### Requirements Coverage

| Requirement | Source Plan(s) | Description | Status | Evidence |
|---|---|---|---|---|
| PARQ-01 | 03-01, 03-04 | Read a Parquet file into a Table (incl. multi-file/directory) | SATISFIED | `test_parquet_roundtrip.py`, `test_parquet_multifile.py`, live re-verification above |
| PARQ-02 | 03-01, 03-02 | Write with chosen compression codec | SATISFIED | `test_parquet_compression.py` |
| PARQ-03 | 03-02 | Row-group size configuration | SATISFIED | `test_parquet_compression.py` boundary/ordering/empty tests |
| PARQ-04 | 03-03 | Row-group statistics enable pushdown | SATISFIED | `parquet_row_group_pruning.rs` (isolated skip-engagement proof) + `test_parquet_pushdown.py` |
| PARQ-05 | 03-03 | Column projection + predicate pushdown combinable | SATISFIED | `test_parquet_pushdown.py` projection+filter-combination tests |
| PARQ-06 | 03-04 | Logical-type fidelity (tz, categorical/dictionary) | SATISFIED (documented, accepted scope carve-out per user checkpoint) | `test_parquet_fidelity.py`; Known Gap in 03-04-SUMMARY.md; STRIDE T-03-09 |

All 6 requirement IDs declared across the four PLAN frontmatters (`[PARQ-01,PARQ-02]`, `[PARQ-02,PARQ-03]`, `[PARQ-04,PARQ-05]`, `[PARQ-01,PARQ-06]`) match exactly the 6 IDs REQUIREMENTS.md maps to Phase 3. No orphaned requirements.

### Anti-Patterns Found

None blocking. Scanned all Parquet-related Rust/Python files for TODO/FIXME/XXX/TBD/placeholder/stub markers, empty handlers, and hardcoded-empty-with-no-population-path patterns — found none except a pre-existing, unrelated `FlintError::NotImplemented` generic variant (not used by any Parquet code path). The two `.expect()` calls in `parquet_io.rs` are on a length-checked-above invariant (`batches.len() == 1` just confirmed), not unguarded parse results — consistent with the module's documented no-panic-on-untrusted-input discipline.

### Code Review Findings (03-REVIEW.md / 03-REVIEW-FIX.md) — independently re-verified, not SUMMARY-trusted

1 critical (CR-01) + 4 warnings (WR-01 through WR-04) were found by the code-review agent and all 5 were fixed per 03-REVIEW-FIX.md. This verifier independently confirmed:

- CR-01 (silent row-drop on out-of-range integer filter literals): fix present in source (`integer_bounds` helper, pre-cast range check) AND behaviorally re-verified live (see spot-check table above) — genuinely fixed, not just claimed.
- WR-01 (`paths[0]` unchecked index): `paths.first().ok_or_else(...)` confirmed present in source.
- WR-02 (directory discovery silently dropping non-file/errored entries): `collect::<Result<Vec<_>,_>>()` + `p.is_file()` filter confirmed present in `table.rs`.
- WR-03 (missing `UInt64` stats arm): confirmed present with the documented lossless-safety fallback.
- WR-04 (`dict_is_ordered` omitted from cross-file schema-equality check): confirmed `a.dict_is_ordered() == b.dict_is_ordered()` present in `fields_match`.

1 info-level finding (IN-01, redundant schema re-read) was explicitly and correctly left unfixed as out-of-scope for the fix pass.

### MVP Mode Note

ROADMAP.md marks Phase 3 `Mode: mvp`. The Phase 3 Goal text is a capability statement, not a `"As a ... I want to ... so that ..."` User Story (only Plan 01's own `<objective>` section reframes a narrower slice that way). Per the escalation-gate pattern, this discrepancy is surfaced for human awareness rather than silently resolved: verification proceeded against the four ROADMAP Success Criteria (the goal-backward fallback), since refusing to verify an already-fully-executed, review-fixed phase over a goal-phrasing technicality would not serve the user. No action is required unless the human wants the ROADMAP goal reworded to match MVP convention for future phases.

### Human Verification Required

1. **Write-interruption durability (process kill / disk full mid-`to_parquet`)**
   **Test:** Kill the Python process (or simulate a full disk) partway through a `to_parquet()` call on a large `Table`, then inspect the target file.
   **Expected:** A partial/truncated Parquet file may be left on disk — `std::fs::File::create` truncates the target up front, and Flint provides no atomic-write/temp-then-rename guarantee. This is the disclosed, accepted design (all four plans' threat models treat this as caller responsibility), not a code defect.
   **Why human:** Cannot be exercised by any test in the existing suite or proven/disproven by static code reading — it requires an actual kill-signal or disk-full experiment, or a human decision that the disclosed behavior is acceptable to ship as-is.

2. **Concurrent-write races on the same path**
   **Test:** Run two `to_parquet(same_path)` calls concurrently (two processes/threads) and confirm the resulting file matches only one writer's data (last-writer-wins, no corruption from an interleaved write).
   **Expected:** Undefined/OS-dependent outcome — Flint does not synchronize writers to the same path. Concurrent *reads* of a file that is not simultaneously being written are safe (verified structurally: no shared mutable state in the Parquet read path, and `read_parquet`/`read_parquet_multi` only ever return a fully-built `RecordBatch` on `Ok`, never a partial one).
   **Why human:** A genuine concurrent-write race cannot be safely or meaningfully reproduced by an automated check in this verification pass; requires either a live concurrency experiment or a human sign-off that the disclosed caller-responsibility framing is acceptable.

### Gaps Summary

No blocking gaps. The functional phase goal (PARQ-01 through PARQ-06, all four ROADMAP success criteria) is genuinely achieved, independently re-verified against the codebase (not just SUMMARY.md claims) via full Rust/Python test suite runs, targeted ad-hoc scripts reproducing the critical review fix and multi-file/schema-mismatch behavior, and direct reading of the fidelity test assertions. The one accepted scope carve-out (categorical `.cat.categories` order / unused-category retention not preserved through Parquet, an arrow-rs `DictEncoder` limitation with no available fix in v1) was explicitly pre-approved by the user at an execution-time checkpoint and is out of scope for this verification per the task's own framing.

The reason this phase is `human_needed` rather than `passed` is structural, not a functional defect: every one of the four plans' `must_haves.truths` includes a final `verification: backstop` item covering write-interruption durability and cross-process write concurrency. These are explicitly disclosed, accepted-risk design decisions in each plan's own threat model — but they are not dischargeable by any automated test or static-analysis check available in this verification pass, and per this agent's own decision rules a backstop truth left undischarged routes to human sign-off rather than a silent pass.

---

_Verified: 2026-07-24T07:30:00Z_
_Verifier: Claude (gsd-verifier)_
