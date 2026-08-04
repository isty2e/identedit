# Identedit

[![CI](https://github.com/isty2e/identedit/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/isty2e/identedit/actions/workflows/ci.yml)

Code editing for autonomous agents.

## Why

Agents often edit code through text-oriented tools such as `sed`, unified patches, and regex replacement. Those tools become unreliable when target text repeats, files change between read and write, or several files must change together.

Identedit treats edits as verified operations:

- **Precondition-verified.** Every edit checks that its target still matches what the agent read.
- **Transactional.** Multi-file edits are preflighted together, and committed changes are rolled back if a later commit fails. Incomplete rollback is reported explicitly.
- **Structural or line-anchored.** Agents can address functions and classes, exact lines, file boundaries, or nested config paths.
- **Diagnosable.** Failures return structured errors with recovery information.

## Choose an entry point

Use `patch` for one verified edit:

```bash
identedit patch src/example.py --symbol process_data \
  --replace 'def process_data(x, y):
    return x + y'

identedit patch config.yaml --config-path server.port --set-value 8080
```

Use `read` -> `edit` -> `apply` for multiple operations, moves, or multi-file transactions:

```bash
identedit read --kind function_definition src/example.py --json
identedit edit --json < request.json > changeset.json
identedit apply --dry-run changeset.json
identedit apply changeset.json
```

Use line anchors when structural targeting is too coarse:

```bash
identedit read --mode line src/example.py
identedit patch src/example.py --at "4:9e0f1a2b3c4d" \
  --set-line "    return x + y"
```

Use failed-diff handoff to discover exact candidates after a conventional patch loses its context. This mode never writes:

```bash
identedit patch --from-diff failed.diff src/example.py
```

The supported CLI commands are `read`, `edit`, `apply`, `patch`, `merge`, and `grammar`. Identedit does not expose a supported Rust library API.

## Install

### Prebuilt binaries

1. Open [GitHub Releases](https://github.com/isty2e/identedit/releases) and choose a version.
2. Download the archive for your platform:
   - `identedit-<tag>-x86_64-unknown-linux-gnu.tar.gz`
   - `identedit-<tag>-aarch64-unknown-linux-gnu.tar.gz`
   - `identedit-<tag>-x86_64-apple-darwin.tar.gz`
   - `identedit-<tag>-aarch64-apple-darwin.tar.gz`
3. Verify the accompanying SHA-256 checksum, extract the archive, and place `identedit` on your `PATH`.

### From source

```bash
cargo install --path .
```

Core editing commands are intended to run on macOS, Linux, and Windows. `identedit grammar install` currently requires macOS or Linux.

## Common workflows

### Preview a structural replacement

Use `--symbol` for a unique local name or containing-name path. Ambiguous targets fail without writing and return candidate context.

```bash
identedit patch src/example.py --symbol Processor.process_data \
  --replace --text-file /tmp/new_body.py --dry-run --diff

identedit patch src/example.py --symbol Processor.process_data \
  --replace --text-file /tmp/new_body.py
```

Use `--kind` plus `--name` when kind-specific glob matching is required:

```bash
identedit patch src/example.py \
  --kind function_definition --name 'process_*' \
  --replace --text-file /tmp/new_body.py
```

### Edit a nested config value

```bash
identedit patch config.yaml --config-path server.port --set-value 8080
identedit patch config.json --config-path items --append-value 4
identedit patch config.toml --config-path database.enabled --delete
```

Config paths use dot-separated bare keys and bracket-quoted JSON strings for literal keys:

```bash
identedit patch config.yaml \
  --config-path 'services["sidecar.port"].enabled' --set-value true
```

### Build an atomic multi-file edit

`edit` only builds a changeset. `apply --dry-run` validates it without writing. If a later commit fails, `apply` attempts guarded rollback and reports any incomplete recovery.

```bash
identedit edit --json < request.json > changeset.json
identedit apply --dry-run changeset.json
identedit apply changeset.json
```

## Safety boundary

- `edit` and `apply --dry-run` never modify files.
- `patch --dry-run --diff` prints a unified diff without writing.
- Failed-diff handoff discovers candidates but never applies them.
- Line repair is opt-in and fails rather than choosing an ambiguous remap.
- Identedit verifies edit preconditions, not semantic correctness. Run project-specific tests and linters after non-trivial edits.
- Use direct editing when placement depends on project-specific comment semantics or unsupported config-format behavior.

## Supported languages

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL, Dockerfile

Additional tree-sitter grammars can be installed on supported hosts.

## Documentation

- Agent routing and operating rules: [`skills/identedit/SKILL.md`](skills/identedit/SKILL.md)
- CLI output, hashes, anchors, and error envelope: [`protocol.md`](skills/identedit/references/protocol.md)
- Structural and multi-file pipeline: [`structural-pipeline.md`](skills/identedit/references/structural-pipeline.md)
- Line-anchored editing and repair: [`line-editing.md`](skills/identedit/references/line-editing.md)
- JSON/YAML/TOML path editing: [`config-path-patching.md`](skills/identedit/references/config-path-patching.md)
- Failed unified-diff recovery: [`failed-diff-handoff.md`](skills/identedit/references/failed-diff-handoff.md)
- Transactions and error recovery: [`transactions.md`](skills/identedit/references/transactions.md)
- Bundled and dynamic languages: [`languages.md`](skills/identedit/references/languages.md)
- Command flags and defaults: `identedit <command> --help`

## Feedback

Open or update a [GitHub issue](https://github.com/isty2e/identedit/issues) with:

- what you were trying to do;
- what happened, including the error or unexpected output;
- what you expected instead.
