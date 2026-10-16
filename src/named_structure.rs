use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::handle::{SelectionHandle, Span};

pub(crate) const QUALIFIED_NAME_SEPARATOR: &str = ".";

#[derive(Debug)]
pub(crate) struct NamedNode {
    pub(crate) span: Span,
    pub(crate) kind: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NamedKey {
    pub(crate) kind: String,
    pub(crate) qualified_name: String,
}

#[derive(Clone, Copy)]
struct Entry<'a> {
    span: Span,
    kind: &'a str,
    name: &'a str,
}

pub(crate) struct QualifiedNames<'a> {
    entries: Vec<Entry<'a>>,
    max_ends: Vec<usize>,
    leaves: usize,
}

impl<'a> QualifiedNames<'a> {
    pub(crate) fn from_handles(handles: &'a [SelectionHandle]) -> Self {
        Self::new(
            handles
                .iter()
                .filter_map(|handle| {
                    Some(Entry {
                        span: handle.span,
                        kind: &handle.kind,
                        name: handle.name.as_deref()?,
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn from_nodes(nodes: &'a [NamedNode]) -> Self {
        Self::new(
            nodes
                .iter()
                .map(|node| Entry {
                    span: node.span,
                    kind: &node.kind,
                    name: &node.name,
                })
                .collect(),
        )
    }

    fn new(mut entries: Vec<Entry<'a>>) -> Self {
        entries.sort_by_key(|entry| (entry.span.start, Reverse(entry.span.end)));
        let leaves = entries.len().max(1).next_power_of_two();
        let mut max_ends = vec![0; leaves * 2];
        for (index, entry) in entries.iter().enumerate() {
            max_ends[leaves + index] = entry.span.end;
        }
        for index in (1..leaves).rev() {
            max_ends[index] = max_ends[index * 2].max(max_ends[index * 2 + 1]);
        }

        Self {
            entries,
            max_ends,
            leaves,
        }
    }

    pub(crate) fn for_handle(&self, handle: &SelectionHandle) -> Option<String> {
        Some(self.qualify(Entry {
            span: handle.span,
            kind: &handle.kind,
            name: handle.name.as_deref()?,
        }))
    }

    pub(crate) fn counts(&self) -> BTreeMap<NamedKey, usize> {
        let mut counts = BTreeMap::new();
        for &entry in &self.entries {
            let key = NamedKey {
                kind: entry.kind.to_string(),
                qualified_name: self.qualify(entry),
            };
            *counts.entry(key).or_default() += 1;
        }
        counts
    }

    fn qualify(&self, target: Entry<'_>) -> String {
        let upper = self
            .entries
            .partition_point(|entry| entry.span.start <= target.span.start);
        let mut parts = Vec::new();
        self.append_ancestors(target, 1, 0, self.leaves, upper, &mut parts);

        let mut name = parts.join(QUALIFIED_NAME_SEPARATOR);
        if !parts.is_empty() {
            name.push_str(QUALIFIED_NAME_SEPARATOR);
        }
        name.push_str(target.name);
        name
    }

    fn append_ancestors(
        &self,
        target: Entry<'_>,
        node: usize,
        start: usize,
        end: usize,
        upper: usize,
        parts: &mut Vec<&'a str>,
    ) {
        if start >= upper || self.max_ends[node] < target.span.end {
            return;
        }
        if end - start == 1 {
            let candidate = self.entries[start];
            if !(candidate.span == target.span
                && candidate.kind == target.kind
                && candidate.name == target.name)
            {
                parts.push(candidate.name);
            }
            return;
        }

        let middle = (start + end) / 2;
        self.append_ancestors(target, node * 2, start, middle, upper, parts);
        self.append_ancestors(target, node * 2 + 1, middle, end, upper, parts);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proptest::prelude::*;

    use super::{NamedNode, QualifiedNames};
    use crate::handle::{SelectionHandle, Span};

    fn legacy_name(handle: &SelectionHandle, handles: &[SelectionHandle]) -> Option<String> {
        let name = handle.name.as_ref()?;
        let mut ancestors = handles
            .iter()
            .filter(|candidate| {
                candidate.name.is_some()
                    && candidate.span.start <= handle.span.start
                    && handle.span.end <= candidate.span.end
                    && !(candidate.span == handle.span
                        && candidate.kind == handle.kind
                        && candidate.name == handle.name)
            })
            .collect::<Vec<_>>();
        ancestors
            .sort_by_key(|candidate| (candidate.span.start, std::cmp::Reverse(candidate.span.end)));
        let mut parts = ancestors
            .iter()
            .filter_map(|candidate| candidate.name.as_deref())
            .collect::<Vec<_>>();
        parts.push(name);
        Some(parts.join("."))
    }

    proptest! {
        #[test]
        fn indexed_names_match_legacy_containment_and_stable_ties(
            nodes in prop::collection::vec(
                (0usize..24, 0usize..24, any::<bool>(), prop::option::of(
                    prop_oneof![Just("target"), Just("a.b"), Just(""), Just("이름")]
                )), 0..35
            )
        ) {
            let handles = nodes.into_iter().map(|(start, length, class, name)| {
                SelectionHandle::from_parts(
                    PathBuf::from("fixture.py"),
                    Span { start, end: start + length },
                    if class { "class" } else { "function" }.to_string(),
                    name.map(str::to_string),
                    "body".to_string(),
                )
            }).collect::<Vec<_>>();
            let names = QualifiedNames::from_handles(&handles);
            for handle in &handles {
                prop_assert_eq!(names.for_handle(handle), legacy_name(handle, &handles));
            }
        }
    }

    #[test]
    fn occurrence_counts_preserve_duplicates_and_opaque_names() {
        let nodes = vec![
            NamedNode {
                span: Span { start: 0, end: 5 },
                kind: "function".into(),
                name: "a.b".into(),
            },
            NamedNode {
                span: Span { start: 6, end: 10 },
                kind: "function".into(),
                name: "a.b".into(),
            },
        ];
        let counts = QualifiedNames::from_nodes(&nodes).counts();
        assert_eq!(counts.len(), 1);
        let (key, count) = counts.first_key_value().unwrap();
        assert_eq!(key.qualified_name, "a.b");
        assert_eq!(*count, 2);
    }

    #[test]
    fn large_flat_projection_does_not_create_ancestor_prefixes() {
        let nodes = (0..10_000)
            .map(|index| NamedNode {
                span: Span {
                    start: index * 4,
                    end: index * 4 + 3,
                },
                kind: "function".into(),
                name: format!("f{index}"),
            })
            .collect::<Vec<_>>();
        let counts = QualifiedNames::from_nodes(&nodes).counts();
        assert_eq!(counts.len(), nodes.len());
        assert!(counts.keys().all(|key| !key.qualified_name.contains('.')));
    }
}
