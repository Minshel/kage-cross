# kage syntax

This document is the reference for the current `build.kage` language implemented by the parser in `src/parse.rs`.

The language is intentionally small. Statements end with `;`, blocks use `{ ... }`, lists use `[ ... ]`, and command templates use `$` expansion.

## 1. File structure

A build file is a sequence of statements:

```text
file := { statement }

statement :=
      tool
    | flags
    | var
    | array
    | instruction
    | compile
    | link
    | default
    | include
```

Statements may be separated by spaces or newlines.

Comments are supported in two forms:

```kage
# comment
// comment
```

Comments run to the end of the line.

## 2. Lexical rules

Unquoted words may contain the characters accepted by the lexer, including letters, digits, `_`, `.`, `/`, `-`, `+`, `@`, `~`, `%`, `$`, and parentheses in word contents.

Quoted strings use double quotes:

```kage
"some text"
```

String escapes currently recognized by the lexer:

| Escape | Result |
| --- | --- |
| `\n` | newline |
| `\t` | tab |
| `\r` | carriage return |
| `\"` | `"` |
| `\\` | `\` |
| `\$` | literal `$` |

Unknown escapes keep the backslash and following character.

Example:

```kage
description: "compile\t$in";
```

A semicolon is required after every top-level statement and every instruction field.

## 3. `tool`

Define a named executable:

```text
tool <name> = <value>;
```

Examples:

```kage
tool cc = clang;
tool asm = nasm;
tool ld = ld.lld;
tool python = python3;
```

The value is a single word or quoted string.

Use a tool in an expansion:

```kage
command: "$(cc) -c $in -o $out";
```

Tool names must be unique within the parsed build.

## 4. `flags`

Define a list of command-line arguments:

```text
flags <name> = [ <item> {, <item>} [,] ];
```

Examples:

```kage
flags cflags = [
    -Wall,
    -Wextra,
    -O2
];

flags ldflags = [];
```

A trailing comma is allowed:

```kage
flags cflags = [
    -Wall,
    -Wextra,
];
```

Flags are shell-quoted when expanded as a group.

For example:

```kage
flags cflags = [
    "-DNAME=value with spaces",
    -O2
];

instruction c {
    command: "$(cc) $(cflags) -c $in -o $out";
}
```

results in a safely separated command-line fragment.

## 5. `var`

Define a scalar variable:

```text
var <name> = <value>;
```

Example:

```kage
var builddir = build;
var output = build/app;
```

Use it as:

```kage
compile c src/main.c > $(builddir)/main.o;
```

The equivalent `$builddir` form is also accepted by the expansion engine.

Variable names use the identifier form:

```text
first character: ASCII letter or _
remaining characters: ASCII letter, digit, or _
```

## 6. `array`

Define a list variable:

```text
array <name> = [ <item> {, <item>} [,] ];
```

Example:

```kage
array objects = [];
```

Arrays can be expanded in commands:

```kage
command: "ar rcs $archive $objects";
```

The array is shell-quoted item by item and joined with spaces.

### Appending rule outputs

A `compile` or `link` statement can append its outputs to an existing array:

```kage
compile c src/main.c > build/main.o | objects;
compile c src/util.c > build/util.o | objects;
```

The array must already exist.

`|` does not create an array and does not replace its contents; it appends.

### Arrays in `link`

On a `link` statement, an input token that exactly matches a declared array name is expanded to all elements of that array:

```kage
link objects > build/app;
```

This expansion happens while the build graph is constructed.

Other array uses remain ordinary variable expansions.

## 7. `instruction`

Define a reusable command template:

```text
instruction <name> {
    command: <value>;
    [description: <value>;]
    [depfile: <value>;]
    [depformat: <value>;]
    [restat: <value>;]
}
```

`command` is mandatory.

Example:

```kage
instruction c {
    command: "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out";
    description: "CC $in";
    depformat: gcc;
}
```

### `command`

The command is expanded after `$in`, `$out`, `$in_newline`, and `$depfile` are known.

Commands are executed as:

```text
/bin/sh -c <expanded command>
```

The shell's current directory is the kage working directory.

### `description`

Optional progress text.

If omitted:

- non-link instructions default to `<instruction> <first-input>`;
- the `link` instruction defaults to `link <first-output>`.

`-v` prints the full expanded command instead of the description.

### `depformat`

Supported values:

```kage
depformat: gcc;
depformat: none;
```

`gcc` enables reading a GCC/Make-style depfile on the next planning pass.

`none` disables depfile tracking for the instruction.

### `depfile`

Optional path for the dependency file.

Example:

```kage
instruction c {
    command: "$(cc) -MMD -MF $depfile -c $in -o $out";
    depformat: gcc;
    depfile: build/deps/$out.d;
}
```

When `depformat: gcc` is used without an explicit `depfile`, the current implementation defaults it to:

```text
<first-output>.d
```

The depfile itself is expanded before the special variables are installed for the command, so the `depfile` field should use normal build variables rather than `$in`/`$out`.

### `restat`

Accepted boolean values:

```text
true
false
yes
no
1
0
```

The parser stores this option, but the current executor/planner does not use it to change rebuild behavior. Treat it as reserved/incomplete functionality until its semantics are implemented.

## 8. `compile`

Syntax:

```text
compile <instruction> <input>... [@ <order-only-input>...] > <output>... [| <array>];
```

Example:

```kage
compile c src/main.c > build/main.o;
```

Multiple inputs and outputs are allowed:

```kage
compile merge a.txt b.txt > build/out.bin;
```

At least one input and one output are required.

### Order-only inputs

Use `@` to introduce order-only inputs:

```kage
compile c src/main.c @ generated_headers > build/main.o;
```

Order-only inputs participate in dependency ordering and must exist or have a producer, but they do not make the rule dirty merely because they have a newer mtime than the output.

### Appending outputs

Use `|` to append outputs to an existing array:

```kage
array objects = [];

