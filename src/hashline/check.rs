use std::collections::BTreeMap;

use super::{
    AnchorCheckRequest, HashlineCheckError, HashlineCheckResult, HashlineCheckSummary,
    HashlineMismatch, HashlineMismatchStatus, HashlineRemapTarget,
};

pub(super) fn check_hashline_anchors(
    source: &str,
    anchors: &[AnchorCheckRequest],
) -> Result<HashlineCheckResult, HashlineCheckError> {
    let hashed_lines = super::show::show_hashed_lines(source);
    let line_to_hash = hashed_lines
        .iter()
        .map(|line| (line.line, line.hash.clone()))
        .collect::<BTreeMap<_, _>>();
    let hash_to_lines = hashed_lines.iter().fold(
        BTreeMap::<super::LineHash, Vec<usize>>::new(),
        |mut acc, line| {
            acc.entry(line.hash.clone()).or_default().push(line.line);
            acc
        },
    );

    let mut summary = HashlineCheckSummary::default();
    let mut mismatches = Vec::new();

    for anchor_request in anchors {
        summary.total += 1;

        let actual_hash = line_to_hash.get(&anchor_request.anchor.line());
        if actual_hash.is_some_and(|actual| actual == anchor_request.anchor.hash()) {
            summary.matched += 1;
            continue;
        }

        summary.mismatched += 1;

        let candidate_lines = hash_to_lines
            .get(anchor_request.anchor.hash())
            .cloned()
            .unwrap_or_default();
        let (status, remaps) = if candidate_lines.len() == 1 {
            summary.remappable += 1;
            (
                HashlineMismatchStatus::Remappable,
                vec![HashlineRemapTarget {
                    line: candidate_lines[0],
                    hash: anchor_request.anchor.hash().clone(),
                }],
            )
        } else if candidate_lines.len() > 1 {
            summary.ambiguous += 1;
            (
                HashlineMismatchStatus::Ambiguous,
                candidate_lines
                    .iter()
                    .map(|line| HashlineRemapTarget {
                        line: *line,
                        hash: anchor_request.anchor.hash().clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            (HashlineMismatchStatus::Mismatch, Vec::new())
        };

        mismatches.push(HashlineMismatch {
            edit_index: anchor_request.edit_index,
            anchor: anchor_request.anchor.clone(),
            line: anchor_request.anchor.line(),
            expected_hash: anchor_request.anchor.hash().clone(),
            actual_hash: actual_hash.cloned(),
            status,
            remaps,
        });
    }

    Ok(HashlineCheckResult {
        ok: summary.mismatched == 0,
        summary,
        mismatches,
    })
}
