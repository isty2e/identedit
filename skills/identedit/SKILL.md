---
name: identedit
description: "Precision code editing with precondition safety. USE WHEN: multi-file atomic edits, repeated target text, config-path edits, or a previous edit attempt failed or landed in the wrong place. NOT for: trivial one-line fixes, full-file rewrites, file-system renames."
---

# Identedit - agent-oriented code editing

Identedit is a surgical editor for cases where target stability matters more than raw editing speed.

It supports two targeting modes:

- **Structural:** functions, classes, blocks, file boundaries, and moves.
- **Line-anchored:** exact lines and ranges with strict precondition checks.

## 10-second trigger

Use identedit when any condition matches:

- Two or more files must succeed or fail together.
- Repeated target text makes direct patching risky.
- A previous edit attempt failed from context mismatch or landed in the wrong location.
- You need multiple operations on the same file in one verified plan.
- You need a precondition-checked file-boundary insertion.
- You need a nested JSON, YAML, or TOML edit by path.

Otherwise prefer direct file editing for speed.

## Route the task

| Situation | Command or tool |
|---|---|
| Replace a unique function or method | `identedit patch file --symbol Class.method --replace 'new body'` |
| Replace an already-read node | `identedit patch file --at <identity-hex16> --replace 'new body'` |
| Preview one patch | Add `--dry-run --diff` |
| Insert at a file boundary | `identedit patch file --at file-end --insert 'new code'` |
| Edit one exact line | `identedit patch file --at "LINE:HASH" --set-line 'new line'` |
| Replace inside one function or class | `identedit patch file --symbol foo --scoped-regex 'old' --scoped-replacement 'new'` |
| Set, append, or delete a config path | `identedit patch file --config-path key.path <operation>` |
| Recover candidates from a failed diff | `identedit patch --from-diff failed.diff file` |
| Multiple operations or files | `identedit edit --json` then `identedit apply` |
| Move a structural unit | `identedit edit --json` with `move_before` or `move_after` |
| Trivial one-line change | Direct file editing |
| Rewrite most of a file | File rewrite |
| Bulk text or path rename | `repren` or `git mv` |

## Default flow: `patch`

Most structural edits fit in one command. Use `--symbol` when the target has a unique local or qualified name:

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --replace 'def process_data(self, x, y):
        return x + y'
```

For a non-trivial replacement, preview the same request before applying it:

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --replace --text-file /tmp/new_body.py --dry-run --diff

identedit patch src/example.py --symbol Processor.process_data \
  --replace --text-file /tmp/new_body.py
```

Ambiguous symbols fail without writing and return `error.candidates`. Use a qualified symbol, an identity from `read`, or `--kind` plus a narrower `--name` glob.

## Large text payloads

Use `--text-file` or `--stdin-text` instead of shell-quoting multiline text:

```bash
identedit patch /abs/path/file.py --symbol target_fn \
  --replace --text-file /tmp/new_block.py

identedit patch /abs/path/file.py --symbol target_fn \
  --replace --stdin-text < /tmp/new_block.py
```

Text sources work with every text-taking flag. For JSON edit requests, use `jq --rawfile` to place file contents in `op.new_text` without shell quoting.

## Multi-step structural pipeline

Use this flow for multiple operations, multiple files, handle tables, or moves:

```bash
# Discover canonical handles and preconditions.
identedit read --kind function_definition example.py --json

# Build a plan without modifying files.
identedit edit --json < request.json > changeset.json

# Validate, then commit.
identedit apply --dry-run changeset.json
identedit apply changeset.json
```

`read` defaults to human-readable text. Add `--json` when its output will feed an edit request. See [`structural-pipeline.md`](references/structural-pipeline.md) for request shapes, operations, handle refs, and merge workflows.

A single-file request has `file` and `operations`:

```json
{
  "command": "edit",
  "file": "example.py",
  "operations": [
    {
      "target": {
        "type": "node",
        "identity": "ca465ff1a2b3c4d5",
        "kind": "function_definition",
        "expected_old_hash": "20ba467fa1b2c3d4"
      },
      "op": {
        "type": "replace",
        "new_text": "def process_data(x, y):\n    return x + y"
      }
    }
  ]
}
```

For a batch, replace `file` and `operations` with `files`, where each entry contains its own `file` and `operations`. Use exactly one request shape. Copy identities and precondition hashes from `read --json`; do not synthesize them.

## Line-anchored editing

Use line mode when structural targeting is too coarse:

```bash
identedit read --mode line example.py
identedit patch example.py --at "4:9e0f1a2b3c4d" \
  --set-line "    return x + y"
```

