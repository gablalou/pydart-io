---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_phase: 04
current_phase_name: benchmark-release-readiness
status: executing
stopped_at: 04-04-PLAN.md Task 2 checkpoint (blocking-human, PyPI trusted publisher)
last_updated: "2026-07-28T12:10:00.000Z"
last_activity: 2026-07-28
last_activity_desc: 04-04 Task 1 (release.yml OIDC workflow) authored and committed; paused at Task 2 human-only PyPI trusted-publisher checkpoint
progress:
  total_phases: 4
  completed_phases: 3
  total_plans: 18
  completed_plans: 17
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-24)

**Core value:** Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow — this must work and must be provably faster, or the project has no reason to exist.
**Current focus:** Phase 04 — benchmark-release-readiness

## Current Position

Phase: 04 (benchmark-release-readiness) — EXECUTING
Plan: 4 of 4
Status: PAUSED — Task 1 of 3 done; Task 2 is a blocking-human checkpoint (PyPI trusted publisher configuration) awaiting a fresh continuation agent
Last activity: 2026-07-28 — 04-04 Task 1 (release.yml) authored and committed (601b787)

Progress: [█████████░] 94%

## Performance Metrics

**Velocity:**

- Total plans completed: 14
- Average duration: - min
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 5 | - | - |
| 02 | 5 | - | - |
| 03 | 4 | - | - |

**Recent Trend:**

- Last 5 plans: -
- Trend: -

