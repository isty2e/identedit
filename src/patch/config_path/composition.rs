use std::cmp::Reverse;

use tree_sitter::Node;

use crate::error::IdenteditError;
use crate::handle::Span;
use crate::transform::conflict::write_spans_conflict;

use super::ConfigPathBatchError;
use super::resolve::span_from_node;
use super::safety::{ConfigFormat, validate_rendered_config_document};

pub(super) struct ConfigWrite {
    pub(super) span: Span,
    pub(super) new_text: String,
}

#[derive(Clone, Copy)]
pub(super) enum ConfigBoundary {
    Start,
    End,
}

#[derive(Clone)]
pub(super) enum ConfigGuard {
    Syntax {
        span: Span,
        kind: String,
        path: Vec<usize>,
    },
    Boundary {
        side: ConfigBoundary,
        offset: usize,
    },
}

impl ConfigGuard {
    fn span(&self) -> Span {
        match self {
            Self::Syntax { span, .. } => *span,
            Self::Boundary { offset, .. } => Span {
                start: *offset,
                end: *offset,
            },
        }
    }

    fn depth(&self) -> usize {
        match self {
            Self::Syntax { path, .. } => path.len(),
            Self::Boundary { .. } => 0,
        }
    }

    fn contains(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Syntax { path, .. }, Self::Syntax { path: other, .. }) => {
                other.starts_with(path)
            }
            _ => false,
        }
    }
}

pub(super) struct ConfigEffect {
    guard: ConfigGuard,
    write: ConfigWrite,
}

impl ConfigEffect {
    pub(super) fn syntax(mut node: Node<'_>, write: ConfigWrite) -> Self {
        let span = span_from_node(node);
        let kind = node.kind().to_string();
        let mut path = Vec::new();
        while let Some(parent) = node.parent() {
            let mut sibling = node;
            let mut index = 0;
            while let Some(previous) = sibling.prev_sibling() {
                index += 1;
                sibling = previous;
            }
            path.push(index);
            node = parent;
        }
        path.reverse();

        Self {
            guard: ConfigGuard::Syntax { span, kind, path },
            write,
        }
    }

    pub(super) fn boundary(side: ConfigBoundary, offset: usize, new_text: String) -> Self {
        Self {
            guard: ConfigGuard::Boundary { side, offset },
            write: ConfigWrite {
                span: Span {
                    start: offset,
                    end: offset,
                },
                new_text,
            },
        }
    }
}

pub(super) struct ComposedConfigEffect {
    pub(super) guard: ConfigGuard,
    pub(super) new_text: String,
    pub(super) first_request_index: usize,
    pub(super) guard_request_index: usize,
}

pub(super) fn compose_config_effects(
    format: &ConfigFormat,
    source: &str,
    effects: Vec<ConfigEffect>,
) -> Result<Vec<ComposedConfigEffect>, ConfigPathBatchError> {
    let mut write_order = (0..effects.len()).collect::<Vec<_>>();
    write_order.sort_unstable_by_key(|&index| {
        let span = effects[index].write.span;
        (span.start, span.end, index)
    });
    for pair in write_order.windows(2) {
        let first = effects[pair[0]].write.span;
        let second = effects[pair[1]].write.span;
        if write_spans_conflict(first, second) {
            return Err(ConfigPathBatchError {
                error: IdenteditError::InvalidRequest {
                    message: format!(
                        "Overlapping config operations are not supported: [{}, {}) conflicts with [{}, {})",
                        first.start, first.end, second.start, second.end,
                    ),
                },
                request_indices: pair.to_vec(),
            });
        }
    }

    if effects.len() > 1 {
        let updated = render_writes(
            source,
            Span {
                start: 0,
                end: source.len(),
            },
            &effects,
            &write_order,
        )?;
        validate_rendered_config_document(format, source, &updated)?;
    }

    let mut guard_order = (0..effects.len()).collect::<Vec<_>>();
    guard_order.sort_unstable_by_key(|&index| {
        let guard = &effects[index].guard;
        let span = guard.span();
        (span.start, Reverse(span.end), guard.depth(), index)
    });
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for index in guard_order {
        if let Some(group) = groups.last_mut() {
            let outer_index = group[0];
            let outer = &effects[outer_index].guard;
            let inner = &effects[index].guard;
            if outer.contains(inner) {
                group.push(index);
                continue;
            }
            if outer.span().end > inner.span().start {
                return Err(ConfigPathBatchError {
                    error: IdenteditError::InvalidRequest {
                        message: "Overlapping config guard scopes have no contributing outer syntax scope".to_string(),
                    },
                    request_indices: vec![outer_index, index],
                });
            }
        }
        groups.push(vec![index]);
    }

    let mut compiled = Vec::with_capacity(groups.len());
    for mut group in groups {
        let guard_request_index = group[0];
        let first_request_index = *group.iter().min().expect("config group is nonempty");
        let guard = effects[guard_request_index].guard.clone();
        group.sort_unstable_by_key(|&index| {
            let span = effects[index].write.span;
            (span.start, span.end, index)
        });
        let new_text = render_writes(source, guard.span(), &effects, &group)?;
        compiled.push(ComposedConfigEffect {
            guard,
            new_text,
            first_request_index,
            guard_request_index,
        });
    }
    compiled.sort_unstable_by_key(|effect| effect.first_request_index);
    Ok(compiled)
}

