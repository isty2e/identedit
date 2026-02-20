---
name: identedit
description: Precision code editing with hash-based safety. Replace/patch/move/copy functions or lines with precondition verification. Also handles config path edits (JSON/YAML/TOML). Not for large-scale rewrites or file-system renames — use Edit/Write/shell for those.
---

# Identedit — Agent-Oriented Code Editing

Identedit provides two complementary editing modes:
- **Structural editing** (`read/edit/apply`) — AST-level: replace, delete, or insert whole functions, classes, and blocks
- **Line-anchored editing** (`read --mode line` + `patch`/`apply --repair`) — precise single-line or range edits with hash-based integrity checks

Canonical command surface:
- `identedit read`
- `identedit edit`
- `identedit apply`
- `identedit patch`
- `identedit merge`
- `identedit grammar`

Supported CLI surface: `identedit read`, `identedit edit`, `identedit apply`, `identedit patch`, `identedit merge`, `identedit grammar`.

## 10-Second Trigger (Recall First)

If any one condition matches, use identedit:
- 2+ files must succeed/fail together (atomic apply needed)
- large file and repeated target text (misapply risk)
- previous `Edit`/`apply_patch` landed in the wrong place

If none match, default to `Edit`/`Write` for speed.

## Quick Choice (identedit vs Edit/Write)

| Situation | Use |
|---|---|
| Multi-file atomic edit/rollback required | `identedit edit --json` + `identedit apply` |
| Same pattern appears multiple times in a large file | `identedit patch` |
| One-line typo / trivial rename | `Edit` |
| Rewriting most of a file | `Write` |
| Bulk rename across many files | `repren` |

## patch — The Default Entry Point

Most identedit use cases fit in one command:

```bash
# Replace a function (read once, then patch)
identedit read --kind function_definition --name process_data --json src/example.py
identedit patch src/example.py --identity <id> --replace 'def process_data(x, y):
    return x + y'

# Patch a specific line
identedit read --mode line src/example.py
identedit patch src/example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"

# Config key update (no read needed)
identedit patch config.yaml --config-path server.port --set-value 8080
```

`patch` handles resolve + precondition validation + apply internally. Use `read → edit → apply` only when you need multi-file atomic or multiple operations in one request.

## Fast Recipe: Large `new_text` Patch

Use this when replacing a big function/class body.

```bash
identedit read --kind function_definition --name target_fn --json /abs/path/file.py

cat <<'EOF' > /tmp/new_block.py
def target_fn(...):
    ...
EOF

jq -n --rawfile new_text /tmp/new_block.py '{
  command:"edit",
  file:"/abs/path/file.py",
  operations:[{
    target:{type:"node", identity:"<from read>", kind:"function_definition", expected_old_hash:"<from read>"},
    op:{type:"replace", new_text:$new_text}
  }]
}' | identedit edit --json | identedit apply
```

Failure loop:
1. `read` again for fresh `identity` / `expected_old_hash`.
2. Rebuild `jq --rawfile` request and retry once.
3. If it still fails, switch to `Edit`/`Write`.

## Detailed Decision Rules

Default to `Edit`/`Write`/`apply_patch`. Switch to identedit when ANY of the following conditions hold.

### Promote to identedit

| Condition | Use |
|---|---|
| Editing 2+ files that must succeed or fail together | `identedit edit --json` + `identedit apply` (multi-file atomic) |
| File > 150 lines AND the target pattern appears more than once | `identedit patch` — prevents silent misapply. Check: `wc -l file` > 150, then `grep -c "target_text" file` > 1. |
| Previous `Edit`/`apply_patch` applied to the wrong location | `identedit patch` — identity-based targeting doesn't rely on text matching |
| Moving or copying a structural unit within or across files | `identedit edit` with `move_before`/`move_after`/`copy_before`/`copy_after` |
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

### Fallback rules

```
identedit patch fails
│
├── precondition_failed / target_missing
│   └── re-run identedit read → rebuild request → retry (attempt 2)
│       ├── succeeds → done
│       └── fails again → Edit/Write. STOP.
│
├── ambiguous_target
│   └── add span_hint from read output → retry (attempt 2)
│       ├── succeeds → done
│       └── still ambiguous → Edit/Write. STOP.
│
└── any other error (parse_failure, no_provider, ...)
    └── Edit/Write immediately. Do not retry identedit.
```

