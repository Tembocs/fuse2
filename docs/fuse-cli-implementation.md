# `fusec` CLI — Implementation Plan

> **Status:** In progress
> **Spec:** `docs/fusec-cli-spec.md`
> **Target:** `stage1/fusec/src/main.rs` (and supporting modules)
> **Purpose:** Make the CLI a real developer tool before stdlib work begins.
> The standard library will be built against this CLI — every mode exists
> because it will be used daily.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked

---

## Phase 1 — Arg Parsing Foundation

Goal: Replace the flat imperative `main()` with a typed args struct,
mode enum, and clean dispatch. Every flag the spec defines is parsed
here, even if the handler is a stub that prints "not yet implemented".

- [ ] **1.1** Define `Mode` enum: `Help`, `Version`, `Check`, `Run`,
      `Compile`, `Repl`, `Emit(EmitStage)`. Define `EmitStage` enum:
      `Tokens`, `Ast`, `Hir`, `Ir`.

- [ ] **1.2** Define `Args` struct with typed fields: `mode: Mode`,
      `file: Option<PathBuf>`, `output: Option<PathBuf>`,
      `color: ColorMode`, `error_format: ErrorFormat`,
      `warn_unused: bool`, `deny_warnings: bool`.

- [ ] **1.3** Implement `parse_args(args: &[String]) -> Result<Args, UsageError>`.
      Hand-written, no external crates. Handles all flags from the spec.
      Invalid combinations caught here, not scattered through dispatch.

- [ ] **1.4** Implement `dispatch(args: Args) -> ExitCode`. Matches on
      `args.mode` and calls the appropriate handler. Stub handlers for
      unimplemented modes print a clear message and exit 1.

- [ ] **1.5** Wire `main()` to call `parse_args` then `dispatch`.
      Usage errors (bad flags, missing args) print a specific message
      to **stderr** and exit **2** (not 1). The message includes a
      usage hint: `fusec --help for full usage`.

- [ ] **1.6** Test: no arguments → usage message on stderr, exit 2.
      Test: unknown flag `--foo` → `error: unexpected argument '--foo'`
      on stderr, exit 2. Test: `--check` without file → specific error,
      exit 2. Test: file without `-o` → specific error, exit 2.

---

## Phase 2 — Meta Commands

Goal: `--help` and `--version` work exactly as the spec defines.
These are trivial but they signal that the tool is real.

- [ ] **2.1** Implement `--help` / `-h`: print help text to **stdout**
      and exit **0**. Format matches the spec exactly — aligned columns,
      USAGE/EMIT STAGES/OPTIONS/FLAGS/EXAMPLES sections.

- [ ] **2.2** Implement `--version` / `-V`: print `fusec {version}` to
      **stdout** and exit **0**. Version read from `Cargo.toml` via
      `env!("CARGO_PKG_VERSION")` — never hardcoded.

- [ ] **2.3** Test: `--help` output matches spec format, goes to stdout,
      exit 0. Test: `-h` is an alias. Test: `--version` prints version
      line, stdout, exit 0. Test: `-V` is an alias.

---

## Phase 3 — Diagnostic Infrastructure

Goal: Build the color and diagnostic rendering modules that all modes
will use. This is the backbone of the developer experience.

- [ ] **3.1** Create `stage1/fusec/src/color.rs`. Implement ANSI SGR
      codes for: red bold, yellow bold, cyan, dim (grey), bold white,
      reset. Implement `ColorMode` enum (`Auto`, `Always`, `Never`).
      `Auto` checks stderr TTY status, `NO_COLOR` env, `TERM=dumb`.

- [ ] **3.2** Parse `--color auto|always|never` in `parse_args`.
      Default is `auto`. Invalid value → usage error, exit 2.

- [ ] **3.3** Define error code registry. Errors: `E0001` type mismatch,
      `E0002` ownership violation, `E0003` missing rank annotation,
      `E0004` rank order violation, `E0005` exhaustiveness failure,
      `E0006` unresolved name, `E0007` arity mismatch, etc.
      Warnings: `W0001` unused binding, `W0002` write guard across await.
      Codes are stable — document them in a const table.

