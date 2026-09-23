---
name: identedit
description: "Precision code editing with precondition safety. USE WHEN: multi-file atomic edits, repeated target text, config-path edits, or a previous edit attempt failed or landed in the wrong place. NOT for: trivial one-line fixes, full-file rewrites, file-system renames."
---

# Identedit

Use for repeated targets, context-mismatch recovery, config paths, precondition-checked file-boundary insertions, or multiple edits that must commit together. Prefer direct editing for trivial changes and full-file rewrites; use `repren` or `git mv` for bulk text/path renames.

## Find the existing feature

**One edit: use `patch`.** Start with the matching row, not the whole manual. Commands below start with `identedit`; text-source and preview flags modify the chosen edit command.

| I need to... | Use | Details |
|---|---|---|
| Inspect one symbol and nearby code, with edit-ready addresses | `read file --symbol Class.method --context 3` | [Symbols](#function-or-method) |
| Replace a function by name, without looking up an identity | `patch file --symbol Class.method --replace --text-file body.txt` | [Symbols](#function-or-method) |
| Change text only inside one function/class | `patch file --symbol name --scoped-regex 'old' --scoped-replacement 'new'` | [Symbols](#function-or-method) |
| Edit an exact line/range among repeated text | `read --mode line file`, then `patch file --at "LINE:HASH"` | [Lines](#exact-line-or-range) |
| Preview a patch without writing | Add `--dry-run --diff` | [Preview](#text-preview-and-verification) |
| Supply multiline text without shell escaping | `--text-file body.txt` or `--stdin-text` | [Text input](#text-preview-and-verification) |
| Add code at the start/end of a file | `patch file --at file-end --insert --text-file code.txt` (or `file-start`) | [Text input](#text-preview-and-verification) |
| Set/create a config key, append an array value, or delete a key | `patch file --config-path path`: `--set-value` (add `--create-missing` to create), `--append-value`, `--delete` | [Config](#config-value) |
| Plan multiple edits/files together, or move a structural unit | `edit --json` -> `apply`; moves use `move_before`/`move_after` | [Plans](#batch-or-multi-step-pipeline) |
| Recover target candidates from a failed patch | `patch --from-diff failed.diff file` (never writes) | [Handoff](#failed-diff-handoff) |

## Single-edit recipes

### Function or method

Use a unique local or qualified symbol; no separate identity lookup is required:

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --replace --text-file /tmp/new_body.py
```

**Replacement text is raw node text, not an automatically indented block.** Whitespace before the node stays in the file. For a Python method indented four spaces, `/tmp/new_body.py` should contain:

```python
def process_data(self, x, y):
        return x + y
```

The first line starts at `def`; subsequent lines retain their intended file indentation. No dedent or reindent is performed. Decorators and comments outside the selected span stay untouched.

Need to inspect the target first? `identedit read src/example.py --symbol Processor.process_data --context 3` shows the complete symbol plus up to three surrounding lines on each side. Copy the identity after `# node --at` for a node edit, or a `LINE:HASH` for a line edit. Context is labeled separately; boundary lines can contain text outside the exact node span. Preserve source indentation after the `|` marker for line edits. Add `--json --verbose` when you need the exact raw node `text` rather than its line view.

To change text only inside a symbol:

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --scoped-regex 'old_name' --scoped-replacement 'new_name'
```

Already have a node identity? Use `--at <identity-hex16>` instead of `--symbol`. For kind-specific glob matching, use `--kind function_definition --name 'process_*'`.

Ambiguity fails without writing and returns `error.candidates`. Inspect them; retry with a qualified symbol, an identity from `read`, or a narrower name glob.

### Exact line or range

Use this when structural targeting is too coarse. Read content and anchors together, then copy the intended anchor:

```bash
identedit read --mode line example.py
identedit patch example.py --at "4:9e0f1a2b" --set-line '    return x + y'
```

For a large file, use `read --mode line --offset 40 --limit 30 example.py`. Offset is a positive, 1-based original line number; anchors are not renumbered. Omitted-line counts show that the view is partial. These bounds apply per file.

For several lines, use `--replace-range --text-file /tmp/new_lines.txt` with optional `--end-anchor "LINE:HASH"`. For insertion, use `--insert-after-line`. These are line operations; `--replace` and `--insert-after` address nodes.

`--replace-range` checks the supplied start and end anchors, not the lines between them. If an interior line changed since `read` while both anchors still match, strict patch replaces that changed line too. Default `apply` protects the range captured by `edit`, but `apply --repair` refreshes line previews from the current file. If the old interior text must still match, use a node target with its content hash where available, or a conventional patch matching the old hunk.

Re-read stale anchors before retrying. `--auto-repair` is opt-in for one bounded retry when deterministic remapping is acceptable; never guess among ambiguous candidates.

### Config value

```bash
identedit patch config.yaml --config-path service.retries --set-value 5
identedit patch config.json --config-path items --append-value 4
identedit patch config.toml --config-path database.enabled --delete
```

- Bare keys: `service.retries`. Array/sequence indices: `items[0].name`.
- Literal keys: quote the shell argument, e.g. `--config-path 'services["sidecar.port"]'`.
- `--create-missing` creates map or standard-table keys, not array/sequence elements. Delete and append reject it; append requires an existing array/sequence.
- Multi-document YAML creation requires `--document-index <N>`.
- Use line or direct editing for YAML anchors, merge keys, tags, sequence growth, TOML table arrays, or placement depending on local comment semantics.

## Text, preview, and verification

Use `--text-file` or `--stdin-text` for multiline payloads; do not encode newlines as literal `\n` in shell strings. Both work with every text-taking flag:

```bash
identedit patch example.py --symbol target_fn --replace --stdin-text < /tmp/new_body.py
```

For a non-trivial patch, preview the same request with `--dry-run --diff` before applying it. This emits unified diff without writing; without those flags, `patch` applies immediately.

For a short location check, omit `--diff`: successful `patch`/`apply` execution or dry-run JSON includes `locations` with up to 16 resolved ranges or insertion points, original byte/line coordinates, and an omitted count. These are **pre-edit locations**, not fresh anchors or proof of a change; check `dry_run`, `transaction.status`, or `changed` for the outcome. Read again before a later edit. Discovery-only `--from-diff` returns candidates instead. Full field definitions: [protocol](references/protocol.md#resolved-edit-locations).

```bash
identedit patch example.py --symbol target_fn --replace --text-file /tmp/new_body.py --dry-run --diff
identedit patch example.py --symbol target_fn --replace --text-file /tmp/new_body.py
```

Identedit checks preconditions, not semantic correctness. After a non-trivial edit, run the narrowest relevant project verifier, e.g. `python -m compileall example.py` and the affected tests. If verification fails, treat the workflow as failed and make at most one bounded follow-up edit attempt.

## Planned edits and recovery

### Batch or multi-step pipeline

Use this for multiple operations/files, handle tables, or structural moves. Discover targets with `read --json`; `ast-grep` can help locate structures. Use `repren` for bulk simultaneous/case-preserving replacements and path renames.

```bash
identedit read --kind function_definition example.py --json
identedit edit --json < request.json > changeset.json
identedit apply --dry-run changeset.json
identedit apply changeset.json
```

`edit` builds a plan without writing. `apply --dry-run` validates without writing; `apply` commits. Pass the generated changeset to `apply`, not the original edit request.

Single-file request shape (copy identities and hashes from `read`; these values are illustrative):

```json
{
  "command": "edit",
  "file": "example.py",
  "operations": [{
    "target": {
      "type": "node",
      "identity": "ca465ff1a2b3c4d5",
      "kind": "function_definition",
      "expected_old_hash": "20ba467fa1b2c3d4"
    },
    "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
  }]
}
```

For batches, replace `file` and `operations` with `files: [{"file": "...", "operations": [...]}]`. Use exactly one request shape. Never synthesize identities or precondition hashes. Use `jq --rawfile` to populate `op.new_text` from a file.

### Failed-diff handoff

```bash
identedit patch --from-diff failed.diff src/example.py > handoff.json
```

This never writes and rejects multi-file, create, delete, and rename diffs. Inspect every candidate preview; never choose candidate zero by convention. Only after verifying one changed block has one intended `unique` candidate, promote it explicitly:

```bash
jq '{command: "patch", file,
     target: .changes[0].candidates[0].target,
     op: .changes[0].candidates[0].op}' handoff.json | identedit patch --json
```

For multiple blocks, build one `edit --json` request so selected operations commit together.

## Recovery and output contracts

Allow at most one retry per target; a second failure means stop using identedit for that target.

| Failure | Next action |
|---|---|
| `precondition_failed`, `target_missing` | Re-read, rebuild, retry once; use `read --mode line` for line targets |
| `ambiguous_target` | Inspect candidates; retry with a qualified symbol, identity, or span hint |
| `parse_failure`, `no_provider`, another hard error | Fall back to direct editing |
| Apply/rollback/resource error | Inspect recovery details; load `references/transactions.md` if bundled |

Line repair shares the same retry budget. References are optional; use reported recovery details rather than blindly retrying a transaction error.

For `invalid_request` with `error.line_check`, inspect its mismatches and remap candidates, then refresh line anchors before the bounded retry. Do not parse diagnostics out of `message`. If apply received an edit request, compile it with `edit --json` first; apply accepts the resulting changeset.

- `read` defaults to text; `--json` returns structured handles or line anchors.
- `edit`, `apply`, `patch`, and runtime request errors emit JSON unless a documented mode says otherwise. Parse JSON, not grep output.
- `patch --dry-run --diff` emits unified diff. Invalid CLI syntax uses argument-parser diagnostics on stderr, not JSON.
- Node identities/content hashes: 16 hex characters. Line anchors: `LINE:8-hex`. Both serialize lowercase and match exactly; no prefix matching.
- Runtime error shape: `{"error":{"type":"...","message":"...","suggestion":"..."}}`; `suggestion` is optional.

## Optional references

Ordinary workflows above work with this file alone. If references are bundled, read only the relevant one, not the entire set. Use `identedit <command> --help` for flags.

- [Structural pipeline](references/structural-pipeline.md): full request shapes, handle refs, operations including `move_before`/`move_after`, file targets, merge, pipes.
- [Line editing](references/line-editing.md): ranges, line endings, repair.
- [Config paths](references/config-path-patching.md): format-specific behavior and path syntax.
- [Failed diff](references/failed-diff-handoff.md): discovery and explicit promotion.
- [Transactions](references/transactions.md): multi-file apply, rollback, resource errors, recovery.
- [Protocol](references/protocol.md): output, complete errors, exit behavior, ingress normalization.
- [Languages](references/languages.md): bundled languages and grammar installation.
