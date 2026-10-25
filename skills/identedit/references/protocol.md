# CLI and protocol reference

This document owns the cross-cutting CLI contract. Workflow-specific request and response shapes remain in the other reference documents.

## Supported interface

The supported commands are:

- `read`: discover structural handles or line anchors;
- `edit`: compile one or more requested operations into a dry-run changeset;
- `apply`: validate or commit a compiled changeset;
- `patch`: build and optionally apply one single-target operation;
- `merge`: combine non-conflicting changesets;
- `grammar`: install a dynamic tree-sitter grammar.

Run `identedit <command> --help` for the complete flag surface. Identedit does not expose a supported Rust library API.

## Exit and output behavior

- Success exits with status `0`.
- Runtime and request errors after argument parsing exit non-zero and write a JSON error response to stdout.
- Invalid subcommands, flags, and argument shapes are reported by the argument parser on stderr and exit non-zero without a JSON envelope.
- `read` defaults to human-readable text in AST and line modes. Add `--json` for structured output.
- `edit`, `apply`, and `patch` emit JSON by default.
- `patch --dry-run --diff` and `apply --dry-run --diff` emit unified diff text.
- Successful `patch`/`apply` commits include a [change diff](#committed-change-diffs) in JSON by default; `--no-diff` omits it.
- Failed-diff handoff emits JSON and never writes files.

Consumers should parse documented JSON output rather than grep its rendered text. Capture stderr separately when command construction itself may be invalid.

## Edit JSON intents

Both `edit --json` envelopes (`file` + `operations`, or `files`) accept strict resolve-now node targets:

```json
{"type":"symbol","name":"Service.run"}
```

```json
{"type":"selector","kind":"function_definition","name":"load_*"}
```

A symbol uses the same local/qualified-name rules as the flag selector; a selector requires a kind and a glob over raw provider names. Exactly one node must match. Unknown fields and mixed guard fields, including flat null guard fields, are rejected. To preserve an earlier observation, use the existing node/flat/handle-ref form with its copied guards; edit never replaces a stale supplied hash with a fresh one.

Node operations also accept `{"type":"scoped_regex","pattern":"old","replacement":"new"}`. It rewrites raw selected-node text, including strings/comments, with the existing regex capture semantics. Invalid patterns and zero matches fail. The result lowers to a literal guarded replacement, not an executable regex in the changeset. Existing node positions in structural moves also accept symbol/selector targets; same-file and cross-file destination-kind restrictions are unchanged.

Both envelopes also accept `config_path` targets with `set`, `append` or `delete`. Compatible original-source writes can be composed under their existing guard scopes, allowing several settings in one container to change together. They lower to fixed literal operations and a [full-file source condition](#file-source-preconditions), not saved paths to rerun. See [batch config intents](config-path-patching.md#batch-edit-intents) for request fields, composition limits and mixed-operation behavior.

Edit JSON rejects duplicate target `type` fields before selecting a target family, including in structural-move destinations.

Operations do not consume earlier operations' proposed output. There is no single-read, atomic query-set or whole-request source-version guarantee: edit may cache observations and reread files before validating fixed guards/building previews. Without a [file source precondition](#file-source-preconditions), unrelated changes or changed containing names can leave guards satisfied; queries are not rechecked by apply. Existing overlap checks still reject conflicting byte effects, and compilation failure returns no partial plan and writes no files.

Sequential virtual-document editing remains unsupported. `patch --json` still accepts guarded node targets, not symbol/selector targets; patch/edit flags support both selectors, and patch JSON retains guarded-node scoped regex. Revisit this deliberate JSON-target asymmetry only as a separately selected extension. No reindentation, all-match selection, external spans, DSL, runtime validation or new filesystem identity/transaction promise is added.

Use [ast-grep](https://ast-grep.github.io/) for all-match structural rewrites; it runs outside identedit's guarded transaction.

## File source preconditions

A compiled `files` entry may contain `expected_file_hash`, the 16-hex content hash of its complete original source bytes. Newly generated config-path plans always include it, including edit JSON intents, config flag edits and the internal plans used by patch flags/JSON. It protects the full source context read by config checks, not just the replaced container. A file containing both config and other operations retains this condition for the whole file; raw-only plans omit it unless explicitly supplied in a compiled plan.

The builder and apply validate this condition against the actual source bytes before UTF-8 decoding, provider parsing or target resolution. Any content change, even outside the selected container, requires a new plan. This applies to dry-run/diff, no-op and empty-operation entries, and whole-file moves. A mismatch fails before transaction writes or renames, with `error.type: "precondition_failed"`, the file path, and guidance to reread/recompile rather than refresh node handles, even if the changed bytes are invalid syntax or UTF-8. `--repair` can repair line anchors but cannot refresh or remove the file condition. This is a source precondition, not a file reservation, query replay or a general configuration-validity guarantee.

Merge conjoins the optional file condition and every whole-file move or file-boundary target hash, including boundary destinations nested in same-file `move_before`/`move_after` operations. Conflicting conditions are rejected even when the optional field is absent. This validation neither adds a file condition to raw plans nor refreshes hashes. Node hashes, line anchors and preview guards remain scoped to their targets, not the whole file. Guarded empty entries are retained as read-only byte checks: they do not parse, rewrite or become rollback targets, and may refer to non-UTF-8 files. Existing apply locking and path checks still apply. A whole-file move must satisfy both its target hash and the optional file condition; differing expected hashes are a conflicting request.

**0.7.0 compatibility:** this is a changeset-format extension. Earlier apply/merge versions reject the unknown file field rather than ignoring it; use a compatible binary instead of stripping the condition. Old saved plans without the field retain target-only protection at apply, not the stronger config protection. Valid non-config plan output without the field is unchanged.

## Node-result parsing

Before admitting a file with any node-target operation, `patch` and `apply` reparse its complete proposed text with the syntax-capable provider selected for the original file. This covers replacements, deletions, insertions, in-file moves, and lowered cross-file moves or scoped-regex edits. All operations in that file, including line and file-boundary edits, contribute to the final text being checked. Individual payloads need not parse in isolation. A rejected result returns `error.type: "parse_failure"` and `error.stage: "proposed_result"`, identifying the file and provider with recovery guidance; no file edits or moves in the transaction have started.

Source syntax failures use the same error type with `error.stage: "source"`. Provider initialization failures retain the `parse_failure` type but omit `stage`; unrelated error types also omit it. Consumers must branch on the structured type and stage, not message text or `suggestion` presence. A missing or unknown stage does not establish source/result failure.

The same check applies to `patch --dry-run`, `patch --dry-run --diff`, and `apply --dry-run`, including `--repair`. `edit` only compiles a plan: successful compilation does not mean its result passes this check. Fallback-provider edits, line/file-boundary-only files, and whole-file moves do not receive this result-parse check. An empty operation list does not enable it. Provider failure never triggers a retry through the fallback provider. For `.h` files, either the C or C++ grammar may accept the result; preserving the original dialect is not required.

For the bundled Python grammar, this result check also rejects a zero-width named `block`, representing an empty required suite or match-case list. The diagnostic names its owning construct's starting line in the proposed result, not the original source. This checks the entire final result, including pre-existing empty blocks; fix them in the same batch or use line-based editing. Original-source reads and plan construction remain unchanged, and empty modules remain valid. This extra check does not apply to dynamically installed grammars. Reassess the predicate when updating the bundled Python grammar because it depends on that grammar's representation.

This is provider acceptance, not complete compiler, indentation, scope, or behavioral validation. Other Python indentation errors and a method unintentionally nested inside another method can still pass. Raw payload and selected node bytes are not dedented or reindented; missing separators are not inferred. Moving Python functions, methods, or statements between node boundaries can concatenate code and be rejected; use [explicit move text](#raw-move-text) or line-based plans with caller-owned newlines and indentation. Existing destination separators can make a raw move valid, for example moving a top-level Python function to `file_end` after an existing newline. Inspect the proposed edits and run project-specific verification.

Generic structural-confinement and symbol/container-preservation rejection rules are unsupported: raw replacement requests do not specify the intended output structure, and legitimate restructuring can resemble an error. These rules remain deferred until requests can express expected structure or maintainers accept a separately scoped contract. The empty-block check does not prevent all Python placement failures discussed in [issue #16](https://github.com/isty2e/identedit/issues/16).

## Named-structure changes

Successful JSON `patch` and `apply` responses include `structure_changes` in both default and verbose output, including dry-run. This observes name/count changes for files containing node-target operations; it never rejects an edit or infers rename/move intent. `edit` only builds a plan, discovery-only `patch --from-diff` keeps its candidate response, and `--diff` emits no report on stdout or informational report on stderr.

Names are not limited to declarations. Any provider-named node can contribute, including default parameters, keyword arguments and local variable declarators; body-only edits can therefore change the report.

For example, a method becoming nested inside another method can produce:

```json
{
  "structure_changes": {
    "summary": {
      "checked_files": 1,
      "unavailable_files": 0,
      "out_of_scope_files": 0,
      "added": 1,
      "removed": 1
    },
    "files": {
      "total": 1,
      "omitted": 0,
      "entries": [{"file": "example.py", "status": "checked"}]
    },
    "changes": {
      "total": 2,
      "omitted": 0,
      "entries": [
        {"file": "example.py", "kind": "function_definition", "qualified_name": "Example.target", "before_count": 1, "after_count": 0},
        {"file": "example.py", "kind": "function_definition", "qualified_name": "Example.first.target", "before_count": 0, "after_count": 1}
      ]
    }
  }
}
```

- Each file has one observation status: `checked`, `unavailable`, or `out_of_scope`. A checked file with no differences contributes no changed entries. Unavailable files include `reason: "unsupported_provider"` for fallback/non-capable observations or `reason: "projection_failed"` for recoverable observation failure. These states are not checked empty observations. Line/file-boundary-only files, empty operation lists and whole-file moves are out of scope and gain no structural parsing for this report. The envelope remains present even when every file is `out_of_scope`.
- `files` describes input changeset file units; a whole-file move contributes its source file. `files.total` and the summary coverage counts include all units, while `files.entries` contains at most 16, sorted by file. `files.omitted` explicitly counts unlisted file identities; do not assume an omitted file was checked or unchanged.
- Compare occurrence counts by file, provider `kind` and full qualified name. Overloads/getters/setters with the same key remain multiple occurrences. Body or decorator changes alone need not change those counts. Names follow the existing selector's named-span containment interpretation, including opaque names and stable equal-span ordering, not compiler/LSP scope. Identity hashes and declaration text are not comparison keys. Stable semantic identity across provider versions or `.h` dialect selection is not promised.
- `summary.added` and `summary.removed` are exact occurrence increases/decreases among checked files only; unavailable files contribute no invented counts. `changes.total` counts changed keys, not occurrences or edit operations. Each entry gives its full-key `before_count` and `after_count`.
- `changes.entries` contains at most 16 entries across the response. Ordering is file, decreases before increases, full qualified name, then kind. `changes.omitted` counts changed entries not shown. Counts and ordering are computed before display limits, including when a container rename changes many qualified names. `--verbose` does not expand either receipt limit.
- Qualified names longer than 120 Unicode scalar values are shortened from the front with `…`, retaining the tail/leaf within that limit, and have `qualified_name_truncated: true`. An absent flag means false. Truncated names are display fragments, not exact selectors; equal displayed fragments are not merged. A leaf longer than the limit is itself shortened from the front.

Observations use the complete locked preflight source and proposed bytes, including co-resident edits. They are computed before writes, without rereading mutable paths after commit. Dry-run reports planned changes; a committed receipt describes that successful transaction's proposed bytes, not a fresh post-write read or reservation. Result-parse errors retain their ordinary failure precedence; recoverable optional observation failures do not change edit acceptance. Error and rollback responses have no success report.

An empty checked difference is not proof of valid syntax, indentation or behavior. Review the named entries rather than only their totals, and retain ordinary language/test verification. The [node-result parsing boundary](#node-result-parsing) and its unsupported generic-rejection policy remain unchanged.

## Raw move text

The `edit --json` operations `move_before`, `move_after`, `move_to_before`, and `move_to_after` accept optional string fields `prefix` and `suffix`. At the resolved destination, Identedit inserts `prefix + selected node bytes + suffix` exactly once. The source selection, content hash checks and deletion remain unchanged: affixes do not expand the source span or remove surrounding whitespace or comments.

Both fields default to the empty string. Empty fields are omitted from compiled serialization, preserving existing raw behavior and output when no affixes are supplied. Strings are copied literally, including Unicode, CRLF, tabs and spaces; Identedit does not trim them, choose line endings, infer separators, or adjust indentation. For Python `move_before`, a suffix such as `"\n\n    "` may be needed to indent the following destination line, not just separate the moved text.

The same composition applies to same-file and cross-file moves. Same-file changesets retain nonempty affixes on the move operation; their preview still describes the source deletion. Cross-file moves lower to source deletion and destination insertion, whose `new_text` and preview contain the composed text. Use an `apply` binary that understands the new fields for same-file changesets with nonempty affixes.

These are raw moves with explicit surrounding text, not formatting-aware or semantically safe relocation. Reindenting the moved body, repairing an emptied source block and deciding whether comments or decorators belong to the selection remain the caller's responsibility. Affixes do not relax target compatibility, overlap checks, destination preconditions, [result parsing](#node-result-parsing), or transaction behavior. Existing target support is unchanged; cross-file moves still require node destinations. Whole-file renames do not accept these fields. Structural moves remain an `edit` -> `apply` workflow, without a `patch` JSON entry point. Preview the compiled plan with `apply --dry-run --diff`.

## Text diff previews

`patch --dry-run --diff` and `apply --dry-run --diff` compare the validated original and proposed full-file text, not isolated node spans. They show actual surrounding indentation, combine edits into one diff per file, include three context lines, and merge nearby hunks. No-op edits emit no diff after validation. A failure emits the usual JSON error, never a partial successful diff.

For `apply`, provide a plan file or raw plan on stdin; wrapped `apply --json` input also supports this output selection. Files appear in canonical absolute source-path sort order, while headers retain the plan's path spelling. `--repair` retains its existing guard rules and shows the repaired plan's validated result. Any whole-file move rejects the entire text-preview request; use ordinary `apply --dry-run` for its JSON preview instead.

`--diff` requires `--dry-run`; `--color auto|always|never` requires `--diff` and defaults to terminal-based `auto`. `--verbose` does not append JSON to diff text. `--diff` and `--no-diff` cannot be combined. Patch JSON mode continues to return JSON and does not accept `--diff`.

For portable replay, generate the diff using `--color never` and paths relative to the replay working directory, with no `..` components; run `patch -p0` from that same directory against the unchanged original. Identedit accepts absolute paths, but GNU patch can reject absolute or parent-traversing names in diff headers. CR bytes are preserved, including CRLF and mixed endings; missing final LF is marked with `\ No newline at end of file`. GNU patch also accepts `--binary`. Replay equivalence is to the proposed bytes, not a guarantee of valid syntax or behavior. Whole-file move previews are unsupported.

Hunk coordinates count LF-delimited physical lines. Read anchors and resolved locations also recognize standalone CR as a terminator, so their line numbers can differ from hunk numbers. **Do not use hunk coordinates as edit anchors.**

The renderer uses Myers alignment after trimming equal outer lines. If the unmatched inputs exceed fixed line-count or byte limits, it emits one exact trimmed replacement hunk with context. Output is never truncated and does not depend on elapsed-time deadlines; global minimality is not guaranteed. A dry-run snapshot does not reserve a later write: applying still rechecks normal preconditions.

## Committed change diffs

Successful `patch` and `apply` commit JSON includes `diff: {text, file_moves}` by default. `text` is a color-free unified diff of the actual pre-write and written buffers, using the same formatter as text previews; it is not reconstructed by rereading after commit. Multiple text files use preflight path order. The text is complete, without the locations/report detail caps or automatic size-based summaries. Large deletions can produce large responses.

`file_moves` lists every whole-file rename as `{source, destination}`, in validated execution order with the same normalized paths as move locations. Renames are not represented as text hunks, and binary move sources are not decoded as text. Text changes and renames can coexist in a successful commit response. A successful content no-op or guard-only plan has empty `text` and `file_moves` when it contains no rename.

Use CLI `--no-diff` to omit the field and skip rendering, for example when the change view is already available or an automated caller does not need it. Patch JSON also accepts `options.no_diff`; either it or the CLI flag suppresses the field. Suppression does not change guards, validation or execution. Ordinary dry-run JSON, discovery responses and errors do not contain a committed diff; a failed transaction or incomplete rollback never publishes a success diff.

This view describes the tool's change, not every external change since an earlier observation or the current contents after another writer acts. It provides neither a new validity guarantee nor fresh edit anchors. Hunk coordinates count LF-delimited lines and must not be used as edit anchors. `locations` remains the bounded, pre-edit checked-target view described below; it is not a substitute for the actual text diff.

## Resolved edit locations

Successful edit execution and dry-run validation through `patch` or `apply` include a bounded `locations` object in JSON, in both default and verbose output. Discovery-only `patch --from-diff` keeps its candidate response instead:

```json
{
  "basis": "pre_edit",
  "total": 1,
  "omitted": 0,
  "entries": [
    {
      "kind": "text",
      "file": "example.py",
      "operation_index": 0,
      "span": {"start": 9, "end": 35},
      "start_line": 2,
      "end_line": 3
    }
  ]
}
```

- Coordinates come from the resolved plan and the original snapshot used for validation, including any line-anchor repair. They are not copied from requested hints or recomputed from the file after writing.
- `span` is a half-open byte range. Line numbers are 1-based; for nonempty spans, `end_line` is the last intersecting line. CRLF counts as one terminator; standalone CR and LF also end a line.
- Inserts have a zero-width span at the resolved insertion point. At EOF, the line number is the final line without a terminator, or the following line after a terminal newline; an empty file uses line 1.
- `operation_index` is zero-based within each file's operations. A same-file move has two entries with the same index: the source range and the destination insertion point. Counts measure locations, not operations.
- Whole-file moves have `kind: "file_move"`, `source`, `destination`, and `operation_index`, with no text coordinates. Paths are those of the validated execution plan, not the submitted spelling. The source and any existing destination are canonicalized. A missing destination becomes an absolute, lexically normalized path (`.` and `..` removed), with relative paths resolved from the command's working directory; its parent symlinks are not resolved by this normalization.
- At most 16 entries are returned across the whole response, even with `--verbose`. `total` includes all resolved locations; `omitted` counts those not shown. This limit does not limit execution. Text plans appear in preflight file order, then whole-file moves in execution order; within a text plan, entries follow operation order.

Locations describe checked targets, not a minimal byte diff: config edits can replace a containing object, and line edits can adjust adjacent line terminators. A no-op may still report a checked location. Use the existing `changed`, `dry_run`, or `transaction.status` fields to interpret the outcome; locations alone do not mean a file was changed. Error responses, including rollback failures, do not include success locations.

Post-edit coordinates, fresh edit anchors, full source views, and semantic validation are outside this bounded location receipt. Use committed change diffs for actual changed lines, `patch --dry-run --diff` or `apply --dry-run --diff` for proposed text changes, and read again for current edit addresses.

## Hashes and identities

Content hashes and node identities contain exactly 16 ASCII hexadecimal characters. Line hashes contain exactly 8 ASCII hexadecimal characters.

Canonical serialization is lowercase. Ingress accepts surrounding whitespace and uppercase hexadecimal characters, then normalizes them. Matching is exact; prefix matching is not supported.

## Line anchors

Canonical line anchors use:

```text
LINE:8-hex-hash
```

Line numbers start at `1`. Ingress also accepts display-form anchors such as:

```text
7:ABCDEF01|original content
```

The canonical serialized form is `7:abcdef01`; the display suffix is not part of the address.
For a range, only the supplied boundary anchors are checked against the current file. See [line editing](line-editing.md) for the interior-line and changeset rules.

## Edit verbs

`patch` and `edit` flag/JSON requests use target-aware verbs. `replace`, `delete`, and `insert_after` accept node or line targets; line targets operate on logical lines rather than raw byte spans. `end_anchor` is accepted only with line `replace` or `delete`. Node `insert_before`, file-boundary `insert`, whole-file moves, and config-path operations keep their own contracts. A line `replace` with empty text leaves one blank line; `delete` removes the line or inclusive range. The compiled changeset may use internal line-operation tags; callers should pass `edit` output to `apply`, not construct changesets directly.

## Error envelope

Errors use this shape:

```json
{
  "error": {
    "type": "precondition_failed",
    "message": "...",
    "suggestion": "..."
  }
}
```

`suggestion` is optional. `ambiguous_target` may also include a `candidates` array with structured target context.

### Edit request locations

Attributable `edit --json` errors add optional `error.request_pointers`, an array of JSON Pointers into the original request. Single-file operations use `/operations/2`; batch operations use `/files/1/operations/2`. A file-level error may point to `/file`, a file entry, or the operation that introduced a move destination. Syntax/envelope failures and failures without known input provenance can omit the field.

Conflict errors identify a sufficient pair of original operations, deduplicated and ordered by numeric input position. One input move can create two effects with the same origin and therefore one pointer. Pointers refer to input positions, not normalized file buckets, compiled operation indexes, filesystem paths or config paths.

The underlying error type, stage, suggestions and candidate/line-check details remain authoritative. This additive metadata is not written to changesets and is not added to flag, patch or apply errors by edit compilation. Do not parse input positions out of `message` or assume an omitted pointer means the request is valid.

### Ambiguity candidate details

Contextual ambiguity errors retain the full match count but show at most the first 16 candidates in their existing order. Matching is not capped or deduplicated, and no candidate is selected automatically. Each displayed record retains its identity, expected-old hash, kind, provider name, qualified name, span, line and preview. Names and guards are not shortened; this is an entry-count bound, not an absolute response-byte bound.

When a collected list is shortened, `error.candidate_summary` contains `total` (all matches) and `omitted` (matches minus displayed records). It is absent for complete lists. Errors from paths that do not collect context, including some JSON/apply ambiguities, keep both `candidates` and `candidate_summary` absent; absence does not mean zero matches. Unshortened and contextless errors keep their existing response, including recovery text. Shortened errors include specific recovery guidance. `--verbose` does not lift the cap.

To discover omitted targets, use `read --json` on the affected file **without `--symbol` or filters**. `read --symbol` uses the same ambiguity resolver and can return another shortened error. Optional kind/name filters intentionally narrow discovery and need not reproduce the original match set; `--name` is a glob over the provider's raw name, not a containing-name path. Do not derive it by splitting a qualified name: opaque provider names can contain dots.

Default AST JSON handles contain addresses and hashes, not the diagnostic `qualified_name`, `line` or `preview`. Add `--verbose` when exact node `text` is needed to distinguish signatures or bodies; it does not add those diagnostic fields. Copy the chosen guards and span into a node target, using `span_hint` for repeated identities. `patch --json` supports this for one operation; `edit --json` builds a plan or batch. Discovery observes current files, not a reserved earlier snapshot. Do not repeat a write command just to expand diagnostics.

Successful AST discovery still returns all matches of its filters; this error limit does not cap it. There is no ambiguity-list paging or full-detail switch; recovery uses read-only discovery. Failed-diff candidates, line-check remap candidates, `locations` and `structure_changes` have separate contracts.

### Other error details

Line checks rejected by `patch` or `apply --repair` keep the `invalid_request` type and expose an optional `line_check` object containing `ok`, `summary`, and `mismatches` (including remap candidates). Read that object directly; diagnostics are no longer JSON encoded inside `message`. Other precondition failures may still use `precondition_failed` without `line_check`.

If apply input declares `command: "edit"` and fails changeset parsing, the error retains the parse reason and suggests `identedit edit --json < request.json | identedit apply`. This is guidance only: input is never converted or applied automatically.

Current error types are:

- `no_provider`
- `invalid_request`
- `resource_busy`
- `path_changed`
- `invalid_selector`
- `parse_failure`
- `grammar_install_failed`
- `io_error`
- `serialization_error`
- `target_missing`
- `ambiguous_target`
- `precondition_failed`
- `rollback_failed`

See [`transactions.md`](transactions.md) for recovery actions. Request and changeset schemas are documented in [`structural-pipeline.md`](structural-pipeline.md), [`line-editing.md`](line-editing.md), [`config-path-patching.md`](config-path-patching.md), and [`failed-diff-handoff.md`](failed-diff-handoff.md).
