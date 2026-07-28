# Line-Anchored Editing Reference

Use this when structural targeting is too coarse but a line/range needs precondition safety.

## Read Anchors

```bash
identedit read --mode line example.py
```

Default text output:

```text
1:a1b2c3d4e5f6|import os
2:f7e8d9c0a1b2|
3:3c4d5e6f7a8b|def process_data(x):
4:9e0f1a2b3c4d|    return x + 1
```

Each anchor is `LINE:HASH`, where `HASH` is a 12-char blake3 hex digest. Matching is exact; prefix matching is not supported.

Use JSON only when needed:

```bash
identedit read --mode line example.py --json
```

## Patch Lines

```bash
identedit patch example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"
```

```bash
identedit patch example.py \
  --at "3:3c4d5e6f7a8b" \
  --replace-range "def process_data(x, y):\n    return x + y" \
  --end-anchor "4:9e0f1a2b3c4d"
```

```bash
identedit patch example.py --at "4:9e0f1a2b3c4d" --insert-after-line "    # added line"
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
- `4:9e0f1a2b3c4d` -> line anchor
- `ca465ff1a2b3c4d5` -> node identity
- `file-start` / `file-end` -> file boundary

## Repair Policy

Default mode is strict. If the line changed or moved, strict matching fails.

Use `--auto-repair` only once, after refreshing anchors or when a deterministic remap is acceptable:

```bash
identedit patch example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y" --auto-repair
```

If repair is ambiguous, identedit fails instead of guessing. Fall back to direct editing or re-read the file and choose a fresh anchor.
