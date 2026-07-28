# Failed Diff Handoff

Use failed-diff handoff after a direct unified patch fails because its context no longer identifies one location.

## Preview

```bash
identedit patch --from-diff /tmp/failed.diff src/example.py > /tmp/handoff.json
```

Use `-` to read the diff from stdin:

```bash
cat /tmp/failed.diff |
  identedit patch --from-diff - src/example.py > /tmp/handoff.json
```

`edit --from-diff` is an equivalent discovery entry point and returns the same JSON. Both commands are preview-only and never write files.

`FILE` may be omitted when a one-file unified diff header or `*** Update File: ...` wrapper identifies it. If both are present, they must resolve to the same existing file.

## Accepted Input

- One-file numbered or unnumbered unified diff hunks.
- Git-style `--- a/path` and `+++ b/path` headers.
- `apply_patch` `*** Update File: path` wrappers.
- Bare `@@` hunks when `FILE` is explicit.
- `-` as the diff path for stdin.

The prototype rejects:

- Multi-file diffs.
- File creation, deletion, or rename diffs.
- Quoted diff paths.
- Malformed hunk counts or body lines.
- Final-newline-only changes.
- Ordinary target, operation, text-source, or patch execution flags mixed with `--from-diff`.

## Response

```json
{
  "mode": "failed_diff_handoff",
  "file": "src/example.py",
  "preview_only": true,
  "changes": [
    {
      "change_index": 0,
      "source_hunk_index": 0,
      "block_index": 0,
      "status": "unique",
      "old_line_count": 1,
      "new_line_count": 1,
      "candidates": [
        {
          "candidate_index": 0,
          "target": {
            "type": "line",
            "anchor": "12:0123456789ab"
          },
          "op": {
            "type": "replace_lines",
            "new_text": "new value"
          },
          "preview": {
            "before": [],
            "matched": [
              {
                "line": 12,
                "content": "old value"
              }
            ],
            "matched_lines_omitted": 0,
            "after": []
          }
        }
      ]
    }
  ],
  "summary": {
    "source_hunks": 1,
    "changes": 1,
    "unique": 1,
    "ambiguous": 0,
    "missing": 0,
    "candidates": 1
  }
}
```

Status meanings:

- `unique`: exactly one byte-exact logical-line candidate.
- `ambiguous`: multiple exact candidates; all are returned in deterministic source order.
- `missing`: no exact candidate; refresh the source or rebuild the diff.

No status causes an implicit write. `ambiguous` is not an error because the response exists to preserve and expose ambiguity.

## Explicit Apply

Only promote a candidate after inspecting its preview. For a response with one unique changed block:

```bash
jq '{
  command: "patch",
  file,
  target: .changes[0].candidates[0].target,
  op: .changes[0].candidates[0].op
}' /tmp/handoff.json |
  identedit patch --json
```

For an ambiguous changed block, inspect every `candidate.preview`, select the intended candidate explicitly, and use its `target` and `op`. Do not select candidate zero by convention.

Multiple change candidates are discovery output, not an atomic changeset. Build an `edit --json` request when several selected operations must commit together.
