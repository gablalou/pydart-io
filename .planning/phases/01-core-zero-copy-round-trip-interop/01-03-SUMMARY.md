---
phase: 01-core-zero-copy-round-trip-interop
plan: 03
subsystem: interop
tags: [zero-copy, proof, allocation-counter, pointer-identity, tests, verification-harness]

# Dependency graph
requires:
  - phase: 01-01
    provides: "Table.buffer_address(index), the flint-core from_numpy_buffer stub, allocation-counter dev-dependency"
  - phase: 01-02
    provides: "The confirmed to_pandas reverse zero-copy mechanism (PyTable::into_pyarrow + pyarrow's to_pandas(types_mapper=ArrowDtype)) this plan's reverse assertion targets"
provides:
  - "tests/python/test_zero_copy_pointer.py: pointer-identity zero-copy proof, forward (numpy int64 + int64[pyarrow]) and reverse direction, plus a discriminating-power sanity check"
  - "tests/rust/zero_copy_alloc.rs: allocation-counting zero-copy proof (bytes_total-based) with black_box elision guard and a deliberate-copy sanity check"
  - "flint_core::from_numpy_buffer: real implementation (was a Plan 01 stub), the pyo3-free analog of flint-python's numpy borrow technique"
affects: [01-04]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "allocation-counter proof discriminates on info.bytes_total against a data size that dwarfs constant Arc metadata overhead, not info.count_total == 0 -- see Decisions"
    - "Python buffer-address proofs read internal/private accessors (ndarray.ctypes.data, ArrowExtensionArray._pa_array chunk buffer .address) deliberately, since the proof's whole purpose is inspecting physical memory identity, not public API behavior"

key-files:
  created:
    - tests/python/test_zero_copy_pointer.py
    - tests/rust/zero_copy_alloc.rs
  modified:
    - crates/flint-core/src/table.rs
    - crates/flint-core/Cargo.toml

key-decisions:
  - "Rust allocation proof asserts on info.bytes_total (against an 80,000-byte fixture, threshold 1024 bytes) rather than the RESEARCH.md/01-PATTERNS.md code example's literal count_total == 0 -- arrow-rs's Buffer::from_custom_allocation unconditionally makes one small constant Arc<Bytes> metadata allocation (confirmed by reading arrow-buffer 59.1.0 source), and wrapping as ArrayRef costs a second constant allocation; neither is a copy of the data buffer, but together they make count_total == 0 unreachable by ANY correct binding of an external buffer into arrow-rs's real API, including the production borrow_numpy_numeric_column path this project's core claim depends on."
  - "flint_core::from_numpy_buffer implemented in this plan (not Plan 01), per 01-02-SUMMARY.md's explicit Next Phase Readiness note that the stub was 'ready for Plan 03 to fill in' -- pure-Rust (pyo3-free), unsafe fn, no owner-lifetime tracking (this crate has no Py<T> to hold); exists to let the allocation-counter proof run without a Python interpreter attached, mirroring but not calling flint-python's own numpy borrow."
  - "The Rust allocation test's deliberate-copy sanity check asserts bytes_total >= the data size (not merely > 0) -- a stronger sanity check than 'nonzero', since the constant metadata overhead alone would already satisfy a bare '> 0' assertion without actually proving the harness can detect a proportional data copy."
  - "Python reverse-direction test targets the exact mechanism 01-02-SUMMARY.md confirmed (PyTable::into_pyarrow + pyarrow's to_pandas(types_mapper=ArrowDtype)), read via the same private ArrowExtensionArray._pa_array accessor used for the forward ArrowDtype case, rather than assuming a direct-borrow mechanism that isn't what actually ships."

patterns-established:
  - "When a locked plan assertion (e.g. count_total == 0) is empirically unreachable due to a verified fact about a third-party dependency's real API, adapt the assertion to the underlying locked truth (here: 'no heap allocation for the data buffer', stated throughout the plan's objective/must_haves/threat model) rather than restructuring production code to chase an unreachable literal number -- document the empirical finding and the reasoning, per Rule 1's 'adapt to actual API' precedent (Plan 01's downcast->cast rename)."

