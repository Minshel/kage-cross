# Contributing to mold

`mold` is a small build-system project. Contributions should keep the language and implementation explicit rather than adding hidden conventions.

## Development setup

Clone the repository and build it with Cargo:

```sh
git clone https://github.com/drwxor/mold.git
cd mold

cargo build
cargo test
```

For a release-style build:

```sh
cargo build --release
```

Format the code with:

```sh
cargo fmt --all
```

There is currently no repository-specific CI workflow or declared minimum supported Rust version. Do not assume a particular MSRV unless the project documents one later.

## Source map

The implementation is intentionally split by responsibility:

```text
src/main.rs      command-line interface
src/lib.rs       top-level orchestration
src/parse.rs     lexer, parser, AST
src/expand.rs    expansion, quoting, hashing
src/graph.rs     dependency graph and incremental planner
src/depfile.rs   GCC depfile parser
src/exec.rs      execution and scheduling
src/error.rs     diagnostics
```

Keep changes in the narrowest appropriate module.

For example:

- parser changes belong in `parse.rs`;
- variable semantics belong in `expand.rs`;
- dirty-state decisions belong in `graph.rs`;
- process execution and scheduler behavior belong in `exec.rs`.

## Changing the language

Changes to the `build.mold` syntax should update all of these together:

1. parser implementation;
2. parser tests;
3. `SYNTAX.md`;
4. relevant README examples.

Avoid documenting a feature before its implementation exists.

When a syntax feature has intentionally incomplete semantics, document that limitation explicitly.

## Tests

Run:

```sh
cargo test
```

The existing parser tests cover examples such as:

- the specification example;
- comments;
- variables;
- missing semicolons;
- unknown arrays.

New syntax should normally come with a focused parser test.

Behavior involving the graph or executor should have a regression test where practical.

Prefer tests that describe observable semantics instead of implementation details.

## Build-file examples

Use small examples.

Good:

```mold
tool cc = clang;

instruction c {
    command: "$(cc) -c $in -o $out";
}

compile c src/main.c > build/main.o;
```

Avoid examples that depend on unrelated external software unless that dependency is the subject of the example.

## Error messages

Parser errors include file/line/column information.

When adding a new syntax error, prefer a message that tells the user:

- what was expected;
- what was actually invalid;
- where the problem occurred.

For graph/build errors, keep the `mold:` prefix and include the relevant path/target when possible.

## CLI changes

The CLI is defined in `src/main.rs`.

When adding an option:

- use existing clap conventions;
- give the option a clear long name;
- choose a short option only when it is useful;
- update the command-line table in `README.md`;
- add/update examples in the README when the feature is user-facing.

## Documentation rules

The documentation should distinguish between:

- implemented behavior;
- implementation limitations;
- proposed future work.

`README.md` should remain a practical entry point.

`SYNTAX.md` is the reference document.

## Generated files

A local build can create:

```text
target/
.mold_log
*.d
```

The current repository ignores `target/` and Rust backup files, but does not currently ignore `.mold_log` or depfiles globally.

Do not commit local build products.

## Pull requests

Keep patches focused.

A useful pull request should make it clear:

- what behavior changed;
- why the change is needed;
- which tests cover it;
- which documentation was updated.

Avoid mixing unrelated refactors with syntax or scheduler changes.

## Compatibility

There is currently no formal compatibility guarantee for the `build.mold` language.

Until one is introduced, document intentional syntax changes and keep them easy to identify in the changelog.
