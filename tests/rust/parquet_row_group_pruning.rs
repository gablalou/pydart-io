//! PARQ-04 skip-engagement integration probe (03-03-PLAN.md Task 1, advisor gap).
//!
//! Builds a real three-row-group Parquet file (disjoint numeric ranges [0,99], [100,199],
//! [200,299] on column "x", row_group_size=100) and proves that row-group-level pruning driven by
//! the file's OWN WRITTEN statistics returns exactly the correct surviving row-group indices --
//! proving PARQ-04's "the skip decision genuinely engages" claim independently of row-level
//! correctness (which `parquet_filter::could_match_range`'s own unit tests already cover in
//! isolation, without needing a real Parquet file).
//!
//! Task 1 commit: this probe calls `flint_core::parquet_filter::could_match_range` directly
//! against `StatisticsConverter`-extracted row-group min/max stats, because
//! `flint_core::parquet_io::surviving_row_groups` does not exist until Task 2. Task 2 upgrades
//! this file to call the real `surviving_row_groups` instead of duplicating its loop here, per
//! 03-03-PLAN.md's explicit instruction that "the committed end state must call the real
//! surviving_row_groups."

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray, Int64Array};
use arrow::datatypes::{DataType, Field, Int64Type, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::statistics::StatisticsConverter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::schema::types::ColumnPath;

use flint_core::parquet_filter::{could_match_range, Op, ScalarValue};

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Writes a 300-row, two-column ("x", "y") Parquet file with `row_group_size=100`, producing
/// three row groups with disjoint ranges [0,99], [100,199], [200,299] on "x". Column "y" has row
/// group statistics explicitly DISABLED, simulating the "stats-less column" case.
fn write_three_row_group_fixture() -> PathBuf {
    let n = FIXTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "flint_parquet_row_group_pruning_{}_{}.parquet",
        std::process::id(),
        n
    ));

    let schema = Arc::new(Schema::new(vec![
        Field::new("x", DataType::Int64, false),
        Field::new("y", DataType::Int64, false),
    ]));
    let x_values: Vec<i64> = (0..300).collect();
    let y_values: Vec<i64> = (0..300).map(|i| i * 7).collect();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(x_values)),
            Arc::new(Int64Array::from(y_values)),
        ],
    )
    .expect("build fixture batch");

    let properties = WriterProperties::builder()
        .set_max_row_group_row_count(Some(100))
        .set_column_statistics_enabled(ColumnPath::from("y"), EnabledStatistics::None)
        .build();

    let file = std::fs::File::create(&path).expect("create fixture file");
    let mut writer =
        ArrowWriter::try_new(file, schema, Some(properties)).expect("create ArrowWriter");
    writer.write(&batch).expect("write fixture batch");
    writer.close().expect("close writer");

    path
}

fn int64_scalar_at(array: &ArrayRef, idx: usize) -> Option<ScalarValue> {
    if array.is_null(idx) {
        return None;
    }
    Some(ScalarValue::Int64(array.as_primitive::<Int64Type>().value(idx)))
}

/// Duplicates the row-group-skip decision loop `surviving_row_groups` (Task 2) will own,
/// calling `could_match_range` directly against real `StatisticsConverter` output.
fn surviving_via_could_match_range(path: &Path, column: &str, op: Op, value: ScalarValue) -> Vec<usize> {
    let file = std::fs::File::open(path).expect("open fixture");
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("build reader");
    let arrow_schema = builder.schema().clone();
    let parquet_schema = builder.parquet_schema().clone();
    let metadata = builder.metadata().clone();

    let converter =
        StatisticsConverter::try_new(column, &arrow_schema, &parquet_schema).expect("converter");
    let mins = converter
        .row_group_mins(metadata.row_groups().iter())
        .expect("row group mins");
    let maxes = converter
        .row_group_maxes(metadata.row_groups().iter())
        .expect("row group maxes");

    (0..metadata.num_row_groups())
        .filter(|&i| {
            let min = int64_scalar_at(&mins, i);
            let max = int64_scalar_at(&maxes, i);
            could_match_range(op, &value, min.as_ref(), max.as_ref())
        })
        .collect()
}

#[test]
fn col_gt_250_keeps_only_last_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_via_could_match_range(&path, "x", Op::Gt, ScalarValue::Int64(250));
    assert_eq!(surviving, vec![2]);
}

#[test]
fn col_lt_50_keeps_only_first_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_via_could_match_range(&path, "x", Op::Lt, ScalarValue::Int64(50));
    assert_eq!(surviving, vec![0]);
}

#[test]
fn col_ge_100_and_lt_200_keeps_only_middle_row_group() {
    // AND semantics (D-24): intersect two independent could_match_range decisions.
    let path = write_three_row_group_fixture();
    let ge_100 = surviving_via_could_match_range(&path, "x", Op::Ge, ScalarValue::Int64(100));
    let lt_200 = surviving_via_could_match_range(&path, "x", Op::Lt, ScalarValue::Int64(200));
    let surviving: Vec<usize> = ge_100.into_iter().filter(|i| lt_200.contains(i)).collect();
    assert_eq!(surviving, vec![1]);
}

#[test]
fn col_eq_1000_keeps_no_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_via_could_match_range(&path, "x", Op::Eq, ScalarValue::Int64(1000));
    assert!(surviving.is_empty());
}

#[test]
fn filter_on_stats_less_column_keeps_all_row_groups() {
    // "y" has row-group statistics explicitly disabled in the fixture -- StatisticsConverter
    // resolves this to null min/max per row group, which could_match_range must treat as
    // conservatively-keep for every row group (never skip on missing stats).
    let path = write_three_row_group_fixture();
    let surviving = surviving_via_could_match_range(&path, "y", Op::Gt, ScalarValue::Int64(999_999));
    assert_eq!(surviving, vec![0, 1, 2]);
}
