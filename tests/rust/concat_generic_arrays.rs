//! Assumption A1 probe (RESEARCH.md Assumptions Log A1): a direct, pandas-free, pyo3-free Rust
//! unit test proving `arrow::compute::concat` succeeds (returns `Ok`, no panic) when given
//! multiple `DictionaryArray`, multiple `TimestampArray` carrying a timezone, and multiple
//! `DurationArray` inputs.
//!
//! This de-risks Plans 03-04's reliance on the existing multi-chunk concat fallback
//! (`crates/pydart-python/src/pandas.rs::import_column_via_pandas_stream`, lines 215-218): if
//! `concat` already handles these array types generically, Plans 03/04 need no type-specific
//! multi-chunk handling of their own for Dictionary/Timestamp(tz)/Duration columns.
//!
//! Self-contained: constructs arrays directly via arrow-rs 59.1.0 APIs, with no dependency on any
//! pydart classification/routing code.

use std::sync::Arc;

use arrow::array::{Array, DictionaryArray, DurationNanosecondArray, TimestampNanosecondArray};
use arrow::datatypes::{DataType, Int8Type, TimeUnit};

/// A1: `arrow::compute::concat` succeeds on multiple `DictionaryArray` (Int8-keyed string
/// dictionaries) inputs, with the concatenated length equal to the sum of input lengths.
#[test]
fn concat_succeeds_on_dictionary_arrays() {
    let a: DictionaryArray<Int8Type> = vec!["red", "green", "red"].into_iter().collect();
    let b: DictionaryArray<Int8Type> = vec!["blue", "green"].into_iter().collect();

    let result = arrow::compute::concat(&[&a, &b]);

    assert!(
        result.is_ok(),
        "concat should succeed for DictionaryArray inputs, got: {:?}",
        result.err()
    );
    let concatenated = result.unwrap();
    assert_eq!(concatenated.len(), a.len() + b.len());
}

/// A1: `arrow::compute::concat` succeeds on multiple `TimestampNanosecondArray` inputs carrying a
/// timezone, with the concatenated array's `DataType` retaining the timezone string.
#[test]
fn concat_succeeds_on_timestamp_arrays_with_timezone() {
    let tz: Arc<str> = Arc::from("America/New_York");
    let a = TimestampNanosecondArray::from(vec![1_000_000_000, 2_000_000_000])
        .with_timezone(Arc::clone(&tz));
    let b = TimestampNanosecondArray::from(vec![3_000_000_000]).with_timezone(Arc::clone(&tz));

    let result = arrow::compute::concat(&[&a, &b]);

    assert!(
        result.is_ok(),
        "concat should succeed for tz-aware TimestampNanosecondArray inputs, got: {:?}",
        result.err()
    );
    let concatenated = result.unwrap();
    assert_eq!(concatenated.len(), a.len() + b.len());
    match concatenated.data_type() {
        DataType::Timestamp(TimeUnit::Nanosecond, Some(out_tz)) => {
            assert_eq!(out_tz.as_ref(), tz.as_ref(), "timezone must be preserved through concat");
        }
        other => panic!("expected Timestamp(Nanosecond, Some(tz)), got {other:?}"),
    }
}

/// A1: `arrow::compute::concat` succeeds on multiple `DurationNanosecondArray` inputs, with the
/// concatenated length equal to the sum of input lengths.
#[test]
fn concat_succeeds_on_duration_arrays() {
    let a = DurationNanosecondArray::from(vec![10, 20, 30]);
    let b = DurationNanosecondArray::from(vec![40]);

    let result = arrow::compute::concat(&[&a, &b]);

    assert!(
        result.is_ok(),
        "concat should succeed for DurationNanosecondArray inputs, got: {:?}",
        result.err()
    );
    let concatenated = result.unwrap();
    assert_eq!(concatenated.len(), a.len() + b.len());
}
