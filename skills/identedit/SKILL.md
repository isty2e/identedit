---
name: identedit
description: "Precision code editing with hash-based safety. USE WHEN: multi-file atomic edit needed, target text appears multiple times in a large file, or previous Edit landed in wrong place. Supports replace/patch/move of functions/lines, config path edits (JSON/YAML/TOML). NOT for: trivial one-line fixes, full-file rewrites, file-system renames."
---

# Identedit — Agent-Oriented Code Editing

Identedit provides two complementary editing modes:
- **Structural editing** (`read/edit/apply`) — AST-level: replace, delete, or insert whole functions, classes, and blocks
- **Line-anchored editing** (`read --mode line` + `patch`/`apply --repair`) — precise single-line or range edits with hash-based integrity checks

## 10-Second Trigger (Recall First)

If any one condition matches, use identedit:
- 2+ files must succeed/fail together (atomic apply needed)
- large file and repeated target text (misapply risk)
- previous `Edit`/`apply_patch` landed in the wrong place
- previous patch failed because context did not match
- insert at file start/end with precondition safety
- update a nested config key in JSON/YAML/TOML by path
- multiple operations on the same file (replace + insert, etc.)

If none match, default to `Edit`/`Write` for speed.

## I Want To...

| Task | Command |
|---|---|
| Replace a unique function/symbol by name | `identedit patch file --symbol foo --replace 'new body'` |
| Replace a method by containing path | `identedit patch file --symbol Class.method --replace 'new body'` |
| Replace using identity hash | `identedit patch file --at <identity-hex16> --replace 'new body'` |
| Insert at end of file | `identedit patch file --at file-end --insert 'new code'` |
| Update a config key | `identedit patch file --config-path key.path --set-value 42` |
| Append to a config array | `identedit patch file --config-path items --append-value '"x"'` |
| Create a missing config key | `identedit patch file --config-path a.b --set-value 1 --create-missing` |
| Edit a specific line | `identedit patch file --at "LINE:HASH" --set-line 'new line'` |
| Replace with large text (10+ lines) | `identedit patch file --symbol foo --replace --text-file /tmp/body.py` |
| Preview as diff before writing | `identedit patch file --symbol foo --replace --text-file /tmp/body.py --dry-run --diff` |
| Regex replace inside one function/class | `identedit patch file --symbol foo --scoped-regex 'old' --scoped-replacement 'new'` |
| Multiple ops or multi-file atomic | `identedit edit --json` + `identedit apply` (see [Reference](#structural-editing-pipeline)) |
| Move a structure | Use `edit --json` with `move_before`/`move_after` (see [Operations](#operations)) |

## Quick Choice (identedit vs Edit/Write)

| Situation | Use |
|---|---|
| Multi-file atomic edit/rollback required | `identedit edit --json` + `identedit apply` |
| Same pattern appears multiple times in a large file | `identedit patch` |
| Add new function/import at end of file | `identedit patch --at file-end --insert 'text'` |
| Multiple ops on the same file (replace + insert) | `identedit edit --json` with `operations[]` array |
| Append item to a config array (JSON/YAML/TOML) | `identedit patch --config-path items --append-value 4` |
| Regex must be scoped to one function/class | `identedit patch --symbol foo --scoped-regex ...` |
| One-line typo / trivial rename | `Edit` |
| Rewriting most of a file | `Write` |
| Bulk rename across many files | `repren` |

## patch — The Default Entry Point

Most identedit use cases fit in one command:

```bash
# Replace a function by name (no read step needed)
identedit patch src/example.py --symbol process_data \
  --replace 'def process_data(x, y):
    return x + y'

# Replace a method by containing-name path
identedit patch src/example.py --symbol Processor.process_data \
  --replace 'def process_data(self, x, y):
        return x + y'

# Same thing using identity hash (when you already have read output)
identedit patch src/example.py --at <identity-hex16> --replace 'def process_data(x, y):
    return x + y'

# Patch a specific line
identedit read --mode line src/example.py
identedit patch src/example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"

# Append a function at end of file
identedit patch src/example.py --at file-end --insert 'def new_helper():
    pass'

# Config key update (no read needed)
identedit patch config.yaml --config-path server.port --set-value 8080

# Config key with a literal dot in the key name
identedit patch config.yaml --config-path 'services["sidecar.port"].enabled' --set-value true

# Append to a config array
identedit patch config.json --config-path items --append-value '"new_item"'

# Preview as unified diff without writing
identedit patch src/example.py --symbol process_data \
  --replace --text-file /tmp/new_body.py --dry-run --diff
```

`--symbol` targets a unique named node directly — no `read` step needed. It accepts a local name (`process_data`) or a containing-name path (`Processor.process_data`). If the match is ambiguous or missing, patch fails without writing. For ambiguous targets, inspect `error.candidates`: each candidate includes `identity`, `kind`, `name`, `qualified_name`, `span`, `line`, and a one-line `preview`.

Use `--kind` + `--name` when you need kind-specific glob matching (e.g., `--kind function_definition --name "process_*"`). The name supports glob patterns. Use `--name "*"` to match by kind only (e.g., the sole class in a file).

`patch` handles resolve + precondition validation + apply internally. Use `read → edit → apply` only when you need multi-file atomic or multiple operations in one request.

For non-trivial replacements, prefer `--dry-run --diff` first.
For config paths, `--create-missing` creates only missing map/table keys; arrays/sequences are never auto-expanded. TOML comment-preserving creation can insert missing standard table parents, but still rejects inline-table parents and table-array conflicts. YAML comment-preserving creation can insert missing mapping parents and missing keys under existing sequence-item mappings. For multiline YAML, use explicit block scalar leaf values only (`|`, `|-`, `|+`, `>`, `>-`, `>+`); multiline mapping/sequence fragments are rejected.

## Large Text: `--text-file` / `--stdin-text`

When replacing a big function/class body (10+ lines), use `--text-file` to avoid shell quoting issues:

```bash
cat <<'EOF' > /tmp/new_block.py
def target_fn(...):
    ...
EOF

identedit patch /abs/path/file.py --symbol target_fn \
  --replace --text-file /tmp/new_block.py
```

Or pipe text via stdin:

```bash
identedit patch /abs/path/file.py --symbol target_fn \
  --replace --stdin-text < /tmp/new_block.py
```

`--text-file` and `--stdin-text` work with any text-taking flag (`--replace`, `--insert`, `--set-line`, `--replace-range`, `--insert-after-line`, `--set-value`, `--append-value`, `--scoped-replacement`, `--insert-before`, `--insert-after`). Provide the flag without inline text, then add one source: `--replace --text-file /tmp/body.py` or `--replace --stdin-text`.

For the `edit` pipeline (multi-op or multi-file), use `jq --rawfile` instead:

```bash
jq -n --rawfile new_text /tmp/new_block.py '{
  command:"edit",
  file:"/abs/path/file.py",
  operations:[{
    target:{type:"node", identity:"<from read>", kind:"function_definition", expected_old_hash:"<from read>"},
    op:{type:"replace", new_text:$new_text}
  }]
}' | identedit edit --json | identedit apply
```

## Retry Discipline

Maximum 2 identedit attempts per target (1 original + 1 retry). If the second attempt fails, fall back to direct file editing. Do not loop.

```
identedit patch fails
│
├── precondition_failed / target_missing
│   └── re-run identedit read → rebuild request → retry (attempt 2)
│       ├── succeeds → done
│       └── fails again → Edit/Write. STOP.
│
├── ambiguous_target
│   └── inspect error.candidates → choose qualified symbol/identity/span_hint → retry (attempt 2)
│       ├── succeeds → done
│       └── still ambiguous → Edit/Write. STOP.
│
└── any other error (parse_failure, no_provider, ...)
    └── Edit/Write immediately. Do not retry identedit.
```

## Post-Edit Verification

Identedit verifies edit preconditions, not semantic correctness. After non-trivial code edits, run the narrowest useful project verifier yourself.

Recommended pattern:

```bash
identedit patch src/foo.py --symbol process_data --replace --text-file /tmp/process_data.py --dry-run --diff
identedit patch src/foo.py --symbol process_data --replace --text-file /tmp/process_data.py
python -m compileall src/foo.py
pytest tests/test_foo.py -q
```

For multi-file edits:

```bash
identedit edit --json < /tmp/edit-request.json > /tmp/changeset.json
identedit apply --dry-run /tmp/changeset.json
identedit apply /tmp/changeset.json
<project-specific verifier command>
```

Examples: `pytest tests/test_foo.py -q`, `cargo test affected_module::tests`, `npm test -- --runInBand`.

If the verifier fails, treat it as a workflow failure, not an identedit failure: the edit already applied. Inspect verifier output and repair with one bounded follow-up edit attempt.

## Detailed Decision Rules

Default to `Edit`/`Write`/`apply_patch`. Switch to identedit when ANY of the following conditions hold.

### Promote to identedit

| Condition | Use |
|---|---|
| Editing 2+ files that must succeed or fail together | `identedit edit --json` + `identedit apply` (multi-file atomic) |
| File > 150 lines AND the target pattern appears more than once | `identedit patch` — prevents silent misapply. Check: `wc -l file` > 150, then `grep -c "target_text" file` > 1. |
| Previous `Edit`/`apply_patch` applied to the wrong location | `identedit patch` — identity-based targeting doesn't rely on text matching |
| Moving a structural unit within or across files | `identedit edit` with `move_before`/`move_after` |
| Regex replace that must stay inside one function/class, not leak to others | `identedit patch` with `scoped_regex` |
| Updating a nested config key in JSON/YAML/TOML by path | `identedit patch --config-path` |

### Stay with direct editing

| Condition | Use |
|---|---|
| Rewriting > 50% of a file | `Write` — identedit adds overhead with no safety gain |
| Trivial change (typo, one-word rename) | `Edit` — faster |
| Bulk rename across many files | `repren` |
| File-system rename or package move | shell (`mv`, `git mv`) |
| File type not supported by identedit | `Edit` |

## Using with ast-grep

ast-grep and identedit cover different parts of the editing workflow and work well together.

**ast-grep** handles pattern-based discovery — finding all occurrences of a code pattern across a codebase, exploratory analysis, quick pattern-driven rewrites.

**identedit** handles verified editing — identity-based targeting, precondition checks, multi-file atomic transactions with rollback.

**Combined workflow:**
1. **Discover** with ast-grep: `sg --pattern 'def $FUNC($$$): $$$' --json` — find what needs changing
2. **Edit** with identedit: run `identedit read`, then use the returned handles to build an `edit` request — apply changes safely

ast-grep finds the targets, identedit ensures the edits land correctly.

## Using with repren

repren and identedit cover different editing scopes and work well together.

**repren** handles bulk text refactoring — project-wide find-and-replace, simultaneous renames (foo↔bar without intermediary), case-preserving variants (camelCase/snake_case/UPPER), file and directory renaming.

**identedit** handles verified structural edits — AST-level or hash-anchored targeting, precondition checks, multi-file atomic transactions.

**When to use which:**
- Rename a class across the entire codebase → repren
- Replace a specific function body safely → identedit
- Rename files and update all references → repren
- Edit multiple structures atomically with rollback → identedit

---

# Reference

Everything below is power-user and reference material. For most tasks, the sections above are sufficient.

## Structural Editing Pipeline

Every structural edit follows three steps: **read**, **edit**, **apply**.

```
read       →  "What structures exist in this file?"
edit       →  "Here's what I want to change." (dry-run, no file modification)
apply      →  "Commit the changeset to disk."
```

### Step 1: Read — Discover Structures

```bash
identedit read --kind function_definition example.py
```

Output: a list of handles with precondition hashes ready for direct use in edit.

```json
{
  "handles": [
    {
      "file": "example.py",
      "span": { "start": 0, "end": 42 },
      "kind": "function_definition",
      "name": "process_data",
      "identity": "ca465ff1...",
      "expected_old_hash": "20ba467f..."
    }
  ],
  "summary": { "files_scanned": 1, "matches": 1 },
  "file_preconditions": [
    { "file": "example.py", "expected_file_hash": "a1b2c3d4..." }
  ]
}
```

By default, `read` returns compact handles (no `text` field). Use `--verbose` when you explicitly need matched text payloads for debugging.

Key fields for the edit step:
- `identity` + `expected_old_hash` → copy directly into a `node` target
- `file_preconditions[].expected_file_hash` → copy into a `file_start`/`file_end` target

Common kind values by language:

| Language | Functions | Classes | Methods |
|---|---|---|---|
| Python | `function_definition` | `class_definition` | `function_definition` |
| JS/TS | `function_declaration` | `class_declaration` | `method_definition` |
| Rust | `function_item` | `struct_item`, `impl_item` | `function_item` |
| Go | `function_declaration` | `type_declaration` | `method_declaration` |

Use `--name "process_*"` to filter by name (glob patterns supported).

Use `--exclude-kind method_definition` to exclude nested structures.

Multiple files: `identedit read --kind function_definition src/*.py`

### Step 2: Edit — Build an Edit Plan

**Flag mode** (single operation):
```bash
identedit edit \
  --identity ca465ff1... \
  --replace 'def process_data(x, y):
    return x + y' \
  example.py
```

For large `new_text` (10+ lines), use `patch --text-file` instead — see [Large Text](#large-text---text-file----stdin-text).

**JSON mode** (multiple operations, recommended):
```bash
echo '{
  "command": "edit",
  "file": "example.py",
  "operations": [
    {
      "target": {
        "type": "node",
        "identity": "ca465ff1...",
        "kind": "function_definition",
        "expected_old_hash": "20ba467f...",
        "span_hint": { "start": 0, "end": 42 }
      },
      "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
    }
  ]
}' | identedit edit --json
```

**Handle ref mode** (reuse read handles, reduces payload size):
```bash
echo '{
  "command": "edit",
  "file": "example.py",
  "handle_table": {
    "h1": { "identity": "ca465ff1...", "kind": "function_definition", "expected_old_hash": "20ba467f...", "span_hint": { "start": 0, "end": 42 } }
  },
  "operations": [
    {
      "target": { "type": "handle_ref", "ref": "h1" },
      "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
    }
  ]
}' | identedit edit --json
```

`handle_table` maps short keys to full node targets. Use `handle_ref` in operations to reference them. In batch mode, each `files[i]` entry has its own `handle_table` (file-scoped, no cross-file refs).

Batch JSON mode (multiple files in one request):
```bash
echo '{
  "command": "edit",
  "files": [
    { "file": "a.py", "operations": [ ... ] },
    { "file": "b.py", "operations": [ ... ] }
  ]
}' | identedit edit --json
```

Rule: request payload must include exactly one shape:
- single-file: `file` + `operations`
- batch: `files`

Output: a changeset JSON with compact preview diffs. **No files are modified** — edit is always a dry-run.

By default, previews are compact (`old_hash` + `old_len` instead of full `old_text`). Use `--verbose` to include `old_text` for debugging.

If the apply fails, follow the [Retry Discipline](#retry-discipline).

#### Merging Multiple Edit Outputs

When you run `edit` separately per file, compose outputs with:

```bash
identedit merge change_a.json change_b.json > merged_changeset.json
```

Then apply once:

```bash
identedit apply merged_changeset.json
```

Merge policy is strict by default:
- non-overlapping edits on the same file are merged
- conflicting/overlapping same-file edits are rejected with `invalid_request`
- move + content edit for the same file is rejected

#### Operations

| Op | Target | Description |
|---|---|---|
| `replace` | `node` | Replace the full text of a structural unit |
| `delete` | `node` | Remove a structural unit |
| `insert_before` | `node` | Insert text immediately before a structure |
| `insert_after` | `node` | Insert text immediately after a structure |
| `move_before` | `node` (source + dest) | Move source node to just before destination node |
| `move_after` | `node` (source + dest) | Move source node to just after destination node |
| `scoped_regex` | `node` | Regex replace within the node's text (precondition-verified) |
| `insert` | `file_start` | Insert text at the beginning of the file |
| `insert` | `file_end` | Insert text at the end of the file |

#### File-Level Targets

For `file_start` and `file_end`, use `expected_file_hash` (blake3 hash of the entire file content) instead of node identity:

```json
{
  "target": {
    "type": "file_end",
    "expected_file_hash": "a1b2c3d4..."
  },
  "op": { "type": "insert", "new_text": "\n\ndef new_function():\n    pass\n" }
}
```

Get the file hash from the `read` output's `file_preconditions` array — no external tools needed.

### Step 3: Apply — Commit to Disk

```bash
identedit edit --json < request.json | identedit apply
```

Or from a saved changeset file:
```bash
identedit apply changeset.json
```

Output (compact by default):
```json
{
  "summary": { "files_modified": 1, "operations_applied": 1, "operations_failed": 0 },
  "transaction": { "mode": "all_or_nothing", "status": "committed" }
}
```

Use `--verbose` for per-file details (`applied` array with per-file operation counts).

**All-or-nothing**: if any operation fails, all changes are rolled back. No partial edits.

### Pipe-first Workflows (Recommended)

Single request, no temp file:
```bash
cat request.edit.json \
| identedit edit --json \
| identedit apply
```

Multiple independent edit requests, merged then applied:
```bash
identedit merge \
  <(cat a.edit.json | identedit edit --json) \
  <(cat b.edit.json | identedit edit --json) \
| identedit apply
```

Note: process substitution (`<(...)`) requires `zsh` or `bash`. For POSIX shells, write intermediate outputs to temp files and merge those paths.

Batch edit (multi-file in one request) then apply:
```bash
cat request.batch-edit.json \
| identedit edit --json \
| identedit apply
```

Notes:
- `identedit apply` (without `--json`) accepts a raw `MultiFileChangeset` from stdin.
- `identedit apply --json` expects wrapper shape:
  - `{ "command": "apply", "changeset": { ... } }`

---

## Line-Anchored Editing

For line-level precision edits where structural targeting is too coarse.

### Step 1: Read Line Anchors

```bash
identedit read --mode line example.py
```

Default output is:

```text
1:a1b2c3d4e5f6|import os
2:f7e8d9c0a1b2|
3:3c4d5e6f7a8b|def process_data(x):
4:9e0f1a2b3c4d|    return x + 1
```

Each line has a `LINE:HASH` anchor (12-char blake3 hex). Use `--json` if you need machine-readable output.

### Step 2: Patch with a Line Target

```bash
identedit patch --at "4:9e0f1a2b3c4d" --set-line "    return x + y" example.py
identedit patch --at "3:3c4d5e6f7a8b" --replace-range "def process_data(x, y):\n    return x + y" --end-anchor "4:9e0f1a2b3c4d" example.py
identedit patch --at "4:9e0f1a2b3c4d" --insert-after-line "    # added line" example.py
```

Line operations:
- `--set-line`
- `--replace-range` (optional `--end-anchor`)
- `--insert-after-line`

Use `--auto-repair` once if strict matching fails but deterministic remap is possible.

`patch --at` auto-detects target type by format:
- `4:9e0f1a2b3c4d` (number:12hex) → line anchor
- `ca465ff1a2b3c4d5` (16hex) → node identity
- `file-start` / `file-end` → file boundary

On failure, follow the [Retry Discipline](#retry-discipline). For line-anchored edits specifically: re-run `read --mode line` for fresh anchors, retry strict once, then try `--auto-repair` as the second attempt.

## Config Path Patching (JSON/YAML/TOML)

Use config-aware path targeting when you need to update nested keys without large brittle block replacement.

```bash
identedit patch --config-path service.retries --set-value 5 example.yaml
identedit patch --config-path items --append-value 4 example.json
identedit patch --config-path database.settings.enabled --delete example.toml
identedit patch --config-path 'services["sidecar.port"].enabled' --set-value true example.yaml
identedit patch manifests.yaml --config-path spec.replicas \
  --document-index 1 --set-value 3 --create-missing
```

JSON mode:

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

Append JSON variant:

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

Path syntax:

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

YAML block scalar creation:

```bash
cat <<'EOF' | identedit patch .github/workflows/ci.yml \
  --config-path jobs.build.steps[0].run --set-value --create-missing --stdin-text
|
  cargo test
  cargo clippy --all-targets -- -D warnings
EOF
```

Config path rules:
- `set` updates an existing path; use `create_missing: true` (JSON mode) or `--create-missing` (flag mode) only when creating missing map/table keys.
- `append` requires the resolved target path to be an existing array/sequence.
- `delete` and `append` reject `create_missing`.
- Missing paths, ambiguous matches, malformed syntax, and out-of-range indices fail with explicit `invalid_request` errors.
- Config path edits are validated against the target format before writing — syntax-breaking edits are rejected.
- TOML `--create-missing` preserves comments and can create missing standard table parents such as `[server.sidecar]`; it still rejects inline-table parents, array indexes, and table-array parent conflicts.
- YAML multi-document `--create-missing` is never implicit. Add `--document-index <N>` for the target document; do not rely on document order unless the manifest format makes that order meaningful.
- YAML `--create-missing` preserves comments for block mappings and can create missing intermediate mapping keys. Existing in-range sequence items can be traversed when the selected item is a mapping, but missing sequences and out-of-range sequence indices are rejected; use append for sequence growth.
- YAML multiline create-missing is an identedit-local policy: only explicit block scalar leaf values are accepted (`|`, `|-`, `|+`, `>`, `>-`, `>+`). Numeric indentation indicators such as `|2`, multiline mappings, and multiline sequences are rejected; use line/direct editing for those broader rewrites.
- YAML create-missing quotes unsafe or implicit-scalar-looking string keys while rendering new entries. Use bracket-quoted path segments for literal keys such as `["true"]`, `["null"]`, `["123"]`, or `["app: conf"]`.

---

## Multi-File Transactions

Use `edit` to compile a multi-file changeset first, then apply it atomically:

```bash
# request.json contains the edit request (single-file or files[] batch shape)
identedit edit --json < request.json > changeset.json

# commit from plan file
identedit apply changeset.json

# equivalent wrapped stdin mode (when you need command envelope)
jq -n --slurpfile plan changeset.json '{
  command: "apply",
  changeset: $plan[0]
}' | identedit apply --json
```

`apply --json` expects a compiled changeset (the output of `identedit edit --json`), not a raw edit request.

If any file fails, all files are rolled back to their original state.

Staging-only rollback rehearsal:
```bash
IDENTEDIT_EXPERIMENTAL=1 identedit apply --inject-failure-after-writes 1 changeset.json
```
Use this only for operational drills. It injects a deterministic commit-stage failure before write `N+1` (for `N=1`, one write commits, then rollback is exercised).

## Error Recovery

| Error | Meaning | Action |
|---|---|---|
| `precondition_failed` | File changed since read | Re-run read, rebuild edit request, retry |
| `target_missing` | Structure no longer exists | Re-run read to discover current state |
| `ambiguous_target` | Multiple matches for a selector | Inspect `error.candidates`, then retry with `--symbol <qualified_name>`, `--at <identity>`, or JSON `span_hint` |
| `path_changed` | File modified during apply | Re-run full pipeline (read, edit, apply) |
| `resource_busy` | Another apply in progress | Wait briefly, retry |
| `rollback_failed` | Apply failed and rollback incomplete | Inspect files manually, then re-run pipeline |
| `parse_failure` | Source file has syntax errors | Fix syntax first, then retry |
| `no_provider` | Unsupported file type | Use direct editing instead |

See [Retry Discipline](#retry-discipline) for attempt limits.

## Important Notes

- `edit` is always a dry-run. Review the `preview` field before piping to apply.
- `patch --dry-run` validates and previews the edit without writing files.
- The `identity` hash is derived from the structure's kind, name, and text content (not position). Two identical functions at different positions share the same identity. It changes when the code content changes.
- The `expected_old_hash` / `expected_file_hash` fields are preconditions. They ensure you are editing what you think you are editing.
- Hashline anchors are 12-char blake3 hex hashes. Exact matching, no prefix matching.
- All identedit output (success and error) is JSON, except `read --mode line` which defaults to text format (`LINE:HASH|content`) and `patch --dry-run --diff` which emits unified diff text. Use `--json` for structured `read --mode line` output. Parse JSON output, do not grep it.
- When creating new files, create the file first (e.g., `touch new_file.py`), then use identedit's `file_end` insert to add content structurally.

## Supported Languages

**Bundled** (work out of the box, no install needed):

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL (Terraform), Dockerfile

**Installable** via `identedit grammar install`:

Any language with a tree-sitter grammar can be added. Three tiers of install convenience:

Host support note:
- `grammar install` currently works on macOS and Linux hosts.
- On Windows hosts, use bundled grammars or copy artifacts built on macOS/Linux.

1. **Registry languages** — no options needed, auto-resolved:
   ```bash
   identedit grammar install elixir
   identedit grammar install zig
   identedit grammar install dart
   ```
   Registry includes: Elixir, Elm, Erlang, Haskell, Julia, Scala, Zig, Dart, OCaml, Clojure, F#, Fortran, Groovy, CUDA, R, Svelte, Vue, Astro, Nix, Racket, Scheme, Solidity, Typst, Pascal, Common Lisp, and more.

2. **Convention languages** — `--ext` required, repo auto-detected:
   ```bash
   identedit grammar install somelang --ext xyz
   ```
   Works when the grammar repo follows `tree-sitter/tree-sitter-{lang}` or `tree-sitter-grammars/tree-sitter-{lang}` naming.

3. **Custom grammars** — specify repo explicitly:
   ```bash
   identedit grammar install mylang --repo https://github.com/user/tree-sitter-mylang --ext ml
   ```

## Feedback

This tool is under active development. When you encounter friction (confusing errors, unexpected behavior, missing features, awkward workflow steps), open an issue at:

- https://github.com/isty2e/identedit/issues

Include:
- What you were trying to do
- What happened (include the error or unexpected output)
- What you expected instead
