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
| Regex-replace raw text inside one function/class | `patch file --symbol name --scoped-regex 'old' --scoped-replacement 'new'` | [Symbols](#function-or-method) |
| Edit an exact line/range among repeated text | `read --mode line file`, then `patch file --at "LINE:HASH" --replace 'new text'` | [Lines](#exact-line-or-range) |
| Preview a patch without writing | Add `--dry-run --diff` | [Preview](#text-preview-and-verification) |
| Supply multiline text without shell escaping | `--text-file body.txt` or `--stdin-text` | [Text input](#text-preview-and-verification) |
| Add code at the start/end of a file | `patch file --at file-end --insert --text-file code.txt` (or `file-start`) | [Text input](#text-preview-and-verification) |
| Set/create a config key, append an array value, or delete a key | `patch file --config-path path`: `--set-value` (add `--create-missing` to create), `--append-value`, `--delete` | [Config](#config-value) |
| Plan multiple edits/files together, or move a structural unit | `edit --json` -> `apply`; raw moves accept explicit `prefix`/`suffix` text | [Plans](#batch-or-multi-step-pipeline) |
| Recover target candidates from a failed patch | `patch --from-diff failed.diff file` (never writes) | [Handoff](#failed-diff-handoff) |

Moves do not infer separators: adjacent Python functions/methods/statements can concatenate and be rejected. This is not a Python ban; a move to `file_end` can work when an existing newline separates the destination. Supply separators and destination indentation explicitly with `prefix`/`suffix` or line-based plans.

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

Node payloads from `--text-file` or `--stdin-text` are inserted verbatim, including any final newline. That newline can split an existing line, add a blank line before a preserved source newline, or terminate an unterminated last line. Omit the payload's final newline unless that effect is intended.

`identedit read src/example.py --symbol Processor.process_data --context 3` shows the complete symbol and up to three surrounding lines per side. Copy the identity after `# node --at` for a node edit. Context is labeled separately; boundary lines may extend beyond the node span. Use `--json --verbose` for the exact raw node `text`. For line edits, copy a `LINE:HASH` and preserve indentation after the `|` marker.

To regex-replace raw text inside a symbol:

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --scoped-regex 'old_name' --scoped-replacement 'new_name'
```

The node only bounds the search: matching text in identifiers, strings, and comments is replaced. This is not a symbol-aware rename.

Already have a node identity? Use `--at <identity-hex16>` instead of `--symbol`. For kind-specific glob matching, use `--kind function_definition --name 'process_*'`.

Ambiguity fails without writing. When collected, `error.candidates` contains at most 16 original-order records; `error.candidate_summary: {total, omitted}` appears only when shortened. To inspect omitted targets, use `read --json` for the file **without `--symbol` or filters**; add `--verbose` for body text. `--name` filters provider names, not qualified paths. Repeated identities need copied `span_hint` in `patch --json` (one operation) or `edit --json` (plans). See the [candidate contract](references/protocol.md#ambiguity-candidate-details).

### Exact line or range

When structural targeting is too coarse, read the content and anchors together:

```bash
identedit read --mode line example.py
identedit patch example.py --at "4:9e0f1a2b" --replace '    return x + y'
```

For a large file, use `read --mode line --offset 40 --limit 30 example.py`. Bounds apply per file; the positive 1-based offset and anchors retain original line numbers. Omitted-line counts mark partial views.

For several lines, use `--replace --text-file /tmp/new_lines.txt --end-anchor "LINE:HASH"`. Use `--delete` to remove one line or a range, and `--insert-after` to add logical lines. These verbs work for both node and line targets; line targets preserve the file's newline layout.

Line-range `--replace` and `--delete` check the supplied start and end anchors, not the lines between them. If an interior line changed since `read` while both anchors still match, strict patch can replace or delete that changed line too. Default `apply` protects the range captured by `edit`, but `apply --repair` refreshes line previews from the current file. If the old interior text must still match, use a node target with its content hash where available, or a conventional patch matching the old hunk.

Re-read stale anchors before retrying. `--auto-repair` is opt-in for one bounded retry when deterministic remapping is acceptable; never guess among ambiguous candidates.

### Config value

```bash
identedit patch config.yaml --config-path service.retries --set-value 5
identedit patch config.json --config-path items --append-value 4
identedit patch config.toml --config-path database.enabled --delete
```

- Bare keys: `service.retries`. Array/sequence indices: `items[0].name`.
- Literal keys: quote the shell argument, e.g. `--config-path 'services["sidecar.port"]'`.
- JSON `--set-value` preserves payload whitespace, including a trailing newline from `--text-file` or `--stdin-text`; omit that newline if unwanted.
- `--create-missing` creates map or standard-table keys, not array/sequence elements. Delete and append reject it; append requires an existing array/sequence.
- Multi-document YAML creation requires `--document-index <N>`.
- Use line or direct editing for YAML anchors, merge keys, tags, sequence growth, TOML table arrays, or placement depending on local comment semantics.

## Text, preview, and verification

Use `--text-file` or `--stdin-text` for multiline payloads; do not encode newlines as literal `\n` in shell strings. Both work with every text-taking flag:

```bash
identedit patch example.py --symbol target_fn --replace --stdin-text < /tmp/new_body.py
```

For a non-trivial patch, run the same request with `--dry-run --diff`, inspect the unified diff, then rerun without those flags to apply it. The preview does not write; plain `patch` does.

Apply validation takes exclusive locks and requires write access even for dry-runs. Avoid concurrent identedit previews or apply runs on the same file.

Diffs show actual validated file bytes, including surrounding indentation, with three context lines. No-ops emit no diff; very large changes may use one non-minimal hunk. For portable replay, use `--color never` and paths relative to the replay directory, without `..` components. Hunk numbers count LF lines, not read anchors (which also count standalone CR); never copy hunk coordinates as anchors. Preview does not reserve a later write.

For a short location check, omit `--diff`. Successful `patch`/`apply` JSON (including dry-run) reports up to 16 resolved `locations` with original byte/line coordinates, plus omitted counts. These are **pre-edit targets**, not fresh anchors or proof of a write; check `dry_run`, `transaction.status`, or `changed`. Read again before a later edit. Discovery-only `--from-diff` returns candidates instead. See [protocol](references/protocol.md#resolved-edit-locations) for field definitions.

Files with node-target operations must pass a final parse by their selected syntax provider before any writes, including in dry-run. The check includes other edits in the same file; line/file-boundary-only files and fallback edits are excluded. Bundled Python also rejects empty required blocks in the final result, even if they existed before the edit; repair them in the same batch or use line editing. A `parse_failure` with `error.stage: "proposed_result"` means no transaction files were changed: inspect the reported location, correct the final result, and retry with `--dry-run`. `edit` only builds a plan and does not run this result check. See [node-result parsing](references/protocol.md#node-result-parsing).

Review `structure_changes` in successful JSON `patch`/`apply` receipts, including dry-run. It reports named-structure occurrence changes: inspect file statuses and decreased/increased names, not just summary counts. Names are not declaration-only; default parameters, keyword arguments and local variable declarators can appear when the provider names them. `checked` with no changes is not a syntax/behavior proof; `unavailable` or `out_of_scope` means no observation. Respect omitted/truncated markers; `--diff` has no report. See the [report contract](references/protocol.md#named-structure-changes).

Provider acceptance is not semantic correctness: Python indentation errors and unintended nesting can still pass. No indentation or missing separators are inferred. After a non-trivial edit, run the narrowest relevant project verifier, e.g. `python -m compileall example.py` and the affected tests. If verification fails, treat the workflow as failed and make at most one bounded follow-up edit attempt.

## Planned edits and recovery

### Batch or multi-step pipeline

Use this for multiple operations/files, handle tables, or structural moves. For unique nodes, `edit --json` accepts `target: {"type":"symbol","name":"Class.method"}` or `{"type":"selector","kind":"function_definition","name":"process_*"}` without a separate handle lookup. It also accepts node `op: {"type":"scoped_regex","pattern":"old","replacement":"new"}`. Use `read --json` when preserving an earlier observation or disambiguating; `ast-grep` can help locate structures. Use `repren` for bulk simultaneous/case-preserving replacements and path renames.

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

`symbol`/`selector` queries must match exactly one node; copied guards are not refreshed. `patch --json` does not accept these targets. See the [intent contract](references/protocol.md#edit-json-intents).

Structural moves can insert caller-specified `prefix` and `suffix` around the exact selected bytes. These strings do not reindent the body, repair the source block or infer trivia ownership; a Python suffix may include the following line's indentation. See [raw move text](references/protocol.md#raw-move-text) and the [move example](references/structural-pipeline.md#operations).

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
| `ambiguous_target` | Inspect candidates and omitted counts; follow [node recovery](#function-or-method), then retry with the intended target |
| `resource_busy` | Another identedit preview/apply may hold the file's apply lock; wait briefly, then retry once within the shared target limit |
| `parse_failure`, `error.stage: "proposed_result"` | Inspect the location; correct payload/other edits or existing empty Python blocks in the same batch, or use line editing; retry once with `--dry-run` |
| `parse_failure`, `error.stage: "source"` | Original source was rejected; use direct editing to repair it |
| Other `parse_failure`, `no_provider`, another hard error | Inspect diagnostics; fall back to direct editing if appropriate |
| Other apply/rollback/resource error | Inspect recovery details; load `references/transactions.md` if bundled |

Line repair shares the same retry budget. References are optional; use reported recovery details rather than blindly retrying a transaction error.

Branch on `error.type` and `error.stage`, not message text or the presence of `suggestion`. A missing/unknown stage does not identify a source failure; provider initialization failures omit it.

For `edit --json`, optional `error.request_pointers` identifies original input operations or file entries using JSON Pointers. A conflict can identify two operations, even after grouping files or expanding a move. Syntax/envelope failures may omit it; see [input locations](references/protocol.md#edit-request-locations).

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
