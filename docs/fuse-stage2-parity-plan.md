# Fuse Stage 2 — Parity & Bootstrap Completion Plan

> **Status:** Not started.
> **Scope:** 13 waves, 45 phases, ~170 granular tasks.
> **Goal:** Close every compiler gap that currently prevents `fusec` from
> building `stage2/src/main.fuse` end-to-end, then verify T4 Parity and
> T5 Bootstrap on the real Stage 2 binary — with zero regressions, zero
> workarounds, and zero compromises.
>
> **Prerequisite:** Stage 1 compiler complete (W0-W8), stdlib complete,
> `tests/stage2/known_failures.txt` empty. (All satisfied as of 2026-04-10.)
>
> **Companion documents:**
> - [docs/fuse-language-guide-2.md](fuse-language-guide-2.md) — authoritative language spec
> - [docs/fuse-stage2-plan.md](fuse-stage2-plan.md) — original Stage 2 self-hosting plan (W0-W7 implemented)
> - [docs/fuse-stage2-test-plan.md](fuse-stage2-test-plan.md) — 440+ fixture test plan
> - [docs/t4-parity-investigation.md](t4-parity-investigation.md) — post-mortem that motivated this plan
> - [docs/learning.md](learning.md) — bug log (L001-L025 resolved)

---

## Language Philosophy (Non-Negotiable)

Fuse is a production language. Every decision in this plan must serve the
three pillars:

1. **Memory safety without garbage collection.** ASAP destruction is
   deterministic. Values are destroyed at their last use point. Every
   codegen fix must preserve the ASAP invariant — no leaks, no double-free,
   no use-after-free.

2. **Concurrency safety without a borrow checker.** Ownership conventions
   (`ref`, `mutref`, `owned`, `move`) are enforced at function signatures.
   `Shared<T>` with ranked locking prevents races and deadlocks. Fixes to
   the codegen must not weaken checker guarantees or bypass ownership
   tracking.

3. **Developer experience as a first-class concern.** Error messages are
   features. Compiler output must be clear, deterministic, and actionable.
   Every diagnostic added by this plan must point at the exact source
   location with a specific fix hint.

**If a fix violates a pillar, the fix is wrong — regardless of how quickly
it would unblock the next phase.**

---

## Mandatory Rules

> These rules apply to every wave and every phase in this document.
> They are intentionally restated and expanded beyond the parent
> [fuse-stage2-plan.md](fuse-stage2-plan.md) rules because the
> investigation that motivated this plan exposed places where shortcuts
> were considered.

### Rule 1 — NO CORNERS CUT

**Every fix is a proper fix.** No workarounds. No "patch the test
fixture." No "rewrite the Stage 2 source to avoid the compiler bug."
No "let's try it and see." No TODO comments promising a later fix.

If a gap is real, the fix lands in the compiler, the runtime, the parser,
or the checker — whichever layer is wrong. Fuse is a production compiler.
Production compilers do not ship bugs hidden behind rewritten tests.

Explicitly forbidden responses to any issue encountered during this plan:

- Rewriting `stage2/src/*.fuse` to avoid a codegen gap (for example,
  adding explicit type annotations to dodge match-as-expression inference).
- Replacing a method call with a manual loop because the method's
  codegen path is broken.
- Deleting or weakening an assertion in a test fixture.
- Adding a TODO comment in place of a fix.
- Introducing a feature flag or build-time switch to enable the buggy path
  only when convenient.
- "Mocking out" `fusec2` in `run_tests.py` so T4 Parity appears to pass.
- Accepting a non-deterministic test as "flaky" — if it fails sometimes,
  it is a bug, and the root cause must be found.

The only legitimate way to close a phase is to fix the compiler and have
every test in the affected region pass reproducibly.

### Rule 2 — Solve Problems Immediately

If a new bug is discovered while executing any phase, stop and fix it in
the same phase. Do not defer. Do not create a "we'll come back to this"
entry. Every incidental gap becomes a numbered task inside the current
phase, with its own checklist entry. If the gap is large enough to
warrant its own phase, insert a new phase at the current position and
renumber.

### Rule 3 — Ground Every Fix in the Code

Before writing any fix, the engineer must cite the exact file and line
range being changed. Before declaring a fix correct, the engineer must
cite the test that exercises it. "It probably works" is not acceptable.
Root-cause each symptom by reading the code path end-to-end.

### Rule 4 — Zero Regressions

After every phase, run:

1. `cargo test -p fusec` in `stage1/`
2. `cargo test -p fuse-runtime` in `stage1/`
3. `cargo test -p fuse-lsp` in `stage1/`
4. `python tests/stage2/run_tests.py`
5. `python tests/stage2/run_tests.py --filter m_memory`

All five must be green. If any test that was passing before the phase is
now failing, the phase is not done. Investigate and fix.

### Rule 5 — Determinism is Load-Bearing

Any code path in the compiler that can produce different outputs on
different runs (HashMap iteration, filesystem order, unseeded randomness)
is a bug. This plan replaces `HashMap` with `BTreeMap` in Wave B1 and
audits for other sources of nondeterminism.

### Rule 6 — Diagnostics Are First-Class

Every new error path must emit a diagnostic with:

- A specific message (not "unsupported X")
- The exact span of the offending token
- A fix hint when the fix is obvious

Silent failure — where the checker lets code through and the codegen
crashes downstream — is the single biggest class of bug in the
investigation that motivated this plan. Close the silence.

### Rule 7 — Document What You Learn

Every bug fixed in this plan gets an `L###` entry in
[docs/learning.md](learning.md): what went wrong, why, and how it was
fixed, with file:line citations. Institutional knowledge survives the
fix.

### Rule 8 — Phase Completion Standard

A phase is done when, and only when:

1. Every checkbox in the phase is `[x]`
2. The deliverables listed in the phase are present in the tree
3. The success criteria for the phase all hold
4. Rule 4 (zero regressions) is satisfied
5. Rule 7 (document what you learn) has been followed
6. This document has been updated — phase status `[x]`, wave header
   status advanced, Task Summary table refreshed, commit hash noted

No phase is "done" because "it should work." It is done when it is
demonstrably, reproducibly correct.

### Rule 9 — Commit at Phase Boundaries

Each phase ends with a commit whose message names the phase ID and
summarizes the fix. Multi-task phases may have intermediate commits.
Phase-boundary commits are the smallest unit this plan's progress can be
audited against.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked (must state what blocks it)

---

## Architecture Overview

### What this plan fixes

The investigation in [docs/t4-parity-investigation.md](t4-parity-investigation.md)
identified six underlying gaps plus three incidental ones that together
prevent `stage1 fusec stage2/src/main.fuse -o fusec2.exe` from succeeding.
This plan addresses all nine:

| Gap | Layer | Fixed in |
|-----|-------|----------|
| Silent unresolved extension methods | Checker | B2 |
| Generic return types not substituted at extension call sites | Codegen | B4 |
| Hardcoded specialization runs after extension resolution | Codegen | B5 |
| User-defined enum variant payload types discarded | Parser + AST + Codegen | B3 + B6 |
| Match-as-expression arm types not unified | Codegen + Checker | B7 |
| Non-deterministic `BuildSession.modules` iteration | Codegen | B1 |
| `module.Type.staticMethod` namespace calls unsupported | Codegen | B8 |
| Tuple field access on unknown tuple type | Codegen | B9 |
| F-string `{{` / `}}` literal brace escape | Lexer | B10 |

Plus: missing `stdlib.core.{list,option,result}` imports in
`stage2/src/*.fuse` (addressed in B11 after the checker gap in B2 is
closed, so the checker — not a reviewer — drives the fix).

### What this plan does NOT change

- **The language semantics.** Nothing in this plan redesigns the ownership
  model, the type system, or the concurrency model. Every fix lands
  inside the existing Fuse Core specification.
- **The module layout of `stage2/src/`.** The self-hosted compiler's
  architecture stays as it is.
- **Post-Stage 2 features.** `dyn` dispatch, operator overloading, green
  threads, GPU, and the other features deferred in
  [docs/fuse-post-stage2.md](fuse-post-stage2.md) remain deferred.

### The key code sites (cited so the plan is auditable)

