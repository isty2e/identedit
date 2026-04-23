# Config Path Patching Reference

Use config-aware path targeting for JSON/YAML/TOML edits where replacing a whole block would be brittle.

## Examples

```bash
identedit patch example.yaml --config-path service.retries --set-value 5
identedit patch example.json --config-path items --append-value 4
identedit patch example.toml --config-path database.settings.enabled --delete
identedit patch example.yaml --config-path 'services["sidecar.port"].enabled' --set-value true
identedit patch manifests.yaml --config-path spec.replicas \
  --document-index 1 --set-value 3 --create-missing
```

## JSON Request Shape

Set:

```json
{
  "command": "patch",
  "file": "example.json",
  "target": {
    "type": "config_path",
    "path": "config.retries",
    "expected_file_hash": "a1b2c3d4e5f6a7b8"
  },
  "op": {
    "type": "set",
    "new_text": "10"
  }
}
```

Append:

```json
{
  "command": "patch",
  "file": "example.json",
  "target": {
    "type": "config_path",
    "path": "items"
  },
  "op": {
    "type": "append",
    "new_text": "4"
  }
}
```

## Path Syntax

- Bare key segments are dot-separated: `service.retries`.
- Array/sequence indices use numeric brackets: `items[0].name`.
- Keys containing dots, spaces, slashes, colons, brackets, quotes, or other non-bare characters must use bracket-quoted JSON string segments.
- Quoted key segments are key names, not array indices: `services["sidecar.port"]` means key `services`, then literal key `sidecar.port`.
- Quoted segments use JSON string escaping: `root["quote\"key"]`, `root["unicode-\uD55C"]`.
- For multi-document YAML streams, use `--document-index <N>` or JSON target field `"document_index": N` when `--create-missing` needs to choose a document. Indices are 0-based. Existing-path edits may omit it only when the path resolves uniquely across documents.

Examples:

```bash
identedit patch config.yaml --config-path '["on"].push.branches[0]' --set-value '"main"'
identedit patch config.toml --config-path 'tool["weird.section"].port' --set-value 9090
identedit patch config.json --config-path 'jobs["build/test"].steps[0]["run:script"]' --set-value '"npm test"'
```

## YAML Block Scalar Creation

Use explicit block scalar leaf values for multiline strings.

```bash
cat <<'EOF' | identedit patch .github/workflows/ci.yml \
  --config-path jobs.build.steps[0].run --set-value --create-missing --stdin-text
|
  cargo test
  cargo clippy --all-targets -- -D warnings
EOF
```

## Rules

- `set` updates an existing path; use `create_missing: true` or `--create-missing` only when creating missing map/table keys.
- `append` requires the resolved target path to be an existing array/sequence.
- `delete` and `append` reject `create_missing`.
- Missing paths, ambiguous matches, malformed syntax, and out-of-range indices fail with explicit `invalid_request` errors.
- Config path edits are validated against the target format before writing.
- TOML/YAML `--create-missing` preserves comments, existing key order, and blank-line groups. It inserts into clearly sorted groups or same-prefix runs, and otherwise appends conservatively without reordering existing keys.
- TOML `--create-missing` can create missing standard table parents such as `[server.sidecar]`; it rejects inline-table parents, array indexes, and table-array parent conflicts.
- YAML multi-document `--create-missing` is never implicit. Add `--document-index <N>` for the target document.
- YAML `--create-missing` preserves comments for block mappings and can create missing intermediate mapping keys. Existing in-range sequence items can be traversed when the selected item is a mapping, but missing sequences and out-of-range sequence indices are rejected; use append for sequence growth.
- YAML multiline create-missing accepts only explicit block scalar leaf values (`|`, `|-`, `|+`, `>`, `>-`, `>+`). Numeric indentation indicators such as `|2`, multiline mappings, and multiline sequences are rejected.
- YAML create-missing quotes unsafe or implicit-scalar-looking string keys while rendering new entries. Use bracket-quoted path segments for literal keys such as `["true"]`, `["null"]`, `["123"]`, or `["app: conf"]`.
- YAML anchors/aliases outside the edited path are allowed and preserved. Create-missing rejects edits inside referenced anchor values and rejects insertion under mappings with YAML merge keys (`<<`). YAML tags remain unsupported for create-missing.
- Fall back to line/direct editing when desired placement depends on project-local comment semantics, cross-section moves, array/table-array restructuring, YAML anchor/merge semantics, or multiline YAML mappings/sequences.
