# Language Support Reference

## Bundled Languages

Bundled grammars work out of the box:

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL (Terraform), Dockerfile

## Dynamic Grammar Install

The installer runs on macOS and Linux and requires Git and a local compiler: `cc` for C grammars or `c++` for grammars with a C++ scanner.

Installation builds and loads native code with your permissions. Later commands may load installed grammars while setting up providers.

Install one of the three catalog grammars by name:

```bash
identedit grammar install elixir
identedit grammar install zig
identedit grammar install dart
```

Identedit checks the fetched revision against the commit pinned in its catalog before building. If it cannot fetch or verify that revision, it fails without falling back to the latest code.

For a grammar outside the catalog or to override a pin, pass `--repo` with a Git URL or local repository you trust. Identedit builds the cloned checkout without a catalog pin check. The repository must include a generated `src/parser.c`. Use `--ext` for a language without default extensions and `--symbol` for a nonstandard grammar symbol.

```bash
identedit grammar install mylang --repo ./tree-sitter-mylang --ext ml
```

On Windows, use bundled grammars or add a manifest entry for a trusted library built for Windows. The manifest is `grammars/manifest.json` under `IDENTEDIT_HOME` or, by default, `.identedit/grammars/manifest.json` under your home directory. Add the entry to the `grammars` array with `lang`, `repo`, `symbol`, `extensions`, and the local `library_path`.
