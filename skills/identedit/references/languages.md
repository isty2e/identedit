# Language Support Reference

## Bundled Languages

Bundled grammars work out of the box:

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL (Terraform), Dockerfile

## Dynamic Grammar Install

`identedit grammar install` builds and loads native Tree-sitter code with your permissions. Later commands may load installed grammars while setting up providers.

The installer runs on macOS and Linux. On Windows, bundled grammars remain available. Manually registered libraries must be built for the host platform.

Install a pinned grammar by name:

```bash
identedit grammar install elixir
identedit grammar install zig
identedit grammar install dart
```

These installs use the exact repository revisions selected for the Identedit release. If the selected source cannot be retrieved or verified, the installer does not use the latest revision instead.

For other grammars, pass `--repo` with a Git URL or local repository you trust. This also overrides a catalog pin. The repository must contain generated C source at `src/parser.c`. Specify `--ext` for languages without default extensions and `--symbol` if the grammar exports a nonstandard symbol.

```bash
identedit grammar install mylang --repo ./tree-sitter-mylang --ext ml
```
