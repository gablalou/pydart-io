//! Rust-side criterion micro-benchmark (BENCH-01) for the pyo3-free `from_numpy_buffer` entry
//! point -- isolates pure-Rust conversion kernel time from PyO3/GIL overhead, per CLAUDE.md's
//! explicit criterion rationale ("separating 'slow in Rust' from 'slow at the FFI boundary'").
//!
//! Mirrors `tests/rust/zero_copy_alloc.rs`'s call shape and safety-comment convention: builds a
//! `Vec<i64>` on the stack and passes its raw pointer/length into the `unsafe` borrow-conversion
//! call inside the timed closure.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

const NUMERIC_1M_ROWS: usize = 1_000_000;

fn bench_numeric_conversion(c: &mut Criterion) {
    let data: Vec<i64> = (0..NUMERIC_1M_ROWS as i64).collect();
    let ptr = data.as_ptr() as *const u8;
    let len = data.len() * std::mem::size_of::<i64>();

    c.bench_function("numeric_1M_conversion", |b| {
        b.iter(|| {
            // SAFETY: `data` is a local on the stack of this function and outlives every
            // iteration of this benchmark closure -- it is not dropped until after the
            // benchmark loop completes, so the pointer/length pair passed here remain valid
            // for the entire borrow, satisfying `from_numpy_buffer`'s safety contract (mirrors
            // `tests/rust/zero_copy_alloc.rs`'s identical safety justification).
            let array = unsafe { pydart_core::from_numpy_buffer(ptr, len) };
            black_box(&array);
        });
    });
}

criterion_group!(benches, bench_numeric_conversion);
criterion_main!(benches);
