//! D-06b: allocation-counting proof that `flint_core::from_numpy_buffer` -- the borrow-conversion
//! entry point wrapping an existing buffer via `arrow_buffer::Buffer::from_custom_allocation`
//! (the same technique `flint-python`'s `borrow_numpy_numeric_column` uses for the real numpy
//! borrow) -- makes NO heap allocation for the data buffer (the locked truth in this plan's
//! `must_haves`/objective/threat-model, all worded "no heap allocation *for the data buffer*").
//!
//! Complementary, not redundant, with `tests/python/test_zero_copy_pointer.py` (RESEARCH.md
//! Summary line 51): the Python test proves the SAME physical memory is shared (pointer
//! identity); this test proves the conversion path allocates no new heap memory proportional to
//! the data being converted. Neither alone proves zero-copy.
//!
//! ## Why this measures `bytes_total`, not `count_total == 0` (deviation from the RESEARCH.md/
//! 01-PATTERNS.md code-example sketch)
//!
//! `arrow_buffer::Buffer::from_custom_allocation` unconditionally performs one small, constant,
//! data-size-independent heap allocation for its internal `Arc<Bytes>` metadata wrapper
//! (confirmed by reading `arrow-buffer` 59.1.0's own source, `buffer/immutable.rs`:
//! `build_with_arguments` always calls `Arc::new(bytes)`), and wrapping the result as `ArrayRef`
//! (`Arc<dyn Array>`) costs a second, equally constant allocation. Both are metadata bookkeeping,
//! not a copy of the data buffer -- their size never changes regardless of how much data is
//! converted. This means `info.count_total == 0` (the literal assertion sketched in
//! RESEARCH.md's D-06b code example and 01-PATTERNS.md) is unreachable by ANY correct binding of
//! an external buffer into arrow-rs's real `Buffer` type, including the production
//! `borrow_numpy_numeric_column` path this project's core zero-copy claim already depends on --
//! it is not a defect specific to this test or to `from_numpy_buffer`.
//!
//! The test below instead asserts on `info.bytes_total`, sized against a fixture large enough
//! (80,000 bytes) that the small constant metadata allocations (well under 200 bytes, confirmed
//! empirically) cannot be confused with a genuine copy of the data buffer. This is a STRONGER,
//! not weaker, proof of the locked "no heap allocation for the data buffer" truth: it directly
//! measures whether an allocation proportional to the data size occurred, rather than merely
//! counting allocations (which conflates fixed-size Arc control-block bookkeeping with an actual
//! data copy).
//!
//! Guarded against the Pitfall 4 optimizer-elision false negative (RESEARCH.md lines 251-261,
//! 01-PATTERNS.md lines 190-193): the measured value is routed through `std::hint::black_box` so
//! LLVM cannot prove the conversion's result is unused and elide it, and a second test proves the
//! harness can actually detect a copy by measuring a deliberately-copying path against the same
//! large fixture and threshold.

use std::hint::black_box;

/// Fixture element count. Large enough that the data buffer's byte size (`DATA_LEN * 8` =
/// 80,000 bytes) dwarfs the small, constant, data-size-independent metadata allocations the
/// borrow path makes (confirmed empirically to be well under 200 bytes total) -- so a threshold
/// far below the data size unambiguously distinguishes "borrowed, not copied" from "copied".
const DATA_LEN: usize = 10_000;

/// An allocation-byte threshold between "constant metadata overhead" and "a genuine copy of the
/// data buffer". The borrow path's actual overhead is under 200 bytes (empirically measured);
/// a deliberate copy of `DATA_LEN` `i64`s allocates exactly `DATA_LEN * 8` = 80,000 bytes. 1,024
/// bytes sits comfortably between the two with a wide margin either side.
const METADATA_OVERHEAD_THRESHOLD_BYTES: u64 = 1024;

/// The core proof: converting a pre-existing, large `i64` buffer via
/// `flint_core::from_numpy_buffer` allocates far less heap memory than the data buffer itself --
/// i.e. it borrows the data rather than copying it. Only small, constant, data-size-independent
/// metadata bookkeeping (`Arc<Bytes>`/`Arc<dyn Array>` control blocks) is allocated; see the
/// module doc comment for why `count_total == 0` is not the right assertion here.
#[test]
fn from_numpy_buffer_allocates_nothing_for_the_data_buffer() {
    let data: Vec<i64> = (0..DATA_LEN as i64).collect();
    let ptr = data.as_ptr() as *const u8;
    let len = data.len() * std::mem::size_of::<i64>();
    assert!(len as u64 > METADATA_OVERHEAD_THRESHOLD_BYTES, "fixture must dwarf the threshold");

    let info = allocation_counter::measure(|| {
        // SAFETY: `data` is a local on the stack of this test function and outlives the
        // measured closure -- it is not dropped until after `info` above is computed, so the
        // pointer/length passed here remain valid for the entire borrow, satisfying
        // `from_numpy_buffer`'s safety contract.
        let array = unsafe { flint_core::from_numpy_buffer(ptr, len) };
        // Pitfall 4 guard: force the optimizer to treat the converted array as used, so it
        // cannot elide the conversion (and thus cannot produce a false-negative zero count).
        // `allocation_counter::measure` takes an `FnOnce()` (no return value), so the guard is
        // applied via a reference rather than returning the array out of the closure.
        black_box(&array);
    });

    assert!(
        info.bytes_total < METADATA_OVERHEAD_THRESHOLD_BYTES,
        "flint_core::from_numpy_buffer allocated {} bytes (data buffer is {len} bytes) -- this \
         is no longer a zero-copy borrow, it looks like a data copy",
        info.bytes_total
    );
}

/// Sanity check (Pitfall 4 warning sign): proves the allocation-counting harness above can
/// actually detect a non-zero-copy regression, by measuring a path that deliberately copies the
/// SAME large buffer via `.to_vec()` and confirming the allocated bytes are at least the size of
/// the data itself (a real copy), not merely nonzero (which the constant metadata overhead alone
/// would already satisfy and so would not be a meaningful sanity check).
///
/// This deliberately-copying path exists ONLY in this test, never in production conversion code.
#[test]
fn deliberately_copying_path_is_detected_by_the_allocation_counter() {
    let data: Vec<i64> = (0..DATA_LEN as i64).collect();
    let len = data.len() * std::mem::size_of::<i64>();

    let info = allocation_counter::measure(|| {
        let copied: Vec<i64> = data.to_vec(); // deliberate copy -- sanity-check only, never ship this
        black_box(&copied);
    });

    assert!(
        info.bytes_total >= len as u64,
        "sanity check failed: allocation-counter reported only {} bytes allocated for a path \
         that deliberately copies a {len}-byte buffer -- the harness itself cannot detect \
         non-zero-copy regressions, so a passing \
         `from_numpy_buffer_allocates_nothing_for_the_data_buffer` would not be trustworthy",
        info.bytes_total
    );
}
