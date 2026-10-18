use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::handle::SelectionHandle;
use crate::named_structure::{NamedKey, QualifiedNames};
use crate::provider::{NameProjectionError, ParsedSource};

use super::move_ops::MovePreflightPlan;
use super::preflight::PreflightFilePlan;

const ENTRY_LIMIT: usize = 16;
const NAME_MAX_CHARS: usize = 120;

#[derive(Debug)]
pub(super) struct FileObservation(Observation);

#[derive(Debug)]
enum Observation {
    Checked(CheckedChanges),
    Unavailable(UnavailableReason),
    OutOfScope,
}

#[derive(Debug)]
struct CheckedChanges {
    added: usize,
    removed: usize,
    total: usize,
    changes: Vec<CountChange>,
}

#[derive(Debug)]
struct CountChange {
    key: NamedKey,
    before_count: usize,
    after_count: usize,
}

impl CountChange {
    fn order(&self) -> (bool, &str, &str) {
        (
            self.after_count > self.before_count,
            &self.key.qualified_name,
            &self.key.kind,
        )
    }
}

impl FileObservation {
    pub(super) fn out_of_scope() -> Self {
        Self(Observation::OutOfScope)
    }

    pub(super) fn unsupported() -> Self {
        Self(Observation::Unavailable(
            UnavailableReason::UnsupportedProvider,
        ))
    }

    pub(super) fn observe(
        source: &str,
        handles: &[SelectionHandle],
        result: &ParsedSource<'_>,
    ) -> Self {
        if handles.iter().any(|handle| {
            handle.name.is_some()
                && (handle.span.start > handle.span.end || handle.span.end > source.len())
        }) {
            return Self(Observation::Unavailable(
                UnavailableReason::ProjectionFailed,
            ));
        }
        let nodes = match result.named_nodes() {
            Ok(nodes) => nodes,
            Err(NameProjectionError::Unsupported) => return Self::unsupported(),
            Err(NameProjectionError::InvalidSpan) => {
                return Self(Observation::Unavailable(
                    UnavailableReason::ProjectionFailed,
                ));
            }
        };

        Self::checked(
            QualifiedNames::from_handles(handles).counts(),
            QualifiedNames::from_nodes(&nodes).counts(),
        )
    }