Line anchors use `LINE:12-hex-hash` and match exactly. Available flag operations are `--set-line`, `--replace-range` with optional `--end-anchor`, and `--insert-after-line`. Re-read before retrying a stale anchor. Use `--auto-repair` only for one bounded retry when deterministic remapping is acceptable.

## Config path editing

Use config-aware targeting for nested JSON, YAML, or TOML values:

```bash
identedit patch config.yaml --config-path service.retries --set-value 5
identedit patch config.json --config-path items --append-value 4
identedit patch config.toml --config-path database.enabled --delete
```

Path rules required for safe use:

- Bare keys are dot-separated: `service.retries`.
- Array or sequence indices use numeric brackets: `items[0].name`.
- Literal keys containing dots or other punctuation use bracket-quoted JSON strings: `services["sidecar.port"]`.
- `--create-missing` creates map or standard-table keys, not array or sequence elements.
- Append requires an existing array or sequence. Delete and append reject `--create-missing`.
- Multi-document YAML creation requires `--document-index <N>`.
- Fall back to line or direct editing for YAML anchors, merge keys, tags, sequence growth, TOML table arrays, or placement that depends on local comment semantics.

## Failed-diff handoff

When a conventional patch fails because its context drifted, discover exact candidates without writing:

```bash
identedit patch --from-diff failed.diff src/example.py > handoff.json
```

Inspect every candidate preview. Never choose candidate zero by convention. The handoff rejects unsupported multi-file, create, delete, and rename diffs.

After verifying that one changed block has one intended `unique` candidate, promote that candidate explicitly:

```bash
jq '{
  command: "patch",
  file,
  target: .changes[0].candidates[0].target,
  op: .changes[0].candidates[0].op
}' handoff.json | identedit patch --json
```

For multiple blocks, build an `edit --json` request so the selected operations can commit together.

## Retry discipline

Allow at most one retry per target:

| Failure | Next action |
|---|---|
| `precondition_failed` or `target_missing` | Re-run `read`, rebuild the request, retry once |
| `ambiguous_target` | Inspect candidates and retry with a qualified symbol, identity, or span hint |
| `parse_failure`, `no_provider`, or another hard error | Fall back to direct editing |
| Second failure for the same target | Stop using identedit for that target |

For line edits, re-run `read --mode line` before the bounded retry. Use repair only within that same retry budget. Load [`transactions.md`](references/transactions.md) when apply, rollback, or resource errors are involved.

## Post-edit verification

Identedit verifies edit preconditions, not semantic correctness. After a non-trivial edit, run the narrowest project verifier that can detect a bad result:

```bash
identedit patch src/foo.py --symbol process_data \
  --replace --text-file /tmp/process_data.py --dry-run --diff
identedit patch src/foo.py --symbol process_data \
  --replace --text-file /tmp/process_data.py
python -m compileall src/foo.py
pytest tests/test_foo.py -q
```

If project verification fails, treat the workflow as failed and make at most one bounded follow-up edit attempt.

## Output rules

- Parse documented JSON output rather than grepping it.
- `read` defaults to text; add `--json` for structured handles or line anchors.
- `edit`, `apply`, `patch`, and runtime request errors emit JSON unless a documented output mode says otherwise.
- Invalid command-line syntax is reported by the argument parser on stderr, not as a JSON error response.
- `patch --dry-run --diff` emits unified diff text.
- Content hashes and node identities use 16 hexadecimal characters. Line anchors use `LINE:12-hex`.
- Hashes and anchors serialize in lowercase and match exactly; prefix matching is not supported.
- Runtime errors use `{ "error": { "type": "...", "message": "...", "suggestion": "..." } }`; `suggestion` is optional.

These rules are sufficient for ordinary use. The protocol reference adds the complete error-type list and normalization details when it is bundled with the skill.

## References

The skill is self-contained for ordinary operation. If the optional reference files are bundled, load only the one needed for advanced or exhaustive details:

- [`protocol.md`](references/protocol.md): supported interface, output modes, hashes, anchors, exit behavior, and error envelope.
- [`structural-pipeline.md`](references/structural-pipeline.md): read/edit/apply schemas, operations, file targets, handle refs, merge, and pipes.
- [`line-editing.md`](references/line-editing.md): line operations, anchor format, line endings, and repair.
- [`config-path-patching.md`](references/config-path-patching.md): path syntax and JSON/YAML/TOML behavior.
- [`failed-diff-handoff.md`](references/failed-diff-handoff.md): failed unified-diff discovery and explicit apply handoff.
- [`transactions.md`](references/transactions.md): multi-file apply, rollback drill, and error recovery.
- [`languages.md`](references/languages.md): bundled languages and dynamic grammar installation.

## Tool pairing

Use `ast-grep` for structural discovery and identedit for verified application. Use `repren` for bulk text refactoring, simultaneous renames, case-preserving variants, and file or directory renames.