compile c src/a.c > build/a.o | objects;
compile c src/b.c > build/b.o | objects;
```

The array must already be defined.

## 9. `link`

Syntax:

```text
link <input-or-array>... [@ <order-only-input>...] > <output>... [| <array>];
```

Example:

```kage
link objects > build/app;
```

Unlike `compile`, a `link` statement always uses the instruction named exactly `link`:

```kage
instruction link {
    command: "$(cc) $in -o $out";
}
```

If that instruction does not exist, graph construction fails.

Input tokens matching declared arrays are expanded to their members before graph creation.

## 10. `default`

Define the targets built when no explicit target is supplied:

```text
default <target>...;
```

Example:

```kage
default build/app;
```

More than one default target is allowed:

```kage
default build/app build/tests;
```

When at least one `default` statement exists, those targets are used.

When no explicit defaults exist, kage automatically chooses produced outputs that have no consumers in the graph.

If no targets can be resolved, `kage` reports `no targets to build`.

## 11. `include`

Include another build file:

```text
include <path>;
```

Example:

```kage
include config/common.kage;
```

Relative paths are resolved relative to the file containing the `include` statement.

Absolute paths are also accepted.

Include cycles are detected and reported as errors.

Definitions from included files are added to the same build description. Redefining an existing tool, flags group, variable, array, or instruction is rejected.

## 12. Variable expansion

The expansion engine supports all of these forms:

```text
$name
$(name)
${name}
```

Example:

```kage
tool cc = clang;

