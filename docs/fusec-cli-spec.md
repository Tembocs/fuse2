# `fusec` CLI — Command-Line Interface Specification

> **Status:** Ready for implementation  
> **Applies to:** Stage 1 (`stage1/fusec/src/main.rs`) and Stage 0 (`stage0/src/main.py`)  
> **Principle:** The CLI is part of the language. Its output is read by developers every day. It must meet the same standard as the compiler's error messages — clear, honest, and respectful of the developer's time.

---

## Overview

The current `fusec` entry point handles two cases and exits immediately on anything else. This specification defines the complete CLI surface that will be implemented now: all flags, their exact semantics, exit codes, and output format rules.

The design is grounded in Fuse's core commitment: **developer experience is a first-class concern**. That applies to compilation errors, yes — but equally to the shell experience. A developer who can't remember a flag should learn what they need in ten seconds. A CI pipeline that gets a non-zero exit code should know exactly why.

---

## Design Principles

**1. The happy path is silent.**  
When `fusec` succeeds — whether checking, compiling, or running — it produces no output unless the program itself produces output. Silence means success. Noise means there is something to read.

**2. Error output goes to stderr. Program output goes to stdout.**  
Diagnostics, usage errors, and warnings are written to `stderr`. The output of an interpreted or compiled program is written to `stdout`. These streams are never mixed.

**3. Structured, coloured output when attached to a terminal; plain text otherwise.**  
When stdout/stderr is a TTY, the compiler uses ANSI colours and Unicode box-drawing to make output scannable. When piped or redirected, it emits plain text that tools can parse. This is the `--color auto` default.

**4. Every non-zero exit has a reason visible on stderr.**  
A bare exit code is useless. If `fusec` exits with a non-zero code, the last line written to stderr explains why. CI logs are often only the last N lines; the reason is always there.

**5. Flags are consistent across modes.**  
`--color`, `--error-format`, and `--deny-warnings` mean the same thing regardless of whether you are checking, compiling, or running. There are no mode-specific flag namespaces.

**6. No flags are hidden.**  
There are no undocumented flags, no "internal" options, no Easter eggs that become load-bearing. Everything `fusec` accepts is in `--help`.

---

## Invocation Forms

```
fusec --help
fusec --version
fusec --check   <file.fuse>        [options]
fusec --run     <file.fuse>        [options]
fusec --repl                       [options]
fusec --emit    <stage> <file.fuse>[options]
fusec           <file.fuse> -o <out>[options]
```

Every form that takes a file accepts exactly one file. Multi-file compilation is not in scope for this implementation — the error message for multiple files must be clear about this (`fusec: multi-file compilation is not yet supported; import between modules is handled by the compiler`).

---

## Flags Reference

### Meta flags

#### `--help` / `-h`

Prints the help text to **stdout** and exits **0**.

Help is not an error. Programs that write help to stderr and exit non-zero are hostile to scripts. `fusec --help | head` should work.

