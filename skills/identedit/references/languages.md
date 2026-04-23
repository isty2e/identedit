# Language Support Reference

## Bundled Languages

Bundled grammars work out of the box:

Python, JavaScript/JSX, TypeScript/TSX, Rust, Go, C, C++, Java, Kotlin, Ruby, C#, Swift, PHP, Perl, Lua, Bash, Zsh, Fish, HTML, CSS, SCSS, Markdown, JSON, YAML, TOML, XML, Protobuf, SQL, HCL (Terraform), Dockerfile

## Dynamic Grammar Install

Any language with a tree-sitter grammar can be added with `identedit grammar install`.

Host support:
- `grammar install` currently works on macOS and Linux hosts.
- On Windows hosts, use bundled grammars or copy artifacts built on macOS/Linux.

Registry languages need no options:

```bash
identedit grammar install elixir
identedit grammar install zig
identedit grammar install dart
```

Registry includes Elixir, Elm, Erlang, Haskell, Julia, Scala, Zig, Dart, OCaml, Clojure, F#, Fortran, Groovy, CUDA, R, Svelte, Vue, Astro, Nix, Racket, Scheme, Solidity, Typst, Pascal, Common Lisp, and more.

Convention languages require `--ext` and use repo auto-detection:

```bash
identedit grammar install somelang --ext xyz
```

This works when the grammar repo follows `tree-sitter/tree-sitter-{lang}` or `tree-sitter-grammars/tree-sitter-{lang}` naming.

Custom grammars require an explicit repo:

```bash
identedit grammar install mylang --repo https://github.com/user/tree-sitter-mylang --ext ml
```
