# Identedit

Code editing for autonomous agents.

## Why

Agents edit code through tools designed for humans — `sed`, `patch`, regex-based find-and-replace. These work when a human is watching, but break down in autonomous workflows:

- **No structural awareness.** `sed` treats code as flat text. An agent can't say "replace function X" — it has to construct fragile regex patterns that break when formatting changes.
- **No precondition checking.** `patch` applies blindly. If another agent (or the same agent in a different step) already modified the file, the edit silently corrupts the codebase.
- **No diagnosable failures.** When edits fail, agents get cryptic error messages instead of structured diagnostics they can act on.

Identedit solves these by treating edits as **verified structural operations** rather than text substitution.

## What It Does

Three entry points covering different editing needs:

**`patch`** — one-shot verified edit (most common):
```bash
identedit patch src/example.py --identity abc123 --replace 'def f(): ...'
identedit patch src/example.py --at "42:9e0f1a2b3c4d" --set-line "    return x + y"
identedit patch config.yaml --config-path server.port --set-value 8080
identedit patch config.json --config-path items --append-value 4
```

**`read` → `edit` → `apply`** — multi-op or multi-file atomic pipeline:
```bash
identedit read --kind function_definition example.py --json   # get handles
identedit edit --json < request.json                          # build changeset (dry-run)
identedit edit --json < request.json | identedit apply        # commit to disk
```

**`read --mode line`** — line-level precision edits:
```bash
identedit read --mode line example.py   # display LINE:HASH|content
identedit patch example.py --at "3:a1b2c3d4e5f6" --replace-range "..." --end-anchor "5:7f6e5d4c3b2a"
```

Use the canonical CLI entry points: `read`, `edit`, `apply`, `patch`, `merge`, `grammar`.

### Key Properties

- **Precondition-verified.** Every edit checks that the target hasn't changed since the agent last read it. No silent corruption.
- **Transactional.** Multi-file edits are all-or-nothing with automatic rollback on failure.
- **Diagnosable.** Failures return structured JSON with specific error types and recovery suggestions.
- **Move and copy.** Structural units can be moved or copied within or across files atomically.
- **Two granularities.** Structure-level for large changes, line-level for small ones. Same safety guarantees for both.

## Supported Languages

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL, Dockerfile

## Install

### Prebuilt binaries (GitHub Releases)

1. Open [GitHub Releases](https://github.com/isty2e/identedit/releases) and pick your tag (for example `v0.1.1`).
2. Download the matching asset:
   - `identedit-<tag>-x86_64-unknown-linux-gnu.tar.gz`
   - `identedit-<tag>-aarch64-unknown-linux-gnu.tar.gz`
   - `identedit-<tag>-x86_64-apple-darwin.tar.gz`
   - `identedit-<tag>-aarch64-apple-darwin.tar.gz`
3. Extract and place `identedit` on your `PATH`.

### From source

```bash
cargo install --path .
```

## Platform Notes

- Core editing commands (`read`, `edit`, `apply`, `patch`, `merge`) are intended to run on macOS, Linux, and Windows.
- `identedit grammar install` is currently supported only on macOS and Linux hosts.
- On Windows hosts, use bundled grammars for now, or install grammar artifacts on macOS/Linux and copy the compiled library plus manifest entry.

## Quickstart

### One-shot patch (most common)

```bash
# Replace a function body
identedit read --kind function_definition --name process_data --json src/example.py
# → copy identity and expected_old_hash from output
identedit patch src/example.py --identity <id> --replace 'def process_data(x, y):
    return x + y'

# Patch a specific line
identedit read --mode line src/example.py
# → copy LINE:HASH anchor from output
identedit patch src/example.py --at "4:9e0f1a2b3c4d" --set-line "    return x + y"

# Update a config key
identedit patch config.yaml --config-path server.port --set-value 8080

# Append to an array-valued config path
identedit patch config.json --config-path items --append-value 4
```

### Multi-file atomic edit

```bash
# 1. Read — discover structures
identedit read --kind function_definition src/example.py --json

# 2. Edit — build changeset (dry-run, no file modification)
identedit edit --json < request.json

# 3. Apply — commit to disk (all-or-nothing)
identedit edit --json < request.json | identedit apply
```

### Large new_text (10+ lines)

```bash
cat <<'EOF' > /tmp/new_block.py
def process_data(x, y):
    return x + y
EOF

jq -n --rawfile new_text /tmp/new_block.py '{
  command:"edit", file:"src/example.py",
  operations:[{
    target:{type:"node", identity:"<id>", kind:"function_definition", expected_old_hash:"<hash>"},
    op:{type:"replace", new_text:$new_text}
  }]
}' | identedit edit --json | identedit apply
```

### Safe Defaults

- `edit` is always a dry-run. No files modified until explicit `apply`.
- Line-anchored patch defaults to strict mode. `--auto-repair` is explicit opt-in.
- `apply --dry-run` validates and returns a summary without writing.
- Most commands emit JSON; `read --mode line` defaults to plain text unless `--json` is set.

## Error Recovery (Agent Loop)

1. If `patch` fails with `precondition_failed` or `target_missing`: re-run `read`, rebuild request, retry once.
2. If `ambiguous_target`: add `span_hint` from `read` output, retry once.
3. Maximum 2 attempts per target. If the second attempt fails, fall back to direct file editing.

## Docs

- Agent workflow guide: [`skills/identedit/SKILL.md`](skills/identedit/SKILL.md)