*Updated after each plan completion*
| Phase 01 P01 | 24min | 2 tasks | 13 files |
| Phase 01 P02 | 35 | 2 tasks | 12 files |
| Phase 01 P03 | 20min | 2 tasks | 4 files |
| Phase 01 P04 | 10min | 2 tasks | 5 files |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase quick P260715-smf | 12min | 2 tasks | 2 files |
| Phase 03 P01 | 40min | 3 tasks | 6 files |
| Phase 03 P02 | 10min | 2 tasks | 4 files |
| Phase 03-parquet-io P03 | resumed | 3 tasks | 7 files |
| Phase 03 P04 | 39min | 3 tasks | 7 files |
| Phase 04 P01 | 40min | 3 tasks | 11 files |
| Phase 04 P02 | 12min | 3 tasks | 1 files |
| Phase 04 P03 | 94min | 3 tasks | 3 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: Vertical MVP slicing chosen over research's horizontal-layer suggestion — Phase 1 delivers a narrow but complete numeric-only round-trip (conversion + strict-mode diagnostics + PyCapsule interop) before broadening dtype coverage in Phase 2, per project_mode=mvp.
- Roadmap: Benchmarking (BENCH-01/02) and packaging (PKG-01/02/03) combined into a single Phase 4 "Benchmark & Release Readiness" phase — both are release-gating validation concerns for the core value claim, coarse granularity favors combining them rather than two thin phases.
- [Phase 01 P01]: Raised PyO3 abi3 floor from abi3-py39 to abi3-py311: pyo3-arrow 0.19.0's buffer-protocol methods require CPython stable-ABI buffer support (>=3.11)
- [Phase 01 P01]: Set pyproject.toml requires-python to >=3.12 to satisfy the RESEARCH.md-pinned numpy==2.5.1 dev dependency under uv's resolver
- [Phase 01 P01]: from_pandas/to_pandas delegate to pandas' own __arrow_c_stream__ export and pyarrow's own Table.to_pandas(types_mapper=pandas.ArrowDtype), avoiding hand-rolled FFI and private pandas attributes
- [Phase 01 P02]: Genuine zero-copy numpy borrow implemented by hand (Buffer::from_custom_allocation + Py<PyArray1<T>> owner) rather than pyo3-arrow's from_numpy(), which was found by reading its source to copy via PrimitiveArray::from_iter_values even on its contiguous fast path
- [Phase 01 P02]: flint.FlintError/ZeroCopyRequiredError implemented via pyo3::create_exception! with Py-prefixed Rust identifiers to avoid colliding with the internal thiserror FlintError enum
- [Phase 01 P02]: to_pandas intentionally does not call plan_column per column (every Table column is already Arrow memory, so the decision is always ZeroCopyBorrow) -- documented as a deviation rather than adding a symbolic always-same-result call
- [Phase 01]: Rust allocation proof asserts info.bytes_total against an 80,000-byte fixture (threshold 1024 bytes) rather than the RESEARCH.md sketch's literal count_total == 0: arrow-buffer's Buffer::from_custom_allocation unconditionally makes a small constant Arc<Bytes> allocation, and wrapping as ArrayRef costs a second constant allocation -- neither copies the data buffer, but together they make count_total == 0 unreachable for any correct binding into arrow-rs's real API
- [Phase 01]: flint_core::from_numpy_buffer implemented in Plan 03 (not Plan 01), per 01-02-SUMMARY.md's explicit note that the stub was ready for Plan 03 to fill in -- pyo3-free, unsafe fn, no owner-lifetime tracking, exists solely as the D-06b allocation-counting proof's measured entry point
- [Phase 01 P04]: from_arrow's obj parameter is typed &Bound<PyAny> (not pyo3_arrow::PyTable) so the extraction call site is explicit and its errors can be remapped onto flint.FlintError -- binding as PyTable directly would run extraction during PyO3's own argument-binding step, before any remap is possible
- [Phase 01 P04]: Untrusted-capsule validation errors are remapped onto diagnostics::PyFlintError (the Python-visible flint.FlintError), not crate::error::FlintError (the Plan 01/02 internal thiserror enum, which maps to builtin PyValueError/PyTypeError and is never visible as flint.FlintError)
- [Phase 01 P04]: DuckDB Open Question 1 / Assumption A2 resolved empirically: pinned duckdb 1.5.4 consumes a flint Table natively via duckdb.sql("FROM <obj>").arrow().read_all(), no pyarrow intermediary needed -- documented fallback implemented but unused
- [Phase ?]: [Quick 260715-smf] Concatenate multi-batch columns via arrow::compute::concat rather than rejecting multi-chunk input outright (fixes CR-01 silent truncation), while keeping the single-batch fast path as a direct Arc clone with no concat call
- [Phase 01 verification]: DIAG-01/DIAG-02 multi-chunk diagnostics-honesty gap (strict mode / copy_report don't detect the CR-01 fix's concat copy for multi-chunk columns) accepted via recorded override rather than fixed immediately -- root cause (plan_column has no chunk-count visibility) is the same mechanism CONV-08 needs to solve anyway, so bundling the fix there avoids a throwaway patch. Accepted by John Columna 2026-07-15.
- [Phase 02 P01]: classify_dtype restructured from dtype.kind-first to isinstance-first dispatch -- the enabling mechanism every later dtype-family slice (string, categorical, datetime/tz, timedelta) extends; also fixed a Rule-1 bug where the masked-extension rejection path mapped to builtin PyTypeError instead of flint.FlintError.
- [Phase 02 P02]: object-dtype validation (D-11) implemented as a Flint-owned pre-conversion scan (validate_object_column_contents) rather than trusting pyarrow's permissive type inference, per RESEARCH Pitfall 2.
- [Phase 02 P03]: Categorical round-trip fidelity required two separate fixes: Field::new_dictionary + with_dict_is_ordered to stop from_pandas silently dropping the dictionary ordered flag, and a per-column-type-aware to_pandas types_mapper closure to stop dictionary columns reconstructing as ArrowDtype instead of real pd.Categorical.
- [Phase 02 P05]: DIAG-01/DIAG-02 resolved via Strategy B -- import_column_via_pandas_stream now returns the observed RecordBatch count, and from_pandas corrects the already-computed ColumnConversionRecord post-hoc when count > 1, rather than giving plan_column its own chunk-count-aware second decision path. diagnostics.rs required no change. strict=True now correctly rejects multi-chunk columns with no bypass flag.
- [Phase 02 gate tooling]: Post-merge/regression gates were sniffing to bare `cargo test`, which never rebuilds the installed PyO3 extension via `maturin develop` -- Python-visible regressions from merged Rust changes went undetected until a manual full pytest run after wave 5 (21 stale-build failures, all resolved by rebuilding, zero were real code defects). Fixed via explicit `workflow.build_command`/`workflow.test_command` in config.json so future waves/phases in this project catch this class of regression.
- [Phase ?]: [Phase 03 P01]: Wave-0 A6 gate PASSED empirically -- arrow-rs default embedded ARROW:schema metadata preserves DataType::Dictionary(dict_is_ordered) and exact tz strings through a bare Parquet round-trip; Plans 02-04 rely on this default, no explicit schema-hint mechanism needed
- [Phase ?]: [Phase 03 P01]: flint-core::parquet_io returns parquet::errors::ParquetError (not FlintError) since flint-core cannot depend on flint-python's pyo3-coupled error type; mapped to FlintError::Other at the from_parquet/to_parquet PyO3 boundary
- [Phase ?]: [Phase 03 P02]: Resolved a plan/architecture conflict -- build_writer_properties stays in flint-core (pyo3-free) returning Result<WriterProperties, ParquetError> rather than the plan-specified Result<_, FlintError> (which would require a circular flint-core -> flint-python dependency); table.rs maps any Err directly to FlintError::UnsupportedCodec since codec is the function's sole fallible input once row_group_size==0 is pre-guarded
- [Phase ?]: [Phase 03 P02]: Confirmed set_max_row_group_row_count (not deprecated set_max_row_group_size, not byte-based set_max_row_group_bytes) as the correct row-count row-group setter for pinned parquet 59.1.0 by reading the vendored crate source directly
- [Phase ?]: ScalarValue is a plain, arrow-crate-free enum (Int64/Float64/Bool/Utf8); Int64/Float64 cross-type comparisons widen to f64; Utf8 column stats are never trusted for row-group pruning (truncation risk) though still filtered exactly via RowFilter; filter-value extraction checks bool before int before float before str.
- [Phase ?]: Resumed a mid-task interruption (prior executor terminated by a provider session/usage-limit error): reviewed uncommitted parquet_io.rs/error.rs work as already correct, completed only the missing table.rs wiring gap.
- [Phase ?]: [Phase 03 P04]: WR-01/D-31 fixed -- build_field sources nullability from declared source pandas schema, not observed null_count(); resolves the 02-REVIEW.md concat_tables ArrowInvalid failure
- [Phase ?]: [Phase 03 P04]: D-21 multi-file/directory Parquet read delivered with strict cross-file schema-match (ParquetSchemaMismatch on divergence, never silent union) and deterministic lexicographic directory ordering
- [Phase ?]: [Phase 03 P04]: CHECKPOINT (user-approved, Option A) -- categorical/dictionary Parquet fidelity tests scoped to what arrow-rs's DictEncoder actually guarantees (DataType::Dictionary, dict_is_ordered, per-row values); exact .cat.categories order and unused-category retention are NOT guaranteed (arrow-rs-vs-pyarrow divergence, no WriterProperties fix in parquet 59.1.0) -- documented as accepted risk, real correctness concern only for ordered categoricals
- [Phase 03 code review]: CR-01 (Critical, fixed df26820) -- evaluate_predicate's row-level filter relied on arrow::compute::cast's null-on-overflow semantics with no range check, silently returning wrong row sets for out-of-range integer filter literals against narrower integer columns (violated D-26). Fixed via an integer_bounds pre-cast range-check helper; independently re-verified live by the phase verifier. Also fixed same pass: WR-01 (unchecked paths[0] panic), WR-02 (silently swallowed read_dir errors + missing is_file() filter), WR-03 (missing UInt64 stats arm), WR-04 (missing dict_is_ordered in cross-file schema-match).
- [Phase 03 secure-phase]: Full STRIDE register across all 4 plans (10 threats: T-03-01 through T-03-09 plus T-03-SC/T-03-02) verified closed -- threats_open: 0, ASVS level 1, see 03-SECURITY.md. Three code-review-fixed bugs (T-03-04, T-03-07, T-03-08) mapped directly onto the CR-01/WR-02/WR-04 findings above.
- [Phase ?]: [Phase 04 P01]: numpy dev-pin loosened from exact ==2.5.1 to a range (>=2.3,<2.6) so uv's universal resolver can pick numpy 2.4.x for Python 3.11 and 2.5.x for 3.12+ within one uv.lock, empirically resolving RESEARCH.md Assumption A1 and letting requires-python drop back to >=3.11 (D-35)
- [Phase ?]: [Phase 04 P01]: pyproject.toml [project].name changed to pydart-io (D-41) to resolve the real-PyPI name collision; import path pydart, module-name pydart._pydart, and all Rust crate/exception names unchanged
- [Phase ?]: [Phase 04 P01]: .claude/CLAUDE.md pandas version-compatibility claim corrected from >=2.2 to >=3.0 (D-36), closing the WR-02 CoW-safety documentation gap carried forward from Phase 3
- [Phase ?]: [Phase 04 P01]: crates/pydart-core/benches/conversion_bench.rs was written during Task 2 (not Task 3) because Cargo parses every workspace member's manifest and the new [[bench]] entry in Cargo.toml requires the file to exist for uv sync --dev's editable maturin build to succeed at all; the file was held uncommitted until Task 3, where it was committed as that task's canonical deliverable
- [Phase ?]: [Phase 04 P01]: benchmark result -- pydart.Table.from_pandas is currently ~2.8-3.5x slower than pyarrow.Table.from_pandas on the numeric_dense scenario at the full Python-level call path, while the isolated Rust conversion kernel (criterion) runs in ~75ns regardless of row count -- the gap lives at the PyO3/GIL/Python-object boundary, not the Rust core; flagged as a concern for Plan 02/03 to investigate before validating the core 'measurably faster than pyarrow' claim
- [Phase ?]: [Phase 04 P02]: chunked_multi_batch reclassified from 'true zero-copy' to 'copy-fallback' throughout BENCHMARKS.md to match its empirical copy_report()==False result (arrow::compute::concat on multi-chunk columns, CR-01/CONV-08) -- human-confirmed at the Task 3 checkpoint
- [Phase ?]: [Phase 04 P02]: Benchmark pass-bar miss (every true-zero-copy scenario 3-19x slower than pyarrow on from_pandas) signed off as accepted and non-blocking -- BENCH-01/BENCH-02 require an honest comparative suite reporting throughput+RSS regardless of outcome, which BENCHMARKS.md satisfies without reworking the methodology
- [Phase ?]: [Phase 04 P02]: FFI/GIL-boundary throughput investigation (pydart 3-43x slower than pyarrow on every axis except to_pandas) deferred to a future phase decision, to be resolved before any real PyPI release
- [Phase ?]: [Phase 04 P03]: Task 1/2 authored .github/workflows/{wheels,ci,compat-matrix}.yml and proved a host wheel installs via uv; rust-numpy 0.29.0 hardcodes its ABI feature version in-crate (verified via its own build.rs and npyffi source) rather than reading an installed numpy at build time, so no explicit numpy-floor pin is applicable/needed in the wheels.yml build step (Pitfall 3 backstop finding)
- [Phase ?]: [Phase 04 P03]: All three new workflows default to read-only permissions (contents: read) per T-04-02 mitigation; id-token: write is deferred entirely to Plan 04's dedicated publish job
- [Phase ?]: [Phase 04 P03] macos-13 GitHub-hosted runner image was retired in Dec 2025 (confirmed via GitHub API job inspection showing no runner assigned + GitHub's own changelog); wheels.yml's x86_64-apple-darwin cell switched to macos-15-intel to unblock the D-34 wheel matrix
- [Phase 04 P04]: release.yml's `wheels`/`compat-matrix` jobs duplicate (not reference) the matrix definitions already in wheels.yml/compat-matrix.yml, because GitHub Actions `needs:` can only depend on jobs declared in the same workflow file -- the plan's own `needs: [wheels, compat-matrix]` requirement is only achievable by defining those jobs inside release.yml itself

### Pending Todos

None yet.

### Blockers/Concerns

- ~~Phase 2 (carried forward from Phase 1 verification override): CONV-08 DIAG-01/DIAG-02 multi-chunk diagnostics honesty gap~~ -- **Resolved** in Phase 2 Plan 05 (see 02-VERIFICATION.md and 02-05-SUMMARY.md).
- ~~Phase 3 (from 02-REVIEW.md WR-01, demonstrated/reproducible): `build_field` in `crates/flint-python/src/pandas.rs` derives Arrow field nullability from the current batch's observed `null_count() > 0` rather than the source pandas dtype's declared nullability.~~ -- **Resolved** in Phase 3 Plan 04 (see 03-04-SUMMARY.md): `build_field` now sources nullability from the declared source schema; `concat_tables` reproduction test passes.
- Phase 3 (from 02-REVIEW.md WR-02, structurally real but not reproduced under pinned config): the zero-copy numpy buffer borrow (`borrow_numpy_numeric_column`/`NumpyBufferOwner`) has no independent immutability guarantee — it relies entirely on pandas' Copy-on-Write to prevent post-borrow mutation from corrupting the Arrow buffer. Did not reproduce under pinned pandas 3.0.3 (CoW blocked all three tried mutation paths), but CLAUDE.md claims `pandas >= 2.2` support with no runtime floor pinned in pyproject.toml, and CoW is off by default pre-3.0 — a latent gap on nominally-supported configurations.
- ~~Phase 3 (research-flagged): categorical/dictionary Parquet round-trip edge cases and tz-aware timestamp handling warrant verification against current pyarrow issues (#35259, #1688) at plan time.~~ -- **Addressed** in Phase 3 Plan 04 (`test_parquet_fidelity.py`): tz-aware round-trip verified exact (zone string + instant + ns precision); categorical round-trip verified for `DataType::Dictionary`/`dict_is_ordered`/per-row values, with the `.cat.categories`-order divergence from pyarrow documented as an accepted gap (T-03-09), not left unverified.
- ~~Phase 3 (research-flagged): confirm pandas ArrowDtype import-side support status (pandas 3.0.x) before finalizing pandas-interop reverse direction — may affect Phase 2 design already, verify at Phase 2 plan time too.~~ -- Moot: Phases 2 and 3 both shipped and verified without this surfacing as a blocker.
- Phase 4 (research-flagged): benchmarking methodology (criterion/pytest-benchmark/codspeed) and manylinux/glibc floor are MEDIUM-confidence, task-derived recommendations — validate current best practice at plan time.
- Phase 3 (accepted, documented -- see 03-04-SUMMARY.md Known Gap): arrow-rs's ArrowWriter/DictEncoder reassigns dictionary keys in first-occurrence-during-encoding order and drops unused categories on Parquet write, so a categorical's .cat.categories order and unused categories do NOT survive a Parquet round-trip (values and dict_is_ordered DO survive correctly). Cosmetic for unordered categoricals; a real correctness concern for ordered categoricals since the < relationship between categories can silently change. No WriterProperties fix exists in parquet 59.1.0 (arrow-rs-only constraint); pyarrow does not share this limitation. Surface in Phase 4 release docs if categorical fidelity is a headline interop claim.
- Phase 4 (from 04-02, human-signed-off finding): pydart's core 'measurably faster than pyarrow' value claim is NOT currently substantiated -- from_pandas/to_parquet/from_parquet are 3-43x slower than pyarrow on every scenario except to_pandas (near-parity/win). Accepted as an honest, non-blocking finding for Plan 04-02 (BENCH-01/BENCH-02 only require an honest suite, not a passing bar). User wants the phase paused before Plan 04-04's real PyPI release until the FFI/GIL bottleneck is investigated.
- Phase 4 Plan 4 (blocking-human checkpoint, IN PROGRESS): `.github/workflows/release.yml` is authored and committed (`601b787`), but PKG-03's real-PyPI half is NOT satisfied yet -- Task 2 requires a human with PyPI account ownership to configure a Trusted Publisher (GitHub OIDC) for `pydart-io` (repo + `release.yml` + `pypi` environment) and re-verify the name is still free, before Task 3 can trigger the release and verify a real `uv add pydart-io` install. Do not mark PKG-03 complete until both are done.
- ~~Phase 4 Plan 3 Task 3 (blocking-human checkpoint): repo has no git remote -- wheels.yml/ci.yml/compat-matrix.yml are authored and a host wheel was proven locally, but the full D-34 wheel matrix and D-37 compat matrix cannot run until the repo is created on GitHub and pushed.~~ -- **Resolved**: public repo `gablalou/pydart-io` created and pushed; all five D-34 wheel cells, ci.yml, and both compat-matrix.yml endpoints confirmed green on GitHub Actions (run IDs 30349901732/30349901156/30349901256 on commit fdeca01), after fixing a retired `macos-13` runner image (see 04-03-SUMMARY.md and the decision above).

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260715-smf | Fix CR-01: from_pandas silently truncates multi-chunk Arrow-backed pandas columns to only the first chunk | 2026-07-15 | b5df2da | [260715-smf-fix-cr-01-from-pandas-silently-truncates](./quick/260715-smf-fix-cr-01-from-pandas-silently-truncates/) |
| 260727-ih5 | Rename the project from flint to pydart across the entire codebase (crate names, Python import path, module name, docs, tests) | 2026-07-27 | ec5bea7 | [260727-ih5-rename-the-project-from-flint-to-pydart-](./quick/260727-ih5-rename-the-project-from-flint-to-pydart-/) |

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| v2 Requirements | IO-01 (CSV read/write), IO-02 (JSON read/write) | Deferred to v2 | Project init |

## Session Continuity

Last session: 2026-07-28T10:23:31.596Z
Stopped at: Completed 04-03-PLAN.md
Resume file: None
