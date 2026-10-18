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

## Batch Edit Intents

Use `edit --json` to plan several config edits without writing:

```json
{
  "command": "edit",
  "file": "example.json",
  "operations": [
    {
      "target": {"type": "config_path", "path": "config.retries"},
      "op": {"type": "set", "new_text": "10"}
    },
    {
      "target": {"type": "config_path", "path": "config.timeout"},
      "op": {"type": "set", "new_text": "30"}
    }
  ]
}
```

The same target and operation forms work in the `files` envelope and may be mixed with ordinary node or line operations. Config targets accept `path`, optional `expected_file_hash` and YAML `document_index`; `set` accepts optional `create_missing`. Unknown or duplicate fields and mixed node guard fields, including nulls, are rejected. Config paths are not structural-move destinations.

- Every path and array index addresses the original config source, not earlier proposed output. Deleting an earlier element does not renumber later requests. A path created by one request cannot be selected by another in the same batch.
- Disjoint writes within overlapping guard scopes are combined under the outermost original contributing syntax scope. Existing guard coverage is not reduced; several sibling settings can therefore share one compiled replacement.
- Overlapping writes are rejected, including repeated sets of the same path, overlapping no-ops, shared deletion commas and shared insertion points. Two appends to one array conflict. An append can also conflict with a value edit ending at its insertion point.
- Missing-path creation retains the existing opaque root rewrite. Two creates, or a create and another edit under that root, generally conflict even when their logical paths differ. Separate YAML document roots can remain disjoint. Use separate edit/apply cycles, recompiling each plan, when later requests need earlier results.
- Individual format checks still apply, and a composed config result is checked again. Co-resident raw operations keep their existing rules and may conflict with the entire lowered config container, even if their smaller writes would be disjoint. No general configuration-validity guarantee is added for raw edits.
- Compilation returns a whole plan or an error without writing. Compiled-operation and apply-receipt counts can be smaller than the input request count. Errors refer to original request positions; a final combined-format failure may identify the file rather than invent a conflicting pair.

Pass the resulting changeset, not the edit request, to `apply --dry-run` and then `apply`. Each config file retains its [original full-file condition](protocol.md#file-source-preconditions), including mixed-operation files; supplied hashes are checked, never refreshed.

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
- New config plans also retain a [full-file source precondition](protocol.md#file-source-preconditions). If any source bytes change after planning, recompile the plan; `apply --repair` does not refresh this condition.
- JSON `set` requires one complete JSON value. Value text is preserved as supplied, including numeric spellings, escapes, duplicate members, and surrounding JSON whitespace; it is not parsed into a numeric/object value and reserialized. The resulting document must also be accepted by the bundled JSON parser.
- For JSON `set`, a trailing newline from `--text-file` or `--stdin-text` is inserted too, before the container's comma or closing delimiter. Omit it from the input if that extra line break is unwanted.
- JSON `--create-missing` inserts after the last member of the deepest existing object. Unrelated bytes, member order, numeric spellings, duplicate members, and outer whitespace stay unchanged. Duplicate keys on the selected path, including escaped aliases of the same key, are ambiguous and rejected.
- Creating a missing JSON path requires a comment-free, single-root source document. JSONC creation is unsupported; use line editing for commented JSON.
- New JSON members follow nearby separator/colon spacing when available; existing keys are never sorted. Empty inline objects and new intermediate objects use compact formatting. Empty multiline objects use a local line ending and the closing indentation plus two spaces. Caller-supplied value text is never reindented or newline-normalized.
- Empty or JSON-whitespace-only files, optionally beginning with a UTF-8 BOM, receive a compact root object at EOF. Existing bytes remain in place; no terminal newline is added. Existing in-range array elements can be traversed, but missing arrays and out-of-range indices are not created; use append for array growth.
- TOML/YAML `--create-missing` preserves comments, existing key order, and blank-line groups. It inserts into clearly sorted groups or same-prefix runs, and otherwise appends conservatively without reordering existing keys.
- TOML `--create-missing` can create missing standard table parents such as `[server.sidecar]`; it rejects inline-table parents, array indexes, and table-array parent conflicts.
- YAML multi-document `--create-missing` is never implicit. Add `--document-index <N>` for the target document.
- YAML `--create-missing` preserves comments for block mappings and can create missing intermediate mapping keys. Existing in-range sequence items can be traversed when the selected item is a mapping, but missing sequences and out-of-range sequence indices are rejected; use append for sequence growth.
- YAML multiline create-missing accepts only explicit block scalar leaf values (`|`, `|-`, `|+`, `>`, `>-`, `>+`). Numeric indentation indicators such as `|2`, multiline mappings, and multiline sequences are rejected.
- YAML create-missing quotes unsafe or implicit-scalar-looking string keys while rendering new entries. Use bracket-quoted path segments for literal keys such as `["true"]`, `["null"]`, `["123"]`, or `["app: conf"]`.
- YAML anchors/aliases outside the edited path are allowed and preserved. Create-missing rejects edits inside referenced anchor values and rejects insertion under mappings with YAML merge keys (`<<`). YAML tags remain unsupported for create-missing.
- Fall back to line/direct editing when desired placement depends on project-local comment semantics, cross-section moves, array/table-array restructuring, YAML anchor/merge semantics, or multiline YAML mappings/sequences.
