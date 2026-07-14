---
phase: 01-core-zero-copy-round-trip-interop
plan: 04
subsystem: interop
tags: [rust, pyo3, pyo3-arrow, pyarrow, polars, duckdb, pycapsule, security, import]

# Dependency graph
requires:
  - phase: 01-01
    provides: Two-crate workspace, flint.Table pyclass composing pyo3_arrow::PyTable, PyCapsule export dunders
  - phase: 01-02
    provides: flint.FlintError exception hierarchy (pyo3 create_exception!), error-boundary conventions
provides:
  - "flint.from_arrow(obj): CAP-02 import of any Arrow PyCapsule-compliant foreign object (pyarrow Table, Polars DataFrame, DuckDB relation) into a flint Table, zero-copy"
  - "Table::from_pytable: internal composition constructor wrapping an already-marshalled pyo3_arrow::PyTable"
  - "tests/python/test_interop.py: CAP-01 export proven accepted by pyarrow, Polars, AND DuckDB; CAP-02 import proven from pyarrow and Polars (D-05)"
  - "Empirical resolution of RESEARCH.md Open Question 1 / Assumption A2: pinned duckdb 1.5.4 consumes a flint Table natively (no pyarrow intermediary needed)"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "CAP-02 import signature is &Bound<'_, PyAny>, not PyTable directly -- extraction is called explicitly inside the function body (one call site) so a validation failure can be caught and remapped onto flint.FlintError, and so the foreign object's stream dunder is provably consumed exactly once (never during PyO3's own argument-binding step)"
    - "Untrusted-capsule validation is entirely delegated to pyo3-arrow's own FromPyObject impl on PyTable (non-null pointer checks, schema/array-length consistency via arrow_array::ffi::from_ffi) -- this project's own code adds no unsafe block, only a PyErr -> flint.FlintError remap"
    - "Empirical spike-then-record pattern for an unconfirmed external-library behavior: a module-level probe function runs once at test-module-import time, the result is cached in a module constant, and the interop test consults that constant to pick between a native path and a documented fallback -- never silently skips the capability either way"

key-files:
  created:
    - crates/flint-python/src/import.rs
    - tests/python/test_interop.py
  modified:
    - crates/flint-python/src/lib.rs
    - crates/flint-python/src/table.rs
    - python/flint/__init__.py

key-decisions:
  - "from_arrow's obj parameter is typed &Bound<'_, PyAny>, not pyo3_arrow::PyTable, deliberately diverging from RESEARCH.md's literal code sketch (`fn from_arrow(obj: PyTable) -> Table`) -- binding the parameter as PyTable directly would run pyo3-arrow's FromPyObject extraction during PyO3's own argument-binding step, before the function body executes, leaving no place to catch a validation failure and remap it onto flint.FlintError (the plan's own acceptance criterion)."
  - "A malformed/inconsistent foreign object's error is remapped onto diagnostics::PyFlintError (the actual Python-visible flint.FlintError class), not left as pyo3-arrow's raw PyValueError/PyTypeError. The plan's acceptance criteria explicitly requires 'surfaces a flint.FlintError, not a panic' -- read literally as the catchable, isinstance-checkable exception, not merely 'some clean PyErr'; crate::error::FlintError (the Plan 01/02 internal thiserror enum) maps to builtin PyValueError/PyTypeError/PyNotImplementedError instead, so remapping through it would NOT satisfy the criterion as written."
  - "DuckDB's Open Question 1 / Assumption A2 resolved empirically via a module-level spike in test_interop.py that runs at import time (not inside a single test), so every DuckDB-touching test in the module shares one recorded answer rather than re-probing per test. Result: native consumption works with the pinned duckdb 1.5.4 (`duckdb.sql(\"FROM <flint.Table>\").arrow().read_all()`, no pyarrow registration step). The documented pyarrow-intermediary fallback is implemented but not exercised in this environment -- included so the test suite degrades gracefully rather than failing outright if a different duckdb version regresses this behavior."

patterns-established:
  - "CAP-02 (or any future foreign-object-accepting FFI entry point) should type its parameter as the generic Python object type and call the specific extraction inside the function body, whenever the extraction's own error needs to be remapped to this project's exception type."

requirements-completed: [CAP-01, CAP-02]