Maximum 2 identedit attempts per target. Never retry a third time.

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

---

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

**Large new_text (10+ lines): use `jq --rawfile` to avoid escape issues:**

Use an absolute path for the temp file (e.g. `/tmp/new_block.py`) to avoid working-directory ambiguity.

```bash
cat <<'EOF' > /tmp/new_block.py
def process_data(x, y):
    # new implementation
    return x + y
EOF

jq -n --rawfile new_text /tmp/new_block.py '{
  command:"edit",
  file:"example.py",
  operations:[{
    target:{type:"node", identity:"ca465ff1...", kind:"function_definition", expected_old_hash:"20ba467f..."},
    op:{type:"replace", new_text:$new_text}
  }]
}' | identedit edit --json | identedit apply
```

If the apply fails:
1. Re-run `identedit read` to get fresh `identity` and `expected_old_hash`.
2. Rebuild the `jq` request and retry once.
3. If it fails again → use `Edit`/`Write` directly. Stop.

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
| `copy_before` | `node` (source + dest) | Copy source node to just before destination (source stays) |
| `copy_after` | `node` (source + dest) | Copy source node to just after destination (source stays) |
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

### Error Recovery Loop

1. Run `read --mode line` to regenerate fresh anchors.
2. Retry strict `patch` once.
3. If strict fails due to stale anchors, retry once with `--auto-repair`.
4. If still failing (or ambiguous), regenerate anchors and rebuild request.
5. Max 2 retries, then fall back to direct editing.

## Config Path Patching (JSON/YAML/TOML)

Use config-aware path targeting when you need to update nested keys without large brittle block replacement.

```bash
identedit patch --config-path service.retries --set-value 5 example.yaml
identedit patch --config-path items --append-value 4 example.json
identedit patch --config-path database.settings.enabled --delete example.toml
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

Path syntax is dot/bracket only (for example `a.b[1].c`).

Config path rules:
- `set` updates an existing path; use `create_missing: true` (JSON mode) or `--create-missing` (flag mode) only when creating missing map/table keys.
- `append` requires the resolved target path to be an existing array/sequence.
- `delete` and `append` reject `create_missing`.
- Missing paths, ambiguous matches, malformed syntax, and out-of-range indices fail with explicit `invalid_request` errors.

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
| `ambiguous_target` | Multiple matches for identity | Add `span_hint` or re-run read for fresh handles |
| `path_changed` | File modified during apply | Re-run full pipeline (read, edit, apply) |
| `resource_busy` | Another apply in progress | Wait briefly, retry |
| `rollback_failed` | Apply failed and rollback incomplete | Inspect files manually, then re-run pipeline |
| `parse_failure` | Source file has syntax errors | Fix syntax first, then retry |
| `no_provider` | Unsupported file type | Use direct editing instead |

**Retry discipline**: maximum 2 attempts per target (1 original + 1 retry). If the second attempt fails, fall back to direct file editing. Do not loop.

## Important Notes

- `edit` is always a dry-run. Review the `preview` field before piping to apply.
- The `identity` hash is derived from the structure's kind, name, and text content (not position). Two identical functions at different positions share the same identity. It changes when the code content changes.
- The `expected_old_hash` / `expected_file_hash` fields are preconditions. They ensure you are editing what you think you are editing.
- Hashline anchors are 12-char blake3 hex hashes. Exact matching, no prefix matching.
- All identedit output (success and error) is JSON, except `read --mode line` which defaults to text format (`LINE:HASH|content`). Use `--json` for structured output. Parse JSON output, do not grep it.
- When creating new files, create the file first (e.g., `touch new_file.py`), then use identedit's `file_end` insert to add content structurally.

## Feedback

This tool is under active development. When you encounter friction (confusing errors, unexpected behavior, missing features, awkward workflow steps), open an issue at:

- https://github.com/isty2e/identedit/issues

Include:
- What you were trying to do
- What happened (include the error or unexpected output)
- What you expected instead
