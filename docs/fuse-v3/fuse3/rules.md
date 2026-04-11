# Fuse Project Rules

> **Read this file at the start of every session.**
> **Include this file in every AI agent prompt that touches this repository.**
>
> This document is the canonical list of discipline rules for the Fuse project. It is designed to be loaded into an AI agent's context on every invocation, so it is dense and imperative. It does not explain the rationale at length; the companion documents do.
>
> **Companion documents** (same directory):
> - `language-guide.md` — the language specification.
> - `repository-layout.md` — the physical layout of the repository.
> - `implementation-plan.md` — the wave-by-wave build plan.
>
> This document uses **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **NEVER** with their RFC 2119 meanings. **NEVER** is stronger than **MUST NOT**: it is a permanent prohibition, not merely a current rule.

---

## Table of contents

1. [Quickstart for AI agents](#1-quickstart-for-ai-agents)
2. [Language guide precedence](#2-language-guide-precedence)
3. [Compiler architecture invariants](#3-compiler-architecture-invariants)
4. [Bug policy](#4-bug-policy)
5. [Stdlib policy](#5-stdlib-policy)
6. [Testing rules](#6-testing-rules)
7. [Determinism rules](#7-determinism-rules)
8. [External dependency rules](#8-external-dependency-rules)
9. [Commit and PR rules](#9-commit-and-pr-rules)
10. [Learning log rules](#10-learning-log-rules)
11. [Multi-machine workflow](#11-multi-machine-workflow)
12. [Safety and `unsafe`](#12-safety-and-unsafe)
13. [Permanent prohibitions](#13-permanent-prohibitions)
14. [AI agent behavior](#14-ai-agent-behavior)

---

## 1. Quickstart for AI agents

Before you write a single line of code in this repository, you MUST:

1. **Read `language-guide.md`** (at least skim its table of contents). If a feature is not in the guide, it does not exist; do not implement it.
2. **Read `implementation-plan.md`** to locate the current wave. Work belongs to a wave and a phase inside that wave. Unscheduled work is out of scope.
3. **Read `repository-layout.md`** to learn where things live. Do not invent new top-level directories.
4. **Open `docs/learning-log.md`** and scan the last ~20 entries. If the current task relates to a known active bug, reference its entry number in your commit.
5. **Check `git status`.** If the working tree is not clean, ask the user before taking destructive actions (see §11).

Before you commit anything, you MUST:

1. Run `fuse check` on every file you touched.
2. Run `fuse test` on any wave whose phase is testable.
3. Run the invariant walkers (debug builds automatically run them; CI runs them; you must not disable them).
4. Verify that `git diff` contains nothing you did not intend.
5. Write a commit message in the style of §9.

**When in doubt, stop and ask.** It is cheaper to stop and ask the user a clarifying question than to produce a large change in the wrong direction.

---

## 2. Language guide precedence

**Rule 2.1 — Guide precedes implementation.**
No feature MAY appear in the compiler, runtime, or stdlib without first appearing in `language-guide.md`. If the task requires a new feature, update the guide first, get the change reviewed, then implement. A compiler pass that implements a feature the guide does not describe MUST be rejected in review, regardless of how correct the pass is.

**Rule 2.2 — The guide is normative.**
If the compiler and the guide disagree, the compiler is wrong. File a learning log entry. Fix the compiler.

**Rule 2.3 — Silence means absence.**
If the guide is silent on a feature, that feature does not exist. A contributor who wants to argue "but the guide doesn't say I can't" is missing the point: the guide is a positive specification, not a list of prohibitions.

**Rule 2.4 — Reserved keywords are not free real estate.**
The lists in §19.1–§19.3 of the guide enumerate every active and reserved keyword. A contributor MUST NOT introduce a new keyword without updating the guide's keyword list and getting review.

**Rule 2.5 — Decorators are not a second syntax.**
Fuse uses `@value`, `@rank`, `@export`, `@override`, and (reserved) `@simd`. These are the only decorators. A contributor MUST NOT invent new decorators. Traits describe behavior, decorators describe directives, and the two do not mix.

---

## 3. Compiler architecture invariants

The compiler has three intermediate representations: **AST → HIR → MIR**. Every contribution to the compiler MUST respect the following eight rules.

**Rule 3.1 — Three IRs, no shortcuts.**
AST is pure syntax. HIR is typed, resolved, and fully annotated with per-node metadata (`Type`, `Ctx`, `LiveAfter`, `Owning`, `DivergesHere`). MIR is flattened, explicit (`Drop`, `Move`), and close to C. A pass that reaches across two IR boundaries (e.g., AST → MIR) is rejected.

**Rule 3.2 — Separate types per IR.**
`ast.Expr`, `hir.Expr`, and `mir.Expr` are **disjoint Go interfaces**. HIR nodes MUST NOT be constructible without their full metadata populated. A HIR node with `Type = Unknown` after the checker pass is a hard error. This rule makes "silent checker bugs" a type-level impossibility.

**Rule 3.3 — Exhaustive node kind lists, frozen at language-guide freeze.**
The set of AST node kinds, HIR node kinds, and MIR node kinds are each an exhaustive enumeration. Adding a kind requires an ADR (architecture decision record) and a review. Every pattern match on a node kind MUST be exhaustive; the compiler's own CI enforces this as a structural rule, not a lint.

**Rule 3.4 — Every pass declares its reads and writes.**
Passes are registered in a manifest that names the metadata fields they read and the ones they write. The pass runner topologically sorts passes by their declared dependencies. A pass that writes a field no one reads, or reads a field no one writes, is a build error.

**Rule 3.5 — Invariant walkers at every pass boundary.**
After each pass, a debug-build invariant walker runs over the IR and asserts the invariants the pass is supposed to have established (`"every HIR node has Type != Unknown"`, `"every owned local has a matching Drop on every path"`, etc.). Invariant walkers run in debug builds **and** in CI. A contributor MUST NOT disable an invariant walker to get a build green. Fix the underlying bug.

**Rule 3.6 — Deterministic collections in IR data structures.**
No Go `map[K]V` inside any type under `hir.*` or `mir.*`. Use ordered-map or slice-based structures. The reason is determinism: Go's map iteration is randomized, and using maps in the IR produces output that changes from build to build. Maps may appear in compiler **side tables** (e.g., name → symbol lookups during a single pass) but MUST NOT be used to order the IR itself or anything that flows into codegen.

**Rule 3.7 — Global `TypeTable` with interned type IDs.**
Types are interned into a global `TypeTable` and referred to as `TypeId` (a u32). Equality of types is integer comparison. A contributor MUST NOT introduce a second route for type equality or a second representation of "the same type."

**Rule 3.8 — Property-based tests on IR lowering.**
Every IR lowering (AST → HIR, HIR → MIR) has a property test that generates random valid input and verifies an observable invariant (type-preservation, semantics-preservation on an interpreter, roundtrip through a pretty-printer). Contributors adding a new pass MUST add a property test before merging. The property-test corpus is checked in and deterministic (§7).

**Rule 3.9 — Liveness is computed once.**
Liveness (and therefore ASAP destruction) is computed exactly once per function, during HIR lowering. The result is stored as `LiveAfter` metadata per HIR node and consumed by every later pass. A contributor MUST NOT recompute liveness in codegen or anywhere else. If a pass needs more information, extend the single liveness computation to produce it.

---

## 4. Bug policy

**Rule 4.1 — The stdlib is the stress test.**
Any bug discovered while implementing the stdlib is a **compiler bug**, not a stdlib bug. It MUST be fixed in the compiler, not worked around in the library. Zero exceptions.

**Rule 4.2 — No workarounds.**
A workaround is any piece of code that exists because a different piece of code is wrong. Workarounds are forbidden in the compiler, the runtime, and the stdlib. If a workaround is the only way to make progress, the task is blocked; file a learning log entry and escalate.

**Rule 4.3 — Fix root causes, not symptoms.**
A symptom fix makes the immediate test pass. A root-cause fix removes the possibility of the bug. When the two diverge, the root-cause fix is the only acceptable option. A symptom fix is grounds for PR rejection.

**Rule 4.4 — Every bug gets a learning log entry.**
Every bug that takes more than a few minutes to diagnose gets an entry in `docs/learning-log.md` (§10). Small, obvious typos do not. A good heuristic: if you learned something, log it.

**Rule 4.5 — Regression tests before the fix.**
A bug fix lands with a test that would have caught the bug. The test MUST be part of the same PR as the fix and MUST fail on the pre-fix state (verify by reverting the fix locally and running the test).

**Rule 4.6 — The bootstrap test compiles the real Stage 2 source.**
The bootstrap / self-host test MUST compile the real `stage2/src/main.fuse`, not a synthetic test input. A test that passes on a trivial program while the real self-host is broken is worse than no test.

---

## 5. Stdlib policy

**Rule 5.1 — Stdlib is Fuse, not C.**
Everything above the runtime surface (§15 of the language guide) is written in Fuse, not in C. The C runtime provides ~40 entry points and nothing more. Strings, lists, maps, hashing, and formatting are Fuse code.

**Rule 5.2 — Core is OS-free.**
The `core` tier MUST NOT import `full` or `ext`. It MUST NOT call any `fuse_rt_*` function that touches the OS (file I/O, time, process). Core is what runs on freestanding targets.

**Rule 5.3 — Full may depend on Core. Ext may depend on Full or Core.**
Dependencies flow one way: Ext → Full → Core. A reverse dependency is a build error. Circular dependencies inside a tier are a build error.

**Rule 5.4 — No hidden special cases.**
The stdlib MUST NOT hardcode "special behavior for `Int` and `String`" anywhere. `Map[K, V]` uses `Hashable`. `Sorted[T]` uses `Comparable`. If a primitive needs a custom behavior, it implements the trait like everything else.

**Rule 5.5 — No `Ptr[T]` in public APIs.**
`Ptr[T]` is for FFI only. A public stdlib function MUST NOT take or return `Ptr[T]`. Internal FFI glue files are exempt and are listed by name in `repository-layout.md`.

**Rule 5.6 — Every public stdlib function has a doc comment.**
`fuse doc` must produce a complete reference. A public function without a `///` doc comment is rejected in review.

**Rule 5.7 — Auto-generation follows the field metadata rule.**
`@value struct` and `data class` auto-derive the Core trait set (§11.7 of the guide). Plain `struct` does not. A contributor MUST NOT paper over the distinction by hand-implementing the Core trait set on a plain `struct` when the right answer is to change it to `@value`.

---

## 6. Testing rules

**Rule 6.1 — Golden tests are deterministic and reviewable.**
Every compiler output test compares the compiler's output against a checked-in golden file. Goldens are byte-for-byte and updated only by explicit `--update-goldens` runs. A PR that updates a golden MUST include a one-line explanation per updated file in the commit message.

**Rule 6.2 — Tests are isolated.**
A test MUST NOT depend on the order of test execution, the ambient environment, the current working directory outside of its own sandbox, or any network resource. Tests that need time MUST use an injected fake clock.

**Rule 6.3 — Tests name the invariant, not the scenario.**
A test file named `scenario_42.fuse` is rejected. A test named `return_type_checked_for_mismatched_expr.fuse` is fine. The test's name tells a future contributor what it guards.

**Rule 6.4 — Property tests are reproducible.**
Every property test has a seed. Failures print the seed. Re-running with the same seed reproduces the failure. Random test generation without seed printing is a rejection.

**Rule 6.5 — Bootstrap test runs on every push to `main`.**
CI compiles `stage2/src/main.fuse` with the current Stage 1, then compiles it again with the resulting Stage 2, and compares the two outputs. This is the three-generation reproducibility gate.

**Rule 6.6 — Integration tests hit a real backend when possible.**
A compiler test that relies on a mocked `cc` is acceptable only for the specific thing it is testing (argument passing, flag forwarding). A test of "the compiler produces a working binary" MUST actually invoke `cc` and run the binary.

**Rule 6.7 — Test fixtures live next to the pass they exercise.**
Fixtures for the parser live next to the parser. Fixtures for the checker live next to the checker. A global `tests/` pile is the worst of both worlds; it exists (see `repository-layout.md`) only for end-to-end fixtures.

---

## 7. Determinism rules

**Rule 7.1 — Same input, same bytes.**
`fuse build` on identical inputs with identical compiler version and identical target triple MUST produce byte-identical artifacts. CI runs every build twice and compares. A failure is a release blocker.

**Rule 7.2 — No nondeterministic iteration in user-visible positions.**
IR data structures MUST NOT use Go's built-in `map`. Symbol ordering, diagnostic ordering, and codegen output are all affected by iteration order and MUST use deterministic collections.

**Rule 7.3 — No ambient randomness.**
The compiler MUST NOT call `rand.Read` or `time.Now()` in a path that affects output. Exceptions: the symbol-uniquifier for fresh temporaries may use a deterministic counter, not a random number.

**Rule 7.4 — Symbol mangling is stable.**
A given Fuse function's mangled C symbol is a deterministic function of its module path, name, and type signature. It MUST NOT depend on compile time, file order, or hash randomization.

**Rule 7.5 — `SOURCE_DATE_EPOCH` is respected.**
If the environment variable is set, the compiler uses it as the source of any embedded timestamp. If it is not set, no timestamp is embedded.

**Rule 7.6 — Goldens never contain timestamps or absolute paths.**
Test goldens use normalized paths (relative to the test root) and never embed a timestamp. A golden that fails because today is Tuesday is a bug in the golden.

---

## 8. External dependency rules

**Rule 8.1 — No runtime spill from the host language.**
The Fuse compiler is implemented in Go, but **no Go runtime code is allowed in emitted Fuse programs**. The compiler emits C11; the C11 is compiled by `cc`; the resulting binary links only against libc and (on non-Windows) pthread. This rule is load-bearing for Pillar 1 and is non-negotiable.

**Rule 8.2 — Ambient tools are allowed.**
The compiler MAY depend on the presence of the following tools at **runtime**:
- A C11 compiler reachable as `cc` or via the `CC` environment variable.
- `clang --target=wasm32-wasi` plus the WASI SDK, for WASM targets.
- A system linker (`ld`, `link.exe`, `lld`).
- `libc` and (on non-Windows) `pthread`.

These are tools the developer already has. They do not spill into the artifact.

**Rule 8.3 — The Go compiler has zero external Go dependencies.**
The Go `go.mod` file lists **zero** non-standard-library dependencies. Argument parsing is hand-rolled. JSON is hand-rolled (if needed during Stage 1; Stage 2 uses the Fuse stdlib). Testing uses the Go standard `testing` package. Anything that requires `go get` from an external module is forbidden.

**Rule 8.4 — Stdlib has zero non-Fuse dependencies.**
The Fuse stdlib MUST NOT shell out, MUST NOT link to C libraries beyond libc and the `fuse_rt_*` runtime, and MUST NOT depend on the compiler's host language in any way.

**Rule 8.5 — WASM is a target, not a backend.**
The compiler emits C11 for every target. WASM is reached via `clang --target=wasm32-wasi`. A contributor MUST NOT add a second backend (direct WASM, LLVM IR, Cranelift, QBE, etc.) without an ADR that makes the case and gets explicit review.

---

## 9. Commit and PR rules

**Rule 9.1 — One logical change per commit.**
A commit that mixes a feature, a refactor, and a formatting pass is split. A reviewer should be able to describe a single commit in one sentence.

**Rule 9.2 — Commit subject format.**
Commit subjects follow `<area>: <subject>`, where `<area>` is one of:

- `compiler/lex`, `compiler/parse`, `compiler/resolve`, `compiler/check`, `compiler/lower`, `compiler/codegen`
- `runtime`
- `stdlib/core`, `stdlib/full`, `stdlib/ext`
- `cli`
- `tests`
- `docs`
- `ci`
- `tools`

Example: `compiler/check: reject return expr whose type disagrees with signature`.

Subjects MUST be under 72 characters. Body paragraphs explain **why**, not what.

**Rule 9.3 — New commits, never amend.**
A contributor MUST NOT `git commit --amend` a commit that has been pushed to any shared branch. If a commit message has an error, add a new commit. If a commit's content is wrong, add a new commit that fixes it.

**Rule 9.4 — Never force-push to `main`.**
Force-push to `main` is forbidden, full stop. Force-push to a topic branch owned by one person is allowed.

**Rule 9.5 — Commit before switching machines.**
A contributor working on multiple machines MUST commit and push before switching machines. Uncommitted work is a source of lost changes and duplicated effort. See §11.

**Rule 9.6 — Pre-commit hooks are not skipped.**
The `--no-verify` flag is forbidden. If a pre-commit hook fails, the contributor fixes the underlying issue and commits again. A hook that is wrong is fixed, not bypassed.

**Rule 9.7 — Every PR runs the full test suite.**
CI runs the full test suite on every PR. A PR cannot land with a failing test, even if the test is "unrelated." An "unrelated" failure is either a flaky test (fix the flake) or the PR did break something (fix the regression).

**Rule 9.8 — Co-author trailers.**
Commits that include AI-agent-authored changes MUST add a co-author trailer identifying the agent. The exact trailer format is up to the contributor and the team; the requirement is that the commit is honest about its authorship.

---

## 10. Learning log rules

**Rule 10.1 — There is one learning log, at `docs/learning-log.md`.**
The log is an ordered, append-only, numbered list of lessons learned. Every entry has a number (`L001`, `L002`, ...), a title, a date, and a short body.

**Rule 10.2 — Entries are never rewritten.**
An entry is an artifact. If it is wrong or outdated, a new entry supersedes it and references the old one by number. Editing an old entry is forbidden.

**Rule 10.3 — Active entries are full; mature entries are summarized.**
An active bug gets a full entry: reproducer, root cause, fix, verification. Once a bug is well-understood and its area is stable, the entry is replaced by a one-line summary in a "mature lessons" section at the top of the file. The full entry is preserved in history.

**Rule 10.4 — The log is flat and greppable.**
The log is a single file. `grep -n` on it works. There is no index, no database, no per-wave subfolder. Flat file, numbers, titles, dates.

**Rule 10.5 — Every non-trivial bug produces a log entry.**
The test of "was this non-trivial?": did you learn something? If yes, log it. If you would want your past self to have known this before starting, log it.

**Rule 10.6 — Log entries reference commits, not vice versa.**
A learning log entry cites the commit that fixed it. A commit does **not** have to cite the log entry — the direction of reference is log → commit, so commits can be written quickly and the log stays coherent.

---

## 11. Multi-machine workflow

The user works on more than one machine. The following rules keep machines in sync and avoid duplicated work.

**Rule 11.1 — Push before leaving a machine.**
Before stepping away from a machine for more than a short break, the contributor MUST commit and push any in-progress work. An explicit "WIP" commit is acceptable for work-in-progress; it is squashed or amended only after the branch lands.

**Rule 11.2 — Pull at the start of every session.**
The first action on any machine, at the start of any session, is `git pull`. An AI agent starting a session MUST run `git status` and `git log --oneline -5` as orientation before doing anything else.

**Rule 11.3 — State lives in files, not in heads.**
Anything future contributors or future sessions need to know MUST live in a file in the repository. The learning log, design notes, the implementation plan — all are files. A contributor who "remembers" the state of a task without it being written down is a contributor who will forget.

**Rule 11.4 — Keep the working tree clean.**
At the end of every session, the working tree is clean (`git status` shows nothing). Untracked files are either committed, added to `.gitignore`, or deleted. A working tree with leftover debug scripts is a future hazard.

**Rule 11.5 — Branches name the task.**
A feature branch is named `wNN.PP.TT/<short-description>` where `wNN` is the wave, `PP` is the phase, `TT` is the task number (see the implementation plan). Short-lived fix branches are named `fix/<short-description>`. Personal branches with no task link should not exist on the shared remote.

**Rule 11.6 — Merges are explicit.**
`main` is updated by merge commits from topic branches, with non-fast-forward merges (`--no-ff`). This leaves a visible merge point for every landed topic.

---

## 12. Safety and `unsafe`

**Rule 12.1 — `unsafe { }` at every weakening site.**
Every call to an `extern fn`, every `Ptr[T]` dereference, and every call to a Fuse `unsafe fn` MUST appear inside an `unsafe { }` block. This is enforced by the checker; a contributor MUST NOT disable the check.

**Rule 12.2 — `#![forbid(unsafe)]` is the stdlib default.**
Every stdlib module starts with `#![forbid(unsafe)]` **unless** it is one of the small set of bridge files that touch the runtime. The bridge files are listed by name in `repository-layout.md`. A new bridge file MUST be added to that list in the same PR that creates the file.

**Rule 12.3 — `unsafe { }` blocks have justification comments.**
Every `unsafe { }` block is preceded by a comment that names the invariant being relied on:
```fuse
// SAFETY: len is bounded above by buf.capacity() because we just allocated
// buf with capacity >= len on line 42.
unsafe { fuse_rt_memcpy(dst, src, len); }
```
A block without a `// SAFETY:` comment is a rejection.

**Rule 12.4 — FFI ownership is documented at the call site.**
If an `extern fn` takes a pointer the Fuse caller is expected to own, free, retain, or ignore, the call site has a comment stating which. FFI raw pointers have no compiler-tracked ownership; the comment is the only contract.

**Rule 12.5 — Closures that escape MUST use `move`.**
A closure that outlives its defining scope MUST annotate its captures with `move`. The compiler rejects the implicit form; a contributor MUST NOT add a workaround that heap-allocates captures silently.

---

## 13. Permanent prohibitions

The following are **NEVER** going to happen in Fuse. They are closed questions. A contributor proposing any of these is directed to this list.

**NEVER: `async` / `await`.**
The concurrency model is `Chan[T]`, `Shared[T] + @rank`, and `spawn`. An executor, a Future type, and the `async`/`await` keywords are permanently off the table. The keywords are reserved only to produce a helpful error when a user tries them.

**NEVER: A second backend.**
The compiler has one backend: C11. WASM, macOS, Linux, and Windows are targets reached through the C11 backend and an appropriate `cc`. A direct-to-LLVM, direct-to-WASM, direct-to-machine-code, or direct-to-QBE backend is out.

**NEVER: A tracing GC.**
Pillar 1 is non-negotiable.

**NEVER: A borrow checker.**
Pillar 2 is non-negotiable. Ownership discipline is provided by the four keywords (`ref`, `mutref`, `owned`, `move`), the liveness pass, and the escape-closure rule. No lifetime variables, no `'a`, no region inference.

**NEVER: Host language runtime in emitted binaries.**
The compiler is written in Go, but the Go runtime, GC, and scheduler NEVER appear in an emitted Fuse program. This is checked at the link step: the linked binary's external symbol list is scanned and any symbol matching a Go runtime pattern is a release blocker.

**NEVER: A package manager that downloads code.**
Day one has no dependency resolver. Consumers vendor what they need. A future package manager might appear, but it will not be present in day one and its design will be done from scratch when it is.

**NEVER: Automatic semicolon insertion.**
Statements end with `;`. No exceptions.

**NEVER: Implicit numeric conversion.**
`I32` to `I64` is `.toI64()`. No exceptions.

**NEVER: Exceptions, null, or silent panics as control flow.**
Errors are `Result`, absence is `Option`, panics abort the thread. No try/catch, no null checks, no recoverable panic.

**NEVER: Trailing commas.**
They mask arity mistakes.

---

## 14. AI agent behavior

This section is for AI agents reading the repository as part of an autonomous or semi-autonomous session.

**Rule 14.1 — Load this file first.**
An AI agent beginning a session on this repository MUST load `rules.md` (this file) into context before doing anything else. If the agent's prompting harness supports "always include files," this file is the first one.

**Rule 14.2 — Stop and ask when in doubt.**
An AI agent that is uncertain about the intended direction of a change MUST stop and ask the user, rather than make a guess. The cost of a clarifying question is cheap. The cost of a large change in the wrong direction is high.

**Rule 14.3 — One wave at a time.**
An AI agent MUST NOT work on tasks that span multiple waves in a single session. Pick a wave. Pick a phase inside it. Pick a task inside the phase. Finish it, commit, push, move on.

**Rule 14.4 — Reference the task ID.**
A commit message for wave work includes the task ID in the body (`Wave W03.P2.T4`). This lets a future reader see which plan item the commit implements.

**Rule 14.5 — Never delete user work.**
An AI agent MUST NOT run destructive git commands (`reset --hard`, `clean -f`, `branch -D`, force-push) without explicit user authorization for that specific command in that specific session. A previous authorization for a different operation does not extend.

**Rule 14.6 — Never bypass safety checks.**
An AI agent MUST NOT use `--no-verify`, MUST NOT disable a test to make a build green, MUST NOT disable an invariant walker, and MUST NOT remove a `#![forbid(unsafe)]` line to work around a checker error. The check exists because it was needed; its firing is a signal, not an obstacle.

**Rule 14.7 — Never invent features.**
An AI agent MUST NOT add a language feature, a new CLI subcommand, a new runtime entry, or a new top-level directory without first updating the relevant document (the language guide, this file, or the repository layout). "The guide doesn't mention it" is a reason to stop, not to proceed.

**Rule 14.8 — State is in files.**
An AI agent MUST NOT rely on "what it remembers from a previous turn" in a new session. Every piece of state an agent needs is in a file under version control. If it is not, put it in one before the session ends.

**Rule 14.9 — Commit discipline.**
An AI agent MUST:
- Create new commits; never `--amend` a pushed commit.
- Write commit messages that explain **why** in the body.
- Include the relevant task ID.
- Co-author trailer to identify the agent.
- Push before the session ends if the work is complete.
- Not push if the work is incomplete and the branch is shared; use a topic branch instead.

**Rule 14.10 — Multi-machine handoff.**
An AI agent working on a machine that is one of several the user uses MUST:
- `git pull` at the start.
- Commit and push at the end, even WIP.
- Leave the working tree clean.
- Not assume local state (untracked files, uncommitted changes, branch positions) matches what another machine sees.

**Rule 14.11 — Respect the permanent prohibitions in §13.**
An AI agent that proposes a feature listed in §13 is wasting the user's time. The list is closed. If a user asks for one of them anyway, cite §13 before proceeding.

**Rule 14.12 — The learning log is append-only.**
An AI agent MUST NOT rewrite a learning log entry. New information goes in a new entry that references the old one by number.

**Rule 14.13 — Ask before expanding scope.**
If a task reveals additional work ("while I'm here I could also..."), the AI agent MUST stop and ask the user before expanding the scope. One task, one commit, one PR.

**Rule 14.14 — Test before declaring done.**
An AI agent MUST NOT report a task as complete without running the relevant tests and verifying they pass. "Probably works" is not a completion report. A task is done when its tests pass on the current working tree.

**Rule 14.15 — Small outputs, not walls of text.**
An AI agent reporting progress to the user SHOULD produce short, structured updates: what was done, what is next, what is blocked. Multi-page narratives about what was considered but not done are noise.

---

*End of rules.*
