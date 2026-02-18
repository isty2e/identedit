use super::super::{ApplyFileResult, ApplyFileStatus, summarize_apply_results};

#[test]
fn summary_derives_failed_operations_from_per_file_totals() {
    let applied = vec![
        ApplyFileResult {
            file: "a.py".to_string(),
            operations_applied: 2,
            operations_total: 3,
            status: ApplyFileStatus::Applied,
        },
        ApplyFileResult {
            file: "b.py".to_string(),
            operations_applied: 0,
            operations_total: 2,
            status: ApplyFileStatus::Applied,
        },
        ApplyFileResult {
            file: "c.py".to_string(),
            operations_applied: 1,
            operations_total: 1,
            status: ApplyFileStatus::Applied,
        },
    ];

    let summary = summarize_apply_results(&applied);
    assert_eq!(summary.files_modified, 2);
    assert_eq!(summary.operations_applied, 3);
    assert_eq!(summary.operations_failed, 3);
}

#[test]
fn summary_never_underflows_when_file_counts_are_inconsistent() {
    let applied = vec![ApplyFileResult {
        file: "a.py".to_string(),
        operations_applied: 5,
        operations_total: 2,
        status: ApplyFileStatus::Applied,
    }];

    let summary = summarize_apply_results(&applied);
    assert_eq!(summary.files_modified, 1);
    assert_eq!(summary.operations_applied, 5);
    assert_eq!(summary.operations_failed, 0);
}

#[test]
fn summary_for_empty_results_is_all_zero() {
    let summary = summarize_apply_results(&[]);
    assert_eq!(summary.files_modified, 0);
    assert_eq!(summary.operations_applied, 0);
    assert_eq!(summary.operations_failed, 0);
}
