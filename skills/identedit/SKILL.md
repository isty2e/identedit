---
name: identedit
description: "Precision code editing with precondition safety. USE WHEN: multi-file atomic edits, repeated target text, config-path edits, or a previous edit attempt failed or landed in the wrong place. NOT for: trivial one-line fixes, full-file rewrites, file-system renames."
---

# Identedit — Agent-Oriented Code Editing

Identedit is a surgical editor for cases where target stability matters more than raw editing speed.

Two modes:
- **Structural**: AST-level `read` / `edit` / `apply` for functions, classes, blocks, file boundaries, and moves.
- **Line-anchored**: `read --mode line` plus `patch` / `apply --repair` for exact line or range edits.

## 10-Second Trigger

Use identedit when any condition matches:
- 2+ files must succeed or fail together.
- Large file and repeated target text make direct patching risky.
- A previous edit attempt failed from context mismatch.
- A previous edit attempt landed in the wrong location.
- You need insert-at-file-start/end with precondition safety.
- You need a nested JSON/YAML/TOML config key edit by path.
- You need multiple operations on the same file in one verified plan.

Otherwise prefer direct file editing for speed.

## Quick Commands

Use this table as a command lookup once you already know identedit is the right tool.

| Task | Command |
|---|---|
| Replace a unique function/symbol by name | `identedit patch file --symbol foo --replace 'new body'` |
| Replace a method by containing path | `identedit patch file --symbol Class.method --replace 'new body'` |
| Replace using identity hash | `identedit patch file --at <identity-hex16> --replace 'new body'` |
| Insert at end of file | `identedit patch file --at file-end --insert 'new code'` |
| Update a config key | `identedit patch file --config-path key.path --set-value 42` |
| Append to a config array | `identedit patch file --config-path items --append-value 4` |
| Create a missing config key | `identedit patch file --config-path a.b --set-value 1 --create-missing` |
| Edit a specific line | `identedit patch file --at "LINE:HASH" --set-line 'new line'` |
| Replace with large text | `identedit patch file --symbol foo --replace --text-file /tmp/body.py` |
| Preview before writing | `identedit patch file --symbol foo --replace --text-file /tmp/body.py --dry-run --diff` |
| Recover candidate targets from a failed diff | `identedit patch --from-diff failed.diff file` |
| Regex replace inside one function/class | `identedit patch file --symbol foo --scoped-regex 'old' --scoped-replacement 'new'` |
| Multi-op or multi-file atomic | `identedit edit --json` then `identedit apply` |
| Move a structure | `identedit edit --json` with `move_before` / `move_after` |

## Quick Choice

Use this table to decide whether identedit is worth the overhead versus direct editing.

| Situation | Use |
|---|---|
| Multi-file atomic edit/rollback required | `identedit edit --json` + `identedit apply` |
| Same pattern appears multiple times in a large file | `identedit patch` |
| Add new function/import at end of file | `identedit patch --at file-end --insert 'text'` |
| Multiple ops on the same file | `identedit edit --json` with `operations[]` |
| Append item to a config array | `identedit patch --config-path items --append-value 4` |
| Regex must stay inside one function/class | `identedit patch --symbol foo --scoped-regex ...` |
| One-line typo / trivial rename | Direct file editing |
| Rewriting most of a file | File rewrite |
| Bulk rename across many files | `repren` |
| File-system rename or package move | shell (`mv`, `git mv`) |

## Default Flow: `patch`

Most uses fit in one command:

```bash
# Replace a function by name; no read step needed.
identedit patch src/example.py --symbol process_data \
  --replace 'def process_data(x, y):
    return x + y'

# Replace a method by containing-name path.
identedit patch src/example.py --symbol Processor.process_data \
  --replace 'def process_data(self, x, y):
        return x + y'

# Use identity from read output when you already have it.
identedit patch src/example.py --at <identity-hex16> --replace 'def process_data(x, y):
    return x + y'

# Patch a specific line.
identedit read --mode line src/example.py
identedit patch src/example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"

# Append at file end.
identedit patch src/example.py --at file-end --insert 'def new_helper():
    pass'

# Config key update.
identedit patch config.yaml --config-path server.port --set-value 8080

# Config key with a literal dot.
identedit patch config.yaml --config-path 'services["sidecar.port"].enabled' --set-value true

# Preview as unified diff without writing.
identedit patch src/example.py --symbol process_data \
  --replace --text-file /tmp/new_body.py --dry-run --diff
```

`--symbol` targets a unique named node directly. It accepts a local name (`process_data`) or containing-name path (`Processor.process_data`). Ambiguous targets fail without writing and return `error.candidates` with identity, kind, name, qualified name, span, line, and preview.

Use `--kind` + `--name` for kind-specific glob matching, e.g. `--kind function_definition --name "process_*"`.

For non-trivial replacements, prefer `--dry-run --diff` first.

## Failed Diff Handoff

When a direct patch fails from context drift, recover exact line candidates without applying it:

```bash
identedit patch --from-diff failed.diff src/example.py > handoff.json
# Or stream the diff:
cat failed.diff | identedit patch --from-diff - src/example.py > handoff.json
```

The command is always preview-only. Each changed block reports `unique`, `ambiguous`, or `missing` and preserves every exact line-boundary candidate in source order. For a `unique` result, `candidate.target` and `candidate.op` can be copied directly into a `patch --json` request. Inspect `candidate.preview` before choosing an ambiguous candidate; never auto-select the first one.