| Location | Role |
|----------|------|
| [stage1/fusec/src/codegen/object_backend.rs:130](../stage1/fusec/src/codegen/object_backend.rs#L130) | `BuildSession.modules` HashMap (nondeterministic) — B1 |
| [stage1/fusec/src/checker/mod.rs:1312-1356](../stage1/fusec/src/checker/mod.rs#L1312-L1356) | Checker silently drops unresolved extension methods — B2 |
| [stage1/fusec/src/ast/nodes.rs:153-158](../stage1/fusec/src/ast/nodes.rs#L153-L158) | `EnumVariant` missing `payload_types` — B3 |
| [stage1/fusec/src/parser/parser.rs:447-465](../stage1/fusec/src/parser/parser.rs#L447-L465) | `parse_enum` discards `parse_type_name` result — B3 |
| [stage1/fusec/src/codegen/object_backend.rs:3454-3457](../stage1/fusec/src/codegen/object_backend.rs#L3454-L3457) | Extension call returns raw `function.return_type` — B4 |
| [stage1/fusec/src/codegen/type_names.rs](../stage1/fusec/src/codegen/type_names.rs) | Generic helpers — needs `substitute_generics` — B4 |
| [stage1/fusec/src/codegen/object_backend.rs:3437-3514](../stage1/fusec/src/codegen/object_backend.rs#L3437-L3514) | `compile_member_call` dispatch ordering — B5 |
| [stage1/fusec/src/codegen/object_backend.rs:4688-4702](../stage1/fusec/src/codegen/object_backend.rs#L4688-L4702) | `bind_pattern` only knows Ok/Err/Some — B6 |
| [stage1/fusec/src/codegen/object_backend.rs:4290-4335](../stage1/fusec/src/codegen/object_backend.rs#L4290-L4335) | `compile_match` arm-type unification gap — B7 |
| [stage1/fusec/src/codegen/object_backend.rs:5007-5013](../stage1/fusec/src/codegen/object_backend.rs#L5007-L5013) | `infer_expr_type` for list literals — B7 |
| [stage1/fusec/src/codegen/object_backend.rs:3352](../stage1/fusec/src/codegen/object_backend.rs#L3352) | `unsupported type namespace call` — B8 |
| [stage1/fusec/src/lexer/lexer.rs:180-213](../stage1/fusec/src/lexer/lexer.rs#L180-L213) | F-string lex path (no `{{` escape) — B10 |
| [tests/stage2/run_tests.py](../tests/stage2/run_tests.py) | `run_parity`, `run_bootstrap` modes — B0, B12 |

---

## Task Summary

| Wave | Name | Phases | Tasks | Depends On | Status |
|------|------|--------|-------|------------|--------|
| B0 | Baseline & Verification Infrastructure | 3 | 10 | — | **Done** (commits 36a8e9c, 11ff7e7, c5fc4b6) |
| B1 | Determinism | 2 | 7 | B0 | **Done** (commits 1ebbf1e, fb09dba) |
| B2 | Checker: Extension Resolution Enforcement | 3 | 11 | B1 | **Done** (commits f5cb947, 99af0a0, 9b0e77c) |
| B3 | Parser & AST: Enum Variant Payload Types | 2 | 8 | B1 | **Done** (commit 730d18e) |
| B4 | Codegen: Generic Type Substitution | 3 | 11 | B1 | Not started |
| B5 | Codegen: Hardcoded Specialization Ordering | 2 | 9 | B4 | Not started |
| B6 | Codegen: User-Defined Enum Variant Binding | 3 | 12 | B3, B4 | Not started |
| B7 | Codegen: Match-as-Expression Type Unification | 5 | 22 | B4, B5 | Not started |
| B8 | Codegen: Namespace Static Method Calls | 3 | 11 | B5 | Not started |
| B9 | Codegen: Tuple Field Access Type Propagation | 3 | 10 | B7 | Not started |
| B10 | Lexer: F-String Brace Escaping | 3 | 9 | — | Not started |
| B11 | Stage 2 Source: Missing Imports | 3 | 14 | B2, B4, B5 | Not started |
| B12 | Stage 2 Self-Compile Verification | 6 | 22 | B1-B11 | Not started |
| B13 | Institutional Knowledge & Document Sync | 4 | 14 | B12 | Not started |
| **Total** | | **45** | **~170** | | |

---

## Wave B0 — Baseline & Verification Infrastructure

> **Purpose:** Establish a reproducible baseline before any fix lands. If
> we cannot reproduce the current state exactly, we cannot prove the plan
> made things better.
>
> **Before starting this wave, read:**
> - [docs/t4-parity-investigation.md](t4-parity-investigation.md) in full
> - [tests/stage2/run_tests.py](../tests/stage2/run_tests.py)
> - The Mandatory Rules section of this document

---

### Phase B0.1 — Capture Pre-Plan Baseline

**What is the issue?** The investigation doc cites counts like "375/378
Stage 2 tests pass" from a reverted prototype. The actual baseline on the
current `main` branch has never been captured in a reproducible form. Any
claim of "X more tests pass after this fix" is unauditable without a
before-state.

**What needs to be done?** Produce a frozen snapshot of exactly which
tests pass, which fail, and with what error text, on the current `main`
commit — and commit that snapshot to the repo.

**How should it be done?**

- Run the full test matrix under a fresh build (no cached `target/`).
- Capture stdout and stderr per test into a baseline file.
- Record the git commit hash the baseline was taken against.
- Commit as `tests/stage2/baseline_pre_parity_plan.txt`.

**Tasks:**

- [x] **B0.1.1** `cargo clean` then `cargo build --release -p fusec` in `stage1/`.
- [x] **B0.1.2** Run `python tests/stage2/run_tests.py --parallel 1` and capture full stdout/stderr to a file.
- [x] **B0.1.3** Run `cargo test -p fusec -- --test-threads=1` and capture full stdout/stderr. (Used `--no-fail-fast` so all targets run; without it cargo short-circuits at the first failing target.)
- [x] **B0.1.4** Run `cargo test -p fuse-runtime` and `cargo test -p fuse-lsp` and capture results.
- [x] **B0.1.5** Write a summary file `tests/stage2/baseline_pre_parity_plan.txt` containing: the commit hash, the date, pass/fail/skip counts for each suite, and the exact error message for each failing test (sorted alphabetically so the file is diffable).
- [x] **B0.1.6** Commit the baseline file.

**Deliverables:** `tests/stage2/baseline_pre_parity_plan.txt` in the repo.

**Success criteria:** The baseline file can be regenerated from the same
commit with identical contents (assuming B1 has not yet removed the
HashMap nondeterminism — for B0.1 we tolerate variance across runs but
record all variants encountered).

---

### Phase B0.2 — Honest T4/T5 Harness Gating

**What is the issue?** `run_tests.py --parity` currently tries to execute
against a `fusec2` that does not exist and fails in a misleading way. A
future CI run must be able to tell the difference between "Stage 2 build
failed" and "Stage 2 parity failed."

**What needs to be done?** Make the parity and bootstrap modes detect the
absence of a valid `fusec2` binary and emit a distinct exit status plus a
clear message — **without** masking a real failure once `fusec2` does
build. This is explicitly not Option D from the investigation doc (which
would skip and silently exit 0); this implementation must exit non-zero
when Stage 2 is expected to exist but does not.

**How should it be done?** Add a precondition check at the top of
`run_parity()` and `run_bootstrap()` in `run_tests.py`. If `fusec2` is
missing, print:

```
T4 Parity cannot run: fusec2 binary not built.
Expected at: <path>
Reason: Stage 2 self-compile is gated by docs/fuse-stage2-parity-plan.md
Exit code: 2 (distinguishable from test failure exit 1)
```

**Tasks:**

- [x] **B0.2.1** Add `fusec2_exists_or_exit(path, mode_name)` helper in `run_tests.py`.
- [x] **B0.2.2** Call it at the top of `run_parity` and `run_bootstrap`. (Implemented in the dispatch in `main()` rather than inside `run_parity` itself, since `--bootstrap` does not call `run_parity`. Same effect, same exit code.) Side fix: `DEFAULT_COMPILER` and `DEFAULT_FUSEC2` now include `.exe` on Windows so the gate's path check is correct on Windows. Per Mandatory Rule 2, this fix landed in the same phase rather than being deferred.
- [x] **B0.2.3** Commit: the harness now reports "blocked" with exit 2, not masked failure.

**Deliverables:** Updated `tests/stage2/run_tests.py` with distinct exit
codes documented in its module docstring.

**Success criteria:** Running `python tests/stage2/run_tests.py --parity`
on the current tree exits with code 2 and a clear message; running it
after B12 completes exits with code 0.

---

### Phase B0.3 — Determinism Regression Harness

**What is the issue?** B1 removes one source of HashMap nondeterminism,
but we need a way to prove no new source creeps in. Without a harness, a
future PR could reintroduce a HashMap iteration and the plan would not
catch it.

**What needs to be done?** Add a test that compiles a representative
multi-module fixture ten times in a row and asserts the generated object
file bytes are byte-identical across all ten runs.

**How should it be done?** Under `stage1/fusec/tests/`, add a new test
`determinism_suite.rs` that picks one fixture from `tests/fuse/core/`
(select one that exercises multiple imported modules), compiles it with a
fresh temp `target/`, captures the resulting `.o` bytes, and repeats ten
times.

**Tasks:**

- [x] **B0.3.1** Pick a multi-module fixture (`tests/fuse/core/integration/stdlib_foundation.fuse` or similar — confirm it currently passes). **Used `tests/fuse/core/modules/import_multiple.fuse` instead:** the suggested `stdlib_foundation.fuse` is the pre-existing-broken fixture from the B0.1 baseline (its expected output disagrees with current compiler output), so it would not have been a clean baseline. `import_multiple.fuse` imports a helper module from `tests/fuse/core/modules/src/helpers/multi.fuse`, which is enough to put two modules in `BuildSession.modules` and surface the iteration-order bug.
- [x] **B0.3.2** Write `determinism_suite.rs` that invokes `fusec` in-process and hashes the object bytes. **Implemented as 10 subprocess invocations of `fusec --emit ir` with byte-for-byte string comparison** (not hashing — keeping the raw text lets the failure message show the actual diff). The Cranelift IR text is a more direct probe of codegen determinism than linked binary bytes (which contain timestamps).
- [x] **B0.3.3** Run it once on the current tree. It may fail (the bug is real). Record the failure in the test as an `#[ignore]` with a link to this phase, then unignore it in B1.2. **Verified locally:** with `--ignored`, the test reproducibly fails with two distinct module orderings across 10 trials (helper module emits first vs entry module emits first, with shifted Cranelift function IDs). Without `--ignored`, the default test run reports `1 ignored` and exits 0, so normal CI is unaffected. The `#[ignore]` message points at Phase B1.2 where the test gets unignored.

**Deliverables:** `stage1/fusec/tests/determinism_suite.rs`.

**Success criteria:** The test is written and runs. Whether it passes now
is not the success criterion — whether it exists and is wired up is.

---

## Wave B1 — Determinism

> **Purpose:** Eliminate HashMap iteration order as a source of
> nondeterministic compilation. Every future error, diagnostic, and
> generated object file must be reproducible byte-for-byte from the same
> inputs.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:128-160](../stage1/fusec/src/codegen/object_backend.rs#L128-L160)
> - The prototype approach in [docs/t4-parity-investigation.md](t4-parity-investigation.md#what-the-fix-got-right)

---

### Phase B1.1 — Replace `BuildSession.modules` HashMap with BTreeMap

**What is the issue?** `BuildSession.modules: HashMap<PathBuf, LoadedModule>`
at [object_backend.rs:130](../stage1/fusec/src/codegen/object_backend.rs#L130).
`emit_object` and several other functions iterate this map; the iteration
order is platform-dependent. This causes different errors on different
runs and different generated object files when none of the inputs have
changed.

**What needs to be done?** Replace `HashMap` with `BTreeMap` so iteration
is sorted by `PathBuf`.

**How should it be done?** `PathBuf` implements `Ord` (lexicographic on
the underlying OsString). The swap is mechanical — change the type and
update any call site that required the `HashMap`-only API (there are
none for this map in the current tree; both maps expose `insert`, `get`,
`values`, `values_mut`, `iter`).

**Tasks:**

- [x] **B1.1.1** Change `modules: HashMap<PathBuf, LoadedModule>` to `modules: BTreeMap<PathBuf, LoadedModule>`.
- [x] **B1.1.2** Fix the corresponding `use` import in `object_backend.rs`. Also updated `load_module_recursive`'s parameter type and `BuildSession::load`'s local variable from `HashMap::new()` to `BTreeMap::new()` so the entire loader pipeline is consistent.
- [x] **B1.1.3** Verify `BuildSession::load` and every reader of `modules` still compiles. Confirmed: 12 readers across `entry_function`, `resolve_function`, `resolve_extension`, `resolve_static`, `resolve_module_function`, `resolve_const`, `resolve_extern`, `resolve_enum`, and three `emit_*` loops compile unchanged because `BTreeMap` exposes `get`, `values`, `contains_key`, `insert`, etc. with the same signatures.
- [x] **B1.1.4** `cargo build -p fusec --release` — must succeed. Build clean in 14.89s. Smoke run of t0_smoke (8/8) and the cli/check/full_smoke/wasi targets (81 tests) all green. **Note:** the BTreeMap swap fixes module ordering but does NOT fully fix the determinism bug — `import_multiple.fuse` still drifts because `LoadedModule.functions` and several other per-module collections are also HashMaps. B1.2 closes the rest.

**Deliverables:** Single-field type change with passing build.

**Success criteria:** Build succeeds; Rule 4 (zero regressions) holds.

---

### Phase B1.2 — Audit and Lock Down Other Nondeterminism Sources

**What is the issue?** `BuildSession.modules` is not the only collection
in the codegen. Any other `HashMap` or `HashSet` whose iteration order
reaches generated output is a latent bug.

**What needs to be done?** Audit every `HashMap`/`HashSet` in
`stage1/fusec/src/codegen/` and `stage1/fusec/src/hir/`. For each one,
determine whether iteration order affects output. If yes, replace with
`BTreeMap`/`BTreeSet` or drain into a sorted `Vec` before emitting.

**How should it be done?** Grep for `HashMap` and `HashSet` under the
codegen and hir crates. For each hit, trace where iteration happens and
decide: (a) internal scratch that never affects output → keep,
(b) iteration reaches emitted bytes or error text → replace.

**Tasks:**

- [x] **B1.2.1** Grep: `rg 'HashMap|HashSet' stage1/fusec/src/codegen stage1/fusec/src/hir`. 22 hits found across object_backend.rs, layout.rs, wasm_backend.rs, hir/nodes.rs, hir/lower.rs.
- [x] **B1.2.2** For each hit, document in a short audit note: location, iterated-over?, affects output?, decision. Audit table committed in commit message and reproduced in this plan: 12 collections replaced with BTreeMap, 11 kept as HashMap/HashSet (lookup-only), 1 unused import in wasm_backend.rs left alone (orphan, not in scope).
- [x] **B1.2.3** Apply the replacements where needed. Replaced: `LoadedModule.{functions, extensions, statics, data_classes, structs, enums, extern_fns, consts}`, the corresponding three locals in `load_module_recursive`, `LoweringState.locals`, `hir::Module.extension_functions`, and the local in `hir::lower_program`.
- [x] **B1.2.4** Unignore `determinism_suite.rs` from B0.3 and run it. Must pass 10/10. **Confirmed:** `cargo test -p fusec --test determinism_suite` reports `1 passed` with the message "10 trials of `fusec --emit ir import_multiple.fuse` all byte-identical (1629 bytes each)". The full failing-tests output now also stabilizes — `unsupported List member call concat` appears as the gating error every run instead of varying.

**Deliverables:** Audit decisions in commit message; code changes; green
`determinism_suite` test.

**Success criteria:** Same fixture compiled ten times produces
byte-identical object files.

---

## Wave B2 — Checker: Extension Resolution Enforcement

> **Purpose:** Close the silent-failure gap where the checker lets
> unresolved method calls through and the codegen crashes downstream
> with a misleading error. This is the single gap that enabled all 284
> missing-import uses in `stage2/src/*.fuse` to go undetected.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/checker/mod.rs:1295-1360](../stage1/fusec/src/checker/mod.rs#L1295-L1360)
> - [stage1/fusec/src/checker/mod.rs](../stage1/fusec/src/checker/mod.rs) `resolve_extension` implementation
> - How the checker reports diagnostics: `Diagnostic::error`, `self.add_error`

---

### Phase B2.1 — Root-Cause the Silence

**What is the issue?** In `check_call`, when the callee is a member
expression and extension resolution fails, the checker falls through to a
no-op (the `if let Some(name) = callee_name { ... }` branch discards the
unresolved member case entirely at
[checker/mod.rs:1312-1356](../stage1/fusec/src/checker/mod.rs#L1312-L1356)).
The compile proceeds, codegen tries hardcoded specialization, and if
that fails too, emits "unsupported List member call `concat`" or
similar from deep in `object_backend.rs`.

**What needs to be done?** Understand every call path through
`check_call` that reaches the silent branch. Enumerate the cases:

1. Call on a known type, method is a known extension → resolved, checked.
2. Call on a known type, method is a known builtin hardcoded specialization → currently silent in checker, handled in codegen.
3. Call on a known type, method is neither an extension nor a builtin → silent in checker, crashes in codegen. **This is the bug.**
4. Call on an unknown type → silent (separate issue, not in scope for this phase).

**How should it be done?** Add a method `resolve_builtin_method(receiver_type, method_name) -> bool` that returns true for every hardcoded specialization in `compile_member_call` (List.{len,get,push,concat,...}, Map.{len,...}, Chan.{send,recv,...}, Shared.{read,write,...}, String.{toUpper,...}). This mirror must stay in sync with the codegen dispatch table, which is why B2.2 includes a regression test.

**Tasks:**

- [x] **B2.1.1** Build the full list of builtin methods by reading `compile_member_call` in `object_backend.rs`. Inventoried 27 hardcoded entries: List (3), Chan (7), Shared (5), Map (9), String (3).
- [x] **B2.1.2** Add `is_builtin_method(receiver_canonical_type, method_name) -> bool` in `checker/mod.rs` or a new `checker/builtins.rs`. Created `stage1/fusec/src/checker/builtins.rs` with `is_builtin_method`, `canonical_receiver` (mirror of layout::canonical_type_name including ownership-prefix stripping), and `suggest_stdlib_import_for` (hint generator backed by per-type stdlib method tables).
- [x] **B2.1.3** Unit test: every hardcoded method in the codegen is recognized by `is_builtin_method`. 7 unit tests in `checker::builtins::tests` cover the hardcoded set, the negative set (stdlib methods that are NOT hardcoded), generic stripping, ownership-prefix stripping, and the hint table.

**Deliverables:** `is_builtin_method` function with unit tests.

**Success criteria:** Any method name currently handled by a hardcoded
block in `compile_member_call` returns true from `is_builtin_method`.

---

### Phase B2.2 — Emit Hard Error on Unresolved Method Calls

**What is the issue?** With B2.1 in hand, we can distinguish "resolved as
extension" from "resolved as builtin" from "actually unresolved." The
checker must reject case 3.

**What needs to be done?** In `check_call`, when the callee is a member
expression, attempt extension resolution first; if that fails, call
`is_builtin_method`; if that also fails, emit a specific error.

**How should it be done?** The diagnostic must include: receiver type,
method name, a fix hint ("did you forget `import stdlib.core.list`?"),
and the exact span of the method identifier. Match the format of
existing checker diagnostics.

**Tasks:**

- [x] **B2.2.1** In `check_call`, at the point where `resolved` is `None`, add a branch: if the callee is a member and the receiver type is known, attempt `is_builtin_method`; if that fails, `self.add_error` with the message and span. Implemented as a third arm in the `if/else` chain (after the resolved-Some and the named-callee arms). Optional-chain calls (`obj?.method()`) are skipped because the codegen handles them via recursive dispatch on the unwrapped value and the checker's `infer_expr_type` does not currently track optional inner types — closing that gap is a separate, deeper task than B2.2.
- [x] **B2.2.2** Hint logic: if the method name appears in `stdlib/core/list.fuse`, `stdlib/core/option.fuse`, `stdlib/core/result.fuse`, or `stdlib/core/map.fuse`, suggest the matching import. Hints route through `builtins::suggest_stdlib_import_for`. Verified end-to-end: a missing `import stdlib.core.list` for `xs.concat(ys)` produces "no method `concat` on type `List<Int>`" with hint "did you forget `import stdlib.core.list`?".
- [x] **B2.2.3** Run full checker test suite. Some existing fixtures may now error — each one is a real bug and must be added to a TODO list for B11. **Five pre-existing checker gaps surfaced and were fixed in the same commit per Mandatory Rule 2:** (a) `Self` was not substituted in extension function return types in the checker (codegen substitutes it; checker now mirrors), (b) `resolve_extension` and `resolve_static_function` did not strip ownership prefixes via `canonical_receiver` (now they do), (c) data class instance/static methods declared inside the type body were not registered as extensions (codegen registers them; checker now mirrors), (d) struct instance/static methods inside the type body had the same gap (now fixed), (e) `compile_member_call`'s autogen-from-fields path created methods the checker did not know about (handled via new `type_has_method_via_interface` helper that checks abstract methods + default methods + autogen targets transitively). **Three fixtures remain blocked on B11:** `tests/fuse/core/types/checker_exhaustiveness.fuse`, `checker_module.fuse`, `checker_ownership.fuse` — all import `stage2.src.*` modules whose `.concat()` calls require stdlib imports that B11 will add. Tracked in `stage1/fusec/tests/check_core_suite.rs` as a `B11_BLOCKED_FIXTURES` list with a self-disable assertion (any blocked fixture that newly passes panics so the list stays accurate). **One Stage 2 fixture had a stale expected error message** (`tests/stage2/t3_diagnostics/type_errors/unknown_method.fuse` expected `unknown extension`; the new diagnostic is `no method`, which is strictly better and the plan permits this kind of fixture edit).
- [x] **B2.2.4** For Stage 2 source files specifically: the new errors should be numerous (~284 expected, per the investigation doc). This is good. It's the silent drift being made loud. **Confirmed:** `fusec --check stage2/src/checker.fuse` and `fusec --check stage2/src/main.fuse` both produce many `no method ... did you forget import stdlib.core.list?` diagnostics. The hint points at exactly the import B11 will add.

**Deliverables:** New error path with hint generation.

**Success criteria:** Calling `.concat()` on a `List<T>` without
importing `stdlib.core.list` produces a clear error at the method span
with a hint to import the module.

---

### Phase B2.3 — T3 Fixtures for the New Diagnostic

**What is the issue?** Per the test plan, every error class must have
fixtures. The new "unresolved method" diagnostic needs its own.

**What needs to be done?** Add fixtures under
`tests/stage2/t3_diagnostics/type_errors/` exercising the new error
paths.

**How should it be done?** One fixture per variation: missing list
import, missing option import, missing result import, method truly
undefined (typo), method on unknown receiver type.

**Tasks:**

- [x] **B2.3.1** Create `unresolved_method_list.fuse` — `.concat` without `import stdlib.core.list`. EXPECTED ERROR. Asserts both the diagnostic message and the stdlib import hint.
- [x] **B2.3.2** Create `unresolved_method_option.fuse` — `.unwrap` without `import stdlib.core.option`. Asserts the receiver type is rendered as `Option<Int>` and the hint suggests `import stdlib.core.option`.
- [x] **B2.3.3** Create `unresolved_method_result.fuse` — `.mapErr` without `import stdlib.core.result`. Asserts the receiver type is rendered as `Result<Int,String>` (parser produces unspaced concatenation) and the hint suggests `import stdlib.core.result`.
- [x] **B2.3.4** Create `unresolved_method_typo.fuse` — `.lenn()` on a List. EXPECTED ERROR with a different hint. Asserts the receiver is `List<Int>` and that NO hint is generated (the typo is not in the stdlib method table, so the generator returns None).
- [x] **B2.3.5** Run: `python tests/stage2/run_tests.py --filter unresolved_method`. All five pass. **Confirmed:** the four new fixtures all pass; the Stage 2 fixture suite total is now 379 passed / 0 failed / 3 skipped (was 375 in the B0.1 baseline; +4 new fixtures).

**Deliverables:** Five fixtures.

**Success criteria:** Fixtures pass; error messages contain the
receiver type, method name, and fix hint.

---

## Wave B3 — Parser & AST: Enum Variant Payload Types

> **Purpose:** Stop discarding the parsed type names on enum variant
> payloads. Downstream code (codegen `bind_pattern`, checker exhaustiveness)
> needs these types to bind payloads to concrete types.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/ast/nodes.rs:153-168](../stage1/fusec/src/ast/nodes.rs#L153-L168)
> - [stage1/fusec/src/parser/parser.rs:440-470](../stage1/fusec/src/parser/parser.rs#L440-L470)
> - [stage1/fusec/src/hir/](../stage1/fusec/src/hir/) — how enum declarations are lowered to HIR

---

### Phase B3.1 — Extend AST and HIR

**What is the issue?** `EnumVariant` stores only `name`, `arity`, and
`span`. The parser at `parser.rs:450` parses each payload type via
`parse_type_name` and throws the result away.

**What needs to be done?** Add `payload_types: Vec<String>` to
`EnumVariant` in both `ast/nodes.rs` and `hir/nodes.rs` (if HIR has its
own). Arity becomes derivable as `payload_types.len()` — keep arity for
backward compatibility of readers that use it, or remove it and update
all readers.

**How should it be done?** Two sub-decisions to make during implementation:

1. Keep `arity` as a redundant field, or compute it from
   `payload_types.len()`? Recommendation: remove `arity` and compute on
   demand. Fewer invariants to maintain.
2. Store the type as `String` (simplest, matches existing conventions) or
   as a structured `TypeName` AST node? Recommendation: `String`. The rest
   of the compiler already uses string type names.

**Tasks:**

- [x] **B3.1.1** Add `pub payload_types: Vec<String>` to `ast::EnumVariant`.
- [x] **B3.1.2** Add the same field to `hir::EnumVariant` if it exists as a separate type. **No-op:** `hir::Module.enums` re-uses `ast::nodes::EnumDecl` directly (HIR has no separate enum types). The single AST update propagates through HIR automatically.
- [x] **B3.1.3** Update HIR lowering (`hir/lower.rs` or equivalent) to copy the field through. **No-op for the same reason** — `hir::lower::lower_program` clones `EnumDecl` whole, so the new field rides along.
- [x] **B3.1.4** Remove `arity` field, or keep it as a derived method — pick one and apply consistently. **Removed.** Per the plan's recommendation: "fewer invariants to maintain". `payload_types.len()` is the count.
- [x] **B3.1.5** Fix every reader of `EnumVariant` in the tree (grep `EnumVariant`) to compile. Three reader sites updated: `parser.rs:459` (constructor), `evaluator.rs:2456` (`v.arity == 0` → `v.payload_types.is_empty()`), and `evaluator.rs:2519-2521` (`variant.arity` → `variant.payload_types.len()`). The `arity` mention in `object_backend.rs:1085` is an unrelated function-arity parameter — left untouched.

**Deliverables:** Updated AST and HIR types; build passes.

**Success criteria:** `cargo build -p fusec` green; no reader of
`EnumVariant` is silently broken.

---

### Phase B3.2 — Parser: Capture Payload Type Names

**What is the issue?** `parse_enum` at `parser.rs:450` calls
`self.parse_type_name(&[...])` and discards the return value.

**What needs to be done?** Capture the return value into a `Vec<String>`
and populate `EnumVariant.payload_types`.

**How should it be done?** Inspect the current `parse_type_name`
signature. It likely returns a `String` representing the parsed type name
including generics (e.g., `"Option<Int>"`). Collect all payload types
into a local vector and pass it to the `EnumVariant` constructor.

**Tasks:**

- [x] **B3.2.1** Rewrite the enum payload loop in `parse_enum` to collect type names into `payload_types`. The loop now captures `parse_type_name(...)`'s return value (previously discarded) and pushes it into the variant's `payload_types` Vec.
- [x] **B3.2.2** Run `cargo test -p fusec` — AST-related tests should still pass. **Confirmed:** 91 passing total (was 89; +2 from the new parser tests), same six pre-existing failures, no new failures, no morphing.
- [x] **B3.2.3** Add a unit test: parse `enum Shape { Circle(Float), Rect(Float, Float) }` and assert `payload_types == [["Float"], ["Float", "Float"]]`. Two unit tests added at the end of `stage1/fusec/src/parser/parser.rs`: `enum_variant_payload_types_capture_concrete_types` (the canonical Shape example plus a unit variant `Empty`) and `enum_variant_payload_types_handle_generic_arguments` (`Option<Int>`, `List<String>`, `Map<String,Int>` payloads — the second confirms that `parse_type_name` produces unspaced concatenation, which downstream `canonical_type_name` handles).

**Note on commit granularity:** B3.1 and B3.2 are mechanically coupled — replacing `arity: usize` with `payload_types: Vec<String>` in the AST forces the parser update in the same commit (otherwise the parser's `EnumVariant { ..., arity, ... }` initializer would not compile). Splitting would require either an additive intermediate state (both fields coexisting, then one removed) or a placeholder commit that pushes empty strings — both add noise without audit value. The two phases ship in a single commit whose message and this checkbox set map every change to its phase ID.

**Deliverables:** Updated `parse_enum` with passing unit test.

**Success criteria:** Typed enum payloads round-trip through the parser.

---

## Wave B4 — Codegen: Generic Type Substitution at Extension Call Sites

> **Purpose:** When a `List<RuntimeFn>.get(i)` call resolves to the
> extension `fn List.get(ref self, i: Int) -> Option<T>`, the codegen
> currently returns the literal string `"Option<T>"`. Downstream code
> tries to look up `T`'s layout and fails. The fix is to substitute
> generic type parameters with their concrete arguments at the call site.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:3437-3458](../stage1/fusec/src/codegen/object_backend.rs#L3437-L3458)
> - [stage1/fusec/src/codegen/type_names.rs](../stage1/fusec/src/codegen/type_names.rs) — existing `split_generic_args`, `option_inner_type`, etc.
> - [stdlib/core/list.fuse](../stdlib/core/list.fuse) — representative generic extension method signatures

---

### Phase B4.1 — Generic Substitution Helpers

**What is the issue?** `type_names.rs` has helpers for inspecting generic
type names but no helper for substituting type parameters. We need one.

**What needs to be done?** Add a `substitute_generics(type_name: &str,
params: &HashMap<String, String>) -> String` function that walks a type
name and replaces whole-word identifiers listed as keys with their
corresponding values.

**How should it be done?** Whole-word replacement is critical:
substituting `T` in `"Option<T>"` must produce `"Option<RuntimeFn>"`,
not `"OpRuntimeFnion<RuntimeFn>"`. Use a character-level scanner that
identifies identifier boundaries (alphanumeric + `_`, starting with
alpha or `_`) and replaces only complete identifiers.

**Tasks:**

- [x] **B4.1.1** Implement `substitute_generics` in `type_names.rs`. Whole-word identifier replacement using a byte-level scanner — identifiers are ASCII letter/underscore start, alphanumeric/underscore continue. Empty map returns input unchanged. Non-identifier bytes (`<`, `>`, `,`, ` `) are emitted verbatim.
- [x] **B4.1.2** Implement `build_type_param_map(type_params: &[String], concrete_args: &[String]) -> HashMap<String, String>` — zips formal params with concrete arguments. Empty inputs yield an empty map. Length mismatch uses the common prefix.
- [x] **B4.1.3** Unit tests covering: `Option<T>` → `Option<Int>`; `List<T>` → `List<String>`; `Result<T, E>` → `Result<Int, String>`; `Map<K, V>` → `Map<String, Int>`; nested: `List<Option<T>>` → `List<Option<Int>>`; no-match: `Int` → `Int`; edge: name prefix collision `Tail` with param `T` → must remain `Tail`, not `Intail`. **16 unit tests** in `codegen::type_names::tests` cover all six requirements above plus: empty-map fast path, identifier-suffix collision (`MyT` vs `T`), underscore-in-identifier (`T_inner` vs `T`), unspaced arglist (`Map<K,V>` matching the parser's output format), and the `builtin_type_params` table.

**Deliverables:** Two new functions with unit test coverage.

**Success criteria:** All six unit tests pass. No false substitution on
identifier prefixes.

---

### Phase B4.2 — Type Parameter Lookup

**What is the issue?** To build the type parameter map, the codegen
needs to know the formal type parameters for any given type name. For
builtins (`List`, `Option`, `Result`, `Map`, `Chan`, `Shared`, `Set`),
these are not stored as data classes anywhere. For user-defined generic
types, they live on `DataClassDecl`/`StructDecl`/`EnumDecl`.

**What needs to be done?** Add `BuildSession::type_params_for_type(type_name: &str) -> Option<Vec<String>>` that returns the formal type parameter names.

**How should it be done?** Check canonical type name against a hardcoded
table of builtins (`List` → `["T"]`, `Map` → `["K", "V"]`, `Option` →
`["T"]`, `Result` → `["T", "E"]`, `Chan` → `["T"]`, `Shared` → `["T"]`,
`Set` → `["T"]`). Otherwise fall back to looking up the user-defined
type across all loaded modules and returning its `type_params` field.

**Tasks:**

- [ ] **B4.2.1** Implement `type_params_for_type` on `BuildSession`.
- [ ] **B4.2.2** Unit test: returns `["T"]` for `"List"`, returns `["K", "V"]` for `"Map"`, returns the declared params for a user-defined generic data class.

**Deliverables:** New method on `BuildSession` with unit test.

**Success criteria:** Lookup returns correct formal type parameters for
every generic type in Fuse.

---

### Phase B4.3 — Substitute at the Extension Call Site

**What is the issue?** `compile_member_call` at line 3454 returns
`function.return_type.clone()` verbatim. This is the exact bug.

**What needs to be done?** After resolving the extension function,
extract concrete type arguments from `receiver_type` (via
`split_generic_args`), look up formal parameters via
`type_params_for_type`, build the substitution map, and apply
`substitute_generics` to `function.return_type` before returning.

**How should it be done?**

1. Parse `receiver_type` to extract concrete args. `"List<RuntimeFn>"` → `["RuntimeFn"]`.
2. Look up formal params. `"List"` → `["T"]`.
3. Zip → `{"T": "RuntimeFn"}`.
4. Substitute into `function.return_type`. `"Option<T>"` → `"Option<RuntimeFn>"`.
5. Use the substituted string as the `ty` field of the returned `TypedValue`.

**Tasks:**

- [ ] **B4.3.1** Modify `compile_member_call` at [object_backend.rs:3454-3457](../stage1/fusec/src/codegen/object_backend.rs#L3454-L3457) to substitute.
- [ ] **B4.3.2** Handle the edge case where `receiver_type` has no generic args (e.g., plain `"List"`) — fall back to no substitution (the return type stays as formal).
- [ ] **B4.3.3** Handle the edge case where the function has no return type (Unit fn).
- [ ] **B4.3.4** Run the full Rust test suite. Any test that previously relied on the buggy behavior is now a real bug to fix.
- [ ] **B4.3.5** Add a focused test: compile a fuse file with `val x: Int = (myList.get(0)).unwrapOr(0)` where `myList: List<Int>`. The `.get()` return type must be `Option<Int>`.
- [ ] **B4.3.6** Add a focused test with a user-defined generic type: `data class Pair<A, B>(val first: A, val second: B)` and an extension `fn Pair.first(ref self) -> A` — verify `Pair<Int, String>.first()` returns `Int`.

**Deliverables:** Correct substitution at the extension call site with
two tests.

**Success criteria:** Both focused tests pass. No existing tests regress.

---

## Wave B5 — Codegen: Hardcoded Specialization Ordering

> **Purpose:** After B4, the codegen correctly types extension calls.
> But the hardcoded List/Map/Chan/Shared blocks still run after extension
> resolution, so importing `stdlib.core.list` still shadows the builtin
> dispatch. Restructure so builtins run first, with fallthrough to
> extension resolution.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:3437-3828](../stage1/fusec/src/codegen/object_backend.rs#L3437-L3828)
> - The investigation doc's analysis of Issue 3
>
> **Before editing:** understand that this wave is a behavioral
> preservation refactor. Each hardcoded block continues to handle the
> same methods it handled before. The only change is the dispatch
> ordering and the `_` arm behavior.

---

### Phase B5.1 — Reorder Dispatch

**What is the issue?** Current order: extension resolution → hardcoded
List → hardcoded Chan → hardcoded Shared → hardcoded Map → hardcoded
String → error. Importing `stdlib.core.list` makes extension resolution
succeed for `List.get` with the (now-substituted) return type, but
hardcoded specializations were tuned for correctness in ways extension
resolution cannot match for some methods. We need builtins first.

**What needs to be done?** Move each hardcoded `if receiver_type.starts_with("List")`
/ `canonical_type_name(...) == "Chan"` block to run **before** extension
resolution. The final order becomes:

1. Hardcoded List
2. Hardcoded Map
3. Hardcoded Chan
4. Hardcoded Shared
5. Hardcoded String
6. Hardcoded Set (if any)
7. Extension resolution (with B4 substitution)
8. Final "unknown extension" error

**How should it be done?** This is a physical code move. The blocks
currently sit at lines 3459-3828 (roughly). Move them to before line
3437.

**Tasks:**

- [ ] **B5.1.1** Cut the hardcoded block at line 3459 (List) and paste it before line 3437. Rewire the `Err(format!("unsupported List member call ..."))` to `{ /* fall through */ }`.
- [ ] **B5.1.2** Same for Chan.
- [ ] **B5.1.3** Same for Shared.
- [ ] **B5.1.4** Same for Map.
- [ ] **B5.1.5** Same for String.
- [ ] **B5.1.6** The extension resolution block stays where it is, but is now the second pass.
- [ ] **B5.1.7** Run `cargo test -p fusec`. Zero regressions required.

**Deliverables:** Reordered dispatch.

**Success criteria:** All existing tests pass. Importing
`stdlib.core.list` and calling `.get` still produces correct code (the
builtin path runs first and has precise type info).

---

### Phase B5.2 — Fallthrough Correctness

**What is the issue?** The `_` arms of the hardcoded blocks used to
return errors. Now they must fall through to extension resolution.

**What needs to be done?** Rewrite each `_` arm to exit the match and
continue the function, so control flow reaches the extension resolution
below.

**How should it be done?** Rust `match` doesn't "fall through" like C,
so the pattern is: wrap the match in a block that uses `break` out of a
labeled loop, or refactor to an `Option<Result<TypedValue, String>>`
return where `None` means "not handled." Prefer the latter for clarity.

**Tasks:**

- [ ] **B5.2.1** Refactor each hardcoded block into a helper `try_compile_XXX_builtin(...) -> Result<Option<TypedValue>, String>`. `None` means "this block did not handle it — continue."
- [ ] **B5.2.2** `compile_member_call` calls each helper in order, returns early on `Some(Ok)` or `Some(Err)`, continues on `None`.
- [ ] **B5.2.3** Test: `.concat` on `List` (which has no hardcoded block) correctly falls through to extension resolution and succeeds after B4.

**Deliverables:** Clean fallthrough semantics via helper functions.

**Success criteria:** A method that exists in stdlib but not in hardcoded
blocks routes to extension resolution and succeeds.

---

## Wave B6 — Codegen: User-Defined Enum Variant Binding

> **Purpose:** `bind_pattern` currently only knows how to bind payload
> types for `Ok`/`Err`/`Some`. For any user-defined variant, the bound
> variable gets `ty: None`, which propagates "cannot infer member"
> errors when the bound variable's fields are accessed.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:4629-4730](../stage1/fusec/src/codegen/object_backend.rs#L4629-L4730)
> - B3 must be complete (`EnumVariant.payload_types` must exist)

---

### Phase B6.1 — `BuildSession::enum_variant_payload_types`

**What is the issue?** The codegen needs a way to look up payload types
for a variant given its enum's name.

**What needs to be done?** Add a method on `BuildSession` that takes an
enum name and variant name and returns the variant's `payload_types`.

**How should it be done?** Iterate the loaded modules, look up the enum
declaration by name, find the variant by name, return its
`payload_types` (or `None` if not found).

**Tasks:**

- [ ] **B6.1.1** Add `enum_variant_payload_types(enum_name: &str, variant_name: &str) -> Option<Vec<String>>` on `BuildSession`.
- [ ] **B6.1.2** Unit test with a user-defined enum.

**Deliverables:** Lookup method with unit test.

**Success criteria:** Returns payload types for known variants; returns
`None` for unknown variants.

---

### Phase B6.2 — Extend `bind_pattern` for User Variants

**What is the issue?** `bind_pattern` at lines 4688-4702 hardcodes
`Ok`/`Err`/`Some` and returns `ty: None` for all other variants.

**What needs to be done?** For non-builtin variants, call
`enum_variant_payload_types`, apply generic substitution (using B4
helpers) if the subject type has concrete arguments, and bind each
payload variable to the corresponding payload type.

**How should it be done?**

1. Extract the enum name from `subject_type` (canonical name).
2. Extract concrete generic args from `subject_type` (via `split_generic_args`).
3. Look up formal type params via `type_params_for_type`.
4. Build substitution map.
5. Look up variant payload types via `enum_variant_payload_types`.
6. Substitute generics in each payload type.
7. Bind each variant argument to the corresponding substituted type.

**Tasks:**

- [ ] **B6.2.1** Implement the new binding logic in `bind_pattern`.
- [ ] **B6.2.2** Preserve Ok/Err/Some special cases — they short-circuit.
- [ ] **B6.2.3** Test with simple user enum: `enum Pattern { Name(NamePattern), Variant(String, List<Pattern>) }`. Assert bound variables get correct types.
- [ ] **B6.2.4** Test with generic user enum: `enum Maybe<T> { Just(T), Nothing }`. For `Maybe<Int>.Just(x)`, assert `x: Int`.

**Deliverables:** Updated `bind_pattern` with two focused tests.

**Success criteria:** Both tests pass. Existing Ok/Err/Some handling
regresses zero tests.

---

### Phase B6.3 — Exercise the Fix with Real Stage 2 Patterns

**What is the issue?** The canonical example in the investigation is
`codegen.fuse` matching on `Declaration.DataClass(dc) => dc.interfaces`.
This must compile end-to-end.

**What needs to be done?** Write a standalone fixture that mirrors the
pattern from `stage2/src/codegen.fuse` and verify it compiles and runs.

**How should it be done?** A new fixture under
`tests/stage2/t1_features/pattern_matching/` that defines an enum,
matches it in an expression position, binds a payload variable, and uses
a field of the bound variable.

**Tasks:**

- [ ] **B6.3.1** Create `user_enum_payload_field_access.fuse`.
- [ ] **B6.3.2** Create `generic_user_enum_payload.fuse` for the generic case.
- [ ] **B6.3.3** Run both. Must pass.

**Deliverables:** Two new fixtures.

**Success criteria:** Both fixtures pass on the current compiler after
this wave.

---

## Wave B7 — Codegen: Match-as-Expression Type Unification

> **Purpose:** This is the deepest gap. `val x = match foo { ... _ => [] }`
> doesn't unify arm types, and empty list literals don't inherit context
> types. The result: `implInterfaces.get(ii)` returns `Option<Unknown>`
> and the compile cascades to "unknown extension `Unknown.len`."
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:4270-4413](../stage1/fusec/src/codegen/object_backend.rs#L4270-L4413) — `compile_match`, `compile_two_arm_match`
> - [stage1/fusec/src/codegen/object_backend.rs:4991-5120](../stage1/fusec/src/codegen/object_backend.rs#L4991-L5120) — `infer_expr_type`
> - [stage2/src/codegen.fuse:332-337](../stage2/src/codegen.fuse#L332-L337) — the canonical trigger
> - [docs/fuse-language-guide-2.md](fuse-language-guide-2.md) — sections on expressions and type inference

---

### Phase B7.1 — Design Note: Arm Unification and Contextual Typing

**What is the issue?** Before writing code, we need a clear type rule:
what *is* the type of a match expression with incompatible-looking arms?

**What needs to be done?** Write a short design note (inline in this
plan, committed alongside the code) answering:

1. What is the type of `match x { A => [1], B => [] }`? **Answer:** `List<Int>`. The non-empty arm supplies the generic; the empty arm inherits.
2. What is the type of `match x { A => 1, B => "a" }`? **Answer:** error — incompatible arm types, reported by the checker.
3. What is the type of `match x { A => Some(1), B => None }`? **Answer:** `Option<Int>`. `None` already exists in the language with pseudo-type `Option<Unknown>`, and Unknown is substituted by the sibling arm's concrete type.
4. How does this flow through variable bindings? **Answer:** the type is inferred after all arms are compiled, not before, and stored on the binding.
5. Do we need bidirectional type checking, or will post-hoc unification suffice? **Answer:** post-hoc unification for match expressions specifically. Full bidirectional checking is out of scope for this plan.

**How should it be done?** Write the rules as a subsection of this phase,
commit them as the authoritative statement, and reference them from
every subsequent task.

**Tasks:**

- [ ] **B7.1.1** Write the unification rules (below).
- [ ] **B7.1.2** Review against `docs/fuse-language-guide-2.md`. If the guide does not already specify match-as-expression typing, add a subsection there too.
- [ ] **B7.1.3** Commit.

**Unification rules (adopted):**

- Compile all arms first, collecting each arm's `TypedValue`.
- Let `arm_types` be the list of `Option<String>` types from all arms
  (dropping `None`s into a "no-info" bucket).
- **Rule U1:** If all `arm_types` are equal → that is the result type.
- **Rule U2:** If `arm_types` contains a mix of `"List"` and
  `"List<X>"` → promote all to `"List<X>"` (empty list inherits from
  non-empty siblings).
- **Rule U3:** If `arm_types` contains a mix of `"Option<Unknown>"`
  (from `None`) and `"Option<X>"` → promote all to `"Option<X>"`.
- **Rule U4:** Same as U3 for `Result<Unknown, X>` and
  `Result<X, Unknown>`.
- **Rule U5:** If `arm_types` contains incompatible concrete types → the
  checker emits an error "match arms have incompatible types `A` and `B`"
  with spans of both arms. Codegen assigns `ty: None` and continues
  (error has already been reported).
- **Rule U6:** If all `arm_types` are `None` → result is `None`.

**Deliverables:** Rules U1-U6 committed to this document.

**Success criteria:** Rules are unambiguous. Every reviewer agrees on
what the type of a given match expression should be.

---

### Phase B7.2 — Empty List Literal Contextual Inference

**What is the issue?** `infer_expr_type` at lines 5007-5013 returns
`List<Unknown>` for `[]`. That bare `Unknown` is what downstream code
tries (and fails) to look up.

**What needs to be done?** Teach list literal typing to accept a
contextual hint. When a list literal appears inside a match arm whose
siblings have concrete list types, or inside a `val x: List<T> = ...`
with an explicit annotation, the literal should pick up the context
type.

**How should it be done?** Two mechanisms:

1. **Annotation-driven (simpler).** In `compile_statement` for `VarDecl`,
   if `var_decl.type_name` is `Some(ty)`, pass `ty` as a contextual hint
   into `compile_expr`. Only the list-literal case uses the hint.
2. **Match-driven (post-hoc).** In `compile_match`, after compiling all
   arms, if the unified type (via U2) is a generic list, update the
   types of any arm whose result is the bare `List`.

Implement both. They are complementary.

**Tasks:**

- [ ] **B7.2.1** Add an optional `expected_type: Option<&str>` parameter to `compile_expr`. Thread it through only the list-literal path.
- [ ] **B7.2.2** In `VarDecl` compilation, pass `var_decl.type_name.as_deref()` as the hint.
- [ ] **B7.2.3** In the list-literal case, if the hint is `"List<X>"`, set the result `TypedValue.ty = Some("List<X>")` even when the literal is empty.
- [ ] **B7.2.4** Test: `val xs: List<Int> = []; println(xs.len())` — must compile and run.
- [ ] **B7.2.5** Test: `val xs: List<String> = [];` round-trip type inspection.

**Deliverables:** Contextual list-literal typing with two tests.

**Success criteria:** Both tests pass. No regression in any test that
relies on the current "first-element wins" inference.

---

### Phase B7.3 — Arm Unification in `compile_match` and `compile_two_arm_match`

**What is the issue?** Both `compile_match` and `compile_two_arm_match`
compute the result type with
`arms.iter().find_map(|arm| match &arm.body { Expr => infer_expr_type, Block => Some("Unit") })`.
This takes the *first* arm's inferred type and stops, ignoring later
arms. Incorrect for all cases where later arms are more specific.

**What needs to be done?** Replace the `find_map` with full unification
per rules U1-U6.

**How should it be done?**

1. Compile each arm into a `TypedValue`.
2. After all arms are compiled, collect all `ty` fields.
3. Apply unification.
4. Use the unified type as the result `TypedValue.ty`.
5. Retroactively update any arm's result type if U2/U3/U4 promoted it
   (the block param at `done` is still the same SSA value — only the
   surface type string changes).

**Tasks:**

- [ ] **B7.3.1** Implement `unify_match_arm_types(types: &[Option<String>]) -> Option<String>` applying U1-U6.
- [ ] **B7.3.2** Replace the `find_map` in `compile_match` (around line 4331) with a call to `unify_match_arm_types`.
- [ ] **B7.3.3** Same in `compile_two_arm_match` (around line 4408).
- [ ] **B7.3.4** Unit test `unify_match_arm_types` with each rule.
- [ ] **B7.3.5** Integration test: the canonical `stage2/src/codegen.fuse:332-337` pattern as a standalone fixture.

**Deliverables:** Unification function and wired call sites.

**Success criteria:** The canonical pattern compiles correctly. The
`unify_match_arm_types` unit tests all pass.

---

### Phase B7.4 — Checker Validation of Arm Compatibility

**What is the issue?** Rule U5 says incompatible arms are an error. The
checker must emit that error so users see it before codegen.

**What needs to be done?** In the checker's match-expression handling,
validate arm compatibility using the same U1-U6 rules.

**How should it be done?** Share the unification helper between checker
and codegen by placing it in a new `types.rs` module the checker can
import.

**Tasks:**

- [ ] **B7.4.1** Move `unify_match_arm_types` to a shared location.
- [ ] **B7.4.2** Call it from the checker's match handling.
- [ ] **B7.4.3** Emit "match arms have incompatible types `A` and `B`" with spans.
- [ ] **B7.4.4** T3 fixture: `match_incompatible_arms.fuse` expecting the new error.

**Deliverables:** Shared helper and checker diagnostic.

**Success criteria:** Incompatible arms are caught at check time with a
clear message.

---

### Phase B7.5 — `infer_expr_type` for Match Expressions

**What is the issue?** `infer_expr_type` at line 4991 does not handle
`fa::Expr::Match(_)`. Code that reads a variable bound to a match
expression (e.g., `val x = match ...; x.len()`) cannot look up `x`'s
type.

**What needs to be done?** Add a case to `infer_expr_type` for match
expressions that recurses into each arm's body and unifies the results.

**How should it be done?** Mirror the logic from B7.3 but as a
pure-function inspection. Depth-limit to prevent runaway recursion on
deeply nested matches.

**Tasks:**

- [ ] **B7.5.1** Add the `fa::Expr::Match(match_expr)` arm to `infer_expr_type`.
- [ ] **B7.5.2** For each arm body, recursively call `infer_expr_type`.
- [ ] **B7.5.3** Unify the results via `unify_match_arm_types`.
- [ ] **B7.5.4** Add `fa::Expr::When(_)` handling with the same logic (when expressions are isomorphic).
- [ ] **B7.5.5** Test: `val x = match foo { A => [1], B => [] }; val y: Int = x.len()` — compiles and runs.
- [ ] **B7.5.6** Test: `val x = when { a => Some(1), else => None }; val y: Int = x.unwrapOr(0)`.

**Deliverables:** Match and When cases in `infer_expr_type`.

**Success criteria:** Both tests pass. `infer_expr_type` returns the
unified type for any match/when expression whose arms unify.

---

## Wave B8 — Codegen: Namespace Static Method Calls

> **Purpose:** `tests/fuse/core/types/parser_decls.fuse` and similar
> fixtures use `parser.Parser.foo()` — importing a module as alias and
> calling a static method on a type defined in that module. The codegen
> errors with `unsupported type namespace call 'parser.Parser'`.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/codegen/object_backend.rs:3352](../stage1/fusec/src/codegen/object_backend.rs#L3352)
> - How the codegen currently handles `Type.staticMethod()` (without module prefix)
> - How imports populate module aliases in the resolver

---

### Phase B8.1 — Root-Cause the Namespace Call Handling

**What is the issue?** The codegen has a dispatch case for `Type.staticMethod()` but not for `module.Type.staticMethod()`. The three-segment path is treated as a generic namespace call and rejected.

**What needs to be done?** Read the current dispatch path end-to-end and document where the three-segment call is rejected vs where the two-segment call succeeds.

**Tasks:**

- [ ] **B8.1.1** Read the case at [object_backend.rs:3352](../stage1/fusec/src/codegen/object_backend.rs#L3352) and the surrounding dispatch.
- [ ] **B8.1.2** Identify the data flow: how is `parser.Parser.foo(...)` parsed? Member-of-member-of-name?
- [ ] **B8.1.3** Document the minimal change needed.

**Deliverables:** A short root-cause note in the commit message.

**Success criteria:** The dispatch path is understood.

---

### Phase B8.2 — Implement Module-Qualified Static Calls

**What is the issue?** Users should be able to write `parser.Parser.new(...)` after `import parser` (or `import stage2.src.parser`).

**What needs to be done?** Add a dispatch case for three-segment calls: module → type → static method. Resolve the module in `BuildSession`, then the type in that module, then the static method on the type.

**How should it be done?**

1. Parse the member chain into `[module_name, type_name, method_name]`.
2. Look up the module in `BuildSession.modules` by name (using the loaded module path → alias mapping).
3. Look up the type in the module's `data_classes` / `structs` / `enums`.
4. Look up the static method on the type.
5. Emit the call.

**Tasks:**

- [ ] **B8.2.1** Implement the dispatch.
- [ ] **B8.2.2** Test: module-qualified static method call.
- [ ] **B8.2.3** Test: module-qualified constructor call (if distinct from static method).
- [ ] **B8.2.4** Unignore or add the `tests/fuse/core/types/parser_decls.fuse` and `parser_infra.fuse` fixtures. They must now pass.

**Deliverables:** Working module-qualified static calls with tests.

**Success criteria:** `parser_decls.fuse` and `parser_infra.fuse` pass.

---

### Phase B8.3 — Checker Validation

**What is the issue?** The checker currently doesn't validate these calls either.

**What needs to be done?** Add checker validation so unknown module, unknown type in module, or unknown static method produce clear errors.

**Tasks:**

- [ ] **B8.3.1** Add checker validation for the three-segment path.
- [ ] **B8.3.2** T3 fixtures for each error path.
- [ ] **B8.3.3** Run: all pass.

**Deliverables:** Checker validation and fixtures.

**Success criteria:** Fixtures pass.

---

## Wave B9 — Codegen: Tuple Field Access Type Propagation

> **Purpose:** `cannot infer member '0'` appears when accessing a tuple
> field (`.0`, `.1`) on a tuple whose type the codegen has not tracked.
> Fixture: `module_loading.fuse`.
>
> **Before starting this wave, read:**
> - Tuple handling in `infer_expr_type`
> - `TupleDestruct` statement compilation in `object_backend.rs:1745`
> - `split_tuple_types` in `type_names.rs`

---

### Phase B9.1 — Root-Cause Tuple Type Loss

**What is the issue?** Somewhere in the pipeline, a tuple literal's type is inferred but the result of a function returning a tuple is not carried through to subsequent field accesses.

**What needs to be done?** Trace a failing example end-to-end and identify the exact point where the type is lost.

**Tasks:**

- [ ] **B9.1.1** Reproduce `module_loading.fuse` failure locally.
- [ ] **B9.1.2** Instrument `infer_expr_type` with debug traces to find where the tuple loses its type.
- [ ] **B9.1.3** Document the loss point.

**Deliverables:** Root-cause note.

**Success criteria:** The loss point is identified.

---

### Phase B9.2 — Fix the Loss

**What is the issue?** Whatever the root cause from B9.1 is, fix it.

**What needs to be done?** Propagate tuple types through the relevant code path.

**How should it be done?** Depends on B9.1 findings. The most likely location is an `infer_expr_type` case for `Call` that returns a function declaration's `return_type`, but tuple types aren't recognized as "data types" downstream and get dropped.

**Tasks:**

- [ ] **B9.2.1** Implement the fix.
- [ ] **B9.2.2** Test: `fn pair() -> (Int, String) { (1, "a") }; val p = pair(); println(p.0)`.
- [ ] **B9.2.3** `module_loading.fuse` passes.

**Deliverables:** Fixed tuple-type propagation with tests.

**Success criteria:** `module_loading.fuse` passes.

---

### Phase B9.3 — Tuple Field Access Checker Validation

**What is the issue?** The checker should verify tuple field indices are in range and produce clear errors otherwise.

**What needs to be done?** Add checker validation.

**Tasks:**

- [ ] **B9.3.1** Validate `.0`, `.1`, etc. against tuple arity in the checker.
- [ ] **B9.3.2** T3 fixture: out-of-bounds tuple field access.
- [ ] **B9.3.3** Pass.

**Deliverables:** Checker validation and fixture.

**Success criteria:** Fixture passes.

---

## Wave B10 — Lexer: F-String Brace Escaping

> **Purpose:** `f"{{ path = \"{x}\" }}"` produces garbage because the
> lexer treats `{{` as two nested braces instead of a literal `{`. The
> investigation doc flags this in `main.fuse`'s `buildWrapper`.
>
> **Before starting this wave, read:**
> - [stage1/fusec/src/lexer/lexer.rs:175-217](../stage1/fusec/src/lexer/lexer.rs#L175-L217)
> - The Fuse language guide's section on f-strings

---

### Phase B10.1 — Implement `{{` / `}}` Escape

**What is the issue?** At `lexer.rs:188-197`, the f-string scan
increments `brace_depth` on every `{` and decrements on every `}`. There
is no provision for literal braces.

**What needs to be done?** Recognize `{{` as a literal `{` and `}}` as a
literal `}`, matching the Python f-string and Rust `format!` convention.

**How should it be done?** When scanning an f-string, peek ahead: if the
next character is the same brace, emit a single literal and advance twice.

**Tasks:**

- [ ] **B10.1.1** Modify the f-string scan in `lexer.rs` to handle `{{` → literal `{`, `}}` → literal `}`.
- [ ] **B10.1.2** Make sure the f-string token representation preserves the literal for the parser (the current representation seems to be a flat string; confirm the parser doesn't re-scan for braces in a way that would re-misinterpret them).

**Deliverables:** Updated f-string lexer.

**Success criteria:** `f"{{hello}}"` lexes as the literal `{hello}`.

---

### Phase B10.2 — Tests for F-String Escaping

**What is the issue?** No tests exercise the escape path.

**What needs to be done?** Add fixtures.

**Tasks:**

- [ ] **B10.2.1** `tests/stage2/t1_features/strings/fstring_brace_escape.fuse`: `println(f"{{hello}}")` → expected `{hello}`.
- [ ] **B10.2.2** Mixed: `println(f"{{x = {x}}}")` with `val x = 5` → expected `{x = 5}`.
- [ ] **B10.2.3** Lexer unit test in `stage1/fusec/src/lexer/lexer.rs` or sibling test file.

**Deliverables:** Three new tests.

**Success criteria:** All three pass.

---

### Phase B10.3 — Exercise `main.fuse`'s `buildWrapper` F-String

**What is the issue?** The investigation cites `buildWrapper` in
`main.fuse` as the canonical broken case.

**What needs to be done?** Verify that `buildWrapper`'s f-string now
compiles correctly after B10.1.

**Tasks:**

- [ ] **B10.3.1** Locate the f-string in `stage2/src/main.fuse`'s `buildWrapper`.
- [ ] **B10.3.2** Compile `stage2/src/main.fuse` with `--check`. The f-string must no longer be flagged.

**Deliverables:** Verification.

**Success criteria:** `main.fuse` `--check` passes (subject to other waves).

---

## Wave B11 — Stage 2 Source: Missing Imports

> **Purpose:** With B2 enforcing extension resolution, compiling
> `stage2/src/*.fuse` now yields loud errors for every missing import.
> Add the imports, driven by the checker's diagnostics — not by guesswork.
>
> **Before starting this wave:** B2, B4, and B5 must be complete.

---

### Phase B11.1 — Enumerate Missing Imports

**What is the issue?** The investigation doc estimates 284 missing
method calls across 8 files. The exact set must come from the checker,
not from a grep.

**What needs to be done?** Run `fusec --check stage2/src/main.fuse` and
collect every "unresolved method" error. Group by file and required
module.

**Tasks:**

- [ ] **B11.1.1** Run `fusec --check stage2/src/main.fuse` after B2 lands.
- [ ] **B11.1.2** Parse the output into a table: file → required imports.
- [ ] **B11.1.3** Commit the table as `docs/stage2-missing-imports.txt` (temporary scratch file; remove after B11.3).

**Deliverables:** Table of missing imports grounded in checker output.

**Success criteria:** Every entry has an exact file and required module.

---

### Phase B11.2 — Add Imports

**What is the issue?** Add the imports.

**What needs to be done?** For each file in the table, add the required
`import stdlib.core.list` / `option` / `result` lines alphabetically with
the existing imports.

**Tasks:**

- [ ] **B11.2.1** `stage2/src/main.fuse` — add missing imports.
- [ ] **B11.2.2** `stage2/src/checker.fuse` — add.
- [ ] **B11.2.3** `stage2/src/codegen.fuse` — add.
- [ ] **B11.2.4** `stage2/src/layout.fuse` — add.
- [ ] **B11.2.5** `stage2/src/lexer.fuse` — add.
- [ ] **B11.2.6** `stage2/src/module.fuse` — add.
- [ ] **B11.2.7** `stage2/src/parser.fuse` — add.
- [ ] **B11.2.8** `stage2/src/runtime.fuse` — add.
- [ ] **B11.2.9** After each file, re-run `fusec --check` on that file. Errors for the added module must be gone.

**Deliverables:** Imports added across all eight files.

**Success criteria:** No missing-import errors remain.

---

### Phase B11.3 — Full Stage 2 Check Passes

**What is the issue?** After imports are added, `fusec --check stage2/src/main.fuse` must succeed (returning zero errors and exit 0).

**What needs to be done?** Verify.

**Tasks:**

- [ ] **B11.3.1** Run `fusec --check stage2/src/main.fuse`. No errors.
- [ ] **B11.3.2** Remove `docs/stage2-missing-imports.txt`.
- [ ] **B11.3.3** Commit.

**Deliverables:** Clean check output.

**Success criteria:** `fusec --check stage2/src/main.fuse` exits 0 with
no diagnostics.

---

## Wave B12 — Stage 2 Self-Compile Verification

> **Purpose:** With every compiler gap closed, verify that Stage 2
> actually self-compiles and that T4 Parity and T5 Bootstrap succeed.
>
> **Before starting this wave:** B1-B11 must all be complete.

---

### Phase B12.1 — Build `fusec2` From Stage 2 Source

**What is the issue?** The ultimate verification.

**What needs to be done?** Run `stage1/target/release/fusec.exe stage2/src/main.fuse -o stage1/target/fusec2.exe`. Must succeed.

**Tasks:**

- [ ] **B12.1.1** Clean build of `fusec`.
- [ ] **B12.1.2** Run the compile command.
- [ ] **B12.1.3** Verify `fusec2.exe` exists and is executable.

**Deliverables:** A working `fusec2.exe`.

**Success criteria:** Compile exits 0; binary runs.

---

### Phase B12.2 — `fusec2` Passes T0 Smoke

**What is the issue?** The first thing the self-hosted compiler must do
is compile the smoke tests.

**What needs to be done?** Run `run_tests.py --compiler fusec2.exe --filter t0_`.

**Tasks:**

- [ ] **B12.2.1** Execute.
- [ ] **B12.2.2** Expect all T0 smoke tests to pass.

**Deliverables:** Green T0 suite against `fusec2`.

**Success criteria:** All T0 tests pass.

---

### Phase B12.3 — `fusec2` Passes T1 Features

**What is the issue?** The feature suite is larger and exercises every
language feature.

**What needs to be done?** Run `run_tests.py --compiler fusec2.exe --filter t1_`.

**Tasks:**

- [ ] **B12.3.1** Execute.
- [ ] **B12.3.2** Triage any failures. Every failure is a real bug — fix it in the corresponding wave before declaring this phase done.

**Deliverables:** Green T1 suite.

**Success criteria:** All T1 tests pass.

---

### Phase B12.4 — `fusec2` Passes T2, T3, and M Tiers

**What is the issue?** The composition, diagnostics, and memory-safety
tiers must all pass on the self-hosted compiler.

**What needs to be done?** Run each tier.

**Tasks:**

- [ ] **B12.4.1** `run_tests.py --compiler fusec2.exe --filter t2_`.
- [ ] **B12.4.2** `run_tests.py --compiler fusec2.exe --filter t3_`.
- [ ] **B12.4.3** `run_tests.py --compiler fusec2.exe --filter m_memory`.
- [ ] **B12.4.4** All three must pass.

**Deliverables:** Green T2, T3, M tiers on `fusec2`.

**Success criteria:** Zero failures.

---

### Phase B12.5 — T4 Parity

**What is the issue?** The headline goal: Stage 1 output must equal
Stage 2 output on every shared fixture.

**What needs to be done?** Run `run_tests.py --parity`.

**Tasks:**

- [ ] **B12.5.1** Ensure `fusec2.exe` is built from B12.1.
- [ ] **B12.5.2** Run `python tests/stage2/run_tests.py --parity`.
- [ ] **B12.5.3** Every `tests/fuse/core/` and `tests/fuse/milestone/` fixture must show identical stdout from both compilers.
- [ ] **B12.5.4** Any mismatch is a bug; root-cause and fix.

**Deliverables:** T4 Parity pass.

**Success criteria:** Zero parity failures.

---

### Phase B12.6 — T5 Bootstrap

**What is the issue?** The three-generation self-compile check.

**What needs to be done?** Run `run_tests.py --bootstrap`.

**Tasks:**

- [ ] **B12.6.1** `python tests/stage2/run_tests.py --bootstrap`.
- [ ] **B12.6.2** Gen 0 (fusec → fusec2-bootstrap), Gen 1 (fusec2-bootstrap → fusec2-stage2), Gen 2 (fusec2-stage2 → fusec2-verified) all succeed.
- [ ] **B12.6.3** Gen 1 and Gen 2 artefacts are byte-identical.
- [ ] **B12.6.4** Core test suite passes with `fusec2-verified`.

**Deliverables:** T5 Bootstrap pass.

**Success criteria:** Bootstrap completes; Gen 1 == Gen 2.

---

## Wave B13 — Institutional Knowledge & Document Sync

> **Purpose:** Fold the findings of this plan back into the permanent
> documentation and update the memory of the project so the next Claude
> session or the next engineer does not re-do the investigation.
>
> **Before starting this wave:** B12 must be complete.

---

### Phase B13.1 — `learning.md` Entries

**What is the issue?** Every fix in this plan must have a post-mortem in
`docs/learning.md` so future debugging can find it by symptom.

**What needs to be done?** For each wave that introduced a fix, add an
`L###` entry.

**Tasks:**

- [ ] **B13.1.1** L026: Nondeterministic codegen (B1).
- [ ] **B13.1.2** L027: Checker silence on unresolved extension methods (B2).
- [ ] **B13.1.3** L028: Enum variant payload types discarded (B3).
- [ ] **B13.1.4** L029: Generic return substitution at extension call sites (B4).
- [ ] **B13.1.5** L030: Hardcoded specialization vs extension dispatch ordering (B5).
- [ ] **B13.1.6** L031: User-defined enum variant binding (B6).
- [ ] **B13.1.7** L032: Match-as-expression arm unification (B7).
- [ ] **B13.1.8** L033: Module-qualified static method calls (B8).
- [ ] **B13.1.9** L034: Tuple field access type propagation (B9).
- [ ] **B13.1.10** L035: F-string brace escape (B10).

**Deliverables:** 10 new learning entries.

**Success criteria:** Every entry follows the template: what happened,
why, root cause with file:line citation, fix plan referencing this
document, status.

---

### Phase B13.2 — Update `fuse-stage2-plan.md`

**What is the issue?** The parent plan's W7.5 bootstrap checkbox was
marked done prematurely. Fix the status and add a reference to this
plan.

**What needs to be done?** Update Wave 7 status in
`docs/fuse-stage2-plan.md` with a note pointing to this document.

**Tasks:**

- [ ] **B13.2.1** Edit `docs/fuse-stage2-plan.md` — add a "Parity & Bootstrap Completion" pointer in the Wave 7 section and in the Task Summary table.

**Deliverables:** Updated parent plan.

**Success criteria:** Reader arrives at this plan from the parent plan.

---

### Phase B13.3 — Update `fuse-stage2-test-plan.md`

**What is the issue?** The test plan says "T4 Parity reuses the existing
145+ core fixtures. No new files needed" without acknowledging the
compiler gaps that block it. Add a reference to this plan.

**What needs to be done?** Add a note in the T4 Parity and T5 Bootstrap
sections pointing to this plan.

**Tasks:**

- [ ] **B13.3.1** Edit `docs/fuse-stage2-test-plan.md` T4 section.
- [ ] **B13.3.2** Edit `docs/fuse-stage2-test-plan.md` T5 section.
- [ ] **B13.3.3** Add `fuse-stage2-parity-plan.md` to the companion documents list at the bottom of the test plan.

**Deliverables:** Cross-references from the test plan to this plan.

**Success criteria:** A reader of the test plan who wonders "why is T4
blocked?" finds this document.

---

### Phase B13.4 — Update `t4-parity-investigation.md`

**What is the issue?** The investigation document was an analysis, not a
plan. Now that the plan exists and has executed, the investigation
should point forward.

**What needs to be done?** Add a "Superseded by" header to
`docs/t4-parity-investigation.md` pointing at this plan and at
`docs/learning.md` for the per-issue post-mortems. Do not delete the
investigation — it is valuable history.

**Tasks:**

- [ ] **B13.4.1** Add the header note at the top of `docs/t4-parity-investigation.md`.
- [ ] **B13.4.2** Ensure every Option (A/B/C/D) in the investigation is clearly marked with which option this plan took. (Answer: Option A, complete the fix, no compromises.)

**Deliverables:** Updated investigation doc.

**Success criteria:** A reader arriving at the investigation is
redirected to this plan for the resolution.

---

## Closing Criteria for the Entire Plan

This plan is complete when **all** of the following are true:

1. Every phase status is `[x]`.
2. `stage1/target/release/fusec.exe stage2/src/main.fuse -o fusec2.exe` succeeds on a clean build.
3. `python tests/stage2/run_tests.py` is green (all tiers).
4. `python tests/stage2/run_tests.py --parity` is green.
5. `python tests/stage2/run_tests.py --bootstrap` is green.
6. `cargo test -p fusec`, `cargo test -p fuse-runtime`, `cargo test -p fuse-lsp` are all green.
7. The determinism harness from B0.3 shows 10/10 identical builds.
8. `tests/stage2/known_failures.txt` remains empty.
9. Every fix has a `learning.md` entry.
10. The memory (`project_overview.md`) reflects bootstrap-verified state.

**When all ten are true, and only then, can the project say "Stage 2
self-hosting is complete."**

---

*Document created: 2026-04-10.*
*Author: Claude (Opus 4.6, 1M context) in session with Tembo Nyati.*
*Parent plan: [docs/fuse-stage2-plan.md](fuse-stage2-plan.md).*
*Motivating investigation: [docs/t4-parity-investigation.md](t4-parity-investigation.md).*