fn render_writes(
    source: &str,
    span: Span,
    effects: &[ConfigEffect],
    indices: &[usize],
) -> Result<String, IdenteditError> {
    let mut updated = String::new();
    let mut cursor = span.start;
    for &index in indices {
        let write = &effects[index].write;
        let prefix =
            source
                .get(cursor..write.span.start)
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: "Config write is outside its original source scope".to_string(),
                })?;
        if write.span.end > span.end {
            return Err(IdenteditError::InvalidRequest {
                message: "Config write extends beyond its original guard scope".to_string(),
            });
        }
        updated.push_str(prefix);
        updated.push_str(&write.new_text);
        cursor = write.span.end;
    }
    let suffix = source
        .get(cursor..span.end)
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "Invalid original config source scope".to_string(),
        })?;
    updated.push_str(suffix);
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::super::safety::parse_tree_for_format;
    use super::*;

    #[test]
    fn actual_parent_scope_wins_even_when_its_span_equals_the_child() {
        let source = "[service]\na = 1\nb = 2";
        let tree = parse_tree_for_format(&ConfigFormat::Toml, source.as_bytes()).unwrap();
        let document = tree.root_node();
        let table = document.named_child(0).unwrap();
        assert_eq!(table.kind(), "table");
        assert_eq!(span_from_node(document), span_from_node(table));
        let a = source.find('1').unwrap();
        let b = source.find('2').unwrap();
        let effects = vec![
            ConfigEffect::syntax(
                table,
                ConfigWrite {
                    span: Span {
                        start: a,
                        end: a + 1,
                    },
                    new_text: "10".to_string(),
                },
            ),
            ConfigEffect::syntax(
                document,
                ConfigWrite {
                    span: Span {
                        start: b,
                        end: b + 1,
                    },
                    new_text: "20".to_string(),
                },
            ),
        ];
        let compiled = compose_config_effects(&ConfigFormat::Toml, source, effects).unwrap();
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].first_request_index, 0);
        assert_eq!(compiled[0].guard_request_index, 1);
        let ConfigGuard::Syntax { kind, .. } = &compiled[0].guard else {
            panic!("expected the real contributing syntax guard")
        };
        assert_eq!(kind, "document");
        assert_eq!(compiled[0].new_text, "[service]\na = 10\nb = 20");
    }

    #[test]
    fn combined_format_failure_does_not_invent_an_input_conflict_pair() {
        let source = "{\"a\":1,\"b\":2}";
        let tree = parse_tree_for_format(&ConfigFormat::Json, source.as_bytes()).unwrap();
        let scope = tree.root_node().named_child(0).unwrap();
        let effects = ['1', '2']
            .into_iter()
            .map(|value| {
                let start = source.find(value).unwrap();
                ConfigEffect::syntax(
                    scope,
                    ConfigWrite {
                        span: Span {
                            start,
                            end: start + 1,
                        },
                        new_text: "[".to_string(),
                    },
                )
            })
            .collect();
        let failure = match compose_config_effects(&ConfigFormat::Json, source, effects) {
            Ok(_) => panic!("the combined output must pass format validation"),
            Err(failure) => failure,
        };
        assert!(matches!(
            failure.error,
            IdenteditError::InvalidRequest { .. }
        ));
        assert!(failure.request_indices.is_empty());
    }
}
