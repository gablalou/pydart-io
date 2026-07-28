---
phase: 04-benchmark-release-readiness
plan: 02
subsystem: testing
tags: [pytest-benchmark, psutil, criterion, pyarrow, benchmarking, arrow, parquet]

# Dependency graph
requires:
  - phase: 04-01
    provides: benchmark harness skeleton (numeric_dense tracer scenario, pytest-benchmark + psutil + criterion methodology, BENCHMARKS.md skeleton)
provides:
  - Full six-scenario benchmark matrix (numeric_dense, numeric_nullable, mixed_object_string, chunked_multi_batch, categorical_ordered, categorical_unordered) crossed with from_pandas, to_pandas, write_parquet, read_parquet
  - Per-cell throughput (pytest-benchmark) and peak RSS (psutil subprocess) for all 24 matrix cells
  - Finalized, scrutiny-surviving BENCHMARKS.md with honest zero-copy/copy-fallback labels, a falsifiable pass bar, and its evaluation against real numbers
  - Human-signed-off finding that pydart's core "measurably faster than pyarrow" claim is currently NOT substantiated on any axis except to_pandas
affects: [04-03, 04-04, future FFI/GIL-boundary-performance phase]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Empirical zero-copy/copy-fallback grouping via Table.copy_report() rather than assumed-from-scenario-name labeling; reclassify+document rather than silently relabel when the empirical result disagrees with the plan's original grouping"

key-files:
  created:
    - benchmarks/test_bench_to_pandas.py
    - benchmarks/test_bench_parquet_io.py
  modified:
    - benchmarks/scenarios.py
    - benchmarks/memory/scenarios_memory.py
    - benchmarks/test_bench_from_pandas.py
    - BENCHMARKS.md

key-decisions:
  - "chunked_multi_batch reclassified from 'true zero-copy' to 'copy-fallback' throughout BENCHMARKS.md (Scenario Shapes table, Pass Bar Evaluation grouping/table, stated pass bar's scenario lists, Known Limitations) to match its empirical copy_report()==False result (arrow::compute::concat on multi-chunk columns is a real copy, per CR-01/CONV-08) -- human-confirmed at the Task 3 checkpoint"
  - "Task 3 pass-bar miss signed off as accepted and non-blocking for this plan: BENCH-01/BENCH-02 require an honest comparative benchmark suite reporting throughput+RSS regardless of outcome, which this document satisfies; the benchmark methodology itself was not reworked or re-tuned to chase the bar"
  - "FFI/GIL-boundary throughput investigation (pydart from_pandas/to_parquet/from_parquet are 3-43x slower than pyarrow on every axis except to_pandas) explicitly deferred to a future phase decision, to be resolved before any real PyPI release -- out of scope for this plan (Rule 4 architectural investigation, not a benchmark-harness task)"

requirements-completed: [BENCH-01, BENCH-02]

coverage:
  - id: D1
    description: "Full six-scenario x four-axis benchmark matrix (24 cells) each reporting both pytest-benchmark throughput and psutil-subprocess peak RSS"
    requirement: "BENCH-01"
    verification:
      - kind: unit
        ref: "uv run pytest benchmarks/ -q (all scenarios collected and passing, from Task 1)"
        status: pass
      - kind: other
        ref: "for s in <six scenarios>; do uv run python benchmarks/memory/measure_rss.py \"$s\"; done -- returns a peak-RSS integer per scenario"
        status: pass
    human_judgment: false
  - id: D2
    description: "BENCHMARKS.md finalized with per-scenario results table, honest zero-copy/copy-fallback labels (no blended headline number), a falsifiable pass bar, its evaluation, and the D-40/T-03-09 categorical Parquet caveat"
    requirement: "BENCH-02"
    verification:
      - kind: other
        ref: "grep -q '2x' BENCHMARKS.md && grep -Eqi 'zero.?copy' BENCHMARKS.md && grep -Eqi 'copy.?fallback' BENCHMARKS.md && grep -q 'T-03-09' BENCHMARKS.md && grep -Eqi 'dict_is_ordered' BENCHMARKS.md && grep -Eqi 'RSS' BENCHMARKS.md"
        status: pass
    human_judgment: false
  - id: D3
    description: "Human sign-off on the benchmark claim vs the stated pass bar (Task 3 checkpoint), including resolving the chunked_multi_batch true-zero-copy-vs-copy-fallback labeling conflict"
    human_judgment: true
    rationale: "Whether an honestly-reported pass-bar miss on the project's existential core-value claim blocks sealing the plan, and how to resolve a scenario-label conflict against the plan's original spec, are judgment calls the plan explicitly routed to a human via a gate=\"blocking\" checkpoint -- not something a passing test can certify."

duration: 12min
completed: 2026-07-28
status: complete
---

# Phase 4 Plan 2: Full Benchmark Matrix & Honest Pass-Bar Sign-Off Summary

**Full six-scenario x four-axis pydart-vs-pyarrow benchmark matrix (throughput + peak RSS), with a signed-off, honestly-reported finding that pydart is currently 3-43x slower than pyarrow at the Python-level call path on every axis except to_pandas.**

## Performance

- **Duration:** 12 min (this session, Task 3 resume-and-close only; Tasks 1-2 executed in a prior session)
- **Tasks:** 3 (Tasks 1-2 completed in a prior executor run; Task 3 completed this session)
- **Files modified:** 1 (BENCHMARKS.md, this session)

## Accomplishments