- [ ] **3.4** Enhance `Diagnostic` struct in `error.rs` to carry:
      error code (`Option<String>`), severity enum (`Error`, `Warning`,
      `Note`), source text for context lines, and optional help text.

- [ ] **3.5** Implement long-format renderer: severity label + code +
      message, `-->` location arrow, context lines with line numbers,
      caret span (`^^^`), optional `= help:` / `= note:` lines.
      Coloured when `ColorMode` is active.

- [ ] **3.6** Implement short-format renderer: single line per diagnostic,
      `file:line:col: severity[code]: message`. Designed for editor
      integration.

- [ ] **3.7** Parse `--error-format short|long` in `parse_args`.
      Default is `long`. Invalid value → usage error, exit 2.

- [ ] **3.8** Implement summary line after all diagnostics:
      `error: N errors found — compilation stopped` or
      `error: N errors, M warnings found — compilation stopped`.

- [ ] **3.9** Test: long format matches spec layout. Test: short format
      matches `file:line:col: severity[code]: message`. Test: color
      codes present with `--color always`, absent with `--color never`.
      Test: `NO_COLOR=1` suppresses color under `--color auto`.

---

## Phase 4 — Core Modes

Goal: The three modes that matter most for daily stdlib development:
check, compile, and run. All diagnostics go to **stderr**. Program
output goes to **stdout**. Silence means success.

- [ ] **4.1** Enhance `--check`: collect all diagnostics, render to
      **stderr** using the format from Phase 3. Exit 0 on clean,
      exit 1 on any error. Support `--error-format` and `--color`.

- [ ] **4.2** Enhance compile mode (`<file> -o <out>`): diagnostics to
      **stderr**. Codegen does not begin if checker found errors. Exit 0
      on success (silent), exit 1 on error. Missing `-o` → exit 2.

- [ ] **4.3** Implement `--run <file>`: run the checker first — if errors,
      print diagnostics to stderr and exit 1 (interpreter does NOT run).
      If clean, run the tree-walking evaluator. Program output goes to
      **stdout**. Runtime errors go to **stderr** and exit 1.

- [ ] **4.4** Verify the evaluator (`evaluator.rs`) can be invoked from
      `main.rs` with a parsed+checked AST/HIR. Wire the call path:
      parse → lower → check → evaluate. If the evaluator entry point
      doesn't exist as a clean public API, create one in `lib.rs`.

- [ ] **4.5** Test `--check` on a clean file → exit 0, empty stderr.
      Test `--check` on a file with errors → diagnostics on stderr,
      exit 1. Test compile on clean file → binary produced, exit 0.
      Test `--run` on a hello-world → "hello" on stdout, exit 0.
      Test `--run` on a file with errors → diagnostics on stderr,
      no program output, exit 1.

---

## Phase 5 — Emit Modes

Goal: Inspection tools for every pipeline stage. These are essential
for debugging stdlib implementations — when something goes wrong in
the checker or codegen, `--emit` tells you exactly where.

- [ ] **5.1** Implement `--emit tokens <file>`: lex the file, print one
      token per line to **stdout**: `line:col  Kind  'text'`. Exit 0.
      If the file cannot be read, error on stderr, exit 1.

- [ ] **5.2** Implement `--emit ast <file>`: parse the file, print the
      AST as an indented tree to **stdout**. Two spaces per indent level.
      Each node shows kind and span `[line:col]`. Exit 0 on success,
      exit 1 on parse error (with diagnostics on stderr).

- [ ] **5.3** Implement `--emit hir <file>`: lower to HIR, print with
      resolved types. Expressions show `:: Type`. Ownership qualifiers
      shown where present. Exit 0 on success, exit 1 on error.

- [ ] **5.4** Implement `--emit ir <file>`: run full pipeline through
      codegen, capture Cranelift IR text for each function, print to
      stdout. Exit 0 on success, exit 1 on error.

- [ ] **5.5** Validate: `--emit` without a stage → usage error, exit 2.
      `--emit` with unknown stage (e.g. `--emit llvm`) → usage error.
      `--emit` without a file → usage error.