instruction c {
    command: "$(cc) -c $in -o $out";
}
```

These forms are equivalent for normal identifiers:

```text
$cc
$(cc)
${cc}
```

A literal dollar sign is written as:

```text
$$
```

A dangling or malformed variable reference is an error in cases where a reference is recognized as such.

### Lookup precedence

The expansion engine checks names in this order:

1. special variables;
2. tools;
3. flags;
4. scalar variables;
5. arrays.

Therefore a user-defined variable cannot override a special variable such as `$in`.

An unknown variable is an error.

## 13. Special variables

The build graph supplies these variables when expanding an instruction command or description.

### `$in`

All normal inputs, shell-quoted and joined with spaces.

```kage
command: "tool $in -o $out";
```

### `$out`

All outputs, shell-quoted and joined with spaces.

```kage
command: "tool $in -o $out";
```

### `$in_newline`

Inputs, one per line, with each input individually shell-quoted.

This is useful for commands that want a newline-separated list.

### `$depfile`

The expanded depfile path.

It is shell-quoted when used in a command.

If no depfile exists for the instruction, it expands to an empty string.

## 14. Quoting and shell safety

Tools, flags, variables, arrays, and special path variables are converted into shell-safe fragments before being inserted into a command.

A value containing only:

```text
A-Z a-z 0-9 - _ . / + = : @ %
```

does not require additional quoting.

Other values are wrapped in single quotes, with embedded single quotes escaped for POSIX shell syntax.

This is important for spaces and shell metacharacters in file names and flag values.

The command itself is still arbitrary shell code. `kage` does not parse the command into individual argv elements.

## 15. Paths and working directory

A relative path is interpreted relative to kage's working directory.

By default the working directory is the current directory.

With:

```sh
kage -C path/to/project
```

the selected directory becomes the build working directory.

Build-file paths given with `-f` are resolved relative to that working directory when they are not absolute.

## 16. Incremental semantics

For each wanted edge, kage may rebuild when:

- an output is missing;
- a GCC depfile is missing;
- an output has no recorded command;
- the expanded command hash differs from the previous build;
- a normal input is dirty;
- a normal or implicit input is newer than the output;
- an implicit GCC depfile input is missing;
- `-B` is active.

The current command hash is a 64-bit FNV-1a-style hash of the fully expanded command string. It is a change detector, not a cryptographic integrity mechanism.

## 17. GCC depfiles

With:

```kage
instruction c {
    command: "$(cc) -MMD -MF $depfile -c $in -o $out";
    depformat: gcc;
}
```

kage reads the generated depfile on the next invocation.

The parser supports Make-style:

- target/dependency separation;
- escaped spaces;
- backslash-newline continuations;
- multiple targets;
- comments beginning at the start of a logical line.

Dependencies are normalized enough for path comparison and are deduplicated/sorted before use.

## 18. Grammar summary

A compact grammar for the current language:

```text
file            := { statement }

statement       := tool
                 | flags
                 | var
                 | array
                 | instruction
                 | compile
                 | link
                 | default
                 | include

tool            := "tool" name "=" word ";"

flags           := "flags" name "=" "[" [ item { "," item } [ "," ] ] "]" ";"

var             := "var" name "=" word ";"

array           := "array" name "=" "[" [ item { "," item } [ "," ] ] "]" ";"

instruction     := "instruction" name "{"
                       "command" ":" word_or_string ";"
                       { field }
                    "}"

field           := "description" ":" word_or_string ";"
                 | "depfile" ":" word_or_string ";"
                 | "depformat" ":" ("gcc" | "none") ";"
                 | "restat" ":" boolean ";"

compile         := "compile" name inputs
                   [ "@" order_only_inputs ]
                   ">"
                   outputs
                   [ "|" name ]
                   ";"

link            := "link" link_inputs
                   [ "@" order_only_inputs ]
                   ">"
                   outputs
                   [ "|" name ]
                   ";"

default         := "default" name { name } ";"

include         := "include" word_or_string ";"

boolean         := "true" | "false" | "yes" | "no" | "1" | "0"

name            := identifier
item            := word_or_string
inputs          := word_or_string { word_or_string }
order_only_inputs
                := word_or_string { word_or_string }
outputs         := word_or_string { word_or_string }
link_inputs     := word_or_string { word_or_string }
```

The grammar above describes the accepted surface syntax; semantic checks such as duplicate definitions, missing instructions, missing arrays, dependency cycles, and multiple producers are performed later.

## 19. Semantic constraints

The current implementation rejects, among other cases:

- missing statement semicolons;
- unknown top-level statements;
- duplicate tool/flags/var/array/instruction definitions;
- instructions without `command`;
- unknown instruction fields;
- unknown `depformat` values;
- invalid `restat` values;
- `compile` without an input or output;
- `link` without an input or output;
- appending to an unknown array;
- unknown compile instruction;
- `link` without an `instruction link { ... }`;
- multiple edges producing the same output;
- dependency cycles;
- missing required input files;
- unknown requested/default targets;
- include cycles;
- malformed variable references.

Diagnostics include file, line, and column information for parser errors.
