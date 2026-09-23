# Line-Anchored Editing Reference

Use this when structural targeting is too coarse but a line/range needs precondition safety.

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

Bounded JSON adds `windows[]`, one entry per file, with `kind: "line"`, `file`, `total_lines`, `start_line`, `end_line`, `omitted_before`, and `omitted_after`. Start/end line numbers are inclusive, or both `null` for an empty window. Selected line handles stay in `handles[]`; `summary.matches` counts those handles. `file_preconditions` still hashes each complete file, not the page. With no bounds, existing full-file output is unchanged.

The bounds limit output, not file loading. LF, CRLF, and bare CR each delimit one logical line; a trailing terminator does not create a phantom final line. The displayed text omits terminators. To read a complete symbol with nearby anchored lines instead, use `read --symbol NAME --context N`.

## Patch Lines

```bash
identedit patch example.py --at "4:9e0f1a2b" --set-line "    return x + y"
```

```bash
identedit patch example.py \
  --at "3:3c4d5e6f" \
  --replace-range --text-file /tmp/new_lines.py \
  --end-anchor "4:9e0f1a2b"
```

`/tmp/new_lines.py` contains the replacement lines with actual newlines, not literal `\n` characters. Copy anchors from the current `read` output; the values above only illustrate the format.

```bash
identedit patch example.py --at "4:9e0f1a2b" --insert-after-line "    # added line"
```

Line operations:
- `--set-line`
- `--replace-range` with optional `--end-anchor`
- `--insert-after-line`

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
identedit patch example.py --at "4:9e0f1a2b" --set-line "    return x + y" --auto-repair
```

If repair is ambiguous, identedit fails instead of guessing. Fall back to direct editing or re-read the file and choose a fresh anchor.
