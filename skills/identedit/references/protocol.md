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
