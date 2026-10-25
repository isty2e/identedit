---
name: identedit
description: "Precision code editing with precondition safety. USE WHEN: multi-file atomic edits, repeated target text, config-path edits, or a previous edit attempt failed or landed in the wrong place. NOT for: trivial one-line fixes, full-file rewrites, pattern-wide structural rewrites (use ast-grep), file-system renames."
---

# Identedit

Use for repeated targets, context-mismatch recovery, config paths, precondition-checked file-boundary insertions, or edits that must commit together. Use `ast-grep` for pattern-wide structural rewrites, separate from Identedit's guarded transactions. Prefer direct editing for trivial changes and full-file rewrites; use `repren` or `git mv` for bulk text/path renames.

## Find the existing feature

**One edit: use `patch`.** Start at the matching row, not the whole manual. Commands start with `identedit`; text-source and preview flags modify the chosen edit command.

| Need | Command | Details |
|---|---|---|
| Inspect a symbol, context and edit-ready addresses | `read file --symbol Class.method --context 3` | [Symbols](#function-or-method) |
| Replace a function by name without identity lookup | `patch file --symbol Class.method --replace --text-file body.txt` | [Symbols](#function-or-method) |
| Raw regex within one function/class | `patch file --symbol name --scoped-regex 'old' --scoped-replacement 'new'` | [Symbols](#function-or-method) |
| Exact line/range among repeated text | `read --mode line file`, then `patch file --at "LINE:HASH" --replace 'new text'` | [Lines](#exact-line-or-range) |
| Preview patch/plan | `patch --dry-run --diff` or `apply --dry-run --diff plan.json` | [Preview](#text-preview-and-verification) |
| Multiline payload without shell escaping | `--text-file body.txt` or `--stdin-text < body.txt` | [Text input](#text-preview-and-verification) |
| Insert at file start/end | `patch file --at file-end --insert --text-file code.txt` (or `file-start`) | [Text input](#text-preview-and-verification) |
| Set/create key, append an array value, delete key | `patch file --config-path path`: `--set-value 5` (add `--create-missing` to create), `--append-value 4`, `--delete` | [Config](#config-value) |
| Multiple edits/files or structural move | `edit --json` -> `apply`; raw moves accept explicit `prefix`/`suffix` text | [Plans](#batch-or-multi-step-pipeline) |
| Candidates after patch context failure | `patch --from-diff failed.diff file > handoff.json` (never writes source) | [Handoff](#failed-diff-handoff) |

## Single-edit recipes

### Function or method

Use a unique local or qualified symbol without a separate identity lookup.

**Node replacement is raw text.** Pre-node whitespace stays; no dedent/reindent occurs. For a four-space-indented Python method, the replacement file should contain:

```python
def process_data(self, x, y):
        return x + y
```

Start the first line at `def`; retain intended file indentation on later lines. Comments/decorators outside the span stay untouched. File/stdin node payloads are verbatim: a final newline can split a line, add a blank before a preserved source newline, or terminate an unterminated last line. Omit it unless intended.

`identedit read src/example.py --symbol Processor.process_data --context 3` shows the complete symbol plus up to three lines per side. Context is separate; boundary lines may exceed the span. Copy the identity after `# node --at`; use `--json --verbose` for exact raw node `text`. For lines, copy `LINE:HASH` and preserve indentation after `|`.

Scoped regex only bounds the search to a node; identifiers, strings and comments match too. It is not a symbol-aware rename. With an identity, use `--at <identity-hex16>` instead of `--symbol`; kind-specific glob matching uses `--kind function_definition --name 'process_*'`.

Ambiguity fails without writing. When collected, `error.candidates` holds up to 16 records in original order; `error.candidate_summary: {total, omitted}` appears only when shortened. Inspect omitted targets with file `read --json` **without `--symbol` or filters**; add `--verbose` for bodies. `--name` matches provider names, not qualified paths. Repeated identities need copied `span_hint` in `patch --json` (one operation) or `edit --json` (plans). See the [candidate contract](references/protocol.md#ambiguity-candidate-details).

### Exact line or range

When structure is too coarse, read content and anchors together before patching.

For large files, use `read --mode line --offset 40 --limit 30 example.py`: bounds are per file, offset is positive/1-based, original line numbers remain, and omitted-line counts mark partial views.

For a range, use `--replace --text-file /tmp/new_lines.txt --end-anchor "LINE:HASH"`; `--delete` removes one line/range and `--insert-after` adds logical lines. These verbs support node and line targets; line targets preserve newline layout.

Line-range `--replace`/`--delete` check only the supplied start/end anchors, not the interior. If interior lines changed since `read` while both anchors still match, strict `patch` can replace/delete those lines. Default `apply` protects the range captured by `edit`; `apply --repair` refreshes previews from current content. To require old interior text, use a content-hashed node where available or a conventional old-hunk patch.

Re-read stale anchors before retrying. `--auto-repair` opts into one bounded retry when deterministic remapping is acceptable; never guess among ambiguous candidates.

### Config value

Config paths support JSON, YAML and TOML; choose the set, append or delete action in the feature index.

- Bare keys: `service.retries`; array/sequence indices: `items[0].name`. Quote literal-key arguments: `--config-path 'services["sidecar.port"]'`.
- JSON `--set-value` preserves payload whitespace, including file/stdin final newlines; omit unwanted final newlines.
- `--create-missing`: map/standard-table keys, not array/sequence elements. Delete/append reject it; append needs an existing array/sequence. Multi-document YAML creation requires `--document-index <N>`.
- Multiple edits: original-source paths through `edit --json`; overlapping writes fail. See [batch rules](references/config-path-patching.md#batch-edit-intents).
- Use line/direct editing for YAML anchors/merge keys/tags/sequence growth, TOML table arrays, or comment-dependent placement.
- Config plans require unchanged full-file source bytes. Replan after changes; repair cannot substitute. See [file source preconditions](references/protocol.md#file-source-preconditions).

## Text, preview, and verification

Use `--text-file` or `--stdin-text` for multiline payloads, not literal `\n` in shell strings. Both support every text-taking flag.

For a non-trivial patch, run the same request with `--dry-run --diff`, inspect it, then rerun without those flags. Preview does not write; plain `patch` does. Apply validation takes exclusive locks and needs write access even for dry-runs; avoid concurrent previews/apply on one file. Preview does not reserve a later write.

Diffs show validated bytes/indentation with three context lines. No-ops emit none; very large changes may use one non-minimal hunk. Portable replay needs `--color never` and paths relative to the replay directory, without `..`. Hunk numbers count LF; read anchors also count standalone CR. Never use hunk coordinates as anchors.

Successful commits include JSON `diff: {text, file_moves}` showing the actual change, not every change since an earlier read. Inspect it before attributing unexpected content to the tool. Large name-selected deletions are especially worth inspecting: the diff exposes the actual removed range. `--no-diff` omits the view, for example after you have inspected the same change; patch JSON also accepts `options.no_diff`. Diffs are not automatically truncated. See [change diffs](references/protocol.md#committed-change-diffs).

Successful JSON including dry-run also has up to 16 `locations` in original byte/line coordinates, plus omitted counts: pre-edit checked targets, not changed lines, fresh anchors or write proof. Check `dry_run`, `transaction.status`, or `changed`; read again before a later edit. Discovery-only `--from-diff` returns candidates. See [locations](references/protocol.md#resolved-edit-locations).

Files with node-target operations must pass a final selected-provider parse before any writes, including dry-run; the check includes other edits in that file. Files containing only line/file-boundary edits, and fallback edits, are excluded. Bundled Python also rejects final empty required blocks, even pre-existing ones; repair them in the same batch or use line editing. `parse_failure` with `error.stage: "proposed_result"` means no transaction files changed: inspect the location, correct the result and retry with `--dry-run`. `edit` only builds a plan, without this check. See [node-result parsing](references/protocol.md#node-result-parsing).

Review `structure_changes` in successful JSON patch/apply, including dry-run: inspect file statuses and decreased/increased names, not summary counts alone. These are provider-named occurrences, not just declarations; default parameters, keyword arguments and local variable declarators may appear. `checked` without changes proves neither syntax nor behavior; `unavailable`/`out_of_scope` means no observation. Respect omitted/truncated markers. Text `--diff` has no report. See the [report contract](references/protocol.md#named-structure-changes).

Provider acceptance is not semantic correctness: Python indentation errors and unintended nesting can pass; no indentation or missing separators are inferred. After a non-trivial edit, run the narrowest relevant project verifier, e.g. `python -m compileall example.py`, and affected tests. Failed verification fails the workflow; make at most one bounded follow-up edit attempt.

## Planned edits and recovery

### Batch or multi-step pipeline

Use plans for multiple operations/files, handle tables or structural moves. For unique nodes, `edit --json` accepts `target: {"type":"symbol","name":"Class.method"}` or `{"type":"selector","kind":"function_definition","name":"process_*"}` without separate handle lookup, and node `op: {"type":"scoped_regex","pattern":"old","replacement":"new"}`. Use `read --json` to preserve an earlier observation or disambiguate; `ast-grep` can locate structures. Use `repren` for bulk simultaneous/case-preserving text replacements and path renames.

For the guarded request below:

```bash
identedit read --kind function_definition example.py --json
identedit edit --json < request.json > changeset.json
identedit apply --dry-run --diff --color never changeset.json
identedit apply changeset.json
```

`edit` builds without writing; `apply --dry-run --diff` validates/previews text. Omit `--diff` for JSON validation, including whole-file renames. `apply` commits the generated changeset, not the original request. Multi-file diffs use canonical source-path sort order, not request order.

Single-file request; copy identities/hashes from `read` (illustrative values below):

```json
{
  "command": "edit", "file": "example.py",
  "operations": [{
    "target": {"type": "node", "identity": "ca465ff1a2b3c4d5", "kind": "function_definition",
               "expected_old_hash": "20ba467fa1b2c3d4"},
    "op": { "type": "replace", "new_text": "def process_data(x, y):\n    return x + y" }
  }]
}
```

For batches, replace `file`/`operations` with `files: [{"file": "...", "operations": [...]}]`; use exactly one shape. Never synthesize identities/precondition hashes. Use `jq --rawfile` to fill `op.new_text` from a file. Symbol/selector queries must match exactly one node; copied guards are not refreshed; `patch --json` lacks those targets. See the [intent contract](references/protocol.md#edit-json-intents).

Moves insert exact selected bytes with optional explicit literal `prefix`/`suffix`; they infer no separators/destination indentation, reindent no body, repair no source block and infer no trivia ownership. Adjacent Python functions/methods/statements can concatenate and be rejected, but Python moves are not banned: `file_end` can work with an existing separating destination newline. Supply separators/destination indentation explicitly or use line plans; a Python suffix may include the next line's indentation. See [raw move text](references/protocol.md#raw-move-text) and the [move example](references/structural-pipeline.md#operations).

### Failed-diff handoff

`patch --from-diff` never writes and rejects multi-file, create, delete, and rename diffs. Inspect every candidate preview; never choose candidate zero by convention. Only after verifying one changed block has one intended `unique` candidate, promote it explicitly:

```bash
jq '{command: "patch", file,
     target: .changes[0].candidates[0].target,
     op: .changes[0].candidates[0].op}' handoff.json | identedit patch --json
```

For multiple blocks, build one `edit --json` request so selected operations commit together.

## Recovery and output contracts

Allow at most one retry per target, including line repair. A second failure means stop identedit for that target. References are optional; inspect reported recovery details, not blind transaction retries.

| Failure | Next action |
|---|---|
| `precondition_failed`, `target_missing` | Re-read, rebuild, then retry once; line targets use `read --mode line` |
| `ambiguous_target` | Inspect candidates/omitted counts; follow [node recovery](#function-or-method), then retry with the intended target |
| `resource_busy` | Another preview/apply may hold the file lock; wait briefly, retry once within the shared target limit |
| `parse_failure`, `error.stage: "proposed_result"` | Inspect location; correct payload/other edits/empty Python blocks in the same batch, or line-edit; retry once with `--dry-run` |
| `parse_failure`, `error.stage: "source"` | Original source rejected; repair through direct editing |
| Other `parse_failure`, `no_provider`, another hard error | Inspect diagnostics; use direct fallback if appropriate |
| Other apply/rollback/resource error | Inspect recovery details; load `references/transactions.md` if bundled |

Branch on `error.type` and `error.stage`, not message text or `suggestion` presence. Missing/unknown stage does not classify source failure; provider initialization failures omit it.

For `edit --json`, optional `error.request_pointers` identifies original input operations or file entries using JSON Pointers. A conflict can identify two operations, even after grouping files or expanding a move. Syntax/envelope failures may omit it; see [input locations](references/protocol.md#edit-request-locations).

For `invalid_request` with `error.line_check`, inspect mismatches/remap candidates, then refresh line anchors before the bounded retry; do not parse diagnostics from `message`. If apply receives an edit request, compile with `edit --json` first and pass the resulting changeset.

- `read`: text by default; `--json` gives handles/anchors. `edit`/`apply`/`patch` and runtime request errors emit JSON except documented modes. Parse JSON, not grep.
- Both `patch --dry-run --diff` and `apply --dry-run --diff` emit text. Invalid CLI syntax uses argument-parser stderr, not JSON.
- Node identities/content hashes: 16 hex characters; line anchors: `LINE:8-hex`. Lowercase serialization, exact matching, no prefix matching.
- Runtime error: `{"error":{"type":"...","message":"...","suggestion":"..."}}`; `suggestion` is optional.

## Optional references

Ordinary workflows above work with this file alone. If bundled, read only the relevant reference, not the whole set. Use `identedit <command> --help` for flags.

- [Structural pipeline](references/structural-pipeline.md): full request shapes, handle refs, operations including `move_before`/`move_after`, file targets, merge, pipes.
- [Line editing](references/line-editing.md): ranges, line endings, repair.
- [Config paths](references/config-path-patching.md): format-specific behavior and path syntax.
- [Failed diff](references/failed-diff-handoff.md): discovery and explicit promotion.
- [Transactions](references/transactions.md): multi-file apply, rollback, resource errors, recovery.
- [Protocol](references/protocol.md): output, complete errors, exit behavior, ingress normalization.
- [Languages](references/languages.md): bundled languages and grammar installation.