- [ ] **5.6** Test each emit stage on the milestone `four_functions.fuse`
      fixture. Verify output is non-empty and structurally correct.
      Test error cases (missing file, parse error) produce stderr
      diagnostics and exit 1.

---

## Phase 6 — REPL

Goal: Interactive exploration for quick experiments during stdlib work.
Not a full IDE — a focused evaluation loop.

- [ ] **6.1** Implement `--repl`: print `fuse>` prompt, read a line,
      parse+check+evaluate, print non-Unit results, loop. Ownership
      state persists across lines. Session ends on `exit`, `quit`,
      `Ctrl-D` (EOF), or `Ctrl-C`.

- [ ] **6.2** On check/eval error, print the diagnostic and continue
      the session — do not exit. The REPL is resilient to errors.

- [ ] **6.3** `--repl` must not accept a file argument. `--repl` with
      a file → usage error, exit 2.

- [ ] **6.4** Test: REPL evaluates `2 + 2` → prints `4`. Test: type
      error in REPL → error printed, session continues. Test: `exit`
      command ends the session.

---

## Phase 7 — Warning Flags

Goal: CI-ready warning control.

- [ ] **7.1** Implement `--warn-unused`: the checker scans for bindings
      that are declared but never referenced in the scope. Each produces
      `warning[W0001]: unused binding 'name'` with a note suggesting
      `_name` prefix to suppress.

- [ ] **7.2** Implement `--deny-warnings`: if any warnings were produced,
      exit 1 instead of 0. Warnings are still printed — only the exit
      code changes.

- [ ] **7.3** Test: `--warn-unused` on a file with unused bindings →
      warnings on stderr, exit 0. Test: `--warn-unused --deny-warnings`
      → same warnings, exit 1. Test: `--deny-warnings` alone (no
      `--warn-unused`) → no effect on a clean file.

---

## Phase 8 — Integration Testing

Goal: A comprehensive test suite that exercises every flag, mode, exit
code, and stdout/stderr routing. This suite is the contract — if a
future change breaks the CLI, these tests catch it.

- [ ] **8.1** Create `stage1/fusec/tests/cli_suite.rs`. Use the existing
      `harness.rs` infrastructure to invoke the `fusec` binary and
      capture stdout, stderr, and exit code.

- [ ] **8.2** Meta tests: `--help` stdout + exit 0, `-h` alias,
      `--version` stdout + exit 0, `-V` alias.

- [ ] **8.3** Usage error tests: no args → exit 2, unknown flag → exit 2,
      `--check` no file → exit 2, `--emit` no stage → exit 2,
      `--emit badstage file` → exit 2, `--color badvalue` → exit 2,
      file without `-o` → exit 2, `--repl` with file → exit 2.

- [ ] **8.4** Check mode tests: clean file → exit 0 + empty stderr,
      error file → exit 1 + diagnostics on stderr, `--error-format short`
      produces parseable one-line output.

- [ ] **8.5** Compile mode tests: clean file → exit 0 + binary exists,
      error file → exit 1 + no binary + diagnostics on stderr.

- [ ] **8.6** Run mode tests: hello-world → stdout output + exit 0,
      file with errors → no output + diagnostics + exit 1.

- [ ] **8.7** Emit mode tests: each stage produces non-empty stdout
      on a valid file.

- [ ] **8.8** Flag combination tests: `--check --color never`,
      `--check --error-format short`, `--check --deny-warnings`.

- [ ] **8.9** Run `cargo test` — all existing + new CLI tests green.
      Verify no regressions in the hardening test suites.

---

## Completion Summary

| Phase | Tasks | Status |
|-------|-------|--------|
| 1 — Arg parsing      | 6  | **done** |
| 2 — Meta commands    | 3  | **done** |
| 3 — Diagnostics      | 9  | **done** |
| 4 — Core modes       | 5  | **done** |
| 5 — Emit modes       | 6  | **done** |
| 6 — REPL             | 4  | **done** |
| 7 — Warning flags    | 3  | **done** |
| 8 — Integration tests| 9  | **done** |
| **Total**            | **45** | **done** |
