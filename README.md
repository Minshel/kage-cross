# mold

A fast, Ninja-inspired build system with a small declarative DSL.

`mold` reads a `build.mold` file, builds a dependency graph, plans only the work that is actually dirty, and executes ready jobs in parallel.

The project is written in Rust and is intentionally small: the build language describes tools, flags, instructions, dependencies, and outputs without introducing a large general-purpose configuration language.

## Status

`mold` is under active development.

The current implementation already provides:

- a lexer/parser for `build.mold`;
- tools, flags, variables, arrays, instructions, includes, defaults, compile rules, and link rules;
- `$in`, `$out`, `$in_newline`, and `$depfile` special variables;
- shell-safe expansion for command arguments;
- dependency graphs with cycle detection;
- incremental rebuilds based on output existence, mtimes, GCC depfiles, and command hashes;
- parallel execution with `-j`;
- `--dry-run`, `--verbose`, `--quiet`, `--keep-going`, `--always-make`, `--explain`, `--clean`, `--list-targets`, and `--compdb`;
- a persistent `.mold_log` used to detect command-line changes.

The package version in the current repository is `26.9.18`. No stable release/compatibility policy is currently declared.

## Why mold?

Traditional build descriptions often become a mixture of shell fragments, implicit conventions, and build-system-specific syntax.

`mold` keeps the build description explicit:

```mold
tool cc = clang;

flags cflags = [
    -Iinclude,
    -Wall,
    -Wextra,
    -O2
];

instruction c {
    command: "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out";
    depformat: gcc;
}

instruction link {
    command: "$(cc) $in -o $out";
}

array objects = [];

compile c src/main.c > build/main.o | objects;
compile c src/util.c > build/util.o | objects;

link objects > build/app;
```

The build graph comes directly from these statements. There is no separate rule registry or implicit file-name convention.

## Installation

### Build from source

`mold` is a Cargo project using Rust edition 2024.

```sh
git clone https://github.com/drwxor/mold.git
cd mold

cargo build --release
```

The resulting binary is:

```text
target/release/mold
```

You can copy it into a directory on your `PATH` manually, or use the normal Cargo installation workflow:

```sh
cargo install --path .
```

No project-specific installer or package definition is currently included in the repository.

### Runtime requirements

Build commands are executed through:

```text
/bin/sh -c <command>
```

Therefore a Unix-like environment with `/bin/sh` is expected by the current executor.

## Quick start

Create a project:

```text
example/
  build.mold
  src/
    main.c
    util.c
  include/
```

A minimal `build.mold`:

```mold
tool cc = cc;

flags cflags = [
    -Wall,
    -Wextra,
    -O2
];

instruction c {
    command: "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out";
    depformat: gcc;
}

instruction link {
    command: "$(cc) $in -o $out";
}

array objects = [];

compile c src/main.c > build/main.o | objects;
compile c src/util.c > build/util.o | objects;

link objects > build/app;

default build/app;
```

Build it:

```sh
mold
```

Build a named target:

```sh
mold build/app
```

Run with four jobs:

```sh
mold -j4
```

Show the commands instead of running them:

```sh
mold -n
```

Force a rebuild:

```sh
mold -B
```

Explain why targets are dirty:

```sh
mold --explain
```

Remove generated outputs, depfiles, and the build log:

```sh
mold --clean
```

List all produced targets:

```sh
mold --list-targets
```

Generate a compilation database to stdout:

```sh
mold --compdb > compile_commands.json
```

## Command line

```text
mold [OPTIONS] [TARGET...]
```

### Build-file and directory selection

| Option | Meaning |
| --- | --- |
| `-f, --file <FILE>` | Build file to read. Default: `build.mold`. |
| `-C, --directory <DIR>` | Use `DIR` as the working directory before loading the build file. |

### Parallel execution

| Option | Meaning |
| --- | --- |
| `-j, --jobs <N>` | Maximum number of jobs. Defaults to the host's available parallelism. |
| `-k, --keep-going` | Continue scheduling independent jobs after a failure. |

### Build behavior

