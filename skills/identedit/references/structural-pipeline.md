# Structural Pipeline Reference

Use this when `patch` is not enough: multiple operations, multi-file atomic edits, moves, handle refs, or explicit edit-plan construction.

## Flow

```text
read       ->  discover structures and precondition hashes
edit       ->  compile requested changes into a dry-run changeset
apply      ->  commit the changeset to disk
```

## Step 1: Read Structures

Use `--json` when the output will feed `edit`.

```bash
identedit read --kind function_definition example.py --json
```

Output shape:

```json
{
  "handles": [
    {
      "target_type": "node",
      "file": "example.py",
      "span": { "start": 0, "end": 42 },
      "kind": "function_definition",
      "name": "process_data",
      "identity": "ca465ff1a2b3c4d5",
      "expected_old_hash": "20ba467fa1b2c3d4"
    }
  ],
  "summary": { "files_scanned": 1, "matches": 1 },
  "file_preconditions": [
    { "file": "example.py", "expected_file_hash": "a1b2c3d4e5f6a7b8" }
  ]
}
```

JSON node handles omit `text` by default. Use `--verbose` for exact matched text. Symbol reads also include a separate source window, described below.

### Symbol with context

```bash
identedit read example.py --symbol Processor.process_data --context 3
identedit read example.py --symbol Processor.process_data --context 3 --json
```

`--symbol` uses the same exact local or containing-name match as `patch --symbol`. Each file must have one match; missing or ambiguous targets fail the entire read without partial output. It cannot be combined with kind/name/exclusion filters. `--context` requires `--symbol`, defaults to zero, and adds up to N lines on each side without truncating the symbol. These options require positional files; JSON-stdin selector requests do not support them.

Text output shows a `# node --at ID` header, exact byte extent, and one line-anchored source window. `# context` and `# target lines` label the regions. The first and last target lines can contain bytes outside the node; the byte extent defines the node replacement, not whole visible lines. No display indentation is added. Even with `--verbose`, text output prints code only once.

JSON keeps the single node in `handles[]` and adds `windows[]`, one entry per file:

- `kind: "symbol"`, `file`, `total_lines`, `start_line`, `end_line`, `omitted_before`, `omitted_after` describe the visible window.
- `target_lines: {start, end}` gives the inclusive original lines intersecting the node.
- `target_start_column` and `target_end_column` are 1-based UTF-8 byte columns on those lines; the end is exclusive. They are not character or display columns. Line terminator bytes are counted if the node contains them.
- `lines: [{line, anchor, text}]` contains the complete window, including context; these records are not extra selected nodes.

`summary.matches` counts node handles, not context lines. `--verbose` adds exact raw node `text` to the JSON handle; without it, source appears only in the window. Metadata, addresses and full-file hashes come from the same loaded bytes. Feed the node's existing fields to `edit` as usual; do not use the surrounding window as replacement text.

Fields for `edit`:
- `identity` + `expected_old_hash`: copy into a `node` target.
- `file_preconditions[].expected_file_hash`: copy into a `file_start` or `file_end` target.

Common kind values:

| Language | Functions | Classes | Methods |
|---|---|---|---|
| Python | `function_definition` | `class_definition` | `function_definition` |
| JS/TS | `function_declaration` | `class_declaration` | `method_definition` |
| Rust | `function_item` | `struct_item`, `impl_item` | `function_item` |
| Go | `function_declaration` | `type_declaration` | `method_declaration` |

Filters:
- `--name "process_*"` filters by glob.
- `--exclude-kind method_definition` excludes nested structures.
- Multiple files: `identedit read --kind function_definition src/*.py --json`.

## Step 2: Edit Plan

Flag mode supports the same single-target selectors and operations as `patch`, but only emits a changeset:

```bash
identedit edit \
  --at ca465ff1a2b3c4d5 \
  --replace 'def process_data(x, y):
    return x + y' \
  example.py
```

For large `new_text`, use `--text-file` or `--stdin-text` in flag mode, or `jq --rawfile` in JSON mode.

JSON mode supports multiple operations:

```bash
echo '{
  "command": "edit",
  "file": "example.py",
  "operations": [
    {
      "target": {
        "type": "node",
        "identity": "ca465ff1a2b3c4d5",
        "kind": "function_definition",
        "expected_old_hash": "20ba467fa1b2c3d4",
        "span_hint": { "start": 0, "end": 42 }
      },
      "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
    }
  ]
}' | identedit edit --json
```

With `jq --rawfile`:

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

## Handle Ref Mode

Use `handle_table` to reduce payload size when several operations reuse handles.

```bash
echo '{
  "command": "edit",
  "file": "example.py",
  "handle_table": {
    "h1": {
      "identity": "ca465ff1a2b3c4d5",
      "kind": "function_definition",
      "expected_old_hash": "20ba467fa1b2c3d4",
      "span_hint": { "start": 0, "end": 42 }
    }
  },
  "operations": [
    {
      "target": { "type": "handle_ref", "ref": "h1" },
      "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
    }
  ]
}' | identedit edit --json
```

In batch mode, each `files[i]` entry has its own file-scoped `handle_table`. Do not reuse refs across files.

## Batch JSON Mode

```bash
echo '{
  "command": "edit",
  "files": [
    { "file": "a.py", "operations": [ ] },
    { "file": "b.py", "operations": [ ] }
  ]
}' | identedit edit --json
```

Request payload must include exactly one shape:
- single-file: `file` + `operations`
- batch: `files`

`edit` output is a changeset JSON. It never modifies files. Compact previews use `old_hash` + `old_len`; add `--verbose` for `old_text` debugging.

## Operations

| Op | Target | Description |
|---|---|---|
| `replace` | `node` | Replace the full text of a structural unit |
| `delete` | `node` | Remove a structural unit |
| `insert_before` | `node` | Insert text immediately before a structure |
| `insert_after` | `node` | Insert text immediately after a structure |
| `move_before` | `node` source + dest | Move source node to just before destination node |
| `move_after` | `node` source + dest | Move source node to just after destination node |
| `scoped_regex` | `node` | Regex replace within the node's text |
| `insert` | `file_start` | Insert text at beginning of file |
| `insert` | `file_end` | Insert text at end of file |

## File-Level Targets

For `file_start` and `file_end`, use `expected_file_hash` instead of node identity.

```json
{
  "target": {
    "type": "file_end",
    "expected_file_hash": "a1b2c3d4e5f6a7b8"
  },
  "op": { "type": "insert", "new_text": "\n\ndef new_function():\n    pass\n" }
}
```

Get the file hash from `read --json` output's `file_preconditions` array.

## Merge and Pipe Workflows

Merge separately built changesets:

```bash
identedit merge change_a.json change_b.json > merged_changeset.json
identedit apply merged_changeset.json
```

Strict merge policy:
- non-overlapping edits on the same file are merged;
- conflicting/overlapping same-file edits are rejected with `invalid_request`;
- move + content edit for the same file is rejected.

Pipe-first examples:

```bash
cat request.edit.json \
| identedit edit --json \
| identedit apply
```

```bash
identedit merge \
  <(cat a.edit.json | identedit edit --json) \
  <(cat b.edit.json | identedit edit --json) \
| identedit apply
```

Process substitution requires `zsh` or `bash`. For POSIX shells, write intermediate outputs to temp files and merge those paths.