coverage:
  - id: D1
    description: "flint.from_arrow(obj) imports a pyarrow Table into a flint Table, zero-copy, by composing pyo3_arrow::PyTable's FromPyObject impl (no hand-rolled FFI)"
    requirement: "CAP-02"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#test_from_arrow_imports_pyarrow_table"
        status: pass
    human_judgment: false
  - id: D2
    description: "flint.from_arrow(obj) imports a Polars DataFrame into a flint Table"
    requirement: "CAP-02"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#test_from_arrow_imports_polars_dataframe"
        status: pass
    human_judgment: false
  - id: D3
    description: "A malformed/inconsistent foreign object (missing dunder, or a dunder that raises) surfaces flint.FlintError, never a panic/segfault (T-01-08)"
    requirement: "CAP-02"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#test_from_arrow_rejects_object_without_pycapsule_protocol, test_from_arrow_rejects_broken_stream_dunder_without_panicking"
        status: pass
    human_judgment: false
  - id: D4
    description: "A foreign object's __arrow_c_stream__ is consumed exactly once by flint.from_arrow, never invoked twice (T-01-09, DuckDB non-idempotency hazard)"
    requirement: "CAP-02"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#test_from_arrow_consumes_foreign_stream_dunder_exactly_once"
        status: pass
    human_judgment: false
  - id: D5
    description: "A flint Table exported via the PyCapsule Interface is accepted by pyarrow.table(), polars.from_arrow(), AND DuckDB (D-05), each with matching schema/row count"
    requirement: "CAP-01"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#test_pyarrow_accepts_flint_table_export, test_polars_accepts_flint_table_export, test_duckdb_accepts_flint_table_export"
        status: pass
    human_judgment: false
  - id: D6
    description: "DuckDB's native-PyCapsule-consumption status (RESEARCH.md Open Question 1 / Assumption A2) resolved empirically against the pinned duckdb 1.5.4, recorded in the test module, DuckDB never silently skipped"
    verification:
      - kind: unit
        ref: "tests/python/test_interop.py#_probe_duckdb_native_consumption (module-level spike, DUCKDB_NATIVE_CONSUMPTION == True)"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-07-14
status: complete
---

# Phase 1 Plan 4: PyCapsule Import (CAP-02) and Full 3-Library Interop Validation (D-05) Summary

**`flint.from_arrow(obj)` composes `pyo3_arrow::PyTable`'s `FromPyObject` impl for zero-copy CAP-02 import, remapping untrusted-capsule validation failures onto `flint.FlintError`; `test_interop.py` proves CAP-01/CAP-02 against pyarrow, Polars, AND a native DuckDB PyCapsule consumption path (Open Question 1 resolved empirically).**

## Performance