requirements-completed: [CONV-01, CONV-02]

coverage:
  - id: D1
    description: "A Python test proves from_pandas shares the same physical data buffer (pointer identity), forward direction, for both a numpy-numeric and an ArrowDtype column"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "tests/python/test_zero_copy_pointer.py#test_from_pandas_forward_zero_copy_pointer_identity_numpy_numeric, test_from_pandas_forward_zero_copy_pointer_identity_arrow_dtype"
        status: pass
    human_judgment: false
  - id: D2
    description: "A Python test proves the confirmed reverse (to_pandas) mechanism shares the Table's physical buffer, targeting the actual shipping mechanism from 01-02-SUMMARY.md"
    requirement: "CONV-02"
    verification:
      - kind: unit
        ref: "tests/python/test_zero_copy_pointer.py#test_to_pandas_reverse_zero_copy_pointer_identity"
        status: pass
    human_judgment: false
  - id: D3
    description: "The pointer-identity proof is discriminating (would fail on a real copy), proven by asserting buffer_address differs from an unrelated DataFrame's source buffer"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "tests/python/test_zero_copy_pointer.py#test_from_pandas_fails_loudly_if_a_copy_is_introduced"
        status: pass
    human_judgment: false
  - id: D4
    description: "A Rust test proves the flint-core borrow-conversion entry point makes no heap allocation for the data buffer, guarded against optimizer elision"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "tests/rust/zero_copy_alloc.rs#from_numpy_buffer_allocates_nothing_for_the_data_buffer"
        status: pass
    human_judgment: false
  - id: D5
    description: "The allocation proof is sanity-checked to fail (i.e. detect) a deliberately-copying path, closing the Pitfall 4 false-negative gap"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "tests/rust/zero_copy_alloc.rs#deliberately_copying_path_is_detected_by_the_allocation_counter"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-07-14
status: complete
---

# Phase 1 Plan 3: Zero-Copy Proof Harness (Pointer Identity + Allocation Counting) Summary

**Two independent, mutually-required proofs that CONV-01/CONV-02's zero-copy claim is real: a Python pointer-identity test (forward numpy/ArrowDtype + confirmed reverse mechanism) and a Rust allocation-counting test (`bytes_total`-based, elision-guarded, sanity-checked against a deliberate copy).**

## Performance

- **Duration:** 20 min
- **Completed:** 2026-07-14
- **Tasks:** 2 completed
- **Files modified:** 4 (2 created, 2 modified)

## Accomplishments