- Benchmark harness expanded from Plan 01's single numeric_dense tracer to the full six-scenario matrix (numeric_dense, numeric_nullable, mixed_object_string, chunked_multi_batch, categorical_ordered, categorical_unordered) across from_pandas, to_pandas, write_parquet, and read_parquet -- 24 matrix cells, each reporting both throughput and peak RSS
- BENCHMARKS.md finalized as the shippable, scrutiny-surviving claim: methodology, per-scenario empirical zero-copy/copy-fallback status (driven by `Table.copy_report()`, not assumed from scenario names), full results matrix, a stated falsifiable pass bar, and its honest evaluation against the measured numbers
- Task 3 human sign-off obtained on two separate decisions: (1) the pass-bar miss is accepted as an honest, non-blocking finding for this plan -- BENCH-01/BENCH-02 are satisfied by the suite's honesty and completeness, not by hitting the bar; (2) `chunked_multi_batch`'s scenario-label conflict (planned as "true zero-copy", empirically `copy_report()==False`) is resolved by reclassifying it as copy-fallback throughout the document
- D-40/T-03-09 categorical Parquet fidelity caveat carried through with full specificity (category order + unused categories don't survive a round-trip; values + dict_is_ordered do)

## Task Commits

Each task was committed atomically:

1. **Task 1: Expand scenario generators and throughput/RSS matrix** - `67d82c5` (feat) -- completed in a prior session
2. **Task 2: Finalize BENCHMARKS.md with matrix, honest labels, pass bar, D-40 caveat** - `83b3581` (docs) -- completed in a prior session
3. **Task 3: Sign off benchmark results against the stated pass bar** - `bb79735` (docs) -- this session: applied the human's two sign-off decisions (accept pass-bar miss as non-blocking; reclassify chunked_multi_batch as copy-fallback) as edits to BENCHMARKS.md

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified

- `benchmarks/scenarios.py` - Full six-scenario `SCENARIOS` list (prior session, Task 1)
- `benchmarks/memory/scenarios_memory.py` - Broadened to accept all six scenario names (prior session, Task 1)
- `benchmarks/test_bench_from_pandas.py` - Parametrized over all six scenarios (prior session, Task 1)
- `benchmarks/test_bench_to_pandas.py` - New: to_pandas benchmark cases, all six scenarios (prior session, Task 1)
- `benchmarks/test_bench_parquet_io.py` - New: Parquet write/read benchmark cases, all six scenarios (prior session, Task 1)
- `BENCHMARKS.md` - Finalized with results matrix, labels, pass bar (prior session, Task 2); reclassified chunked_multi_batch and recorded the Task 3 human sign-off (this session, Task 3)

## Decisions Made

- `chunked_multi_batch` reclassified from "true zero-copy" to "copy-fallback" throughout BENCHMARKS.md, per human confirmation at the Task 3 checkpoint -- its measured `copy_report()==False` result (a real `arrow::compute::concat` copy on multi-chunk columns, CR-01/CONV-08) contradicted the plan's original grouping; re-evaluated against the more lenient +/-20% copy-fallback bar using already-measured numbers, it still fails on all four axes.
- The stated pass-bar miss (every true-zero-copy scenario 3-19x slower than pyarrow on `from_pandas`) is accepted as an honest, non-blocking finding for this plan -- BENCH-01/BENCH-02 require an honest comparative suite reporting throughput+RSS regardless of outcome, which BENCHMARKS.md satisfies. The benchmark methodology itself was not reworked or re-tuned to chase the bar.
- Investigating and closing the underlying FFI/GIL-boundary throughput gap is deferred to a future phase decision, to be resolved before any real PyPI release -- explicitly out of scope for this plan (this is the orchestrator's concern going forward, not an action taken in this plan).

## Deviations from Plan

None - Task 3 was executed exactly as the checkpoint's resume instructions specified: reclassify `chunked_multi_batch` per the human's decision, record the human sign-off on the pass-bar verdict, and close the plan. No new bugs, missing functionality, blocking issues, or architectural changes were encountered.

## Issues Encountered

None. This was a continuation session resuming from a `checkpoint:human-verify` (`gate="blocking"`) left by a prior executor run; the human's decisions were already recorded verbatim in the resume instructions, so this session only needed to apply them as documentation edits to BENCHMARKS.md.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None.

## Next Phase Readiness

- BENCH-01 and BENCH-02 are satisfied: the full six-scenario x four-axis matrix is measured, documented, and honestly evaluated against a falsifiable pass bar.
- The project's core value claim ("measurably faster than pyarrow") is **not currently substantiated** by this benchmark matrix on any axis except `to_pandas` (near-parity or a pydart win). This is a real, human-signed-off finding, not a benchmark-harness defect -- the ~75ns criterion-measured Rust kernel confirms the bottleneck lives at the PyO3/GIL/pandas-interop boundary, not the Rust core.
- Per the human's separate instruction (not part of this plan's task, and not acted on here): the phase should pause before Plan 04-04's real PyPI release until the FFI/GIL bottleneck is investigated. Plans 04-03 and 04-04 were not touched by this session.

---
*Phase: 04-benchmark-release-readiness*
*Completed: 2026-07-28*

## Self-Check: PASSED

- FOUND: BENCHMARKS.md
- FOUND: .planning/phases/04-benchmark-release-readiness/04-02-SUMMARY.md
- FOUND commit: 67d82c5 (Task 1)
- FOUND commit: 83b3581 (Task 2)
- FOUND commit: bb79735 (Task 3)
