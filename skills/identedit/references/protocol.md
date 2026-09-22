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
- `patch --dry-run --diff` emits unified diff text.
- Failed-diff handoff emits JSON and never writes files.

Consumers should parse documented JSON output rather than grep its rendered text. Capture stderr separately when command construction itself may be invalid.

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

Post-edit coordinates, fresh edit anchors, source snippets, and semantic validation are outside this receipt contract. The bounded receipt supports a quick location check, not a second read view; richer output is deferred until usage shows a concrete need. For full text preview, use `patch --dry-run --diff`; read again for current edit addresses.

## Hashes and identities

Content hashes and node identities contain exactly 16 ASCII hexadecimal characters. Line hashes contain exactly 12 ASCII hexadecimal characters.

Canonical serialization is lowercase. Ingress accepts surrounding whitespace and uppercase hexadecimal characters, then normalizes them. Matching is exact; prefix matching is not supported.

## Line anchors

Canonical line anchors use:

```text
LINE:12-hex-hash
```

Line numbers start at `1`. Ingress also accepts display-form anchors such as:

```text
7:ABCDEF012345|original content
```

The canonical serialized form is `7:abcdef012345`; the display suffix is not part of the address.

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
