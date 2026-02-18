use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub kind: String,
    #[serde(default)]
    pub name_pattern: Option<String>,
    #[serde(default)]
    pub exclude_kinds: Vec<String>,
}

impl Selector {
    pub fn validate(&self) -> Result<(), IdenteditError> {
        if self.kind.trim().is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message: "selector.kind must not be empty".to_string(),
            });
        }

        if let Some(name_pattern) = self.name_pattern.as_deref() {
            Pattern::new(name_pattern).map_err(|error| IdenteditError::InvalidNamePattern {
                pattern: name_pattern.to_string(),
                message: error.msg.to_string(),
            })?;
        }

        Ok(())
    }

    pub fn filter(
        &self,
        handles: Vec<SelectionHandle>,
    ) -> Result<Vec<SelectionHandle>, IdenteditError> {
        self.validate()?;

        let compiled_pattern = self
            .name_pattern
            .as_deref()
            .map(Pattern::new)
            .transpose()
            .map_err(|error| IdenteditError::InvalidNamePattern {
                pattern: self.name_pattern.as_deref().unwrap_or_default().to_string(),
                message: error.msg.to_string(),
            })?;

        let filtered = handles
            .into_iter()
            .filter(|handle| self.matches(handle, compiled_pattern.as_ref()))
            .collect();

        Ok(filtered)
    }

    fn matches(&self, handle: &SelectionHandle, name_pattern: Option<&Pattern>) -> bool {
        if self
            .exclude_kinds
            .iter()
            .any(|excluded_kind| excluded_kind == &handle.kind)
        {
            return false;
        }

        if self.kind != handle.kind {
            return false;
        }

        if let Some(pattern) = name_pattern {
            return handle
                .name
                .as_deref()
                .is_some_and(|symbol_name| pattern.matches(symbol_name));
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::handle::{SelectionHandle, Span};

    use super::Selector;

    fn handle(kind: &str, name: Option<&str>, text: &str) -> SelectionHandle {
        SelectionHandle::from_parts(
            PathBuf::from("fixture.py"),
            Span {
                start: 0,
                end: text.len(),
            },
            kind.to_string(),
            name.map(ToString::to_string),
            text.to_string(),
        )
    }

    #[test]
    fn validate_rejects_empty_kind() {
        let selector = Selector {
            kind: "   ".to_string(),
            name_pattern: None,
            exclude_kinds: vec![],
        };

        let error = selector.validate().expect_err("empty kind should fail");
        assert!(
            error
                .to_string()
                .contains("selector.kind must not be empty")
        );
    }

    #[test]
    fn validate_rejects_invalid_glob_pattern() {
        let selector = Selector {
            kind: "function_definition".to_string(),
            name_pattern: Some("[".to_string()),
            exclude_kinds: vec![],
        };

        let error = selector.validate().expect_err("invalid glob should fail");
        assert!(error.to_string().contains("Invalid selector glob pattern"));
    }

    #[test]
    fn filter_applies_kind_name_and_exclude_rules() {
        let selector = Selector {
            kind: "function_definition".to_string(),
            name_pattern: Some("process_*".to_string()),
            exclude_kinds: vec!["comment".to_string()],
        };

        let handles = vec![
            handle(
                "function_definition",
                Some("process_data"),
                "def process_data(): pass",
            ),
            handle("function_definition", Some("helper"), "def helper(): pass"),
            handle(
                "class_definition",
                Some("Processor"),
                "class Processor: pass",
            ),
            handle("comment", None, "# process_data"),
            handle("function_definition", None, "lambda x: x"),
        ];

        let filtered = selector.filter(handles).expect("filter should succeed");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name.as_deref(), Some("process_data"));
    }

    #[test]
    fn filter_supports_literal_wildcard_matching() {
        let selector = Selector {
            kind: "function_definition".to_string(),
            name_pattern: Some("process[*]".to_string()),
            exclude_kinds: vec![],
        };

        let handles = vec![
            handle(
                "function_definition",
                Some("process*"),
                "def process_star(): pass",
            ),
            handle(
                "function_definition",
                Some("process_data"),
                "def process_data(): pass",
            ),
        ];

        let filtered = selector.filter(handles).expect("filter should succeed");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name.as_deref(), Some("process*"));
    }

    #[test]
    fn filter_with_name_pattern_excludes_handles_without_symbol_name() {
        let selector = Selector {
            kind: "function_definition".to_string(),
            name_pattern: Some("process_*".to_string()),
            exclude_kinds: vec![],
        };

        let handles = vec![
            handle("function_definition", None, "lambda value: value + 1"),
            handle("function_definition", Some("helper"), "def helper(): pass"),
        ];

        let filtered = selector.filter(handles).expect("filter should succeed");
        assert!(
            filtered.is_empty(),
            "name_pattern should ignore handles that do not carry symbol names"
        );
    }

    #[test]
    fn filter_exclude_kind_overrides_primary_kind_match() {
        let selector = Selector {
            kind: "function_definition".to_string(),
            name_pattern: None,
            exclude_kinds: vec!["function_definition".to_string()],
        };

        let handles = vec![handle(
            "function_definition",
            Some("process_data"),
            "def process_data(): pass",
        )];

        let filtered = selector.filter(handles).expect("filter should succeed");
        assert!(
            filtered.is_empty(),
            "exclude_kinds should take precedence over kind inclusion"
        );
    }
}
