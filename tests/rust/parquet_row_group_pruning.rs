//! PARQ-04 skip-engagement integration probe (03-03-PLAN.md Task 1, advisor gap).
//!
//! Builds a real three-row-group Parquet file (disjoint numeric ranges [0,99], [100,199],
//! [200,299] on column "x", row_group_size=100) and proves that row-group-level pruning driven by
//! the file's OWN WRITTEN statistics returns exactly the correct surviving row-group indices --
//! proving PARQ-04's "the skip decision genuinely engages" claim independently of row-level
//! correctness (which `parquet_filter::could_match_range`'s own unit tests already cover in
//! isolation, without needing a real Parquet file).
//!
//! Task 2 upgrade: this probe now calls the REAL `flint_core::parquet_io::surviving_row_groups`
//! directly (rather than duplicating its skip-decision loop against `could_match_range`, as the
//! Task 1 stub version of this file did) -- per 03-03-PLAN.md's explicit instruction that "the
//! committed end state must call the real surviving_row_groups."

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::schema::types::ColumnPath;

use flint_core::parquet_filter::{FilterExpr, Op, ScalarValue};
use flint_core::parquet_io::surviving_row_groups;

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

/// Calls the REAL `flint_core::parquet_io::surviving_row_groups` against the fixture's own
/// `ParquetMetaData`/schemas for a single `FilterExpr` -- this is the actual PARQ-04
/// skip-engagement assertion (Task 2), not a re-derived stand-in.
fn surviving_for(path: &PathBuf, column: &str, op: Op, value: ScalarValue) -> Vec<usize> {
    let file = std::fs::File::open(path).expect("open fixture");
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("build reader");
    let arrow_schema = builder.schema().clone();
    let parquet_schema = builder.parquet_schema().clone();
    let metadata = builder.metadata().clone();

    let filters = vec![FilterExpr {
        column: column.to_string(),
        op,
        value,
    }];
    surviving_row_groups(&metadata, &arrow_schema, &parquet_schema, &filters)
        .expect("surviving_row_groups")
}

#[test]
fn col_gt_250_keeps_only_last_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_for(&path, "x", Op::Gt, ScalarValue::Int64(250));
    assert_eq!(surviving, vec![2]);
}

#[test]
fn col_lt_50_keeps_only_first_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_for(&path, "x", Op::Lt, ScalarValue::Int64(50));
    assert_eq!(surviving, vec![0]);
}

#[test]
fn col_ge_100_and_lt_200_keeps_only_middle_row_group() {
    // AND semantics (D-24): pass both filters together so surviving_row_groups itself performs
    // the intersection (the real AND-combined code path), not a post-hoc Vec intersection here.
    let path = write_three_row_group_fixture();
    let file = std::fs::File::open(&path).expect("open fixture");
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("build reader");
    let arrow_schema = builder.schema().clone();
    let parquet_schema = builder.parquet_schema().clone();
    let metadata = builder.metadata().clone();

    let filters = vec![
        FilterExpr {
            column: "x".to_string(),
            op: Op::Ge,
            value: ScalarValue::Int64(100),
        },
        FilterExpr {
            column: "x".to_string(),
            op: Op::Lt,
            value: ScalarValue::Int64(200),
        },
    ];
    let surviving = surviving_row_groups(&metadata, &arrow_schema, &parquet_schema, &filters)
        .expect("surviving_row_groups");
    assert_eq!(surviving, vec![1]);
}

#[test]
fn col_eq_1000_keeps_no_row_group() {
    let path = write_three_row_group_fixture();
    let surviving = surviving_for(&path, "x", Op::Eq, ScalarValue::Int64(1000));
    assert!(surviving.is_empty());
}

#[test]
fn filter_on_stats_less_column_keeps_all_row_groups() {
    // "y" has row-group statistics explicitly disabled in the fixture -- StatisticsConverter
    // resolves this to null min/max per row group, which surviving_row_groups (via
    // could_match_range) must treat as conservatively-keep for every row group (never skip on
    // missing stats).
    let path = write_three_row_group_fixture();
    let surviving = surviving_for(&path, "y", Op::Gt, ScalarValue::Int64(999_999));
    assert_eq!(surviving, vec![0, 1, 2]);
}