    fn checked(before: BTreeMap<NamedKey, usize>, after: BTreeMap<NamedKey, usize>) -> Self {
        let mut counts = before
            .into_iter()
            .map(|(key, count)| (key, (count, 0)))
            .collect::<BTreeMap<_, _>>();
        for (key, count) in after {
            counts.entry(key).or_default().1 = count;
        }
        let mut changes = counts
            .into_iter()
            .filter_map(|(key, (before_count, after_count))| {
                (before_count != after_count).then_some(CountChange {
                    key,
                    before_count,
                    after_count,
                })
            })
            .collect::<Vec<_>>();
        let added = changes
            .iter()
            .map(|change| change.after_count.saturating_sub(change.before_count))
            .sum();
        let removed = changes
            .iter()
            .map(|change| change.before_count.saturating_sub(change.after_count))
            .sum();
        let total = changes.len();
        changes.sort_by(|left, right| left.order().cmp(&right.order()));
        changes.truncate(ENTRY_LIMIT);

        Self(Observation::Checked(CheckedChanges {
            added,
            removed,
            total,
            changes,
        }))
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StructureChanges {
    summary: ChangeSummary,
    files: Receipt<FileEntry>,
    changes: Receipt<ChangeEntry>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct ChangeSummary {
    checked_files: usize,
    unavailable_files: usize,
    out_of_scope_files: usize,
    added: usize,
    removed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Receipt<Entry> {
    total: usize,
    omitted: usize,
    entries: Vec<Entry>,
}

impl<Entry> Receipt<Entry> {
    fn new(total: usize, entries: Vec<Entry>) -> Self {
        Self {
            total,
            omitted: total - entries.len(),
            entries,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FileEntry {
    file: String,
    #[serde(flatten)]
    status: FileStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum FileStatus {
    Checked,
    Unavailable { reason: UnavailableReason },
    OutOfScope,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum UnavailableReason {
    UnsupportedProvider,
    ProjectionFailed,
}

#[derive(Debug, Clone, Serialize)]
struct ChangeEntry {
    file: String,
    kind: String,
    qualified_name: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    qualified_name_truncated: bool,
    before_count: usize,
    after_count: usize,
}

impl StructureChanges {
    pub(super) fn from_plans(plans: &[PreflightFilePlan], moves: &[MovePreflightPlan]) -> Self {
        let out_of_scope = FileObservation::out_of_scope();
        Self::from_observations(
            plans
                .iter()
                .map(|plan| {
                    (
                        plan.file(),
                        plan.text_update()
                            .map_or(&out_of_scope, |plan| &plan.structure),
                    )
                })
                .chain(
                    moves
                        .iter()
                        .map(|plan| (plan.source.as_path(), &out_of_scope)),
                ),
        )
    }

    pub(crate) fn out_of_scope(file: &Path) -> Self {
        Self::from_observations([(file, &FileObservation::out_of_scope())])
    }

    fn from_observations<'a>(
        observations: impl IntoIterator<Item = (&'a Path, &'a FileObservation)>,
    ) -> Self {
        let mut observations = observations
            .into_iter()
            .map(|(path, observation)| (path.display().to_string(), observation))
            .collect::<Vec<_>>();
        observations.sort_by(|left, right| left.0.cmp(&right.0));

        let mut summary = ChangeSummary::default();
        let mut files = Vec::new();
        let mut changes = Vec::new();
        let mut total_changes = 0;
        for (file, observation) in &observations {
            let status = match &observation.0 {
                Observation::Checked(checked) => {
                    summary.checked_files += 1;
                    summary.added += checked.added;
                    summary.removed += checked.removed;
                    total_changes += checked.total;
                    for change in checked.changes.iter().take(ENTRY_LIMIT - changes.len()) {
                        let (qualified_name, qualified_name_truncated) =
                            display_name(&change.key.qualified_name);
                        changes.push(ChangeEntry {
                            file: file.clone(),
                            kind: change.key.kind.clone(),
                            qualified_name,
                            qualified_name_truncated,
                            before_count: change.before_count,
                            after_count: change.after_count,
                        });
                    }
                    FileStatus::Checked
                }
                Observation::Unavailable(reason) => {
                    summary.unavailable_files += 1;
                    FileStatus::Unavailable { reason: *reason }
                }
                Observation::OutOfScope => {
                    summary.out_of_scope_files += 1;
                    FileStatus::OutOfScope
                }
            };
            if files.len() < ENTRY_LIMIT {
                files.push(FileEntry {
                    file: file.clone(),
                    status,
                });
            }
        }

        Self {
            summary,
            files: Receipt::new(observations.len(), files),
            changes: Receipt::new(total_changes, changes),
        }
    }
}

fn display_name(name: &str) -> (String, bool) {
    let length = name.chars().count();
    if length <= NAME_MAX_CHARS {
        return (name.to_string(), false);
    }
    let tail = name
        .chars()
        .skip(length - (NAME_MAX_CHARS - 1))
        .collect::<String>();
    (format!("…{tail}"), true)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{FileObservation, StructureChanges, display_name};
    use crate::named_structure::NamedKey;

    #[test]
    fn duplicate_count_changes_are_not_set_changes() {
        let key = NamedKey {
            kind: "method".into(),
            qualified_name: "Example.target".into(),
        };
        let observation = FileObservation::checked([(key.clone(), 2)].into(), [(key, 1)].into());
        let report = StructureChanges::from_observations([(Path::new("example"), &observation)]);
        assert_eq!(report.summary.removed, 1);
        assert_eq!(report.summary.added, 0);
        assert_eq!(report.changes.total, 1);
        assert_eq!(report.changes.entries[0].before_count, 2);
        assert_eq!(report.changes.entries[0].after_count, 1);
    }

    #[test]
    fn coverage_states_survive_empty_change_lists() {
        let checked = FileObservation::checked(Default::default(), Default::default());
        let unavailable = FileObservation::unsupported();
        let out_of_scope = FileObservation::out_of_scope();
        let report = StructureChanges::from_observations([
            (Path::new("z"), &checked),
            (Path::new("a"), &unavailable),
            (Path::new("m"), &out_of_scope),
        ]);
        assert_eq!(report.summary.checked_files, 1);
        assert_eq!(report.summary.unavailable_files, 1);
        assert_eq!(report.summary.out_of_scope_files, 1);
        assert!(report.changes.entries.is_empty());
        assert_eq!(
            report
                .files
                .entries
                .iter()
                .map(|entry| entry.file.as_str())
                .collect::<Vec<_>>(),
            ["a", "m", "z"]
        );
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["files"]["entries"][0]["status"], "unavailable");
        assert_eq!(
            json["files"]["entries"][0]["reason"],
            "unsupported_provider"
        );
        assert_eq!(json["files"]["entries"][2]["status"], "checked");
    }

    #[test]
    fn truncation_retains_unicode_tail_without_changing_short_names() {
        let name = format!("{}.Inner.대상", "오".repeat(140));
        let (shortened, truncated) = display_name(&name);
        assert!(truncated);
        assert_eq!(shortened.chars().count(), 120);
        assert!(shortened.starts_with('…'));
        assert!(shortened.ends_with(".Inner.대상"));
        assert_eq!(
            display_name("Example.target"),
            ("Example.target".into(), false)
        );
    }

    #[test]
    fn identical_truncated_names_remain_distinct_full_keys() {
        let tail = format!("{}.target", "x".repeat(140));
        let after = ["first", "second"]
            .into_iter()
            .map(|prefix| {
                (
                    NamedKey {
                        kind: "function".into(),
                        qualified_name: format!("{prefix}.{tail}"),
                    },
                    1,
                )
            })
            .collect();
        let checked = FileObservation::checked(Default::default(), after);
        let report = StructureChanges::from_observations([(Path::new("example"), &checked)]);
        assert_eq!(report.changes.total, 2);
        assert_eq!(report.summary.added, 2);
        assert_eq!(
            report.changes.entries[0].qualified_name,
            report.changes.entries[1].qualified_name
        );
        assert!(
            report
                .changes
                .entries
                .iter()
                .all(|entry| entry.qualified_name_truncated)
        );
    }

    #[test]
    fn bounded_file_metadata_does_not_change_coverage_totals() {
        let paths = (0..20)
            .rev()
            .map(|index| std::path::PathBuf::from(format!("file{index:02}")))
            .collect::<Vec<_>>();
        let unavailable = FileObservation::unsupported();
        let report = StructureChanges::from_observations(
            paths.iter().map(|file| (file.as_path(), &unavailable)),
        );
        assert_eq!(report.summary.unavailable_files, 20);
        assert_eq!(report.files.total, 20);
        assert_eq!(report.files.omitted, 4);
        assert_eq!(report.files.entries.len(), 16);
        assert_eq!(report.files.entries[0].file, "file00");
    }
}
