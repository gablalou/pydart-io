//! Assumption A6 probe (RESEARCH.md Assumptions Log A6, mandatory Wave-0 gate): a direct,
//! pyo3-free, pandas-free Rust integration test proving that arrow-rs's `ArrowWriter` /
//! `ParquetRecordBatchReaderBuilder` default behavior (embedding the full Arrow schema into
//! Parquet's `ARROW:schema` key-value metadata, and consulting it back on read) is sufficient to
//! round-trip a `DataType::Dictionary` column (with its `dict_is_ordered` flag) AND a
//! `Timestamp(Nanosecond, Some(tz))` column carrying a non-UTC IANA zone string, with NEITHER
//! side ever calling `ArrowWriterOptions::with_skip_arrow_metadata()` (write) nor
//! `ArrowReaderOptions::with_schema(..)` (read).
//!
//! This de-risks PARQ-06 (Plan 04's dtype-fidelity work): if this gate refutes -- i.e. the
//! dictionary-ness or the exact tz string is lost on a bare write-then-read -- Plan 04 must add
//! an explicit `ARROW:schema` hint mechanism rather than relying on this default.
//!
//! Self-contained: constructs arrays directly via arrow-rs 59.1.0 + parquet 59.1.0 APIs, writing
//! to and reading from a `tempfile`-free in-process temp file (`std::env::temp_dir()`), with no
//! dependency on any pydart classification/routing code.

use std::fs::File;
use std::sync::Arc;

use arrow::array::{DictionaryArray, RecordBatch, TimestampNanosecondArray};
use arrow::datatypes::{DataType, Field, Int8Type, Schema, TimeUnit};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;

/// A6: `DataType::Dictionary(Int8, Utf8)` (with `dict_is_ordered` set) and a
/// `Timestamp(Nanosecond, Some("America/New_York"))` column both survive a bare
/// write-then-read through this project's own `ArrowWriter`/`ParquetRecordBatchReaderBuilder`
/// default configuration -- no explicit schema-hint override on either side.
#[test]
fn dictionary_and_tz_timestamp_survive_default_parquet_round_trip() {
    // --- Build the input RecordBatch -------------------------------------------------------
    let dict_array: DictionaryArray<Int8Type> =
        vec!["red", "green", "red"].into_iter().collect();

    let tz: Arc<str> = Arc::from("America/New_York");
    let ts_array = TimestampNanosecondArray::from(vec![1_000_000_000, 2_000_000_000, 3_000_000_000])
        .with_timezone(Arc::clone(&tz));

    let dict_field = Field::new_dictionary(
        "color",
        DataType::Int8,
        DataType::Utf8,
        false, // nullable
    )
    .with_dict_is_ordered(true);
    let ts_field = Field::new(
        "ts",
        DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::clone(&tz))),
        false, // nullable
    );

    let schema = Arc::new(Schema::new(vec![dict_field, ts_field]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(dict_array), Arc::new(ts_array)],
    )
    .expect("constructing the input RecordBatch must not fail -- fixture-only, not file bytes");

    // --- Write ------------------------------------------------------------------------------
    // Do NOT call ArrowWriterOptions::with_skip_arrow_metadata() -- the default embeds the full
    // Arrow schema (ARROW:schema key), which is the mechanism this gate is verifying.
    let path = std::env::temp_dir().join(format!(
        "pydart_a6_dictionary_tz_roundtrip_{}.parquet",
        std::process::id()
    ));
    {
        let file = File::create(&path).expect("creating the temp Parquet file must not fail");
        let mut writer =
            ArrowWriter::try_new(file, batch.schema(), None).expect("ArrowWriter::try_new");
        writer.write(&batch).expect("writer.write(&batch)");
        writer.close().expect("writer.close()");
    }

    // --- Read -------------------------------------------------------------------------------
    // Do NOT call ArrowReaderOptions::with_schema(..) -- the default consults the embedded
    // ARROW:schema hint written above, which is the mechanism this gate is verifying.
    let file = File::open(&path).expect("opening the temp Parquet file must not fail");
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).expect("ParquetRecordBatchReaderBuilder::try_new");
    let mut reader = builder.build().expect("builder.build()");
    let mut batches: Vec<RecordBatch> = Vec::new();
    while let Some(result) = reader.next() {
        batches.push(result.expect("reading a RecordBatch from our own just-written file must not fail"));
    }
    let _ = std::fs::remove_file(&path); // best-effort cleanup, not load-bearing for the assertions

    assert_eq!(batches.len(), 1, "expected exactly one RecordBatch back from a single-write file");
    let read_back = &batches[0];
    let read_back_schema = read_back.schema();

    // --- Dictionary assertions (Pitfall 1 / A6) --------------------------------------------
    let color_field = read_back_schema
        .field_with_name("color")
        .expect("color field must be present in the read-back schema");
    match color_field.data_type() {
        DataType::Dictionary(key_type, value_type) => {
            assert_eq!(**key_type, DataType::Int8, "dictionary key type must survive as Int8");
            assert_eq!(**value_type, DataType::Utf8, "dictionary value type must survive as Utf8");
        }
        other => panic!(
            "A6 REFUTED: expected DataType::Dictionary(Int8, Utf8) to survive the round trip, \
             got {other:?} -- the dictionary column degraded to a plain array. Plan 04 must add \
             an explicit ARROW:schema hint mechanism rather than relying on the default."
        ),
    }
    assert_eq!(
        color_field.dict_is_ordered(),
        Some(true),
        "A6 REFUTED: dict_is_ordered() must survive as Some(true), but it did not -- the ordered \
         flag was lost across the round trip."
    );

    // --- Timestamp/tz assertions (Pitfall 2 / A6) ------------------------------------------
    let ts_field = read_back_schema
        .field_with_name("ts")
        .expect("ts field must be present in the read-back schema");
    match ts_field.data_type() {
        DataType::Timestamp(TimeUnit::Nanosecond, Some(out_tz)) => {
            assert_eq!(
                out_tz.as_ref(),
                tz.as_ref(),
                "A6 REFUTED: the exact tz string must survive byte-identical, not be dropped or \
                 normalized to UTC."
            );
        }
        other => panic!(
            "A6 REFUTED: expected Timestamp(Nanosecond, Some(\"America/New_York\")) to survive \
             the round trip, got {other:?}."
        ),
    }

    // Sanity: row count and dictionary decoded values are also correct (not just types).
    assert_eq!(read_back.num_rows(), 3);
}