- **Duration:** 10 min
- **Started:** 2026-07-14T06:53:05Z (immediately following 01-03's completion commit)
- **Completed:** 2026-07-14T07:03:17Z
- **Tasks:** 2 completed
- **Files modified:** 5 (2 created, 3 modified)

## Accomplishments

- `flint.from_arrow(obj)` (CAP-02) implemented in `crates/flint-python/src/import.rs`: accepts any object claiming Arrow PyCapsule compliance, delegates the actual FFI marshalling to `pyo3_arrow::PyTable`'s existing `FromPyObject` impl (non-null pointer checks + schema/array-length consistency already built in), and wraps the result in a flint `Table` via a new `Table::from_pytable` composition constructor. No hand-rolled `FFI_ArrowArray`/`FFI_ArrowSchema` code was written (`.claude/CLAUDE.md` "What NOT to Use").
- Untrusted-input handling (T-01-08): a malformed/inconsistent foreign object (no `__arrow_c_stream__`, or a dunder that raises) is remapped from pyo3-arrow's raw `PyErr` onto `flint.FlintError` -- verified directly against both a plain object with no PyCapsule dunder and an object whose dunder itself raises, neither of which panics or segfaults.
- Consume-once discipline (T-01-09, RESEARCH.md Pitfall 3): `from_arrow`'s parameter is typed `&Bound<'_, PyAny>` (not `pyo3_arrow::PyTable` directly), so extraction happens at one explicit call site inside the function body rather than during PyO3's own argument-binding. An instrumented wrapper object proves the foreign stream dunder is invoked exactly once per import.
- `tests/python/test_interop.py` (D-05) proves CAP-01 export is accepted by `pyarrow.table()`, `polars.from_arrow()`, and DuckDB, and CAP-02 import works from both a pyarrow `Table` and a Polars `DataFrame` via `flint.from_arrow`.
- RESEARCH.md Open Question 1 / Assumption A2 resolved empirically (not assumed): a module-level spike in `test_interop.py`, run once at import time, confirms the pinned `duckdb` 1.5.4 consumes a flint `Table` **natively** via `duckdb.sql("FROM <flint.Table instance>").arrow().read_all()` -- no `pyarrow` registration step needed. The documented pyarrow-intermediary fallback is implemented in the same helper but was not exercised in this run (native path succeeded).

## Task Commits

Each task was committed atomically:

1. **Task 1: CAP-02 import path (from_arrow) with untrusted-capsule validation** - `a6d8f9b` (feat)
2. **Task 2: Interop validation against pyarrow, Polars, and DuckDB (D-05), starting with a DuckDB spike** - `321bbfc` (test)

**Plan metadata:** pending (this commit)

## Files Created/Modified

- `crates/flint-python/src/import.rs` - NEW: `from_arrow` pyfunction (CAP-02), composes `pyo3_arrow::PyTable`'s `FromPyObject`, remaps validation errors to `flint.FlintError`
- `crates/flint-python/src/table.rs` - adds `Table::from_pytable` (plain `impl` block, not a `#[pymethods]`), the composition constructor `from_arrow` uses to wrap an already-marshalled `PyTable`
- `crates/flint-python/src/lib.rs` - registers `from_arrow` on the `_flint` pymodule via `wrap_pyfunction!`
- `python/flint/__init__.py` - re-exports `from_arrow`, adds it to `__all__`
- `tests/python/test_interop.py` - NEW: CAP-01 export tests (pyarrow/Polars/DuckDB), CAP-02 import tests (pyarrow/Polars), malformed-object rejection tests, consume-once test, and the DuckDB native-consumption spike

## Decisions Made

- **`from_arrow`'s parameter is `&Bound<'_, PyAny>`, not `PyTable` directly** -- see key-decisions in frontmatter. RESEARCH.md's code sketch (`fn from_arrow(obj: PyTable) -> Table`) is illustrative, not verbatim (per Assumption A3's own caveat about the Code Examples being illustrative). Typing the parameter generically and calling `.extract::<PyTable>()` explicitly inside the function body is what makes the error-remap-to-`flint.FlintError` acceptance criterion achievable at all -- if PyO3 ran the extraction during its own argument-binding step (which it would if the parameter were typed `PyTable`), a validation failure would surface as pyo3-arrow's raw `PyValueError`/`PyTypeError` with no code of this project's own in the call path to intercept it.
- **Errors are remapped onto `diagnostics::PyFlintError` (the actual Python-visible `flint.FlintError` class), not `crate::error::FlintError`** (the Plan 01/02 internal `thiserror` enum, which maps to builtin `PyValueError`/`PyTypeError`/`PyNotImplementedError` and is never visible to Python as `flint.FlintError`). The plan's acceptance criteria states "surfaces a `flint.FlintError`, not a panic" -- read as the literal, `isinstance`-checkable Python exception class users would `except flint.FlintError` on. Using `crate::error::FlintError` instead would satisfy "not a panic" but not "flint.FlintError" as written; this is flagged here as a pre-existing minor naming ambiguity between the two "FlintError"s introduced across Plans 01/02, resolved in favor of the literal, testable reading for this plan's own acceptance criteria.
- **The DuckDB spike is a module-level function run once at test-module-import time (not inside a fixture or per-test), caching its result in a module constant `DUCKDB_NATIVE_CONSUMPTION`.** This matches the plan's "starting with an empirical DuckDB spike" framing (a one-time environment probe, not a per-test check) and lets every DuckDB-touching test consult one shared, recorded answer -- avoiding redundant probing and keeping the "native vs. fallback" decision auditable in one place.
- **DuckDB import (a `duckdb.sql(...)` relation passed through `flint.from_arrow`) was verified manually during implementation (interactively, not as a committed test)** and works correctly, but was not added as a committed test: the plan's Task 2 action text explicitly scopes CAP-02's required import fixtures to "a pyarrow Table and a Polars DataFrame," and D-05's DuckDB requirement is specifically about the export direction (CAP-01) plus the general non-idempotency hazard (already covered by the consume-once test using a pyarrow-based wrapper). Adding an uncalled-for DuckDB import test would have been scope creep beyond the plan's explicit fixture list.

## Deviations from Plan

### Auto-fixed Issues

None -- no Rule 1/2/3 bugs, missing-critical-functionality, or blocking issues were encountered. Two interpretation decisions were made (both documented above under "Decisions Made" and in the frontmatter `key-decisions`, not silently applied): (1) `from_arrow`'s parameter type diverges from RESEARCH.md's illustrative code sketch to make the error-remap acceptance criterion achievable; (2) errors are remapped onto `diagnostics::PyFlintError` specifically (the Python-visible `flint.FlintError`), not the differently-named `crate::error::FlintError` internal enum from Plans 01/02.

---

**Total deviations:** 0 auto-fixes; 2 documented interpretation decisions (both necessary to satisfy the plan's own literal acceptance criteria against an ambiguity between two same-named-but-different Rust types established in earlier plans). No scope creep.
**Impact on plan:** All stated success criteria, requirements (CAP-01, CAP-02), and the phase's threat-model mitigations (T-01-08, T-01-09, T-01-10) are met. The interpretation decisions above are precision/honesty notes for future readers, not functional gaps.

## Issues Encountered

- **DuckDB's replacement-scan mechanism (`duckdb.sql("FROM <local-variable-name>")`) requires the referenced identifier to be a valid, non-reserved SQL token** -- an initial interactive spike using a local variable literally named `table` failed with a SQL parser error (`table` is a reserved keyword), not a PyCapsule/consumption failure. Renamed the probe variable (e.g. `flint_table`, `probe_table`) and the native-consumption spike succeeded immediately. This was caught before writing the committed test, so no test in the final suite uses a SQL-reserved-word variable name for a `duckdb.sql("FROM ...")` call.
- **`DuckDBPyRelation.arrow()` returns a `pyarrow.RecordBatchReader`, not a `pyarrow.Table`** -- attempting `.num_rows`/`.schema.names` directly on the `.arrow()` result failed with `AttributeError`. Resolved by calling `.read_all()` on the reader to materialize a real `pyarrow.Table` before asserting row count/schema, consistent with the "materialize once, reuse the materialized result" consume-once discipline the plan requires.
- **A quick interactive check of whether the pinned duckdb's own relation objects error on a second `__arrow_c_stream__()`/consumption call (the specific behavior RESEARCH.md's Pitfall 3 / `duckdb/duckdb#17084` describes) did NOT reproduce in this environment** -- calling `pa.table(rel)` twice on the same DuckDB relation succeeded both times rather than raising `Invalid Input Error` on the second call. This does not change this plan's implementation or test strategy (the consume-once discipline is enforced regardless, as a defensive property of `flint.from_arrow` itself, verified via the instrumented-wrapper test) -- it is noted here as a data point: the non-idempotency hazard may be version-specific, scenario-specific (e.g. only triggered by relations built directly `FROM` an external capsule object rather than a plain SQL query), or already fixed in the currently pinned `duckdb` 1.5.4. The mitigation (never rely on a foreign object being re-consumable) remains correct and tested either way.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 1's full success criteria are now met: zero-copy pandas<->Table round-trip (Plans 01-02), dual formal zero-copy proofs (Plan 03), and full 3-library PyCapsule interop in both directions (this plan) are all implemented and tested.
- `flint.from_arrow` is stable, documented public API surface for Phase 2 and beyond -- any future phase adding new column types (nulls, strings, categoricals, datetime) does not need to touch the CAP-02 import path itself, since it delegates entirely to `pyo3-arrow`'s own type-agnostic marshalling; only `from_pandas`'s per-column `plan_column` classification (a separate module) will need extension for those types.
- No blockers. One item worth flagging forward: this plan's DuckDB empirical finding (native PyCapsule consumption works against 1.5.4) is captured only in this SUMMARY and in `test_interop.py`'s own docstring/spike -- if a future phase bumps the pinned `duckdb` version, re-running `test_interop.py` will automatically re-probe and fall back cleanly if the native path regresses, but the STATE.md blockers list does not currently track "verify DuckDB PyCapsule support on version bump" as an explicit follow-up. Not a blocker for Phase 1, since D-05 is satisfied either way (native confirmed now, fallback implemented and ready).

---
*Phase: 01-core-zero-copy-round-trip-interop*
*Completed: 2026-07-14*

## Self-Check: PASSED

All 6 key files verified present on disk (`import.rs`, `test_interop.py`, `table.rs`, `lib.rs`,
`python/flint/__init__.py`, this SUMMARY); both commits (`a6d8f9b`, `321bbfc`) verified present in
`git log --all`.