The help text format is described in detail in the [Help Output Format](#help-output-format) section below.

#### `--version` / `-V`

Prints a single line to **stdout** and exits **0**:

```
fusec 0.1.0
```

The version is read from `Cargo.toml` at compile time via `env!("CARGO_PKG_VERSION")` — it is never hardcoded in source. Stage 0 reads it from a `__version__` constant in `main.py` that is kept in sync with `pyproject.toml`.

---

### Pipeline flags

These flags control how far along the compilation pipeline `fusec` goes, and what it does with the result.

#### `--check <file.fuse>`

Runs the full pipeline up to and including the checker — lex, parse, HIR lower, type-check, ownership-check, exhaustiveness — and then stops. No code is generated and no binary is produced.

**Exit 0:** The file is clean. No output is produced.  
**Exit 1:** One or more diagnostics were emitted. They are written to stderr.

This is the fastest feedback loop. Editors, language servers, and pre-commit hooks should use `--check`.

The checker runs every pass before reporting — all errors in a file are shown together, not just the first. This respects the developer's time: fix four errors, recheck, fix two more is not an acceptable workflow when all six could have been shown at once.

#### `--run <file.fuse>`

Runs the full checker and then interprets the program using the tree-walking evaluator. This is Stage 0's primary mode and Stage 1's quick-execution path.

**Exit 0:** The program ran and exited cleanly (or returned `unit`).  
**Exit 1:** The file failed to check, or the program raised an unhandled error at runtime.  
**Exit N:** If the program calls a hypothetical `exit(n)` builtin, the exit code is forwarded.

Program output is written to **stdout** exactly as the program produces it. No framing, no prefixes.

If the checker finds errors before interpretation begins, the interpreter does not run. Errors are shown and the process exits 1. A partially-checked program never starts executing.

#### `<file.fuse> -o <output>` (compile)

The default compile mode. Runs the full pipeline — lex, parse, HIR lower, check, codegen, link — and writes a native binary to `<output>`.

**Exit 0:** The binary was written. No output is produced.  
**Exit 1:** The file failed to check or compile. Diagnostics are written to stderr.

The `-o` flag is required in this mode. Omitting it is a usage error:

```
error: missing output path; use `-o <path>` to specify the output binary
```

Like `--check`, all diagnostics are collected before any are shown. Codegen does not begin if the checker found errors.

#### `--repl`

Starts an interactive read-eval-print loop. Does not take a file argument.

The REPL evaluates one statement or expression at a time. Ownership rules are enforced across lines — a value moved in one line cannot be used in a later line. The REPL state persists for the session.

The REPL prompt is:

```
fuse> _
```

On a successful expression evaluation that produces a non-unit value, the result is printed on the next line, unlabelled:

```
fuse> 2 + 2
4
fuse> _
```

On a type or ownership error, the error is shown and the REPL continues — the session is not ended:

```
fuse> val x: Int = "hello"
error[E0012]: type mismatch — expected `Int`, found `Str`
  --> <repl>:1:16
   |
 1 | val x: Int = "hello"
   |              ^^^^^^^ this is `Str`

fuse> _
```

Exit the REPL with `exit`, `quit`, or `Ctrl-D`.

---

### Emit flags

`--emit <stage> <file.fuse>` stops the pipeline at the named stage and prints the intermediate representation to **stdout**, then exits 0 (or exits 1 with diagnostics if an earlier stage failed).

The `--emit` flag is a development and debugging tool. Its output is not stable across compiler versions. Do not parse it programmatically.

#### `--emit tokens`

Prints the token stream, one token per line:

```
     1:1   KwVal          'val'
     1:5   Identifier     'x'
     1:6   Colon          ':'
     1:8   Identifier     'Int'
     1:12  Equals         '='
     1:14  IntLiteral     '42'
     1:16  Newline
```

Columns: line:col, token kind (fixed-width), raw text. Useful for verifying lexer position tracking.

#### `--emit ast`

Prints the AST as an indented tree. Each node shows its kind and, where relevant, its source span:

```
Module [1:1]
  FnDecl 'main' [1:1]
    Params []
    ReturnType Unit
    Body [2:3]
      ValDecl 'x' [2:3]
        TypeAnnotation Int
        Init
          IntLiteral 42 [2:12]
```

The indentation is two spaces per level. Spans are shown as `[line:col]`. This format is readable by a human and greppable by a script.

#### `--emit hir`

Prints the HIR after type information has been attached. Every expression node shows its resolved type:

```
FnDecl 'main' -> Unit
  ValDecl 'x': Int
    IntLiteral 42 :: Int
```

The `::` notation is deliberate — it mirrors the way type annotations would appear if they were written explicitly in source. Ownership qualifiers are shown where present:

```
FnDecl 'greet' (name: ref Str) -> Unit
  Call println :: Unit
    FStringExpr :: Str
      Ref 'name' :: ref Str
```

#### `--emit ir`

Prints the Cranelift IR text for each function in the file, separated by blank lines. This is the raw intermediate representation passed to Cranelift for code generation.

This output is Cranelift's native text format, unchanged. It is only available in Stage 1 (Stage 0 has no codegen). Requesting `--emit ir` from Stage 0 is a usage error with a clear message:

```
error: `--emit ir` is not available in Stage 0 (no codegen); use Stage 1 (`fusec`) to inspect IR
```

---

### Output control flags

#### `-o <path>`

Specifies the output path for the compiled binary. Only valid in compile mode. Required when compiling — omitting it is a usage error.

If the path's parent directory does not exist, `fusec` creates it. If the path already exists as a file, it is overwritten without warning. If the path already exists as a directory, that is an error.

#### `--color auto|always|never`

Controls ANSI colour and Unicode decoration in all diagnostic output.

| Value | Behaviour |
|---|---|
| `auto` | Use colour when stderr is a TTY; plain text otherwise. **Default.** |
| `always` | Always emit ANSI escape codes, even when piped. |
| `never` | Never emit ANSI escape codes. |

The `TERM=dumb` and `NO_COLOR` environment variables are respected under `auto`: if either is set, colour is disabled without requiring `--color never` explicitly.

#### `--error-format short|long`

Controls the verbosity of diagnostic messages.

| Value | Behaviour |
|---|---|
| `long` | Full context: file path, source line, caret, help text. **Default.** |
| `short` | One line per diagnostic: `file:line:col: error[E####]: message`. |

`short` is designed for editor integrations that parse diagnostic output. The format is stable and follows the pattern `<file>:<line>:<col>: <severity>[<code>]: <message>`. Severity is one of `error`, `warning`, or `note`.

---

### Diagnostic tuning flags

#### `--warn-unused`

Enables warnings for unused bindings, unused function parameters, and unused imports. Off by default — these are noisy in development, useful in CI and review.

When enabled, each unused binding produces a `warning[W0001]` with the name and location of the binding, and a suggestion:

```
warning[W0001]: unused binding `result`
  --> src/main.fuse:14:7
   |
14 |   val result = compute()
   |       ^^^^^^ defined here, never read
   |
   = note: prefix with `_` to suppress: `_result`
```

#### `--deny-warnings`

Causes any warning to be treated as an error. The exit code is 1 if any warnings are produced. Combine with `--warn-unused` in CI to enforce clean code.

`--deny-warnings` does not suppress warnings — they are still printed. It only changes the exit code. This means a developer can see exactly what triggered the failure.

---

## Diagnostic Output Format

The long-form diagnostic format is the primary user-facing output of the compiler. It is modelled on the clarity of Rust's diagnostics while being adapted to Fuse's simpler ownership model.

### Structure of a diagnostic

```
error[E0012]: type mismatch — expected `Int`, found `Str`
  --> src/main.fuse:14:16
   |
13 |   fn greet(name: ref Str) -> Unit {
14 |     val count: Int = name
   |                      ^^^^ this is `Str`
   |
   = help: did you mean to call `name.len()`?
```

Fields:

- **Severity label** — `error`, `warning`, or `note`. Coloured in terminal output (red, yellow, cyan).
- **Error code** — `E####` for errors, `W####` for warnings. Four digits, zero-padded. Stable across compiler versions.
- **Message** — One sentence. No trailing period. Written in plain English, not compiler jargon. The word "unexpected" never appears in an error message — it tells the developer nothing about what *was* expected.
- **Location arrow** — `-->` followed by `file:line:col`. Always present.
- **Context lines** — The source lines surrounding the error. One line of context before and after where possible. Line numbers are right-aligned to the width of the longest line number in the block.
- **Caret** — Points at the exact span that caused the error. `^` characters for spans, `_` for underlines, `|` for multi-line spans.
- **Help / note** — Indented `= help:` or `= note:` lines. Help is actionable. Notes are informational. Both are optional.

### Multiple diagnostics

All diagnostics for a file are shown together, separated by a blank line. They appear in source order (top of file first). The final line is a summary:

```
error: 3 errors found — compilation stopped
```

Or, when warnings are present alongside errors:

```
error: 2 errors, 1 warning found — compilation stopped
```

When `--check` finds no errors:

```
(nothing — silence means clean)
```

### Colour scheme (terminal mode)

| Element | Colour |
|---|---|
| `error` label and carets | Red (bold) |
| `warning` label and carets | Yellow (bold) |
| `note` label | Cyan |
| Error code (`E####`) | Red |
| Source line numbers and `|` margin | Dark grey (dim) |
| `-->` location arrow | Dark grey (dim) |
| Highlighted spans in source | Bold white |
| Help / note text | Normal |

Colours are implemented with ANSI SGR codes. No third-party crate is required — the full set of codes used fits in a small module.

---

## Exit Codes

| Code | Meaning |
|---|---|
| 0 | Success — the requested operation completed without error |
| 1 | Compile error, type error, ownership error, or runtime error |
| 2 | Usage error — bad flags, missing arguments, unrecognised option |

Exit code 2 is distinct from exit code 1 so that callers can distinguish "the compiler ran and found problems" from "the compiler was invoked incorrectly". CI scripts can alert differently on each.

Usage errors always print a short, specific message to stderr followed by the usage hint:

```
error: unexpected argument `--optimise`
       did you mean `--optimize`? (this flag is not yet implemented)

usage: fusec <file.fuse> -o <output>
   or: fusec --check <file.fuse>
   or: fusec --help for full usage
```

---

## Help Output Format

`fusec --help` prints to stdout:

```
fusec — the Fuse compiler

USAGE:
  fusec --check  <file.fuse>           Type-check only, no output
  fusec --run    <file.fuse>           Check and interpret
  fusec          <file.fuse> -o <out>  Compile to native binary
  fusec --repl                         Start interactive REPL
  fusec --emit   <stage> <file.fuse>   Print intermediate representation

EMIT STAGES:
  tokens    Token stream (after lexing)
  ast       Abstract syntax tree (after parsing)
  hir       High-level IR with type annotations (after HIR lowering)
  ir        Cranelift IR text (after codegen, Stage 1 only)

OPTIONS:
  -o <path>                     Output path for compiled binary (required in compile mode)
  --color auto|always|never     Diagnostic colour output (default: auto)
  --error-format short|long     Diagnostic verbosity (default: long)
  --warn-unused                 Warn on unused bindings, parameters, and imports
  --deny-warnings               Exit 1 if any warnings are produced

FLAGS:
  -h, --help                    Print this message and exit
  -V, --version                 Print version and exit

EXAMPLES:
  fusec --check src/main.fuse
  fusec src/main.fuse -o bin/main
  fusec --run examples/hello.fuse
  fusec --emit ast src/main.fuse
  fusec --check src/main.fuse --color never --error-format short
```

Rules for the help text:

- Columns are aligned. The flag column and description column are separated by at least two spaces. Alignment is computed at build time, not hardcoded with spaces.
- No line exceeds 100 characters.
- The `EXAMPLES` section is present and non-trivial. Examples are the fastest documentation.
- The help text is written to stdout, not stderr.
- `--help` exits 0.

---

## Implementation Notes

### `main.rs` structure

The current `main.rs` is a flat imperative function. After this implementation it should be structured as:

```
main()
  parse_args() -> Args | UsageError
  dispatch(args) -> ExitCode
    Mode::Help      -> print_help()
    Mode::Version   -> print_version()
    Mode::Check     -> run_check()
    Mode::Run       -> run_interpret()
    Mode::Compile   -> run_compile()
    Mode::Repl      -> run_repl()
    Mode::Emit      -> run_emit()
```

`parse_args` returns a typed `Args` struct, not a bag of strings. Every flag is represented as a typed field. Invalid combinations are caught in `parse_args`, not scattered through `dispatch`.

### No external CLI crates

The argument parser is hand-written. The full flag surface fits in under 150 lines of Rust. Adding `clap` or `argh` to the compiler's dependency tree would slow build times and introduce churn. Hand-written argument parsing also means error messages can be written exactly to the standard defined here, rather than adapted from a framework's defaults.

### Version string

```rust
const VERSION: &str = env!("CARGO_PKG_VERSION");
```

This is a compile-time constant. It is never read from a file at runtime.

### Stage 0 parity

Stage 0 (`fusec0`) implements the same flag set except for `--emit ir`, which requires codegen. All other flags — `--check`, `--run`, `--repl`, `--emit tokens`, `--emit ast`, `--version`, `--help`, `--color`, `--error-format`, `--warn-unused`, `--deny-warnings` — are present and behave identically. The developer experience of Stage 0 and Stage 1 is indistinguishable for the flags they share.

---

## What Is Explicitly Out of Scope

The following are not implemented now and should not be mentioned in `--help`:

- Multi-file compilation or a project manifest (`fuse.toml`)
- Watch mode (`--watch`)
- LSP / language server mode
- Profile-guided optimisation flags
- Cross-compilation target flags
- Incremental compilation cache control

These will be specified when they are ready to implement. Listing unimplemented flags in help text is a form of lying to the developer.

---

*End of specification.*
