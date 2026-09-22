use std::path::Path;

use serde::Serialize;

use crate::handle::Span;
use crate::hashline::source_line_spans;

use super::move_ops::MovePreflightPlan;
use super::preflight::PreflightFilePlan;

const LOCATION_LIMIT: usize = 16;

#[derive(Debug, Clone, Serialize)]
pub struct EditLocations {
    basis: CoordinateBasis,
    total: usize,
    omitted: usize,
    entries: Vec<EditLocation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum CoordinateBasis {
    PreEdit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EditLocation {
    Text {
        file: String,
        operation_index: usize,
        span: Span,
        start_line: usize,
        end_line: usize,
    },
    FileMove {
        source: String,
        destination: String,
        operation_index: usize,
    },
}

impl Default for EditLocations {
    fn default() -> Self {
        Self {
            basis: CoordinateBasis::PreEdit,
            total: 0,
            omitted: 0,
            entries: Vec::new(),
        }
    }
}

impl EditLocations {
    pub(crate) fn for_text(
        file: &Path,
        source: &str,
        spans: impl IntoIterator<Item = (usize, Span)>,
    ) -> Self {
        let line_ends: Vec<_> = source_line_spans(source).map(|span| span.end).collect();
        let eof_line = line_ends.len() + usize::from(source.ends_with(['\r', '\n']));
        let line_at = |offset| {
            if offset == source.len() {
                eof_line.max(1)
            } else {
                line_ends.partition_point(|end| *end <= offset) + 1
            }
        };

        let mut result = Self::default();
        for (operation_index, span) in spans {
            result.total += 1;
            if result.entries.len() == LOCATION_LIMIT {
                result.omitted += 1;
                continue;
            }

            let start_line = line_at(span.start);
            let end_line = line_at(if span.end > span.start {
                span.end - 1
            } else {
                span.end
            });
            result.entries.push(EditLocation::Text {
                file: file.display().to_string(),
                operation_index,
                span,
                start_line,
                end_line,
            });
        }
        result
    }

    pub(super) fn from_plans(edits: &[PreflightFilePlan], moves: &[MovePreflightPlan]) -> Self {
        let mut result = Self::default();
        for plan in edits {
            result.total += plan.locations.total;
            let remaining = LOCATION_LIMIT - result.entries.len();
            result
                .entries
                .extend(plan.locations.entries.iter().take(remaining).cloned());
        }
        for plan in moves {
            result.total += 1;
            if result.entries.len() < LOCATION_LIMIT {
                result.entries.push(EditLocation::FileMove {
                    source: plan.source.display().to_string(),
                    destination: plan.destination.display().to_string(),
                    operation_index: 0,
                });
            }
        }
        result.omitted = result.total - result.entries.len();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_distinguish_line_terminators_eof_and_empty_input() {
        for (source, cases) in [
            ("", vec![(0, 0, 1, 1)]),
            (
                "a\r\nb\rc\n",
                vec![
                    (0, 3, 1, 1),
                    (1, 2, 1, 1),
                    (2, 3, 1, 1),
                    (3, 5, 2, 2),
                    (5, 7, 3, 3),
                    (7, 7, 4, 4),
                    (0, 7, 1, 3),
                ],
            ),
            ("a\r\nb", vec![(3, 4, 2, 2), (4, 4, 2, 2)]),
            ("\n\r\r\n", vec![(0, 4, 1, 3), (4, 4, 4, 4)]),
        ] {
            for (start, end, first, last) in cases {
                let locations =
                    EditLocations::for_text(Path::new("test"), source, [(0, Span { start, end })]);
                let EditLocation::Text {
                    start_line,
                    end_line,
                    ..
                } = &locations.entries[0]
                else {
                    panic!("expected text location");
                };
                assert_eq!(
                    (*start_line, *end_line),
                    (first, last),
                    "{source:?} [{start}, {end})"
                );
            }
        }
    }

    #[test]
    fn empty_and_over_limit_receipts_preserve_counts() {
        for total in [
            0,
            1,
            LOCATION_LIMIT - 1,
            LOCATION_LIMIT,
            LOCATION_LIMIT + 1,
            1024,
        ] {
            let locations = EditLocations::for_text(
                Path::new("test"),
                "source",
                (0..total).map(|index| (index, Span { start: 0, end: 6 })),
            );
            assert_eq!(locations.total, total);
            assert_eq!(locations.entries.len(), total.min(LOCATION_LIMIT));
            assert_eq!(locations.omitted, total.saturating_sub(LOCATION_LIMIT));
        }
    }
}