- `tests/python/test_zero_copy_pointer.py` proves D-06a end-to-end: forward-direction pointer identity for a contiguous numpy `int64` column (`ndarray.ctypes.data`) and an `int64[pyarrow]` `ArrowDtype` column (Arrow buffer `.address`), both asserted exactly equal to `table.buffer_address(0)`; reverse-direction pointer identity targeting the exact mechanism 01-02-SUMMARY.md confirmed for `to_pandas` (`PyTable::into_pyarrow` + pyarrow's own `to_pandas(types_mapper=ArrowDtype)`); and a discriminating-power sanity check proving the accessor would actually catch a copy (compares against an unrelated DataFrame's buffer, asserting inequality).
- `tests/rust/zero_copy_alloc.rs` proves D-06b: `allocation_counter::measure` around `flint_core::from_numpy_buffer` (filled in this plan; a pyo3-free analog of `flint-python`'s numpy borrow), guarded by `std::hint::black_box` against Pitfall 4 optimizer elision, plus a sanity-check test that a deliberately-copying `.to_vec()` path is detected.
- Filled in `flint_core::from_numpy_buffer` (Plan 01's stub, explicitly flagged in 01-02-SUMMARY.md as "ready for Plan 03 to fill in") -- wraps an existing `i64` buffer via `arrow_buffer::Buffer::from_custom_allocation` with no copy of the data bytes, mirroring but not calling `flint-python`'s own `borrow_numpy_numeric_column` (which needs `pyo3`/GIL access this crate deliberately does not have).
- Empirically discovered and resolved a genuine tension between the plan's locked assertion wording (`count_total == 0`) and arrow-rs's real API: `Buffer::from_custom_allocation` always makes one small, constant, data-size-independent `Arc<Bytes>` metadata allocation, and wrapping the result as `ArrayRef` costs a second -- neither is a copy of the data buffer, but together they make `count_total == 0` unreachable by ANY correct binding of an external buffer into arrow-rs, including the project's own production zero-copy path. Resolved by asserting on `info.bytes_total` against a data size (80,000 bytes) that dwarfs the ~180-byte constant overhead -- a stronger proof of the plan's actual locked truth ("no heap allocation for the data buffer", the wording used throughout the objective/must_haves/threat model) than the literal count proxy the code-example sketch assumed.
- Verified empirically (before writing assertions, not assumed) that both pointer-identity mechanisms actually hold in this pinned environment (pandas 3.0.3 / pyarrow 25.0.0): forward numpy borrow, forward ArrowDtype import, and reverse `to_pandas` -- all three addresses matched exactly in manual spikes before being encoded as test assertions.

## Task Commits

1. **Task 1: Python pointer-identity proof (forward and reverse), D-06a**
   - `0547b00` (test) -- `tests/python/test_zero_copy_pointer.py`
2. **Task 2: Rust allocation-counting proof with optimizer-elision guard, D-06b**
   - `2e22b05` (test) -- `tests/rust/zero_copy_alloc.rs`, `flint_core::from_numpy_buffer` implementation, `crates/flint-core/Cargo.toml` test registration

**Plan metadata:** pending (this commit)

## Files Created/Modified

- `tests/python/test_zero_copy_pointer.py` - NEW: forward (numpy int64, ArrowDtype int64) and reverse pointer-identity proofs, plus a discriminating-power sanity check
- `tests/rust/zero_copy_alloc.rs` - NEW: `bytes_total`-based allocation-counting proof with `black_box` elision guard and a deliberate-copy sanity check
- `crates/flint-core/src/table.rs` - `from_numpy_buffer` implemented (was `unimplemented!()` stub); wraps an external `i64` buffer via `Buffer::from_custom_allocation`, marked `unsafe fn`
- `crates/flint-core/Cargo.toml` - added `[[test]] name = "zero_copy_alloc" path = "../../tests/rust/zero_copy_alloc.rs"` so `cargo test -p flint-core --test zero_copy_alloc` finds the repo-root-level test file

## Decisions Made

- **Rust allocation proof asserts `info.bytes_total < 1024` (fixture: 10,000 `i64`s = 80,000 bytes) rather than `info.count_total == 0`.** Confirmed by direct experimentation (probe tests, since discarded) and by reading `arrow-buffer` 59.1.0's own source (`buffer/immutable.rs`, `build_with_arguments`) that `Buffer::from_custom_allocation` unconditionally allocates one `Arc<Bytes>` control block (56 bytes measured), and returning `ArrayRef` (`Arc<dyn Array>`) costs a second constant allocation (112 bytes measured) -- 3 total allocations when the owner `Arc` is also constructed inside the measured closure. None of these three allocations scale with the data size; none copies the actual buffer bytes. This means `count_total == 0` (the RESEARCH.md/01-PATTERNS.md code-example sketch's literal assertion) is unreachable by any correct binding of an external buffer into arrow-rs's real `Buffer` type -- including this project's own production `borrow_numpy_numeric_column` zero-copy path, which makes the identical class of constant metadata allocations. `bytes_total` against a fixture size that dwarfs the ~180-byte constant overhead directly measures "was an allocation proportional to the data made" -- the property that actually matters for the locked truth ("no heap allocation for the data buffer", the consistent wording throughout this plan's objective, `must_haves.truths`, task name, and threat model), and is a strictly stronger discriminator than a bare allocation count (which conflates fixed-size bookkeeping with a genuine data copy).
- **`flint_core::from_numpy_buffer` implemented in this plan, touching `crates/flint-core/src/table.rs` and `crates/flint-core/Cargo.toml` despite the plan frontmatter's `files_modified` listing only the two test files.** 01-02-SUMMARY.md's "Next Phase Readiness" section explicitly states the stub is "ready for Plan 03 to fill in" and the plan's own Task 2 action text requires "wrap[ping] the flint-core borrow conversion entry point... in `allocation_counter::measure`" -- calling the existing stub (which panics via `unimplemented!()`) would make the test always fail, and the task cannot be completed without a real implementation to measure. Treated as Rule 2 (auto-add missing critical functionality: the task's own stated purpose requires it) rather than a scope violation; documented here per Rule 2's "no user permission needed, track as deviation" process.
- **`from_numpy_buffer` marked `unsafe fn`** (the Plan 01 stub was not `unsafe`): the function binds a raw pointer/length into an `arrow_buffer::Buffer` with no owner-lifetime tracking (this crate has no `pyo3`/`Py<T>` to hold, unlike `flint-python`'s `borrow_numpy_numeric_column`) -- the caller is fully responsible for the source buffer's lifetime, which is an unsafe contract per Rust convention and this project's own PyO3-safety-conscious ethos (CLAUDE.md).
- **Deliberate-copy sanity check asserts `bytes_total >= len` (the full data size), not merely `> 0`.** A bare `> 0` would already be satisfied by the constant metadata overhead alone (as discovered above), making it a much weaker sanity check that would not actually prove the harness can detect a genuine, proportional data copy -- the stronger assertion is a direct analog of the main test's own threshold logic.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - missing critical functionality] Implemented `flint_core::from_numpy_buffer` (was a panicking stub)**
- **Found during:** Task 2, before writing the allocation-counting test.
- **Issue:** `flint-core/src/table.rs`'s `from_numpy_buffer` was `unimplemented!()` (Plan 01 stub, explicitly deferred to this plan per 01-02-SUMMARY.md).
- **Fix:** Implemented a pyo3-free `unsafe fn from_numpy_buffer(ptr: *const u8, len: usize) -> ArrayRef` that wraps the buffer via `arrow_buffer::Buffer::from_custom_allocation` with no data copy, mirroring `flint-python`'s numpy borrow technique without needing `pyo3`.
- **Files affected:** `crates/flint-core/src/table.rs`.
- **Verification:** `cargo test -p flint-core --test zero_copy_alloc` passes; `cargo test --workspace` passes with no regressions.
- **Committed in:** `2e22b05`.

**2. [Rule 3 - blocking build config] Registered `tests/rust/zero_copy_alloc.rs` as an explicit `[[test]]` in `crates/flint-core/Cargo.toml`**
- **Found during:** Task 2, first `cargo test -p flint-core --test zero_copy_alloc` attempt.
- **Issue:** The plan's locked test path (`tests/rust/zero_copy_alloc.rs`, repo root) is outside `flint-core`'s own crate-relative `tests/` convention, so cargo would not discover it without an explicit `[[test]]` `path` entry.
- **Fix:** Added `[[test]] name = "zero_copy_alloc" path = "../../tests/rust/zero_copy_alloc.rs"` to `crates/flint-core/Cargo.toml`.
- **Files affected:** `crates/flint-core/Cargo.toml`.
- **Verification:** `cargo test -p flint-core --test zero_copy_alloc` runs and passes.
- **Committed in:** `2e22b05`.

**3. [Rule 1 - bug, API mismatch] `allocation_counter::measure` takes `FnOnce()` (no return value), not `FnOnce() -> T`**
- **Found during:** Task 2, first compile attempt (following RESEARCH.md's code-example sketch, which returns the converted value out of the measured closure).
- **Issue:** The pinned `allocation-counter` 0.8.1's actual signature is `pub fn measure<F: FnOnce()>(...)`, not `FnOnce() -> T` as the illustrative sketch implied -- `black_box(array)` / `black_box(copied)` (returning the value) failed to compile (`expected unit type ()`).
- **Fix:** Apply the Pitfall 4 elision guard via `black_box(&array)` / `black_box(&copied)` (by reference, as a statement) instead of returning the value from the closure.
- **Files affected:** `tests/rust/zero_copy_alloc.rs`.
- **Verification:** `cargo test -p flint-core --test zero_copy_alloc` compiles and passes.
- **Committed in:** `2e22b05`.

**4. [Rule 1 - bug, unreachable literal assertion] `count_total == 0` replaced with `bytes_total`-based assertion**
- See "Decisions Made" above for the full empirical finding and reasoning. Documented here as well since it is also a corrective fix to a locked-but-unreachable literal assertion, not solely a design preference.
- **Files affected:** `tests/rust/zero_copy_alloc.rs`.
- **Verification:** Confirmed via probe experiments (discarded, not committed) that the metadata allocation is genuinely constant-size and data-independent; `cargo test -p flint-core --test zero_copy_alloc` passes with the fixed assertion.
- **Committed in:** `2e22b05`.

---

**Total deviations:** 4 auto-fixed (1 missing-critical-functionality, 1 blocking build-config, 2 bugs/API-mismatches against the RESEARCH.md/01-PATTERNS.md illustrative sketches). No architectural changes, no scope creep beyond what Task 2's own stated purpose required (measuring a real, not stubbed, borrow-conversion entry point). All were required just to make the D-06b proof honestly measurable and correctly assert the plan's actual locked truth.
**Impact on plan:** All stated success criteria and requirements (CONV-01, CONV-02) are met. `count_total == 0` was empirically proven unreachable for any correct implementation using arrow-rs's real API (source-verified), not merely difficult to achieve in this specific function -- the `bytes_total`-based assertion is a more faithful proof of the plan's own stated intent than the literal proxy the code-example sketch assumed.

## Issues Encountered

- **`Buffer::from_custom_allocation`'s constant metadata allocation cost was not apparent from the RESEARCH.md/01-PATTERNS.md code-example sketch**, which asserted `count_total == 0` as if a minimal buffer wrap makes zero allocations. Caught before committing to the literal assertion by writing small probe tests (measuring `Buffer::from_custom_allocation` alone, `ScalarBuffer::new` alone, `PrimitiveArray::new` alone, and the final `Arc::new(dyn Array)` upcast alone) against the actual pinned `arrow` 59.1.0, then reading `arrow-buffer`'s own source to confirm the allocation is unconditional and size-constant rather than an artifact of this specific implementation. The probe test file was deleted before committing (never part of the shipped test suite).
- Verified all three pointer-identity mechanisms (`forward numpy`, `forward ArrowDtype`, `reverse to_pandas`) empirically via one-off `uv run python -c "..."` spikes against the pinned pandas 3.0.3/pyarrow 25.0.0 BEFORE writing the corresponding test assertions, rather than assuming the RESEARCH.md sketch's accessor paths (`_pa_array.chunk(0).buffers()[1].address`) would work as-is -- they did, and `buffers()[0]` was confirmed to be `None` (no validity bitmap needed) for a fully non-null chunk in this environment.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness

- Both halves of D-06 (pointer identity + no-heap-allocation) are now permanent, passing tests -- CONV-01/CONV-02's zero-copy claim is a measured fact, not an assertion, satisfying this plan's core purpose (the verification harness for the project's entire reason to exist).
- `flint_core::from_numpy_buffer` is now a real, tested, pyo3-free zero-copy buffer-wrap utility -- available (though not currently called by production `flint-python` code, which has its own GIL-aware `borrow_numpy_numeric_column`) for Plan 04 or later phases if a pyo3-free zero-copy entry point is ever needed outside a Python-attached context.
- No blockers. One item worth flagging forward: `from_numpy_buffer`'s `unsafe fn` contract (no owner-lifetime tracking) means it must never be called from production `flint-python` code as a substitute for `borrow_numpy_numeric_column` -- it exists purely as this plan's allocation-counting proof target, not as a general-purpose replacement. If a future phase considers unifying the two, the `Py<T>`-owner-holding requirement (T-01-03/T-01-04 mitigations, Plan 02) must be preserved.

---
*Phase: 01-core-zero-copy-round-trip-interop*
*Completed: 2026-07-14*