Use this for a one-file unified diff, an `apply_patch` `*** Update File` block, or a bare hunk plus explicit `FILE`. It rejects multi-file/create/delete/rename diffs and conflicting file paths.

See `references/failed-diff-handoff.md` for the response schema and a safe preview-to-apply example.

## Large Text Payloads

Use `--text-file` or `--stdin-text` for multi-line payloads instead of shell-quoted strings.

```bash
cat <<'EOF' > /tmp/new_block.py
def target_fn(...):
    ...
EOF

identedit patch /abs/path/file.py --symbol target_fn \
  --replace --text-file /tmp/new_block.py
```

```bash
identedit patch /abs/path/file.py --symbol target_fn \
  --replace --stdin-text < /tmp/new_block.py
```

These work with text-taking flags such as `--replace`, `--insert`, `--set-line`, `--replace-range`, `--insert-after-line`, `--set-value`, `--append-value`, `--scoped-replacement`, `--insert-before`, and `--insert-after`.

For the `edit` pipeline, use `jq --rawfile` instead. See `references/structural-pipeline.md`.

## Multi-Step Structural Pipeline

Use this for multi-op, multi-file, handle-table, or move workflows.

```bash
# 1. Read handles and precondition hashes.
identedit read --kind function_definition example.py --json

# 2. Build an edit plan. This is always dry-run.
identedit edit --json < request.json > changeset.json

# 3. Validate or commit.
identedit apply --dry-run changeset.json
identedit apply changeset.json
```

`read` defaults to human-readable text. Add `--json` when you need handles for `edit`. Add `--verbose` only when you need full matched text.

For schema details and operations, read `references/structural-pipeline.md`.

## Line-Anchored Editing

Use line mode when structural targeting is too coarse.

```bash
identedit read --mode line example.py
identedit patch example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"
identedit patch example.py --at "3:3c4d5e6f7a8b" \
  --replace-range "def process_data(x, y):\n    return x + y" \
  --end-anchor "4:9e0f1a2b3c4d"
```

Line anchors are `LINE:HASH` with a 12-char blake3 hex hash. Matching is exact; no prefix matching. Use `--auto-repair` once if strict matching fails but deterministic remap is possible.

For line JSON and repair details, read `references/line-editing.md`.

## Config Path Patching

Use config-aware path targeting for nested JSON/YAML/TOML edits.

```bash
identedit patch config.yaml --config-path service.retries --set-value 5
identedit patch config.json --config-path items --append-value 4
identedit patch config.toml --config-path database.settings.enabled --delete
identedit patch config.yaml --config-path 'services["sidecar.port"].enabled' --set-value true
identedit patch manifests.yaml --config-path spec.replicas \
  --document-index 1 --set-value 3 --create-missing
```

Important limits:
- `--create-missing` creates missing map/table keys only; arrays/sequences are never auto-expanded.
- `append` requires an existing array/sequence.
- `delete` and `append` reject `--create-missing`.
- YAML anchors/merge keys and tags have conservative restrictions.
- Use line/direct editing when placement depends on project-local comment semantics, array/table-array restructuring, YAML anchor/merge semantics, or multiline YAML mappings/sequences.

For path syntax and format-specific rules, read `references/config-path-patching.md`.

## Retry Discipline

Maximum 2 identedit attempts per target: original attempt plus one retry. Do not loop.

```text
identedit patch fails
|
├── precondition_failed / target_missing
|   └── re-run identedit read -> rebuild request -> retry once
|       └── if it fails again, fall back to direct file editing
|
├── ambiguous_target
|   └── inspect error.candidates -> choose qualified symbol/identity/span_hint -> retry once
|       └── if still ambiguous, fall back to direct file editing
|
└── parse_failure / no_provider / other hard error
    └── fall back to direct file editing immediately
```

For line-anchored edits: re-run `read --mode line`, retry strict once, then try `--auto-repair` only as that bounded retry.

## Post-Edit Verification

Identedit verifies edit preconditions, not semantic correctness. After non-trivial code edits, run the narrowest useful project verifier yourself.

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

If the verifier fails, treat it as a workflow failure, not an identedit failure. Inspect verifier output and repair with one bounded follow-up edit attempt.

## Output Contract

- `edit`, `apply`, `patch`, and errors emit JSON by default.
- `read` defaults to human-readable text in both AST and line modes.
- Use `read --json` for structured handles or line anchors.
- `patch --dry-run --diff` emits unified diff text.
- Parse JSON output when available; do not grep JSON text.

## References

Load these only when the task needs the details:
- `references/structural-pipeline.md`: `read --json`, `edit --json`, handle refs, operations, file-level targets, merge, pipe workflows.
- `references/line-editing.md`: line anchor format, line JSON, strict/repair behavior.
- `references/config-path-patching.md`: JSON/YAML/TOML path syntax, create-missing policy, anchors/tags/comments.
- `references/failed-diff-handoff.md`: failed unified diff recovery, candidate status, and explicit apply handoff.
- `references/transactions.md`: multi-file transaction details, apply wrapper shape, rollback drill, error table.
- `references/languages.md`: bundled languages and `grammar install` tiers.

## Tool Pairing

Use `ast-grep` for pattern-based discovery and identedit for verified application. Use `repren` for bulk text refactoring, simultaneous renames, case-preserving variants, and file/directory renames.
