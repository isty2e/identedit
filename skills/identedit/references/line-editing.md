# Line-Anchored Editing Reference

Use this when structural targeting is too coarse but an exact line location matters. A line anchor checks one line; it does not guard a whole multi-line range.

## Read Anchors

```bash
identedit read --mode line example.py
```

Default text output:

```text
1:a1b2c3d4|import os
2:f7e8d9c0|
3:3c4d5e6f|def process_data(x):
4:9e0f1a2b|    return x + 1
```

Each anchor is `LINE:HASH`, where `HASH` is an 8-character hex prefix of the BLAKE3 content hash. Matching is exact; prefix matching is not supported.
The hash covers line content, not its `LF`, `CRLF`, or `CR` terminator.
Refresh any 12-char anchors saved from older versions with `read --mode line`; they are not accepted as new inputs.

Use JSON only when needed:

```bash
identedit read --mode line example.py --json
```

### Bounded reads

```bash
identedit read --mode line --offset 40 --limit 30 example.py
```

`--offset` is a positive, 1-based original line number (default `1`); `--limit` is a positive maximum line count (default: all remaining lines). Bounds apply separately to each file. Empty files and offsets past EOF succeed with no handles. Text output reports the window and omitted-line counts, including empty windows. Addresses and indentation remain those of the original file.

Bounded JSON adds `windows[]`, one entry per file, with `kind: "line"`, `file`, `total_lines`, `start_line`, `end_line`, `omitted_before`, and `omitted_after`. Start/end line numbers are inclusive, or both `null` for an empty window. Selected line handles stay in `handles[]`; `summary.matches` counts those handles. `file_preconditions` still hashes each complete file, not the page; line patch/edit does not use it as an implicit guard. With no bounds, existing full-file output is unchanged.

The bounds limit output, not file loading. LF, CRLF, and bare CR each delimit one logical line; a trailing terminator does not create a phantom final line. The displayed text omits terminators. To read a complete symbol with nearby anchored lines instead, use `read --symbol NAME --context N`.

## Patch Lines

```bash
identedit patch example.py --at "4:9e0f1a2b" --replace "    return x + y"
```

```bash
identedit patch example.py \
  --at "3:3c4d5e6f" \
  --replace --text-file /tmp/new_lines.py \
  --end-anchor "4:9e0f1a2b"
```

`/tmp/new_lines.py` contains the replacement line contents separated by actual newlines, not literal `\n` characters. A trailing newline in this payload creates an additional empty logical line. If the original range had a terminator, that becomes an extra blank line; at unterminated EOF it instead adds the final terminator. Omit the payload's trailing newline unless that effect is intended. Copy anchors from the current `read` output; the values above only illustrate the format.

With `--end-anchor`, strict patch checks only the start and end lines. It replaces the current lines between them even if those lines changed after `read`. `edit --json` behaves the same way when creating a changeset. Default `apply` then checks the full old range captured by that changeset before writing. `apply --repair` instead refreshes line previews from the current file, so do not use it as an old-range guard. None of these paths compares the interior with an earlier `read`.

**Current non-goal:** line ranges do not provide whole-range read-to-write stale detection. Maintainers retain endpoint-only addressing without requiring an additional range guard. Revisit this if field reports show materially harmful interior overwrites. When old interior text must still match, use a node target with its expected content hash where available or a conventional patch that matches the old hunk.

```bash
identedit patch example.py --at "4:9e0f1a2b" --insert-after "    # added line"
```

Line operations use the same public verbs as node edits: `--replace`, `--delete`, and `--insert-after`. Add `--end-anchor` to replace or delete an inclusive line range. `--insert-before` remains node-only. `edit` flag mode accepts the same verbs. In `edit --json` and `patch --json`, use `op.type: "replace"` or `"insert_after"` with `new_text`, or `"delete"` without text. For example:

```json
{"command":"edit","file":"example.py","operations":[{"target":{"type":"line","anchor":"4:9e0f1a2b"},"op":{"type":"replace","new_text":"    return x + y"}}]}
```

The target determines semantics: a node replacement uses exact byte text; a line replacement uses logical lines and preserves local line terminators. Empty `--replace ''` sets the selected line or range to one blank line of content. On a one-line file without a terminator, that produces an empty file because no terminator remains; use `--delete` when removal is intended. Otherwise, `--delete` removes the selected line(s). Empty `--insert-after` text is invalid. `end_anchor` is invalid with `insert_after`. A compiled changeset retains internal logical-line operation tags; construct requests with the public verbs rather than writing changesets by hand. `apply` rechecks resolved anchors and preview before writing. Cached changesets from older versions with a raw `replace`/`insert_after` line operation or an empty `replace_lines` operation are rejected; regenerate them with `edit`.

## Line Ending Preservation

- Untouched lines retain their original `LF`, `CRLF`, or `CR` terminators, including in mixed files.
- Multiline replacement and insertion text uses the target boundary's local terminator style.
- Replacing or deleting the final line preserves whether the original file ended with a line terminator.

## Target Auto-Detection

`patch --at` detects target type by format:
- `4:9e0f1a2b` -> line anchor
- `ca465ff1a2b3c4d5` -> node identity
- `file-start` / `file-end` -> file boundary

## Repair Policy

Default mode is strict. If the line changed or moved, strict matching fails.

Use `--auto-repair` only once, after refreshing anchors or when a deterministic remap is acceptable:

```bash
identedit patch example.py --at "4:9e0f1a2b" --replace "    return x + y" --auto-repair
```

If repair is ambiguous, identedit fails instead of guessing. Fall back to direct editing or re-read the file and choose a fresh anchor.