| Option | Meaning |
| --- | --- |
| `-B, --always-make` | Mark wanted edges dirty and rebuild them. |
| `-n, --dry-run` | Print scheduled jobs without executing their commands. |
| `-v, --verbose` | Print full expanded commands instead of descriptions. |
| `-q, --quiet` | Suppress normal progress and summary output. |
| `--explain` | Print the reason each dirty job needs a rebuild. |
| `--clean` | Remove declared outputs, depfiles, and `.mold_log`. |

### Inspection and tooling

| Option | Meaning |
| --- | --- |
| `--list-targets` | Print produced targets and exit. |
| `--compdb` | Print a JSON compilation database and exit. |
| `--color <WHEN>` | Color mode: `auto`, `always`, or `never`. The implementation also accepts common aliases such as `on/off/yes/no`. |

Targets can be supplied after the options:

```sh
mold build/app
mold build/main.o
mold app
```

A unique produced output can be addressed by its full path or by a unique matching suffix/file name.

## Incremental builds

The planner considers a rule dirty when one of the relevant conditions is true:

- an output does not exist;
- a GCC depfile required by the rule does not exist;
- the output has no recorded command in `.mold_log`;
- the expanded command changed;
- an explicit input is dirty;
- an explicit input is newer than an output;
- an implicit input from a GCC depfile is missing;
- an implicit input is newer than an output;
- `-B` was supplied.

For GCC depfiles, headers reported by the compiler become implicit inputs for the next planning pass.

The build log is stored as `.mold_log` in the working directory. It records a 64-bit hash of the fully expanded command for each output.

## Dependency ordering

Normal inputs and order-only inputs both participate in graph traversal and job ordering.

This form:

```mold
compile c src/main.c @ generated_headers > build/main.o;
```

means that `generated_headers` must be available before the compile job may run.

The `@` inputs are not used by the timestamp comparison for the rule itself; they are ordering prerequisites. Their own producer edges can still be built first.

## Instructions

An instruction defines the command template used by a group of build statements:

```mold
instruction c {
    command: "$(cc) $(cflags) -c $in -o $out";
    description: "CC $in";
}
```

Supported instruction fields are:

- `command` — required;
- `description` — optional human-readable progress text;
- `depfile` — optional explicit depfile path;
- `depformat` — `gcc` or `none`;
- `restat` — parsed as a boolean, but currently retained without changing execution behavior.

See [SYNTAX.md](SYNTAX.md) for the full language reference.

## Arrays

Arrays are useful for collecting outputs:

```mold
array objects = [];

compile c src/a.c > build/a.o | objects;
compile c src/b.c > build/b.o | objects;
compile c src/c.c > build/c.o | objects;

link objects > build/app;
```

The `| objects` form appends the rule's outputs to an already-declared array.

On a `link` statement, an input token matching a declared array name expands to the array's contents:

```mold
link objects > build/app;
```

Arrays can also be expanded inside commands:

```mold
command: "ar rcs $archive $objects";
```

## Includes

Build files can include other build files:

```mold
include config.mold;
```

Relative include paths are resolved relative to the file containing the `include` statement.

Include cycles are rejected.

## Compilation database

`--compdb` prints JSON for compile-style edges and skips the special `link` instruction.

Example:

```sh
mold --compdb > compile_commands.json
```

Each generated entry contains:

- `directory`;
- `command`;
- `file`;
- `output`.

## Generated files

The current implementation creates these files as part of normal builds:

```text
.mold_log
<output>.d          # when depformat is gcc and no custom depfile is set
```

The repository's current `.gitignore` does not ignore these files automatically. Projects using `mold` may want to add them to their own ignore rules.

## Source layout

```text
src/
├── main.rs      CLI
├── lib.rs       orchestration and public API
├── parse.rs     lexer, parser, AST
├── expand.rs    variable expansion, quoting, command hashing
├── graph.rs     dependency graph, dirty checking, build log
├── depfile.rs   GCC depfile parsing
├── exec.rs      job execution and parallel scheduler
└── error.rs     diagnostics and error types
```

See [DESIGN.md](DESIGN.md) for how these pieces fit together.

## Documentation

- [SYNTAX.md](SYNTAX.md) — complete `build.mold` language reference.
- [CONTRIBUTING.md](CONTRIBUTING.md) — development workflow and contribution notes.

## License

Licensed under the GNU Lesser General Public License, version 3.

See [LICENSE](LICENSE).
