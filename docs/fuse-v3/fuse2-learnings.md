# Fuse2 Learnings — Source Material for the Fuse3 Language Guide

> **Purpose.** One document that carries everything worth learning from the
> Fuse2 journey into the Fuse3 design conversation. Read this before writing
> the Fuse3 language guide. Every claim here is cited back to a Fuse2 doc,
> ADR, learning entry, or bug log so you can dig deeper on demand.
>
> **Scope.** Philosophy, language design, compiler architecture, concurrency
> model, stdlib structure, bug taxonomy, process rules, and — most
> importantly — *what hurt and why*. This is not a Fuse3 plan. It is the
> raw material a plan will be built on.
>
> **How to read.** Skim Part 1 (philosophy) and Part 9 (crosswalk) first.
> Then jump to whichever part informs the next Fuse3 design decision you
> need to make.

---

## Table of Contents

**Part 1 — Non-negotiable foundation (what MUST carry over)**
- 1.1 The three pillars
- 1.2 Language DNA (what Fuse steals from production languages)
- 1.3 Core vs Full tiering
- 1.4 Fuse3 staging: Go → Fuse → Fuse (the two-stage contract)
- 1.5 Module system decisions (locked for Fuse3)

**Part 2 — Type system and memory model**
- 2.1 Ownership: four keywords, no borrow checker
- 2.2 ASAP destruction
- 2.3 Result/Option/? — error handling without null or exceptions
- 2.4 The type catalogue (primitives, compound, user-defined)
- 2.5 Pattern matching and arm unification
- 2.6 Memory safety case audit (every allocation path)

**Part 3 — Concurrency (user flagged for from-scratch redesign)**
- 3.1 The three-tier model
- 3.2 `@rank` compile-time deadlock prevention
- 3.3 Scheduling model: single-thread → OS threads → green threads
- 3.4 What got rejected: async/await, actors, select, SpawnHandle
- 3.5 Open questions for Fuse3

**Part 4 — Compiler architecture (what worked, what broke)**
- 4.1 Three-stage bootstrap: rationale and cost
- 4.2 Uniform `FuseHandle` ABI — blessing and curse
- 4.3 ASAP analysis, future uses, dead binding release
- 4.4 Checker → codegen information loss (the recurring disease)
- 4.5 Determinism: the HashMap lesson
- 4.6 Diagnostics as a first-class concern
- 4.7 Fuse3 compiler architecture invariants (the eight rules)

**Part 5 — Stdlib design**
- 5.1 Core / Full / Ext tiers
- 5.2 Auto-generation from field metadata (ADR-013)
- 5.3 The stdlib-first test strategy and its yield
- 5.4 Maps (user flagged for from-scratch redesign)

**Part 6 — Bug taxonomy (what actually broke)**
- 6.1 L001–L029: codegen fundamentals, control flow, lambdas, patterns, generics, ASAP
- 6.2 B-wave taxonomy: determinism, silent checker, generic substitution, enum payloads, match unification, tuple propagation, f-strings
- 6.3 B12 session (new): the 8 ABI-mismatch cascade that finally built fusec2
- 6.4 Pattern analysis: what classes of bug bit the most

**Part 7 — Process learnings**
- 7.1 The "plan doc, execute strictly" pattern
- 7.2 The "no corners cut" rules
- 7.3 Bug Policy (stdlib as stress test)
- 7.4 Rules that worked, rules that didn't

**Part 8 — Items flagged for fresh design in Fuse3**
- 8.1 Concurrency — user directive
- 8.2 Maps — user directive
- 8.3 Thread memory management — user directive
- 8.4 Other candidates surfaced by the learnings

**Part 9 — Crosswalk: Fuse2 pain → Fuse3 design implications**

**Part 10 — Open design questions for the Fuse3 language guide**

---

# Part 1 — Non-negotiable foundation

## 1.1 The three pillars

These are the contract. They appear verbatim in
[`docs/fuse-language-guide-2.md §1.1`](../fuse-language-guide-2.md) and are
restated at the top of every plan doc
([`fuse-pre-stage2.md`](../fuse-pre-stage2.md),
[`fuse-stage2-plan.md`](../fuse-stage2-plan.md),
[`fuse-stage2-parity-plan.md`](../fuse-stage2-parity-plan.md),
[`fuse-post-stage2.md`](../fuse-post-stage2.md)). Every Fuse3 design
decision must serve all three:

1. **Memory safety without garbage collection.** Deterministic, not
   tracing. No GC pauses, no manual free. Values are destroyed at their
   last use point. Implementation: **ASAP destruction** (§2.2).

2. **Concurrency safety without a borrow checker.** No lifetime
   annotations. No borrow wars. Four keywords
   (`ref` / `mutref` / `owned` / `move`) carry the complete ownership
   picture, and `Shared<T>` + `@rank(N)` proves the absence of deadlock
   *at compile time* (§2.1, §3.2).

3. **Developer experience as a first-class concern.** Clean syntax;
   helpful error messages; fast compilation. Specifically: *"every
   keyword, annotation, and error message is chosen so that reading code
   aloud produces a correct description of what it does."*

The pillars are non-negotiable. The pre-Stage 2 plan put it bluntly:

> If a change undermines memory safety, concurrency safety, or
> developer experience, it is wrong — regardless of how clever or
> expedient it may be.
> — [`docs/fuse-pre-stage2.md`](../fuse-pre-stage2.md)

**Fuse3 implication.** These pillars survive verbatim. The implementation
language changes (Go instead of Python+Rust), not the semantic contract.

## 1.2 Language DNA

Fuse is explicitly *not* a research language.

| Source | What Fuse takes |
|---|---|
| **Mojo** | `owned`/`mutref`/`ref` argument conventions, ASAP destruction, `@value` auto-lifecycle, `SIMD<T,N>` primitives |
| **Rust** | `Result<T,E>`, `Option<T>`, `?` error propagation, exhaustive `match` |
| **Kotlin** | `val`/`var` type inference, Elvis `?:`, optional chaining `?.`, `data class`, scope functions (`let`, `also`, `takeIf`) |
| **C#** | LINQ-style method chains (`.map`, `.filter`, `.sorted`) |
| **Python** | `f"..."` interpolation, `@decorator` syntax |
| **Go** | `spawn` (goroutines), `defer`, typed channels (`Chan<T>`) |
| **TypeScript** | Union types (`A \| B \| C`), optional chaining `?.`, `interface` constraints |

**Fuse3 implication.** Keep the source table. Reading a Fuse feature and
being able to point at the origin language is a sanity check: it
prevents designers from drifting into novel territory that hasn't been
validated elsewhere. When the answer to "why this?" is "because Rust
did it and it worked" or "because Go did it and it worked", you're on
safe ground.

## 1.3 Core vs Full tiering

From [`fuse-language-guide-2.md §1.3`](../fuse-language-guide-2.md#L77):

**Fuse Core** — minimum sufficient to write a compiler. `fn`, `struct`,
`data class`, `enum`, `@value`, `@entrypoint`; `val`/`var`/type
inference; `ref`/`mutref`/`owned`/`move`; `Result`/`Option`/`match`;
control flow (`if`/`for`/`while`/`loop`/`break`/`continue`/`return`);
`List<T>`/`Map<K,V>`/`String`/`Int`/`Float`/`Bool`; f-strings; `defer`;
extension functions; modules.

**Fuse Full** — added on top: `spawn`, `Chan<T>`, `Shared<T>`, `@rank`;
`SIMD<T,N>`; `interface`, `implements`, generic bounds.

**The rule:** implement Core first. A working Core interpreter validates
the language design before concurrency complexity is introduced. This
rule was the single most load-bearing phase decision in Fuse2. Stage 0
(the Python interpreter) shipped before anyone wrote a Cranelift back
end, and that sequencing paid dividends through phases 1–5.

**Fuse3 implication.** Keep the tier. The Core-first rule is
orthogonal to the language stack — it's about validating semantics
before touching machine code. Fuse3 inherits it unchanged: Stage 1
(Go) implements Fuse Core first, Fuse Full lands on top. The
sequence that worked for Fuse2 works for Fuse3.

---

## 1.4 Fuse3 staging: Go → Fuse → Fuse

**Fuse2 was a three-stage project:**

```
Stage 0   Python tree-walking interpreter
          Purpose:  validate language semantics before any codegen
          Outcome:  proved ASAP destruction is tractable, proved the
                    four-keyword ownership model works, proved
                    exhaustive match + arm unification rules are
                    implementable. Those answers are settled — they
                    live in this learnings doc and in the Fuse2
                    language guide.
          Status:   retired. Its job is done.

Stage 1   Rust compiler + Cranelift backend
          Purpose:  produce native binaries
          Status:   ~26K LOC of Rust, working, but heavy — three
                    crates (fusec, fuse-runtime, cranelift-ffi) and
                    a uniform FuseHandle ABI that produced the B12
                    bug cascade (§4.2, §6.3).

Stage 2   Self-hosted Fuse compiler (compiler written in Fuse)
          Purpose:  prove the language is complete
          Status:   in-flight; during the B12 triage session the
                    first real self-compile of stage2/src/main.fuse
                    succeeded end-to-end for a trivial Fuse program
                    (`@entrypoint fn main() {}`). Non-trivial
                    programs still hit one more ABI layer deeper.
```

**Fuse3 is a two-stage project.** There is no Fuse3 Stage 0. Fuse2's
Stage 0 already answered the "is this implementable?" question. That
experiment has been run, the answer is *yes*, and the results are
captured here. Redoing it in Python for Fuse3 would burn time and
teach nothing new.

```
Stage 1   Fuse compiler hosted in Go (replaces Fuse2's Rust Stage 1)
          Lexer, parser, checker, type inference, C-code emission.
          Written in Go for host-language leverage: strong standard
          library, fast iteration, stable tooling.

          Codegen path: emit portable C99 source + a small Fuse
          runtime (also C, ~500 LOC target) and invoke the system
          C compiler (cc / gcc / clang / msvc) with cross-target
          flags to produce native binaries for Windows, Linux,
          macOS on amd64 and arm64.

          No Go runtime linked into emitted binaries. No Go GC.
          No goroutine scheduler under the hood. No bundled
          codegen library — `cc` is an ambient OS tool the same
          way `link.exe` is, not a dependency we ship. The
          philosophy is honest all the way down: a Fuse program
          has exactly the runtime Fuse defines, nothing more.

          One Go program. Multiple modes:
            fuse run     — interpret (typecheck + tree-walk, for REPL/tests)
            fuse build   — emit C, invoke cc, produce native binary
            fuse check   — typecheck only, emit diagnostics
            fuse build --target=linux-arm64  — cross-compile

Stage 2   Self-hosted Fuse compiler (same purpose as Fuse2's Stage 2)
          Written in Fuse. Compiled first by Stage 1 in Go, then
          by itself. Once Stage 2 is self-hosting, the canonical
          story is "Fuse builds Fuse, end-to-end" — Stage 1 in Go
          retires to a boot-and-recover tool.

          The tiny Fuse runtime (initially ~500 LOC of C) is
          rewritten in Fuse over time, compiled by Stage 1,
          bundled as C headers or precompiled objects. At that
          point the only non-Fuse code in the shipped toolchain is
          libc + the system C compiler — both already present on
          every developer machine, neither part of the Fuse binary
          we ship.
```

**The bootstrap chain, post-self-host:**

```
Step 1:   Stage 1 (Go)      compiles stage2/src/main.fuse  →  fusec3-bootstrap
Step 2:   fusec3-bootstrap   compiles stage2/src/main.fuse  →  fusec3-stage2
Step 3:   fusec3-stage2      compiles stage2/src/main.fuse  →  fusec3-verified
Step 4:   sha256(fusec3-stage2) == sha256(fusec3-verified)  ← reproducibility proof
```

Fuse3 inherits this chain verbatim. It was Fuse2's strongest
correctness check and there's nothing to redesign.

**What this staging means for every Fuse3 design decision:**

- **"Should this feature exist now or wait?"** — the same question
  Fuse2 asked via Core vs Full. Stage 1 (Go) implements Fuse Core
  first — enough to write the self-hosted Stage 2 compiler. Fuse
  Full (concurrency, SIMD, traits) lands on top.

- **"What runtime primitives does Fuse3 need, minimum viable?"** —
  the C backend gives us total control over what ends up in the
  emitted binary, which means we have to enumerate the runtime
  surface explicitly. The minimum set targets ~500 LOC of C:
  malloc/free (or ASAP-friendly bump allocators), stdin/stdout/
  stderr via libc, file I/O, process exit, OS-thread spawn, atomics
  for `Shared[T]`, mutexes for rank-based locking, thread-local
  storage. Everything above that primitive layer — `Map`, `List`,
  `Option`, `Result`, `Chan`, `String` extensions — is written in
  Fuse itself and compiled by Stage 1.

  Fuse2 leaked this question. It imported Rust's entire
  `std::collections` and `std::sync` wholesale, which is a major
  reason the B12 ABI cascade had so many layers (runtime function
  signatures, string representations, handle conventions, uniform
  `FuseHandle` tagging). Fuse3 enumerates the runtime surface
  explicitly in the language guide and keeps it small.

- **"Where do we write new stdlib modules?"** — in Fuse, as soon
  as Stage 1 can compile them. Fuse2's Bug Policy (§5.3, §7.3)
  applies verbatim: any bug found while writing the stdlib is a
  compiler bug, fixed in the compiler, not worked around in the
  library.

- **"When does Stage 2 become the primary compiler?"** — the
  moment three-generation reproducibility passes on real Fuse
  programs. At that point Stage 1 in Go becomes a boot-and-recover
  tool: used to seed a fresh environment from source, used to
  recover from a self-hosting regression, but not the day-to-day
  compiler anyone runs. Fuse builds Fuse. End-to-end.

**Why the C backend specifically, not Go's native tools.** The
obvious path from "Stage 1 in Go" is to emit Go source and invoke
`go build`. Rejected: every Go toolchain path that produces a
native binary links in Go's runtime — goroutine scheduler, stack
growth, `defer` machinery, `mallocgc`. You can disable Go's GC at
startup via `debug.SetGCPercent(-1)` but the rest of the runtime
stays in the binary. A Fuse binary produced that way is "a Go
program pretending it isn't" — philosophy leak, whether or not
the user sees it. Pillar 1 ("memory safety without a GC") and
pillar 2 ("concurrency safety without a borrow checker") both
become lies the moment Go's runtime is a second-order authority
in the emitted binary. Option A (emit C + invoke system `cc`)
was selected specifically to preserve those pillars at the
binary level, not just at the source level.

**One implication worth making explicit.** The B12 B-wave bug
classes (§6.3) that took weeks to untangle in Fuse2 were produced
by Fuse2's split of the Rust `fuse-runtime` crate from the
`cranelift-ffi` crate from the `fusec` compiler — three
independently-versioned modules connected by an untyped uniform
pointer ABI. Fuse3 eliminates that split: one Stage 1 compiler
in Go, one small C runtime we own, one emitted C source file per
Fuse program. No bridge crate to version-drift. No opaque handle
convention. There is still a runtime boundary (between emitted
C and the runtime's C), but it's entirely under our control,
defined in one file, auditable in one reading. The architectural
headwind that produced the B12 cascade is gone.

That's the Fuse3 staging contract. Every other design question
in Part 10 is downstream of it.

---

## 1.5 Module system decisions (locked for Fuse3)

Fuse2's module system worked well and transfers unchanged, plus
two small additions:

**Inherited verbatim from Fuse2:**

- One file = one module. The file name is the module name.
- Import paths use dots, not slashes: `import stdlib.core.list`,
  `import a.b.c` → `src/a/b/c.fuse` relative to project root.
- `pub` marks exported items; everything is private by default.
- `import a.b.{X, Y}` imports specific items.
- `import a.b` imports the whole module; all public symbols are
  available directly, qualified access (`b.X`) also works.
- Circular imports are a compile error.

**New in Fuse3:**

1. **Module-level doc comments.** Fuse2 had function doc comments
   but no convention for module headers. Fuse3 adopts a Rust-style
   `//!` prefix at the top of a file:

   ```fuse
   //! stdlib.core.list
   //! Extension methods for List<T> — push, map, filter, etc.

   import stdlib.core.option

   pub fn List<T>.push(mutref self, item: T) { ... }
   ```

   Surfaced by the LSP on hover over the import name and by any
   future docgen.

2. **`pub import` for explicit re-exports.** Fuse2 had no clean
   way to build a prelude module that gathered symbols from
   children. Fuse3 adds the `pub` prefix on `import`:

   ```fuse
   // stdlib/core/prelude.fuse
   pub import stdlib.core.list
   pub import stdlib.core.map
   pub import stdlib.core.option
   pub import stdlib.core.result
   pub import stdlib.core.string
   ```

   Users write one line:

   ```fuse
   import stdlib.core.prelude
   // List, Map, Option, Result, String extensions all in scope
   ```

   Two concrete use cases this unblocks: batteries-included
   entry points (one import instead of many), and facade modules
   that re-export a stable public surface from a refactorable
   internal layout. Without `pub import`, Fuse2 users reached for
   hand-written wrapper functions — which don't work for types
   and are noise for everything else. Zero runtime cost; the
   resolver walks `import` chains already.

**Explicitly rejected:** trailing commas in import lists
(`import a.b.{X, Y,}`). Cosmetic, not worth the parser special
case.

---

# Part 2 — Type system and memory model

## 2.1 Ownership: four keywords, no borrow checker

The heart of Fuse's memory/concurrency story
([`fuse-language-guide-2.md §1.9`](../fuse-language-guide-2.md#L588)):

```
ref  ->  mutref  ->  owned  ->  move
read it    change it    own it     transfer it
```

| Convention | Where written | Mutates caller | Transfers | Runtime cost |
|---|---|---|---|---|
| `ref` | parameter | no | no | zero (pointer) |
| `mutref` | parameter + call site | yes | no | zero (pointer) |
| `owned` | parameter | n/a | callee decides | move or copy |
| `move` | call site only | n/a | yes, enforced | zero |

**Key design principle: call-site annotations.** `mutref` must appear at
both the signature *and* the call. Reading the call site tells you
which arguments will be modified without looking up the function
signature. From the ADR set (5.2, `mutref` not `inout`):

> `mutref` is self-documenting — mutable reference. `inout` is audio
> engineering jargon.

**What worked.** The four-keyword model survived five full stages of
implementation, a complete rewrite from Python→Rust, and the start of a
self-host. No user ever asked for a fifth keyword. No one ever asked for
lifetimes. The checker rules that enforce it are ~50 lines of code per
rule. This is the purest success story in the entire Fuse2 project.

**What rough-edged.** Two specific gaps surfaced during Stage 2 self-host
(L022, L029 in [`learning.md`](../learning.md)):

- **L022** — The checker forgot to block `var` mutation inside
  `spawn { }` bodies unless the mutation went through `mutref`. An
  assignment like `count = count + 1` inside a spawn silently compiled,
  which is a data race. Fixed by adding an `assign_target_root` check
  in `check_spawn_statement`.

- **L029** — Statement-position matches with mixed arm types (`Ok(v)` arm
  returns `List<T>`, `Err(_) => { return }` arm returns `Unit`) were
  false-positive rejected by the B7.4 `check_match_arm_compatibility`
  because the checker has no `in_value_context: bool` flag. The
  Rule 2 fix was to filter `Unit`/`!` arms out of unification and
  defer the real fix (threading `value_context` through `check_expr`'s
  ~27 call sites) as a B12 follow-up. **This gap is still open as of
  the B12 triage session.**

**Fuse3 implication.** Keep the four keywords *and* their semantics
unchanged. But: design the checker's internal `value_context` tracking
into the type system *from day one*, not as a B-wave retrofit. Every
expression has a value-used-by-caller bit. Every statement has the same
information on the way in. The checker should never be in the position
of having to decide "is this match being used as an expression?" at
unification time — the answer should already be in the HIR node's
context.

## 2.2 ASAP destruction

From [`fuse-language-guide-2.md §1.10`](../fuse-language-guide-2.md#L717):

> Values are destroyed at their last use, not at the end of their
> lexical scope.

Implementation in Fuse2:

1. The checker tracks future uses of every binding statement-by-statement
   (`compute_future_uses`).
2. After each statement, `release_dead` walks locals and releases any
   binding whose name does not appear in any following statement.
3. At function exit, `release_remaining` drops everything still live.
4. `defer` statements fire *after* ASAP release, in LIFO order.

**What worked.** The semantic is loud in output. Reading a program's
stdout and seeing `[del] X` exactly at the last use of `X` is a
debuggability superpower. Users write destructors and immediately know
if their code is correct. The ADR for ASAP
([`ADR-010` via §5.10 of guide](../fuse-language-guide-2.md#L2048)) is
short and clear.

**What hurt.**

- **Pattern-binding lifetimes.** When a match binds `Ok(m)`, the bound
  `m` is a copy of the inner pointer, not a clone. `fuse_result_unwrap`
  returns the inner value's raw handle. If the Result is released
  between bind time and use time, `m` dangles. Stage 1 fixed this
  by restoring `base_locals` after each arm and marking outer locals
  `destroy = false` inside the arm — see
  [`object_backend.rs:4693`](../../stage1/fusec/src/codegen/object_backend.rs#L4693).
  But it took two bug-fix rounds to get right, and the B7 match-arm
  unification work touched the same code path again.

- **ASAP crossed with loop frames.** Loop scopes must snapshot/restore
  locals to prevent premature ASAP of the enclosing scope. This was
  discovered the hard way around Stage 1 Phase 7 — listed as a known
  pitfall in the language guide
  ([§3.7 Phase 7](../fuse-language-guide-2.md#L1820)).

- **Defer capture.** `defer` captures names by name. If the defer body
  mentions a local, that local is kept alive past ASAP — but only the
  names explicitly mentioned. This is correct but subtle and needed
  regression tests for every edge case.

- **F-string dead-binding analysis.** Four different `collect_expr_names`
  call sites had to re-scan the f-string template to know which bindings
  the `f"..."` referenced, and any one of them could get out of sync.
  This was the final straw that motivated the shared
  `fstring::parse_fstring_template` helper in
  Wave B10 (L028 in [`learning.md`](../learning.md)).

**Fuse3 implication.** ASAP stays. But the analysis infrastructure
should be a single pass that produces a per-statement "names used in
all later statements" set, consumed by every release site and every
destructor site — not four separate walkers that have to agree. In Go,
this maps naturally to a `releaseSet map[*ast.Node]StringSet` computed
once during IR lowering.

## 2.3 Result/Option/? — error handling without null or exceptions

From [`fuse-language-guide-2.md §1.11`](../fuse-language-guide-2.md#L814):

- Every fallible operation returns `Result<T, E>`.
- Every nullable value is `Option<T>`. There is no `null`.
- `?` unwraps `Ok(v)` / `Some(v)` or propagates `Err(e)` / `None` from
  the function.
- `?.` chains through optional values (short-circuits to `None`).
- `?:` (Elvis) provides a fallback for `None`.
- `match` is exhaustive on `Result` and `Option`.

**What worked.** The three operators (`?`, `?.`, `?:`) cover the
100% of common error-handling boilerplate. Users do not write
`if result.is_err() { return }` by hand. The spec got this exactly
right on the first try.

**What rough-edged.**

- **Checker does not verify `return <expr>` against the function's
  declared return type.** Discovered in B12: stage2's `parseOutputFlag`
  had `return next.unwrap()` (returning a `String`) inside a function
  declared `Option<String>`. The stage 1 checker silently accepted it.
  At runtime, `fuse_option_is_some` returned false on the mis-typed
  handle, the match always took `None`, and `-o <path>` was silently
  dropped. **Still open.** See [`learning.md L029`](../learning.md)
  follow-up notes and the B12 commit `9126baf`.

- **Match-as-expression arm unification** got an entire B-wave
  (B7, L027) because the Stage 1 codegen hardcoded block arms as
  `Some("Unit")` even when the block actually diverged. The fix was
  to collect each arm's compiled `TypedValue.ty` *during* arm
  compilation (while pattern bindings are live), then unify via rules
  U1–U6 — and mirror the same analysis in the checker. The B12 triage
  round discovered the same mistake for Never-typed arms and added
  a Never-filtering rule to `unify_match_arm_types`.

**Fuse3 implication.** Carry the syntax verbatim. But:

1. The checker must verify `return <expr>` against the declared return
   type from day one. There should be exactly one code path for
   "returning a value", not separate logic for trailing expressions
   vs explicit returns.
2. Match-arm type unification must be a single, well-documented
   algorithm in the checker, not a helper shared-but-not-quite between
   the checker and the codegen. Fuse2 had the rules U1–U6 written down
   in the parity plan but they still took three commits to get right
   in each side.
3. `!` (Never) must be a real type in the type system, not a
   post-hoc filter. See §2.5 for more.

## 2.4 The type catalogue

**Primitives.** `Int` (64-bit signed), `Float` (64-bit IEEE 754),
`Float32` (32-bit IEEE 754), `Bool`, `String` (UTF-8), `Unit`. Sized
integers added in Wave 2: `Int8`, `UInt8`, `Int32`, `UInt32`, `UInt64`.
Deferred: `Int16`, `UInt16`, `Float16`/`BFloat16`.

**Compound.** `List<T>`, `Map<K,V>`, `Option<T>`, `Result<T,E>`,
tuples `(T, U, ...)`.

**User-defined.** `struct` (opaque fields, manual methods), `@value
struct` (auto lifecycle), `data class` (positional constructor,
structural equality, auto `toString`), `enum` (tagged unions with
multi-payload variants).

**No nulls, no exceptions, no implicit numeric coercion.** All
conversions are explicit. `Bool` is not convertible from `Int`.

**What worked.** The primitive set is right-sized. `Int` at 64 bits is
the right default for a systems language. `String` as UTF-8 with an O(1)
`len()` (byte length) and an O(n) `charCount()` — and both documented —
avoids the JS/Go footgun where iterating a string iterates bytes by
default.

**What hurt.**

- **String indexing.** `charAt(i)` is O(n). `byteAt(i)` is O(1).
  Users who write `for i in 0..s.len()` get the wrong semantics.
  Documented as a guide rule, but still costs lexers and parsers real
  performance.

- **Map as a first-class primitive.** `Map<K,V>` was built with
  requires-`Hashable` semantics but the Wave 0 Stage 0 interpreter
  shipped without `Hashable` in stdlib. This led to two kinds of bug:
  maps that "worked" for `Int` and `String` keys only (because those
  types had special-case hash handling in the runtime), and maps that
  crashed or misbehaved for user types. The stdlib-interface plan
  (Phases 0–5 in [`stdlib-interfaces-plan.md`](../stdlib-interfaces-plan.md))
  fixed this, but not before the bug pattern recurred several times.

- **Primitive types that should have been data classes.** `File`,
  `TcpStream`, `Duration`, `Instant`, `DateTime` — all of these had to
  become `struct`s in stdlib because the compiler did not have a clean
  "opaque but with methods" type. `data class` was for structural
  equality; plain `struct` had no auto-generation. The gap was filled
  post-hoc.

**Fuse3 implication.**

- Maps and the whole `Hashable`-dependent cluster of stdlib types
  should be designed *with the trait system already decided*.
  Fuse2's Wave 5 interface system landed after maps were in use,
  which meant retrofitting. Fuse3 flips the order: traits first
  (`Equatable`, `Hashable`, `Comparable`, `Printable`, `Debuggable`
  — see §5.2 for the rename from Fuse2's "interface" vocabulary),
  then maps on top of `K: Hashable`.

- The user has explicitly flagged **maps and memory management for
  threads** for from-scratch design (from
  [`fusev3-meta-plan.md`](fusev3-meta-plan.md)). Good — these are the
  two areas where Fuse2 accumulated the most debt.

- String indexing is worth reconsidering. Consider giving users a
  single `chars()` iterator (explicit, O(n) upfront cost) and reserving
  `len()` for "length in the unit you asked for" without ever making
  `s[i]` look cheap.

## 2.5 Pattern matching and arm unification

Pattern shapes supported: literal, variable binding, enum constructor,
qualified enum constructor, tuple, wildcard `_`. Nested patterns work:
`Some(Ok(value))` destructures through both wrappers.

`match` is exhaustive (the compiler rejects incomplete matches on
enums, Result, Option, Bool). `when` is the condition-based variant —
each arm is a boolean, an `else` arm is required unless the conditions
are provably exhaustive.

**Arm unification rules (U1–U6)** were formalised during Wave B7:

- **U1 — Identity.** All arms same concrete type → that type.
- **U2 — Empty list promotion.** Mix of `List`/`List<Unknown>` and
  `List<X>` → `List<X>`.
- **U3 — Option `None` promotion.** `Option<Unknown>` + `Option<X>`
  → `Option<X>`.
- **U4 — Result half promotion.** `Result<T, Unknown>` +
  `Result<Unknown, E>` → `Result<T, E>`.
- **U5 — Incompatible concrete arms** → diagnostic.
- **U6 — No information** → result type is `None`, let downstream
  callers supply it.

Later added (during B12 triage): **U7 — Never promotion.** A `!`
(Never) arm imposes no type constraint. `[T, !]` → `T`. All arms `!`
→ `!`.

**What worked.** Once U1–U6 were written down, there was a single
reference for checker and codegen to agree on. Eight unit tests in
[`stage1/fusec/src/codegen/type_names.rs`](../../stage1/fusec/src/codegen/type_names.rs)
pinned the behaviour.

**What hurt.**

- **Two subsystems, one rule, separate implementations.** The checker
  and the codegen both called `unify_match_arm_types`, but collecting
  arm types required different orchestration. The checker gathered
  types post-hoc via `infer_expr_type`, which lost pattern bindings.
  The codegen collected types during arm compilation while bindings
  were still live. Both had to be rewritten.

- **`check_match_arm_compatibility` rejected statement-position
  matches.** B11 discovery: `match mode { Ok(m) => dispatch(m),
  Err(msg) => { sys.exit } }` where the result was discarded. The
  checker flagged "incompatible arms" when the arms' value types
  never needed to unify. L029.

- **Divergent block arms.** `compile_match` hardcoded block arms as
  `Unit`, even when they ended in `return`. Any
  `val x = match y { Ok(v) => v, Err(_) => { return } }` got
  arm types `[T, Unit]`, U5-failed, `x` lost its type. B12.

**Fuse3 implication.**

- **`!` (Never) is a real type.** In Go terms, a function returning
  `Never` cannot return normally. Go's type system doesn't have this,
  so Fuse3-on-Go will have to synthesize it. Every diverging
  statement (`return`, `panic`, unreachable loop) must be analysed
  to produce Never at the type level during AST→TypedAST lowering.

- **Arm unification must be one algorithm, called once.** The checker
  and codegen must share not only the rules but the *collection
  logic*. Fuse3 should centralise arm type discovery into the HIR
  lowering so neither pass does post-hoc inference.

- **Statement vs expression context is a property of the surrounding
  expression, not a flag threaded through check_expr.** The HIR
  should mark each subexpression with its context at lowering time.

---

## 2.6 Memory safety case audit (every allocation path)

Pillar 1 promises: *memory safety without garbage collection, without
manual `free`, without a borrow checker.* That is a strong claim. This
section audits it case by case so Fuse3's language guide can state
confidently which allocation patterns are automatic, which are
compiler-enforced, and where the one documented exception lives.

The question the audit answers: **given a value that needs heap
storage, is its destruction guaranteed without developer action?**

### 2.6.1 The allocation-path catalogue

Every value in Fuse lives in exactly one of these shapes. The table
below is the exhaustive list — if Fuse3 adds a shape that isn't here,
this audit has to be extended.

| # | Allocation shape                             | Destruction mechanism                            | Automatic? |
|---|----------------------------------------------|--------------------------------------------------|------------|
|  1 | Stack primitive (`Int`, `Bool`, `F64`, …)    | Stack unwind                                     | ✅ |
|  2 | Stack struct with only primitive fields      | Stack unwind                                     | ✅ |
|  3 | `@value struct` (trivially copyable bundle)  | Stack unwind; field dtors run at last use        | ✅ |
|  4 | Owned `String`                               | ASAP dtor at last use                            | ✅ |
|  5 | Owned `List<T>`                              | ASAP dtor, element dtors run first               | ✅ |
|  6 | Owned `Map<K,V>`                             | ASAP dtor, key/value dtors run first             | ✅ |
|  7 | Owned `Set<T>`                               | ASAP dtor, element dtors run first               | ✅ |
|  8 | Owned `Option<T>` / `Result<T,E>`            | ASAP dtor, payload dtor runs iff variant holds   | ✅ |
|  9 | Owned tuple `(A, B, …)`                      | ASAP dtor, field dtors in declared order         | ✅ |
| 10 | `data class` (auto `@value`-like)            | Compiler-generated dtor, fields in order         | ✅ |
| 11 | `struct` with owning fields                  | Compiler-generated dtor, fields in order         | ✅ |
| 12 | Nested owners (`List<List<String>>`, …)      | Recursive dtor walk, innermost first             | ✅ |
| 13 | Enum with payloads                           | Tag-dispatched dtor, only the live variant       | ✅ |
| 14 | Temporary from expression (`f(g(h()))`)      | Dtor at end of enclosing statement               | ✅ |
| 15 | Pattern-bound value (`Some(s) => use(s)`)    | Dtor at arm exit; binding extends source lifetime | ✅ |
| 16 | Function parameter (`owned` / `move`)        | Dtor at last use inside callee                   | ✅ |
| 17 | Return value                                 | Ownership transferred to caller; caller owns dtor | ✅ |
| 18 | Closure capturing owned values by reference  | Captures extend enclosing scope lifetime          | ✅ (non-escaping); ⚠️ escaping — see 2.6.2 |
| 19 | Closure capturing by move                    | Dtor runs when closure itself is destroyed      | ✅ |
| 20 | `Shared<T>` (ref-counted, `@rank`-ordered)   | Refcount drop; dtor runs when count hits zero    | ✅ |
| 21 | `Chan<T>` buffered elements                  | Channel dtor drains buffer, destroying each item | ✅ |
| 22 | Task-local heap inside `spawn { … }`         | Task teardown walks the task arena               | ✅ (needs Fuse3 redesign — Part 8.3) |
| 23 | Cyclic data structure                        | **Disallowed by design** — no back-pointers      | ✅ (by construction) |
| 24 | FFI raw `Ptr<T>` / `Unsafe.alloc`            | **Manual** — developer owns lifetime             | ❌ (the documented exception) |

Twenty-three of the twenty-four cases are automatic. The two that need
commentary — case 18 (the real gap) and case 24 (the documented kernel
exception) — are below.

### 2.6.2 The one real gap: escaping closures (case 18)

A non-escaping closure is fine. When a closure is passed to
`list.map(…)` and consumed before the function returns, the captures
are alive because the enclosing scope is alive. ASAP destruction
covers it.

The problem is the *escaping* closure: one that outlives the scope
that created its captures. Returning a closure from a function, or
storing it in a struct field, or sending it over a channel to another
task — all of these need the captures to outlive the original stack
frame, and ASAP destruction on the caller side would otherwise destroy
them too early.

**Fuse2 status.** Fuse2 dodges this because its closures are
second-class: they are almost exclusively consumed in place
(`map`/`filter`/`fold`), and the compiler does not permit storing them
across awaits or returning them from functions that own their
captures. The problem was never stressed.

**Fuse3 decision (recommended, pending confirmation).** Adopt the
Rust-style discipline: **escaping closures must capture by `move`
explicitly**. Non-escaping closures stay implicit and ergonomic.

```fuse
// non-escaping — implicit capture is fine
list.map(|x| x + offset)           // offset stays alive; closure dies before return

// escaping — compiler demands an explicit move
fn make_counter(start: Int) -> Fn() -> Int {
    var n = start
    return move || { n = n + 1; n }   // `move` transfers `n` into the closure
}
```

The compiler rule: if a closure's lifetime cannot be proven to end
before its captures' enclosing scope, it must be marked `move`, and
the captures become *owned* by the closure. The closure itself is now
a heap-allocated object with a dtor that destroys each captured value.
Same ASAP discipline, same uniform ownership model — just made
explicit at the escape boundary.

This is the only case in the entire catalogue where the developer
writes something extra (`move`). It is not manual allocation; it is
an ownership annotation at the boundary where ownership actually
transfers.

### 2.6.3 The documented exception: FFI raw pointers (case 24)

The language guide has always promised an escape hatch for the
extreme case: kernel programming, bare-metal embedded, custom
allocators, hand-rolled data structures for a hot loop. These live
under `Unsafe` / `Ptr<T>` and are the *only* place a developer can
call `alloc` / `free` by hand.

The audit does not weaken pillar 1, because:

1. **The escape hatch is syntactically marked.** Any use of `Ptr<T>`
   or `Unsafe.alloc` must appear inside an `unsafe { … }` block.
   Search-and-audit is trivial: `grep -rn 'unsafe {' src/` tells you
   every place in a codebase where manual memory management lives.

2. **The escape hatch is rare by default.** Standard library
   containers, channels, shared references, strings, lists, maps —
   none of them expose raw pointers. A pure application programmer
   can write a full Fuse3 program and never type the word `unsafe`.

3. **The escape hatch is opt-in at the crate level.** Fuse3 should
   allow a module to declare `#![forbid(unsafe)]` so that higher-tier
   code (stdlib-level libraries, business logic) mechanically rejects
   `unsafe` blocks at compile time. This is the Rust discipline and
   it works.

### 2.6.4 How ASAP becomes C in the Fuse3 backend

Because §1.4 locked in "emit C, invoke system cc", the audit above
has to hold down to the generated C. Here is a minimal example —
a function that owns a `String` and a `List<String>`, both of which
should be destroyed before the function returns, the `List` inner
element first, then the outer `List`, then the `String`.

Fuse source:

```fuse
fn greet(name: String) -> Int {
    val items: List<String> = ["hello", name]
    println(items.len())
    return items.len()
}
```

Emitted C (illustrative, not final syntax):

```c
int64_t fuse_greet(fuse_string_t name) {
    fuse_list_t items = fuse_list_new_cap(2);
    fuse_list_push_string(&items, fuse_string_lit("hello"));
    fuse_list_push_string(&items, name);          // ownership of `name` moves in
    int64_t _t0 = fuse_list_len(&items);
    fuse_println_int(_t0);
    int64_t _ret = fuse_list_len(&items);
    fuse_list_destroy(&items);                    // outer dtor walks elements first
    return _ret;
    // `name`'s dtor is NOT here — ownership was moved into `items`
}
```

Key properties:

- **Every owning binding has a matching `fuse_*_destroy` call** at
  its last-use point. The codegen emits these during ASAP lowering;
  they are not optional.
- **Moved values do not get a dtor on the source side.** The `name`
  parameter moved into `items`, so the only dtor for it is the one
  `fuse_list_destroy` calls while walking elements.
- **Destructors recurse.** `fuse_list_destroy` walks the backing
  array and calls `fuse_string_destroy` on each element before
  freeing the array itself. No tracing, no refcounts in the hot
  path, just a single deterministic walk.
- **The C compiler sees plain C.** No Fuse runtime, no GC, no
  dependency on `libgo`. Just `fuse_runtime.c` (a small Fuse-owned
  file) and the system's `cc`.

The runtime library (`fuse_runtime.c` / `fuse_runtime.h`) is written
in C for stage 1, then replaced by a Fuse-authored version once the
compiler is self-hosting. It exposes `fuse_string_*`, `fuse_list_*`,
`fuse_map_*`, `fuse_chan_*`, and `fuse_shared_*`. Every one of them
has a `*_destroy` entry point that codegen calls directly.

### 2.6.5 Summary

Pillar 1 holds: **23 of 24 allocation shapes are destroyed
automatically by the compiler, with zero developer action**. The 24th
(`Ptr<T>` / `Unsafe.alloc`) is the documented kernel-programming
exception, syntactically marked, lintable, and forbiddable at the
module level. The one shape that needs developer input — escaping
closures — needs only an ownership annotation (`move`), not a manual
allocation or free.

Fuse3's language guide should open the memory-safety chapter with the
24-row table above. It is the shortest possible proof that the pillar
is real.

---

# Part 3 — Concurrency (user flagged for from-scratch redesign)

## 3.1 The three-tier model

From [`fuse-language-guide-2.md §1.17`](../fuse-language-guide-2.md#L1160):

- **Tier 1 — Channels.** `Chan<T>.bounded(n)` or unbounded; `send`,
  `recv`, `tryRecv`, `close`. The preferred communication primitive.
  Zero locks, zero shared state.

- **Tier 2 — `Shared<T>` + `@rank(N)`.** When multiple tasks must
  share a live mutable value. `config.read()` / `config.write()` acquire
  locks. `@rank(N)` is mandatory.

- **Tier 3 — `try_write(timeout)`.** When the lock order is genuinely
  dynamic (locking items from a list in arbitrary order).

Decision hierarchy:

```
Does data flow between tasks?          → Tier 1 (Chan<T>)
Must tasks share live mutable state?   → Tier 2 (Shared<T> + @rank)
Is the lock order dynamic?             → Tier 3 (try_write)
```

**What worked.** The hierarchy matches how real concurrent code is
written. Almost everything is channels. The minority that genuinely
needs shared state gets @rank. The minority of *that* that needs
dynamic locking gets timeouts with explicit error handling. There is
no "default path" that leads to deadlock.

## 3.2 `@rank` compile-time deadlock prevention

`Shared<T>` without `@rank(N)` is a compile error (not a warning).
The compiler tracks the maximum rank held in scope; acquiring a
lower rank is a compile error. From
[`ADR-004` via §5.4](../fuse-language-guide-2.md#L2004):

> Optional safety annotations get skipped under pressure. Compile
> error = never unguarded.

Example:

```fuse
@rank(1) val config  = Shared::new(Config.load())
@rank(2) val db      = Shared::new(Db.open("localhost"))
@rank(3) val metrics = Shared::new(Vec::<Metric>.new())

fn ok() {
  val ref    cfg  = config.read()    // rank 1
  val mutref conn = db.write()       // rank 2 > 1 — ok
}

fn broken() {
  val mutref m    = metrics.write()  // rank 3
  val mutref conn = db.write()       // rank 2 < 3 — COMPILE ERROR
}
```

**Same rank** means independent — safe to acquire in any order.
**Guards release via ASAP destruction** — no forgotten unlocks.

**What worked.** The compile-time proof is complete. It's the only
feature in the language that makes compile-time deadlock prevention
accessible to end users without requiring them to understand lock
ordering theory.

**What hurt.** Almost nothing — but only because Stage 1 was
single-threaded and we never actually exercised deadlocks. The
checker tests use fixtures that declare ranks and verify the
rejection messages; no real lock ever held in anger. ADR-014
acknowledges this explicitly.

## 3.3 Scheduling model: single-thread → OS threads → green threads

From [`ADR-014`](../adr/ADR-014-threading-model.md):

| Phase | What runs | `Shared<T>` | `Chan<T>` | `spawn` |
|---|---|---|---|---|
| **Stage 1** (current) | Everything sequential in the evaluator | `RwLock` replaced by clone-on-read, live handle on write | Plain `VecDeque` | Parsed and checked, executed inline |
| **Stage 2** (planned) | OS threads | Real `RwLock` | `Mutex` + `Condvar` / lock-free queue | `std::thread::spawn` |
| **Post-Stage 2** | M:N green threads | Same API | Can suspend | Same API |

The **FFI surface stays stable** across all three:
`fuse_shared_read`, `fuse_shared_write`, `fuse_chan_send`,
`fuse_chan_recv`, etc. No ABI break when the runtime swaps.

**What worked.** Stage 1's "concurrency correctness without
real concurrency" stance let the team validate the compile-time
rank check and the ownership rules *without* fighting data races
in the runtime. Fuse caught spawn-capture bugs (L022) at the
checker level because the checker was the only place that mattered.

**What hurt.**

- **Stage 2 self-host hit the ABI surface before OS threads
  were wired up.** The Cranelift FFI crate was the only place where
  "stage 2 compiler runtime meets the Fuse runtime" happened, and
  every shortcut taken there (raw-pointer conventions where stage 2
  could only produce Fuse handles) surfaced as segfaults or silent
  wrong output in the B12 triage. See §6.3.

- **`spawn move { body }` syntax was not parsed until L021** (W-wave
  bug). The syntactic shape was documented in the guide but the
  parser skipped the modifier check.

## 3.4 What was rejected

From [`ADR-014`](../adr/ADR-014-threading-model.md) and
[`fuse-post-stage2.md`](../fuse-post-stage2.md):

- **`async`/`await` — rejected.** Function coloring splits the
  ecosystem, viral propagation forces callers to be async, hidden
  state machines are hard to debug. Fuse's model is simpler: every
  function is synchronous; concurrency is a call-site decision via
  `spawn`. Removed from the language in Wave W0.6.

- **Actor model — rejected.** Channels are more flexible. Users
  can build actors from channels if they want.

- **`select` expression — deferred to post-Stage 2.** Needs runtime
  scheduler integration that doesn't exist yet. Workaround: one
  `spawn` per channel, or poll with `try_recv`.

- **`SpawnHandle<T>` (joinable) — deferred.** Requires runtime task
  tracking and result storage. Fire-and-forget + channels is
  equivalent in capability.

- **Green threads in Stage 2 — deferred.** Premature without
  profiling data. OS threads are simple, correct, proven.

## 3.5 Open questions for Fuse3 concurrency

The user's meta-plan explicitly flags **concurrency** and **memory
management for threads** for from-scratch redesign. With the
C-backend decision locked in (see §1.4), the questions to answer
in the Fuse3 language guide are:

1. **OS threads or green threads?** The C backend gives us
   direct access to pthreads / Windows threads via the C runtime.
   OS threads are simple, correct, proven — the same default
   Fuse2 planned for Stage 2. Green threads (M:N scheduling with
   work-stealing) would require us to *write* a scheduler in C
   or Fuse, which is several thousand lines of careful code.
   Recommendation for Fuse3: start with OS threads. Defer green
   threads until profiling data says OS thread overhead is
   actually a bottleneck — same decision Fuse2's ADR-014 made for
   Stage 2, except now unblocked earlier.

2. **How is `Shared[T]` implemented?** Wraps a C mutex
   (pthread_rwlock_t on POSIX, SRWLOCK on Windows) plus the
   wrapped value. Acquired via Fuse-level `.read()` / `.write()`
   methods that call into the C runtime. The `@rank(N)` check is
   entirely compile-time in the Fuse3 checker — it emits no code,
   it just rejects programs that would deadlock. The runtime
   primitive is ~20 LOC of C.

3. **What's the memory model?** Fuse2 was single-threaded and
   dodged this. Fuse3 needs to be explicit. Recommendation: adopt
   a sequentially-consistent model for `Shared[T]` reads and
   writes (acquire on read lock, release on write unlock, full
   barrier on lock acquisition). `Chan[T]` send/recv form
   happens-before edges. Everything else is undefined and the
   checker rejects it via the ownership rules. This is the same
   contract Rust's `std::sync` gives, documented in the guide.

4. **How is `Chan[T]` implemented?** Fuse-owned ring buffer or
   linked list of payloads, guarded by a C mutex and a condition
   variable for blocking send/recv. Unbounded and bounded variants
   both need explicit implementations. Fuse's ownership checker
   enforces that a sent value cannot be used by the sender
   afterward (the runtime doesn't enforce this — it's a
   compile-time rule). ~100 LOC of C for the primitive + whatever
   Fuse code we write on top for the `.bounded(n)` / `.unbounded()`
   constructors.

5. **`select` — ship it day one or defer?** Fuse2 deferred because
   it needed runtime scheduler integration. Fuse3 has the same
   constraint: `select` over multiple channels requires the
   runtime to wait on multiple condition variables
   simultaneously, which is doable (pthread_cond_wait on a single
   condition with a shared wait flag, plus book-keeping), but
   non-trivial. Recommendation: defer to Wave 2 of Fuse3, document
   the workaround (`spawn` per channel + collect results) in the
   guide day one.

6. **`@rank` — keep it, unconditionally.** The entire point of
   pillar 2 is "concurrency safety without a borrow checker at
   compile time." `@rank` is the mechanism. It's compile-time,
   cheap, and catches deadlocks before the program runs. Fuse3
   keeps `@rank` exactly as Fuse2 designed it. There is no
   runtime race detector because there is no runtime with enough
   metadata to have one — and that's fine, because `@rank`
   replaces it at compile time.

**Key reframing from my earlier draft of this section:** the
questions used to assume Fuse3 would map `spawn` → Go goroutines,
`Chan[T]` → Go `chan T`, `Shared[T]` → Go `sync.RWMutex`. That
plan linked the Go runtime into every Fuse binary, which §1.4
now rejects. The new answers are: everything concurrency-related
is implemented in the Fuse3 C runtime (or, over time, in Fuse
itself), not borrowed from Go's runtime. The implementation is
more work up front, but it's the only way to keep pillar 1 and
pillar 2 honest.

---

# Part 4 — Compiler architecture (what worked, what broke)

## 4.1 Three-stage bootstrap: rationale and cost

The bootstrap chain:

- **Stage 0** — Python interpreter. Validates semantics.
- **Stage 1** — Rust + Cranelift. Produces native binaries.
- **Stage 2** — Fuse compiler written in Fuse. Proves self-hosting.

Ground rules
([`fuse-repository-layout-2.md`](../fuse-repository-layout-2.md)):

- Nothing in `stage1/` depends on `stage0/`.
- Nothing in `stage2/` depends on `stage0/` or `stage1/` *source*
  (only on compiled binaries).
- **A test passing in Stage 0 must produce identical output in
  Stage 1 and Stage 2.**
- The guide precedes implementation. If behavior is not in the
  guide, it does not exist.

**What worked — ground rule 3 is load-bearing.** Stage 0's existence
meant that every Wave in Stage 1 had an oracle: if the new Cranelift
output didn't match what Stage 0 printed, the new code was wrong. This
trivially caught hundreds of codegen bugs during Phase 7.

**What worked — guide precedes implementation.** Every new feature
went into the guide first. This sounds bureaucratic but it was the
reason the language stayed coherent across 9 waves, 43 phases, ~300
tasks.

**What hurt.**

- **Stage 0's Python interpreter accreted Stage 2 features.** L001
  was the first lesson: when `stage2/src/token.fuse` couldn't be run
  through Stage 0, the fix was to teach Stage 0 about enum runtime,
  module constructors, and module path resolution — *except that was
  backwards*. Stage 0 is a completed prototype; Stage 2 is validated
  by Stage 1. ADR-012 codified this and added Rule 8 to the
  implementation plan ("Fixes Go Forward, Not Backward").

- **Stage 2 wrote large amounts of code before it could be run.**
  The W7.5 three-generation bootstrap test passed *on synthetic
  input* months before anyone tried to compile `stage2/src/main.fuse`
  end-to-end. When the real build was attempted, six cascading codegen
  gaps surfaced, which the T4 parity investigation catalogued
  ([`t4-parity-investigation.md`](../t4-parity-investigation.md))
  and the B-wave remediation plan fixed one at a time.

- **The bootstrap test as written did not exercise the real stage2
  compiler.** This is the big one. Three generations of "compile stage2
  with X, then compile stage2 with the resulting binary, then compare"
  passed green for months. But the bootstrap's input was a trivial
  Fuse program, not `stage2/src/main.fuse`. When B12 finally tried to
  compile the real stage 2 source, the test surface exposed 8 distinct
  bug classes.

**Fuse3 implication.**

- **Two stages, not three: Stage 1 (Go) → Stage 2 (self-hosted
  Fuse).** See §1.4 for the full staging contract. Fuse3 has no
  Python Stage 0 because Fuse2's Stage 0 already answered the
  "is this implementable?" question — those answers are captured
  in this learnings doc. Skipping Stage 0 is not a shortcut; it's
  the correct response to "the experiment has already been run."

- **Stage 1 is one Go program emitting portable C.** Lexer,
  parser, checker, type inference, C-code emission — all in one
  binary with modes `fuse run` / `fuse build` / `fuse check`. The
  emitted C is compiled by the system C compiler
  (cc / gcc / clang / msvc). One small Fuse-owned C runtime
  (~500 LOC target) replaces Fuse2's Rust `fuse-runtime` crate.
  The Cranelift + cranelift-ffi + fuse-runtime triangle — which
  produced the entire B12 ABI cascade (§4.2, §6.3) — is gone
  because there is no bridge crate to drift. See §1.4 for why
  the C backend specifically, not Go's native tools.

- **Keep the "guide precedes implementation" rule.** It scales.

- **The Stage 2 self-host bootstrap test must compile the real
  `stage2/src/main.fuse`, not a synthetic input.** Fuse2's W7.5
  test passed for months on a trivial program while the real
  self-host was broken — an entire class of latent bugs hid
  because the bootstrap lied. If the real compile is expensive
  to run, it is expensive to run, and CI handles it. Any
  bootstrap test that
  doesn't use the production source is a lie.

## 4.2 Uniform `FuseHandle` ABI — blessing and curse

Fuse2's runtime represents every value as `FuseHandle = *mut FuseValue`
(an i64 pointer). Stage 1's codegen passes every value as
pointer-typed Cranelift values; type dispatch happens at construction
and destruction time via tag fields inside the `FuseValue` struct.

**Blessing.** Polymorphism without monomorphization. Generics erase at
the IR level. `List<Int>` and `List<String>` call the same IR
functions. Signatures stay fixed at `(ptr, ptr, ...) -> ptr`.

**Curse — the B12 cascade.** The uniform ABI created a *protocol
mismatch class* that is very hard to catch statically. Several
cranelift-ffi functions accepted "a raw pointer to something, wrapped
as an Int-typed FuseHandle" for convenience — that was the smoke test
convention. Stage 2 could only produce actual `FuseHandle` values
(real Fuse strings, real Fuse lists). The two conventions were never
unified. Symptoms:

- `read_values(ptr, count)` dereferenced the "pointer" as a raw
  `*const i64`. Stage 2 passed a `ValueKind::List` FuseHandle. Result:
  null deref or garbage.
- `str_from_raw(ptr, len)` did the same for strings.
- `module_define_data(bytes, byte_len)` did the same for byte
  buffers.
- `fuse_result_is_ok` returned `bool` (i8), but stage 2 registered
  it as returning pointer (i64). Result: garbage in upper bits of the
  return register, brif on garbage, every `Ok(_)` match branch taken
  wrong.

Each of these was fixed during the B12 triage session, but each had
been latent *since W0.4 when the cranelift-ffi crate was introduced*.
They never manifested until someone actually ran the self-hosted
compiler end-to-end.

**Root cause.** The ABI was written from two ends (Rust side and Fuse
side) that never had a formal contract to meet in the middle. The
smoke test exercised the Rust-side convention. The stage2 code used
the Fuse-handle convention. No test exercised the interaction.

**Fuse3 implication.**

- **Fuse3 has one runtime boundary, entirely under Fuse3's own
  control.** With the C-backend decision (§1.4), Fuse3 still has
  a runtime — a small C module covering malloc/free, I/O, thread
  primitives, atomics. But it's *our* code, versioned with the
  compiler, defined in one file. There is no bundled Rust crate
  (no `fuse-runtime`), no bundled codegen library (no Cranelift,
  no LLVM), no external versioning authority to disagree with.
  The uniform-pointer-ABI pattern that produced B12 is
  specifically the *untyped bridge between independently-versioned
  crates* — and Fuse3 has no such bridge. There is exactly one
  contract: emitted C ↔ Fuse C runtime. That contract is
  specified in the Fuse3 language guide alongside the emitted C
  format, and the Stage 1 compiler has a checker-level rule that
  the emitter only produces calls the runtime can receive.

- **Type tagging is visible at the C layer.** A Fuse3 `Int` is a
  `int64_t` in the emitted C (with debug tag info in debug mode).
  A Fuse3 `List<Int>` is a pointer to a small struct `{ items,
  len, cap }`. There is no `FuseValue` tagged union — the type
  information stays at compile time, erased into direct C types.
  This preserves the Fuse2 insight ("no runtime type tags leak
  into the binary") while getting rid of the `FuseHandle = *mut
  FuseValue` indirection that made the B12 cascade possible.

- **The general lesson transcends the specific bug.** Any time a
  value crosses a language or subsystem boundary, the boundary
  needs a typed contract, a test that exercises *both* sides of
  the contract simultaneously, and a failure mode when the
  contract is violated (not silent wrong output). Fuse3 codifies
  this as a rule: the emitted-C ↔ runtime-C contract is verified
  by a test that compiles a reference set of Fuse programs, runs
  them, and compares output byte-for-byte against an oracle.

## 4.3 ASAP analysis, future uses, dead binding release

Fuse2's Stage 1 implementation:
[`stage1/fusec/src/codegen/object_backend.rs`](../../stage1/fusec/src/codegen/object_backend.rs)
lines around `compile_statements`,
`compute_future_uses`, `release_dead`, `release_remaining`.

The algorithm:

1. For each block of statements, compute future uses: for every
   position `i`, the set of names used in statements `i+1..end`.
2. Walk statements forward. After each statement, release bindings
   that are *not* in the future set and have `destroy = true`.
3. At block exit, release remaining live bindings.
4. Loop scopes snapshot/restore locals so ASAP inside the loop doesn't
   leak outward.

**What worked.** The algorithm itself is correct. When it ran, it
produced the right output.

**What hurt.** The scanner was duplicated across multiple users
(f-string names, match-arm types, ASAP sites). Every time one of
them needed to be updated, all of them needed to be updated, and
the "did I update them all?" question didn't have a mechanical
answer. L027 (match-arm type inference) and L028 (f-string
re-scanners) both hit this pattern.

**Fuse3 implication.** Make "the set of names used after this AST
node" a single computed property on the AST, exposed as
`node.LiveAfter()` or similar. Every consumer reads from the same
source. Updating the logic updates all consumers.

## 4.4 Checker → codegen information loss (the recurring disease)

A pattern that shows up in L005, L020, L027, L029, and half of the
B-wave bugs: the *checker* knows something about a program (the type
of an expression, the payload types of an enum variant, whether a
pattern binding is in scope), but the information is *recomputed*
post-hoc in the *codegen* via `infer_expr_type`, *after* the
authoritative source has gone out of scope.

Concrete instances:

- **L027 (match-as-expression arm types).** Codegen's
  `compile_match` computed the result type by calling
  `arms.iter().find_map(|arm| infer_expr_type(arm))` *after*
  `self.locals` had been restored to `base_locals`. Pattern-bound
  names (`dc` in `Ok(dc) => dc.interfaces`) were no longer in scope,
  so the first arm returned `None`, and the second arm's `[]`
  dominated via `List<Unknown>`. Fix: collect each arm's
  `TypedValue.ty` during arm compilation while the bindings are
  still live.

- **L028 (f-string templates).** The lexer stored the raw f-string
  template as a `String` with `{{...}}` embedded. Four downstream
  consumers (compile_fstring, render_fstring, two
  collect_expr_names) independently re-parsed the template,
  sometimes disagreeing about how to handle escapes. Fix: produce a
  typed `Vec<FStringPart>` at lexing time, consumed by all sites.

- **L029 (statement-position match compat).** The checker needed to
  know "is this match being used as a value?" when deciding whether
  to run arm unification. That information existed at the call site
  (the parent statement) but was not passed to `check_expr`.

**Principle.** *When a helper produces a "throwaway" type via
post-hoc inference, ask whether the authoritative type exists
during the operation being inferred.* Quote from L027:

> The fix was to plumb it through, not to improve the post-hoc
> inference. Every type ingested from `infer_expr_type` where
> `compile_expr` already knew the answer is a bug waiting to bite.

**Fuse3 implication.** Use a **typed AST** (let's call it TAST)
produced by a single checker pass. Every node carries its inferred
type, context (statement vs value), and liveness (future uses) as
attached data. Codegen reads from TAST without doing any type
inference. "Type inference in the backend" is an anti-pattern.

## 4.5 Determinism: the HashMap lesson

B1 of the parity plan replaced `HashMap<PathBuf, LoadedModule>` with
`BTreeMap`. Before: errors were reproduction-resistant because
different runs processed modules in different order. Failures
appeared at random across `unsupported List member call concat`,
`cannot infer member`, `missing layout for T`, etc.

**Principle.** Any data structure iterated during codegen or
error reporting must be deterministic. `HashMap` is not, unless
seeded.

**Fuse3 implication.** Go's `map` is also non-deterministic by
design. Any code path that iterates a map and produces output must
sort the keys first, or use a deterministic container. Fuse3
should have a lint that flags `for k := range mapvar` inside
codegen/diagnostic paths.

## 4.6 Diagnostics as a first-class concern

Pillar 3 says developer experience matters. In practice:

**Worked.**

- Rich diagnostics with source snippets and color in Stage 1's
  `render_long()` path.
- The "hint" field in every diagnostic (e.g., `did you forget
  import stdlib.core.list?`).
- The "expected output" / "expected error" test format is the
  simplest possible fail-fast mechanism.

**Hurt.**

- **Codegen errors had no source spans until B12.** Every `format!`
  in `compile_member_call`, `compile_call`, `compile_match`, etc.
  returned a bare `String`. When the user saw `cannot infer receiver
  type for \`len\``, they had no way to find which line. B12 added
  `LoweringState::err_at(span, msg)` as the retrofit, but only
  touched the one site that was causing blocking problems.

- **Cranelift verifier errors printed raw IR with no Fuse symbol
  name.** B12 also tagged these. Before: the user saw 50 lines of
  `sig0 = () -> i64 windows_fastcall` with a single error message
  buried 40 lines down and no indication of which Fuse function
  failed. After: `Cranelift verifier failed while compiling <name>
  (module <file>)`.

- **`unresolved method` was silently allowed by the checker.**
  Issue 1 of the T4 parity investigation: checker let `scope.concat(...)`
  through even when `stdlib.core.list` wasn't imported. B2 fixed
  this by enforcing extension resolution. But the gap had existed
  for the entire Stage 2 development period.

**Fuse3 implication.**

- Every error carries a span from day one. `Error` is a struct with
  `message`, `span`, `hint`, `severity`, `code` fields. No
  `fmt.Errorf`-style bare strings in the compiler.
- The checker must be complete. A checker that silently accepts
  something it cannot handle produces downstream codegen errors that
  are much more expensive to triage. **Silent failure is the single
  biggest class of bug in the Fuse2 backlog.**

## 4.7 Fuse3 compiler architecture invariants (the eight rules)

Fuse3 is a systems language whose compiler is itself held to
systems-level correctness. This section codifies the eight
architectural rules that make the Fuse2 bug classes structurally
impossible rather than "caught by tests". The aggregate costs ~3000
extra lines of compiler framework before the first line of real
compilation logic — it is exactly the right investment given what
Fuse2's B-wave triage taught us about where bugs actually came from.

Each rule eliminates a concrete bug class that bit Fuse2. Adopting
only some of the rules leaves those classes open; they are designed
to be adopted together.

### Rule 1 — Three IRs: AST → HIR → MIR

The compiler has three intermediate representations:

- **AST** (Parse AST): pure syntax. Produced by the parser, consumed
  by name resolution and type checking. No types, no resolved names,
  every node carries only `span: SrcRange`.
- **HIR** (High-level IR): names resolved, types inferred, every node
  carries its full metadata contract (Rule 2). Produced by the
  checker, consumed by the ASAP lowering pass.
- **MIR** (Mid-level IR): a flattened, nearly-C form where every
  `Drop(local)` and every `Move(dst, src)` is an explicit statement,
  and nested expressions have been lifted into named temporaries.
  Produced by ASAP lowering, consumed by the C emitter.

**Why three, not two.** MIR exists so that ASAP destruction placement
is a unit-testable pass that operates on HIR and produces MIR with no
codegen noise. Table-driven tests of the form "given this HIR, expect
these exact Drops in this exact order" catch L027-class bugs before
they reach C. The MIR→C emitter is then a mechanical pretty-printer
with no judgment calls, which means a user debugging a miscompile can
read the emitted C line-by-line against the source.

**Bug classes eliminated.** L027 (pattern-bound value lifetimes), B7
(match arm unification re-derivation), B12 (divergence tagging):
every bug where codegen had to re-derive something the checker
already knew.

### Rule 2 — Separate Go types per IR, no nullable post-checker fields

`ast.Expr`, `hir.Expr`, `mir.Expr` are three disjoint Go interfaces.
A function that takes an HIR node cannot accept an AST node — Go's
type system rejects it at compile time. Inside HIR, every field that
the checker is responsible for is non-nullable:

```go
type HirExpr interface {
    Span() SrcRange              // from AST
    Type() TypeId                // always valid; never Unknown
    Ctx() ExprContext            // Statement | Value | Tail
    LiveAfter() BitSet           // which bindings survive
    Owning() Option[LocalId]     // produces-owned-value annotation
    DivergesHere() bool          // Never-type propagation
}
```

`Type() TypeId` returns a valid interned type ID or the HIR node
wasn't constructible. A checker that fails to infer a type emits a
diagnostic and halts; it *cannot* produce a half-inferred HIR node.
HIR constructors take all required metadata as parameters, so you
literally cannot build an underspecified node.

**Bug classes eliminated.** The silent-checker bug class (the single
biggest class in Fuse2's backlog per §4.6). "Checker knew but
codegen saw None" becomes unrepresentable at the type level.

### Rule 3 — Exhaustive node kind list, frozen at language-guide freeze

Every AST, HIR, and MIR node kind lives in an appendix of the Fuse3
language guide. Adding a kind after freeze requires an ADR. Every
compiler pass is then an exhaustive `switch` over a known-finite set,
and self-hosted Fuse3 itself checks exhaustiveness at compile time.

**Bug classes eliminated.** Fuse2's organic AST growth created
passes that handled 19 of 20 cases and silently dropped the 20th.
Freezing the node set means the "did you handle every kind?"
question becomes a literal compile error.

### Rule 4 — Pass manifest framework

Every compiler pass registers with a framework declaring which HIR
fields it reads and which it writes:

```go
type PassManifest struct {
    Name    string
    Reads   []FieldId   // e.g. [hir.FieldType, hir.FieldCtx]
    Writes  []FieldId   // e.g. [hir.FieldLiveAfter]
    After   []string    // passes that must run before this one
}
```

The framework enforces ordering: a pass that reads `LiveAfter`
cannot run before the pass that writes it. At registration time,
the framework computes a topological order and rejects cycles.
This is how LLVM's analysis manager works.

**Bug classes eliminated.** Pass-order bugs (Fuse2 hit this twice
during B-wave triage). The cost is ~200 LOC of framework plus a
few lines per pass.

### Rule 5 — Invariant walkers at every pass boundary

Debug builds run a walker after every pass verifying its
post-conditions:

- Checker done → every HIR node has `Type() != Unknown`.
- ASAP lowering done → every owned local has a matching `Drop` on
  every control-flow path; no local is dropped before its last use.
- Codegen done → every C declaration has a use or is `@export`.
- Name resolution done → every `IdentExpr` has a resolved binding.

Release builds skip the walkers. Cost: negligible. Value: **catches
bugs at the layer they were introduced, not three passes later**.
This is the single highest-ROI discipline in the list — it turns
invariant violations from "mysterious downstream crash" into "the
pass that violated the invariant names itself".

**Bug classes eliminated.** The whole category of "introduced
early, manifested late" bugs that dominated B12 triage.

### Rule 6 — Deterministic collections only in IR data structures

No Go `map` in HIR or MIR. IR containers are ordered slices or a
`SortedMap[K, V]` wrapper. Go's map iteration is intentionally
randomized, and Fuse2 re-learned this lesson during Wave B1
(nondeterministic monomorphization order made codegen
non-reproducible; fixed by `HashMap` → `BTreeMap`).

Block it at the type level: the `hir` and `mir` packages do not
import Go's raw `map` for any field that participates in emission
order. A lint (part of Rule 4's framework) flags
`for k := range mapvar` inside codegen/diagnostic paths.

**Bug classes eliminated.** Wave B1 reproducibility loss, and the
whole family of "CI passes, local fails" flakes that depend on
map iteration order.

### Rule 7 — Global `TypeTable` with interned type IDs

Types are `u32` IDs, not structs. Type equality is integer
comparison. The `TypeTable` is a global intern table keyed by
structural hash:

```go
type TypeTable struct {
    types  []TypeKind     // indexed by TypeId
    intern map[uint64]TypeId
}

func (t *TypeTable) Intern(kind TypeKind) TypeId { ... }
func (t *TypeTable) Kind(id TypeId) TypeKind     { ... }
```

Two HIR nodes with the same `Type() TypeId` are *provably* the same
type — not "structurally equivalent, let me re-walk to make sure".
This is how Rust's `TyCtxt` works and it is both faster (integer
compare) and more robust (single source of truth).

**Bug classes eliminated.** Type-equality bugs where two
structurally-identical types were compared via walk and one walker
was slightly wrong. Fuse2 had several of these in generic
substitution (Wave B3).

### Rule 8 — Property-based tests on IR lowering

A property-test harness generates random valid HIR, lowers it to
MIR, and verifies semantic preservation invariants:

- No owned local is dropped before its last use.
- No variable is read before it is assigned.
- Every function has exactly one entry and its returns type-check
  against the declared return type.
- Every `Drop` has a matching owning binding.
- `!`-typed expressions never have a successor in the CFG.

These find the bugs nobody thinks to write a unit test for. Fuse2
had zero property tests; roughly half of the L-bugs would have been
caught by a modest fuzzer.

**Bug classes eliminated.** "We didn't know to test that combination"
bugs — the long tail that unit tests miss because a human has to
imagine them first.

### Summary: what the eight rules cost and what they buy

**Cost.** ~3000 lines of compiler framework (pass manager, invariant
walkers, type table, MIR lowering, property-test harness, HIR node
hierarchy) before the first real compilation logic. For Fuse2 that
would have felt like over-engineering. For Fuse3, after learning
what the bug classes actually were, it is exactly the right
investment.

**Buy.** The Fuse2 bug pattern — "pass A knew something, pass B
re-derived it and got it wrong" — becomes *structurally impossible*:

- Rules 1, 2, 4, 5 make "metadata missing" unrepresentable.
- Rules 3, 6 make "nondeterministic output" unrepresentable.
- Rule 7 makes "type equality mismatch" unrepresentable.
- Rule 8 catches the long tail.

**Temptation to skip.** The rules most likely to be cut as
"over-engineering" are Rule 4 (pass manifest) and Rule 8 (property
tests). Don't. Those two alone would have prevented roughly 60% of
Fuse2's B-wave grief.

**The bottom line.** Adopt all eight together. They are mutually
reinforcing — skipping one leaves its bug class open and weakens
the others (e.g. Rule 5 invariant walkers depend on Rule 2's
non-nullable fields being enforceable). Fuse3's compiler is a
systems program that compiles a systems language; the correctness
bar is the same at both layers.

---

# Part 5 — Stdlib design

## 5.1 Core / Full / Ext tiers

From [`fuse-stdlib-spec.md`](../fuse-stdlib-spec.md):

- **`stdlib/core/`** — pure computation, no OS interaction, no
  FFI to external systems. Must work in a Core interpreter. 22
  modules in Fuse2: `result`, `option`, `list`, `map`, `set`,
  `string`, `int`, `int8`, `int32`, `uint8`, `uint32`, `uint64`,
  `float`, `float32`, `bool`, `math`, `fmt`, plus the interfaces
  `equatable`, `hashable`, `comparable`, `printable`, `debuggable`.
  ~2,499 LOC.

- **`stdlib/full/`** — OS syscalls, FFI, concurrency. 14 modules:
  `io`, `path`, `os`, `env`, `sys`, `time`, `random`, `process`,
  `net`, `json`, `http`, `chan`, `shared`, `simd`. ~1,981 LOC.

- **`stdlib/ext/`** — optional, heavyweight, not bundled.
  11 modules: `argparse`, `crypto`, `http_server`, `json_schema`,
  `jsonrpc`, `log`, `regex`, `test`, `toml`, `uri`, `yaml`.
  ~1,900 LOC.

Pattern: **extension functions on built-in types.** `fn String.scream(ref self) -> String { self.toUpper() + "!" }`. Reads clean.
Resolves at compile time.

**What worked.** The tiering is correct. Users opt into `full`
(gets OS, threads, FFI) or `ext` (gets heavier dependencies) by
importing. `core` is always available.

**What hurt.** Missing imports silently compiled wrong. See §4.6
above (checker gap) and Wave B11 (where it took adding missing
`import stdlib.core.list` to 8 files to unblock stage 2 self-host).

## 5.2 Auto-generation from field metadata (ADR-013)

The big win of Wave 5:
[`ADR-013`](../adr/ADR-013-compile-time-reflection.md).

- **Interfaces define behavior.** `Equatable`, `Hashable`, `Comparable`,
  `Printable`, `Debuggable`, `Serializable`, `Encodable`, `Decodable`.
- **`implements` triggers auto-generation.** The compiler sees the
  field list and generates the required methods.
- **Decorators are compiler directives, not behavior.** `@value`,
  `@entrypoint`, `@export`, `@rank`, `@test`, `@inline`, `@builder`,
  `@deprecated`, `@ignore`.

The distinction is **load-bearing for the language's aesthetic**: the
user never writes `@hashable data class Key(...) implements Hashable`
— the `implements` clause is the single signal.

**What worked.**

- `data class Point(x: Int, y: Int) implements Hashable, Comparable,
  Debuggable` gets 6 methods for free, all zero-runtime-cost,
  specialised per-type.
- Users can override any auto-generated method by writing it
  manually. The compiler picks the manual version.
- Runtime reflection is rejected in favour of this system. No
  metadata in the binary.

**What hurt.** Auto-generation is only available for `data class` and
`@value struct` — the types whose fields the compiler fully knows.
Plain `struct` without `@value` must implement manually. This is
correct, but users wanted to use plain structs for encapsulation and
still get free equality. Compromise: the checker's error message is
explicit — *"either add @value to enable auto-generation, or
implement the method manually"*.

**Fuse3 implication.** Keep ADR-013's mechanism exactly — it's the
most elegant piece of Fuse design. One terminology change:
**Fuse3 calls this construct a `trait`, not an `interface`.** The
word change matches Rust and Mojo (both of which have the same
feature set: default methods, parent composition, generic bounds,
auto-generation) and reflects what Fuse's construct actually is —
a set of capabilities a type has, not a point of interaction. The
`implements` keyword stays (`data class Point(...) implements
Hashable`) because it reads naturally with either vocabulary. All
stdlib "interfaces" from Fuse2 (`Equatable`, `Hashable`,
`Comparable`, `Printable`, `Debuggable`) are renamed to "traits"
in the Fuse3 guide; the semantics and auto-generation rules are
unchanged.

Fuse3's compile-time autogen reads directly from the typed AST,
same as Fuse2's autogen module. The emitted C includes the
generated trait-method implementations as regular C functions —
no vtables, no runtime metadata, same zero-cost story as Fuse2.

## 5.3 The stdlib-first test strategy and its yield

Quote from the Compiler Bug Policy in
[`fuse-stdlib-spec.md`](../fuse-stdlib-spec.md):

> The standard library is not a workaround surface. It is a stress
> test. When a library implementation triggers a compiler bug [...]
> Stop. Reproduce it minimally. Fix the compiler. Verify. Resume.
>
> Cutting corners in the library to avoid a compiler bug is not a
> solution. It is a debt that will be collected during Stage 2, with
> interest.

During Wave 1 of stdlib implementation (11 modules), the team found
and fixed **10 compiler/evaluator bugs** (Bugs #1–#10 in
[`stdlib_implementation_learning.md`](../stdlib_implementation_learning.md)).
Patterns:

- Extension function resolution dispatch didn't match between
  zero-arg and multi-arg paths (#1)
- Cranelift "block already filled" after return in match arm (#2)
- Spec conformance: concrete types shipped where generics were
  required (#3)
- Missing language primitive: `!` (Never) via trap (#4)
- Evaluator f-string interpolation silently dropped method calls (#5)
- Parser rejected keywords as member/method names (#6)
- Float display, float arithmetic, float comparison (#7–#9)
- ASAP name extraction missed f-string references (#10)

**Yield per bug.** Every bug produced a regression test. Every bug
went into the bug log. Every bug was fixed in the compiler, not
worked around in the library.

**Fuse3 implication.** This is a *strategy*, not a feature. Fuse3
should adopt it verbatim: write the stdlib in Fuse3 itself as the
first real stress test of the compiler. Any bug found there goes
into a learning log identical to
[`stdlib_implementation_learning.md`](../stdlib_implementation_learning.md).

## 5.4 Maps (user flagged for from-scratch redesign)

Fuse2's `Map<K,V>`:

- Created with `Map::<K, V>.new()`.
- Key must support equality (and eventually `Hashable`).
- `.get(key) -> Option<V>` (no null, no panic).
- `.set(key, value)`, `.contains(key)`, `.remove(key)`, `.len()`,
  `.isEmpty()`, `.keys()`, `.values()`, `.entries()`.
- Runtime-backed by Rust `std::collections::HashMap` behind a
  `FuseHandle` wrapper.

**What worked.** The API is right. The `Option<V>` return on `get`
eliminates the null/panic/sentinel footgun. The `.keys()`, `.values()`,
`.entries()` separation is clean.

**What hurt.**

- **Map had to exist before `Hashable` did.** This pushed the
  runtime into supporting `Int`/`String` keys via special cases,
  and other types via string-formatted hashes. The
  [`stdlib-interfaces-plan.md`](../stdlib-interfaces-plan.md)
  explicitly calls out "Stage 2 relevance: Equatable and Hashable
  are blockers — the self-hosted compiler needs HashMap with
  custom-type keys and real `==` dispatch."

- **Map iteration order is non-deterministic.** Rust's HashMap
  (like Go's) iterates in random order. This bit the codegen
  when it iterated over the program's modules (fixed in Wave B1
  by switching to `BTreeMap`). It will bite anyone who tries to
  use `Map` to produce deterministic output.

- **Runtime FFI is 30 functions** (`fuse_map_new`, `fuse_map_set`,
  `fuse_map_get`, `fuse_map_remove`, `fuse_map_contains`,
  `fuse_map_len`, `fuse_map_keys`, `fuse_map_values`,
  `fuse_map_entries`, plus specialisations). Each one crosses the
  FuseHandle ABI boundary. See §4.2.

**Fuse3 implication — why the user flagged this.**

- **Map is a Fuse type, implemented in Fuse over a small C
  runtime primitive.** The C backend (§1.4) means there's no Go
  `map[K]V` to borrow — we write our own hashtable. The primitive
  layer in C provides just the unsafe byte-level hashtable
  operations (hash, probe, resize), and the friendly `Map[K, V]`
  API with `Hashable` keys is written in Fuse itself. ~200 LOC of
  C for the primitive, a few hundred lines of Fuse for the type.

- **The trait system (`Hashable`, `Equatable`) must exist
  *before* Map is designed.** This is the opposite order from
  Fuse2. It means Fuse3's Wave 1 implements traits; Wave 2
  implements Map on top. Map has no special cases for `Int` or
  `String` keys — every key type implements `Hashable` through
  the same mechanism.

- **Deterministic iteration by default.** Fuse3 maps iterate in
  insertion order — the same choice modern Python dicts and
  Swift's `OrderedDictionary` made. It is friendlier, catches
  non-determinism bugs earlier, and costs one extra pointer per
  bucket (small linked list or index vector). Users who want
  sorted iteration get `map.sortedKeys()` explicitly. Users who
  want unspecified-order iteration for performance get an
  opt-out (`map.unorderedKeys()`), but the *default* is
  deterministic. This choice alone would have prevented Fuse2's
  Wave B1 HashMap→BTreeMap rewrite.

---

# Part 6 — Bug taxonomy (what actually broke)

## 6.1 L001–L029 summary

The [`learning.md`](../learning.md) file groups bugs into 8 triage
groups (G1–G8). Every L-entry has a reproduction, root cause, fix
plan, and status. As of the B12 session, L001–L029 are all fixed
(or deferred to B12 follow-up with documented trade-offs). Summary
of what bit:

| Group | Area | Representative bugs |
|---|---|---|
| **G1 — Codegen fundamentals** | IR value correctness | L006 (map import → Cranelift verifier errors), L010 (Comparable `<`/`>` → bad IR), L011 (`.get()` None path → null handle crash) |
| **G2 — Control flow** | Loops, break/continue, divergence | L002 (`continue` in `for` → infinite loop), L003 (`loop { return }` → type mismatch because loop is Unit not Never) |
| **G3 — Lambdas & closures** | Higher-order function dispatch | L007 (lambda fn pointer boxed as FuseHandle passed to `call_indirect`) |
| **G4 — Parser gaps** | Missing syntax | L008 (`struct<T>`), L009 (`implements<T>`), L018 (`enum<T>`), L021 (`spawn move { }`) |
| **G5 — Pattern matching & chaining** | Destructuring, optional chains | L005 (enum multi-payload binding), L016 (`?.` into method call), L027 (match-as-expression arm unification), L029 (statement-position match compat) |
| **G6 — Generics codegen** | Type-param substitution | L019 (`<T: Printable>` bound parsing), L020 (generic `List<T>.push()` in user fn) |
| **G7 — Stdlib, tests, tooling** | Missing imports, missing helpers | L004 (struct field privacy), L012 (`Int.toFloat()` missing import), L014 (parallel test runner hash collisions), L015 (`Int.hash()` missing) |
| **G8 — Concurrency safety** | Checker gaps | L022 (var mutation inside spawn), L023 (multiple `__del__` duplicate symbols) |

**Newest entries (from B10–B12):** L026 (fuse-lsp referenced removed
field after B3 rename), L027 (match-as-expression inference),
L028 (f-string `{{`/`}}`), L029 (statement-position match).

## 6.2 B-wave taxonomy

The B-wave plan ([`fuse-stage2-parity-plan.md`](../fuse-stage2-parity-plan.md))
organized the Stage 2 parity work into 13 waves:

- **B0** — Baseline & verification
- **B1** — Determinism (HashMap → BTreeMap)
- **B2** — Checker extension resolution enforcement (silent drops)
- **B3** — Parser & AST enum variant payload types
- **B4** — Codegen generic substitution at extension call sites
- **B5** — Codegen hardcoded specialization ordering
- **B6** — Codegen user-defined enum variant binding
- **B7** — Codegen match-as-expression type unification (rules U1–U6)
- **B8** — Codegen namespace static method calls
- **B9** — Codegen tuple field access type propagation
- **B10** — Lexer f-string brace escaping
- **B11** — Stage 2 source missing imports
- **B12** — Stage 2 self-compile verification (**in-flight as of this doc**)
- **B13** — Institutional knowledge & document sync

Each wave is one *bug class*. Each was load-bearing. Each was known
before B0 started (from the T4 parity investigation). Each took
between 30 minutes and a full day to fix properly.

**Meta-observation.** The B-waves were not "new bugs found during
Stage 2" — they were *latent bugs catalogued in the T4 investigation*
that had to be fixed in a specific order because they compounded.
The fact that the plan could be written down at all means the bug
class was already understood. What was *not* understood was the B12
ABI cascade.

## 6.3 B12 session (new): the 8 ABI-mismatch cascade

This is fresh material from the current session — not yet
documented in `learning.md` beyond the commit messages. Capturing it
here so it's not lost.

Root cause was **"stage 2 was never exercised end-to-end"**. The
W7.5 bootstrap test input was a trivial program, not the real
`stage2/src/main.fuse`. When the real source was compiled for the
first time, the following cascade surfaced:

| # | Layer | Bug | Commit |
|---|---|---|---|
| 1 | Checker + Codegen | `compile_match` / `compile_two_arm_match` / `compile_when` hardcoded block arms as `Some("Unit")` even when the block diverged (`return` / `sys.exit` / trap). Every `val x = match y { Ok(v) => v, Err(_) => { return } }` lost its type. | [41a0cb4](../../) |
| 2 | Codegen diagnostics | `cannot infer receiver type for X` had no source span. Cranelift verifier errors had no Fuse symbol name. | [41a0cb4](../../) |
| 3 | Linker | 6 Cranelift FFI wrappers declared in stage 2 but never implemented in the Rust crate: `builder_block_param`, `builder_inst_result`, `builder_declare_var`, `builder_def_var`, `builder_use_var`, `ins_call_n`. | [d50f90c](../../) |
| 4 | Extern decl mismatch | `cranelift_ffi_signature_new` declared with 1 param in `codegen.fuse` and 2 in `runtime.fuse`. The declare loop picked one by BTreeMap order, producing a silent Cranelift verifier error on the first call that disagreed. Fix: align the declaration, plus compiler error on *any* arity mismatch between duplicate externs. | [196fbb0](../../) |
| 5 | Runtime type mismatch | `fuse_result_is_ok` declared as `(ptr) -> ptr` in stage 2's `declareAllRuntime` but the Rust function returns `bool` (I8). Garbage upper bits caused every `match result { Ok(_) => ..., Err(_) => ... }` to pick the wrong arm. Fix: `runtimeReturnTypeId` lookup for 11 non-uniform runtime functions. | [196fbb0](../../) |
| 6 | List ABI | `cranelift_ffi::read_values(args, count)` dereferenced `args` as a raw `*const i64`. Stage 2 passes a Fuse `List<Int>` handle. Fix: `fuse_runtime::extract_int_list` iterates the list, `read_values` uses it, smoke test updated. | [9126baf](../../) |
| 7 | String ABI | `cranelift_ffi::str_from_raw(ptr, len)` same problem. Fix: `fuse_runtime::extract_string_pub`, `str_from_raw` dual-path. Also: `cranelift_ffi_module_define_data` with raw bytes. Same treatment. | [9126baf, 9fdc21c](../../) |
| 8 | Stage 2 source | `parseOutputFlag` had `return next.unwrap()` where the function signature promised `Option<String>`. The checker did not verify explicit `return` types against function signatures. | [9126baf](../../) |

**Result after the 8 fixes.** For the first time in the project's
history, `fusec stage2/src/main.fuse -o fusec2.exe` produced a
self-hosted binary *and* that binary successfully compiled a trivial
Fuse program (`@entrypoint fn main() { }`). The compiled binary ran
and exited 0.

**What's still broken as of this writing.** `fusec2 hello.fuse -o
hello.exe` still panics with another `runtime received null Fuse
handle` somewhere in the `println` runtime call chain — one more
layer of the same ABI-mismatch cascade. Another audit pass would
find it; we stopped to write this learnings doc instead.

## 6.4 Pattern analysis

Across L001–L029 and B1–B12, the bug classes by count:

| Class | Count | Representative |
|---|---|---|
| **Information loss between checker and codegen** | 10+ | L027, L028, L029, B7, B9 |
| **Silent failure in the checker** | 5 | B2 (extension resolution), L022, L029, parseOutputFlag, L013 (test authoring) |
| **ABI mismatch at a subsystem boundary** | 8 | All of B12 |
| **Pattern-match ownership / bind lifetime** | 5 | L005, L027, L029, plus two in stdlib |
| **Missing diagnostics / bad error messages** | 5 | B12 span work, extern arity check, Cranelift verifier tagging |
| **Parser gap for new syntax** | 4 | L008, L009, L018, L021 |
| **Control-flow / divergence typing** | 3 | L002, L003, B12 Never-arm |
| **Determinism** | 2 | B1 (HashMap), L014 (parallel test runner) |
| **Stdlib workaround retained** | 3 | Bug #11 (stack frame), Bug #5 workaround (hand-rolled f-string eval) |

**Top three root-cause patterns.** If Fuse3 eliminates these three,
it eliminates most of Fuse2's backlog:

1. **Post-hoc type inference in the codegen.** Fix: typed AST, one
   checker pass, codegen never re-infers.
2. **Silent failure in the checker.** Fix: every "could not resolve"
   path returns an error, never falls through to "try something else"
   unless that something else is documented and deterministic.
3. **ABI boundary without a contract test.** Fix: one runtime ABI
   under Fuse3's sole control (the Fuse3 C runtime, §1.4 / §4.2),
   specified in the guide alongside the emitted C format, and
   verified by a contract test that compiles reference Fuse
   programs and diffs output byte-for-byte against an oracle.

---

# Part 7 — Process learnings

## 7.1 The "plan doc, execute strictly" pattern

Every wave in Fuse2 had a plan doc. Every plan doc had:

- A status section
- Mandatory rules (philosophy, Rules 1–8)
- Resolved design decisions (with rationale for each)
- Task summary table
- One section per wave with phase sections inside
- Each phase had a "what is the issue", "what needs to be done",
  "how should it be done", and a task checklist with checkbox tasks
- Each phase ended with a commit at the phase boundary

**What worked.** The plan docs were load-bearing for the entire
project. They survived multiple sessions, multiple AI agents, and
multiple implementers. Anyone could pick up a phase, read the
mandatory rules, read the phase section, and start executing.

**What hurt.** The plans were the most expensive documents to write
and the most painful to keep in sync. When Wave B10 discovered a
new gap (L028), the plan was updated, the learning was written, the
commit messages were detailed, *then* the code landed. That cadence
is sustainable for one person writing carefully. It's expensive.

**Fuse3 implication.** Keep the plan-driven pattern. Compress it if
possible. The rules section can live in one shared file (`rules.md`)
and be linked from every plan. The "mandatory rules" section at the
top of every document is long. Better: a single rules file, every
plan doc starts with "read rules.md and this doc".

## 7.2 The "no corners cut" rules

From [`fuse-stage2-parity-plan.md`](../fuse-stage2-parity-plan.md),
Mandatory Rules section:

- **Rule 1 — No corners cut.** Every fix is a proper fix. No
  workarounds. No "patch the test fixture." No "rewrite the Stage 2
  source to avoid the compiler bug." No TODO comments.
- **Rule 2 — Solve problems immediately.** If a new bug is
  discovered while executing any phase, stop and fix it in the same
  phase. Do not defer.
- **Rule 3 — Ground every fix in the code.** Cite the exact file and
  line range before the fix; cite the test that exercises it after.
- **Rule 4 — Zero regressions.** All five test suites must be green
  after every phase.
- **Rule 5 — Determinism is load-bearing.** Non-determinism is a
  bug.
- **Rule 6 — Diagnostics are first-class.**
- **Rule 7 — Document what you learn.**
- **Rule 8 — Phase completion standard.** A phase is done only when
  all checkboxes are `[x]`, all tests pass, and the plan doc is
  updated.
- **Rule 9 — Commit at phase boundaries.**

**What worked.** Rule 1 is the reason the B-wave plan actually
closed its bugs instead of accumulating technical debt. Every time
the temptation arose to "just patch stage 2 source to avoid the
compiler bug", the rule said no, and the compiler fix landed instead.

**What hurt.** Rule 2 is hard to hold in the presence of scope
creep. B11 discovered L029 (statement-position match compat) as a
Rule 2 fix, but the proper fix (threading `value_context` through
`check_expr`) was judged too invasive and a surgical Unit-filter
was applied instead with a B12 follow-up note. This is documented
honestly, but it's a soft violation of "no corners cut".

**Fuse3 implication.** Rules 1, 4, 5, 6, 7, 9 transfer directly.
Rule 2 should be softened to "Rule 2a: Solve problems immediately
*if the solution fits in the phase's scope*. Otherwise, log the
problem, defer to a follow-up phase, and be explicit about what
the compromise costs." This is what Fuse2's L029 did in practice.

## 7.3 Bug Policy (stdlib as stress test)

See §5.3. The bug policy from [`fuse-stdlib-spec.md`](../fuse-stdlib-spec.md)
is the most load-bearing single-paragraph rule in the Fuse2
project. **Transfer verbatim.**

## 7.4 Rules that worked, rules that didn't

**Worked.**

- "The guide precedes implementation."
- "No timelines — a language is complete when correct, not when a
  calendar says so."
- "Every feature has been proven in production at scale."
- "Stage 0 = Stage 1 = Stage 2 output."
- "When the guide and the code disagree, fix the code."

**Didn't work as well.**

- "No TODO, no defer, no workaround." Works for small fixes.
  Collides with reality when a fix requires a refactor larger than
  the current phase. L029 is the canonical example — the proper fix
  exists, was documented, but was not done because the wave didn't
  budget for it.

- "Stage 0 is a completed prototype." (ADR-012.) This was the
  *intended* rule but L001 shows it was broken in practice when
  implementers added features to Stage 0 to let Stage 2 code run
  there. Needs to be stronger: Stage 0 is read-only after Phase 5.

---

# Part 8 — Items flagged for fresh design in Fuse3

The meta-plan ([`fusev3-meta-plan.md`](fusev3-meta-plan.md))
explicitly calls out:

> Anything that was not done from scratch in Fuse2 will be put in
> guide, planed and developed from scratch based on what we have
> learnt. These include but not limited to the following:
> - concurrency
> - maps
> - memory management for threads

## 8.1 Concurrency — user directive

Covered in detail in Part 3 (§3.5). With the C-backend decision
(§1.4) locked in, the redesign surface is:

- **Primitives.** `spawn`, `Chan[T]`, `Shared[T]`, `@rank(N)`.
  Keep semantics from Fuse2 unchanged. Implement in the Fuse3 C
  runtime: pthread_create / Windows CreateThread for `spawn`,
  pthread_rwlock_t / SRWLOCK for `Shared[T]`, condition-variable-
  guarded ring buffer for `Chan[T]`. `@rank` is compile-time and
  emits no code.
- **`select`** — deferred to Fuse3 Wave 2, workaround (`spawn`
  per channel) documented day one. Implementing `select` properly
  requires waiting on multiple condition variables simultaneously;
  not hard but not trivial.
- **`SpawnHandle<T>`** — deferred. Fire-and-forget + channels
  covers the common cases. Reconsider if real-world Fuse3 code
  demands typed join.
- **Memory model** — documented explicitly as sequentially
  consistent across `Shared[T]` lock acquisition and `Chan[T]`
  send/recv edges. Outside those, the ownership model forbids
  concurrent access at compile time.

## 8.2 Maps — user directive

Covered in detail in §5.4. Redesign decisions already made:

- **`Hashable` (a trait, see §5.2) must exist before `Map` is
  designed.** Fuse3 Wave 1 implements traits; Wave 2 implements
  `Map[K, V]` on top. No runtime special cases for `Int`/`String`
  keys — every key type implements `Hashable` uniformly.
- **Iteration order is insertion-order by default.** Same
  decision modern Python, Swift `OrderedDictionary`, and JavaScript
  `Map` made. Prevents the Wave B1 HashMap→BTreeMap class of
  bug. Users who want explicit sorted iteration get
  `map.sortedKeys()`; users who want unspecified for performance
  get `map.unorderedKeys()`. The default is deterministic.
- **Map primitive is a ~200 LOC C hashtable** exposed to Fuse as
  opaque pointer plus hash/probe/resize operations. The friendly
  `Map[K, V]` API is written in Fuse itself on top of this
  primitive. No runtime FFI crate to drift.

## 8.3 Thread memory management — user directive

Covered partially in §4.2 and §3.5. With the C backend:

- **No FuseHandle.** No uniform-pointer ABI. Emitted C uses
  direct C types: `int64_t`, `struct { char *data; size_t len; }`
  for strings, `struct { T *items; size_t len, cap; }` for
  `List[T]`, etc. Type information stays at compile time; the
  emitted code has no runtime tags.
- **Thread-safe `Shared[T]`.** A struct holding the wrapped value
  plus a `pthread_rwlock_t` (or platform equivalent). `.read()`
  acquires a read lock, returns the value. `.write()` acquires
  a write lock, returns a mutable reference. Unlock via ASAP
  destruction of the guard. `@rank` is a pure compile-time check
  in the Fuse3 checker — no runtime race detector, no runtime
  ordering metadata.
- **Channels own their values.** A `Chan[T]` sending a `move` or
  `owned` value transfers ownership through the channel; the
  sender cannot use the value after send. Enforced at the Fuse
  checker, not by the C runtime. The C runtime just copies the
  bytes across the channel memory — the checker guarantees no
  aliasing on either side.
- **OS-thread stacks.** Default stack size set by the Fuse3 C
  runtime when calling pthread_create (Linux/macOS) or
  CreateThread (Windows). Start conservative (1 MB) and let users
  request larger via spawn options if a future use case demands
  it. Fuse2's "8 MB default stack" was a workaround for the
  compiler's own stack frame size; Fuse3's Go-hosted Stage 1
  doesn't have that problem.
- **No Go runtime.** Explicit. This whole section is about
  Fuse3-native thread memory management specifically because
  borrowing Go's runtime would leak Go's memory model and
  scheduler into Fuse binaries, which §1.4 rejects.

## 8.4 Other candidates surfaced by the learnings

These are not in the user's directive but are strong candidates
for from-scratch redesign based on the learnings:

- **The checker's `value_context` propagation.** See §2.1, §2.5.
  Fuse2 added it post-hoc during B11 as a Unit-filter. Fuse3
  should design it in.

- **Arm type unification algorithm.** See §2.5. Fuse2's U1–U6
  rules plus B12's U7 (Never) should be frozen into the Fuse3
  guide from the start, with matching checker and codegen code.

- **String indexing.** See §2.4. Fuse2's O(n) `charAt` vs O(1)
  `byteAt` split is correct but easy to misuse. Fuse3 should
  hide the trap.

- **F-string representation.** See L028 and §4.3. Fuse2 stored
  f-strings as `String` templates and re-scanned them four times.
  Fuse3 should parse f-strings into a typed AST node
  (`FStringNode { parts: []FStringPart }`) at lex time.

- **Diagnostic format.** See §4.6. Fuse3 should have `Diagnostic`
  as a struct from day one with `span`, `message`, `hint`,
  `severity`, `code`.

- **Module loading determinism.** See §4.5. Fuse3 should sort
  map keys wherever they feed into user-visible output or
  codegen.

- **Checker exhaustiveness.** The checker currently has several
  "silently pass through" code paths. Fuse3's checker should be
  a total function: every AST node is either accepted or
  rejected with a specific diagnostic, never ignored.

---

# Part 9 — Crosswalk: Fuse2 pain → Fuse3 design implications

This is the one-page version. If you have 5 minutes to read, read
this part and Part 1.

| Fuse2 pain point | What hurt | Fuse3 design implication |
|---|---|---|
| **Three-stage bootstrap (Python → Rust → Fuse)** | 3 independent implementations of lexer/parser/checker to keep in sync. Stage 0 accreted features for Stage 2. Bootstrap test never exercised real `stage2/src/main.fuse`. | **Two stages: Stage 1 (Go-hosted) → Stage 2 (self-hosted Fuse).** See §1.4. No Stage 0 — Fuse2's Python prototype already answered the implementability question. Stage 1 is one Go program that emits portable C99 and invokes the system C compiler (cc/gcc/clang/msvc); no Go runtime links into emitted binaries. Stage 2 is Fuse compiled by Stage 1; once self-hosting passes 3-gen reproducibility, Stage 2 becomes the primary compiler and Stage 1 retires to a boot-and-recover tool. Fuse builds Fuse, end-to-end. Bootstrap test always compiles the real `stage2/src/main.fuse`, not a synthetic input. |
| **Uniform FuseHandle ABI** | Elegant at the type system but brittle at the boundary: the smoke-test convention and the stage-2 convention disagreed silently, producing 8 different bug classes in B12. | **Single runtime boundary, under Fuse3's sole control.** No bundled Rust crate, no Cranelift/LLVM, no external convention. Emitted C talks to a Fuse3-owned C runtime (~500 LOC) through a contract specified in the Fuse3 guide and verified by an oracle test. No FuseHandle — emitted C uses direct C types (`int64_t`, `struct { items, len, cap }`, etc.) with type information erased at compile time. The B12 cascade pattern — an untyped bridge between independently-versioned modules — is structurally impossible. |
| **Checker silently accepts unknowns** | `unresolved extension method` fell through to codegen in Fuse2, producing obscure downstream errors. B2 fixed it in the B-wave retrofit. | **Checker is a total function.** Every node is accepted or rejected. No silent pass-through. The compiler is **complete** — never tentative. |
| **Post-hoc type inference in codegen** | Codegen re-computed types via `infer_expr_type` after pattern bindings had gone out of scope, producing L027, L028, L029 and half the B-waves. | **Typed AST (TAST).** Every node carries its type, value-context, and live-set *after* the checker pass. Codegen reads TAST; never infers. |
| **Post-hoc ASAP analysis with 4 independent walkers** | F-string name collection, match-arm type collection, ASAP dead-binding release, defer capture analysis — four separate walkers, all had to agree. | **Single AST attribute.** `node.LiveAfter() stringSet` computed once during TAST construction, consumed by everyone. |
| **Non-deterministic `HashMap` iteration** | Errors were reproduction-resistant; different runs hit different failures. B1 fix was `BTreeMap`. | **Fuse3 lint.** Iterating a Go `map` in a codegen/diagnostic path is a lint error unless preceded by `sort.Strings(keys)`. |
| **Bare-string codegen errors** | `cannot infer receiver type for X` with no span. Cranelift verifier errors with no Fuse symbol name. B12 retrofit added both. | **`Diagnostic` struct from day one.** No `fmt.Errorf` in the compiler. Every error carries span, message, hint, severity, code. |
| **Scheduling model deferral** | Stage 1 was single-threaded; Stage 2 planned OS threads; green threads deferred. Every phase had to imagine what the next phase would do. | **Day-one OS threads via the Fuse3 C runtime** (pthread_create / CreateThread). Same choice Fuse2 planned for its Stage 2, now unblocked earlier. Green threads stay deferred — they require us to write a scheduler, and Fuse3 is not borrowing Go's. |
| **Async/await re-rejected in Wave 0.6** | Distracted the team in an earlier iteration until it was killed. | **Never entertain async/await in Fuse3.** Closed question. |
| **Matches can be used as expressions or statements** | The checker flagged statement-position matches with mixed arm types (L029). Fix was a Unit-filter; proper fix deferred. | **`ValueContext bool` propagated through the checker from day one.** Every `check_expr` call knows whether the value will be used. |
| **`return <expr>` not checked against function signature** | Latent bug in `parseOutputFlag`. | **Checker verifies explicit return types.** Single entry point for "is this expression compatible with the function's declared return type". |
| **Missing imports silently allowed because of hardcoded runtime paths** | Wave B2 fixed it, but not before 284 unresolved extension calls existed across 8 files. | **Imports are the only way to resolve extensions.** No hardcoded fallback in the codegen. No "did you forget an import" at link time. |
| **`Map` before `Hashable`** | Required runtime special-casing for `Int`/`String` keys. | **Traits before Map.** Sequence: `Equatable`, `Hashable`, `Comparable` traits first, then `Map[K, V]` as a generic container requiring `K: Hashable`. Iteration is insertion-order by default; sorted and unordered are explicit opt-ins. |
| **Stdlib written around compiler bugs (pre Bug Policy)** | Bug #11 (stack frame workaround) retained in stdlib. | **Bug Policy from day one.** No workarounds in stdlib, ever. Bug found → compiler fixed → stdlib rewritten the natural way. |
| **Plan docs expensive to maintain** | Every B-wave required the plan, the learning entry, the commit message, the status table, the memory update. | **Shared `rules.md`.** Every plan links to it instead of re-stating it. Also: tool assistance for plan updates. |
| **Docs sprawl (18 docs, 15k lines)** | Finding the right place for a new learning was hard. | **Fewer, denser docs.** One guide. One plan (per active wave). One learning log. One ADR directory. |

---

# Part 10 — Open design questions for the Fuse3 language guide

These are the questions the guide must answer. Decisions already
made during the learnings-doc review are marked **[DECIDED]** with
a short summary and a pointer to where the details live. Remaining
questions are listed in rough dependency order.

## Foundation (mostly settled)

1. **Implementation language.** **[DECIDED]** Stage 1 is written
   in Go. Go is used for its host-language leverage (strong
   stdlib, fast iteration, stable tooling), *not* for its runtime
   or codegen — Fuse3 does not emit Go source and does not link
   against the Go runtime. See §1.4.

2. **Codegen path.** **[DECIDED]** Emit portable C99 source plus
   a small Fuse-owned C runtime (~500 LOC target), invoke the
   system C compiler (cc / gcc / clang / msvc) to produce native
   binaries. `cc` is treated as an ambient OS tool, not a bundled
   dependency. Cross-compilation works via cross-cc toolchains.
   Options (emit Go + disable GC) and (custom backend) both
   rejected — see §1.4 for the reasoning. The specific question
   "what primitives does the C runtime expose?" is still open —
   see Q14 below.

3. **Module system.** **[DECIDED]** Inherit Fuse2's `import a.b.c`
   → `src/a/b/c.fuse` one-file-per-module model. Add `pub import`
   for explicit re-exports. Add `//!` module-level doc comments.
   Reject trailing commas in import lists. See §1.5.

4. **Behavioral contracts.** **[DECIDED]** Renamed from
   `interface` to `trait` to match Rust/Mojo vocabulary and more
   accurately describe what the construct is. `implements`
   keyword retained. ADR-013 semantics (default methods, parent
   composition, generic bounds, auto-generation from field
   metadata) unchanged. See §5.2.

5. **AST representation.** **[DECIDED]** Three IRs:
   AST → HIR → MIR, with strict Go-type separation, non-nullable
   post-checker metadata, frozen exhaustive node kind list, a
   pass manifest framework, invariant walkers at every pass
   boundary, deterministic-only IR collections, a global interned
   `TypeTable`, and property-based tests on IR lowering. The full
   specification is in §4.7 "Fuse3 compiler architecture
   invariants (the eight rules)". The aggregate cost is ~3000
   lines of framework; the benefit is that the checker→codegen
   information-loss bug class that dominated Fuse2's B-wave
   triage becomes structurally impossible rather than
   test-covered.

## Language semantics (carried over from Fuse2, mostly decided)

6. **Ownership keywords.** **[DECIDED]** Keep `ref` / `mutref` /
   `owned` / `move` exactly as Fuse2 shipped. `mutref` must
   appear at *both* the parameter declaration AND the call site
   — the call-site annotation is load-bearing for pillar 3
   ("reading the call site tells you which arguments will be
   modified without looking up the function signature"). See §2.1.

7. **ASAP destruction.** **[DECIDED]** Keep. Implementation note:
   the liveness analysis is a single pass computed during HIR
   lowering, exposed as a per-node `LiveAfter` attribute and
   consumed by every release site and destructor site — NOT a
   separate walker per consumer, which was the Fuse2 bug pattern
   behind L027/L028. See §2.2 and §4.3.

8. **Result / Option / `?` / `?.` / `?:`.** **[DECIDED]** Keep the
   Fuse2 semantics verbatim. The only addition: the checker MUST
   verify `return <expr>` against the function's declared return
   type — the same code path that validates trailing-expression
   return types. This closes the `parseOutputFlag` bug class (a
   function declared `Option<String>` was silently shipping a
   `String` via `return next.unwrap()` because the checker only
   validated trailing expressions). See §2.3.

9. **Match / when / exhaustiveness.** **[DECIDED]** Keep Fuse2's
   semantics. Formalise U1–U7 arm unification rules, including
   `!` (Never) as a real type in the type system. No conflict
   with boolean negation — Fuse uses the `not` keyword for that,
   never `!`. The `!` character appears only as the two-character
   `!=` token (lexed as a unit) and as the Never type marker in
   type position; the two uses are syntactically unambiguous
   because they never occupy the same grammar slot. Rust uses
   both, and it works for them; Fuse is cleaner because `not`
   handles negation and `!` is left solely for Never.

10. **Traits and `implements`.** **[DECIDED]** Keep ADR-013
    semantics verbatim under the new `trait` vocabulary.
    Default methods, parent composition, generic bounds
    (`<T: Hashable>`), auto-generation from field metadata — all
    of it. `implements` keyword retained because it reads
    naturally with either "interface" or "trait" and sits at the
    declaration site. See §5.2.

11. **Decorators.** **[DECIDED]** Keep the Fuse2 list unchanged:
    `@value`, `@entrypoint`, `@export`, `@rank`, `@test`,
    `@inline`, `@builder`, `@deprecated`, `@ignore`. No
    behavioral decorators — behavior is declared via
    `implements Trait`, never via a decorator. The distinction
    is load-bearing (ADR-013).

12. **Data class / struct / @value.** **[DECIDED]** Keep the
    three-way split exactly:
    - **plain `struct`** — opaque/private fields, manual
      lifecycle, no auto-trait generation. Maximum control,
      maximum responsibility. Use for FFI wrappers and
      performance-critical types.
    - **`@value struct`** — auto-generated `__copyinit__`,
      `__moveinit__`, `__del__` (user can override `__del__`
      only); auto-trait generation enabled. Opaque API with
      value semantics. Use for resource types like `Connection`,
      `File`.
    - **`data class`** — shorthand for `@value struct` plus
      public positional fields plus auto-generated `==`, `!=`,
      `toString`. Everything on. Use for records: `Point`,
      `User`, AST nodes, etc.

    `@value` is the "my fields are knowable at compile time"
    opt-in that enables ADR-013 auto-generation. Plain `struct`
    cannot auto-generate traits — the checker emits an explicit
    error pointing the user at `@value` or manual implementation.

13. **Generics.** **[DECIDED]** Monomorphise at compile time.
    Type parameters on free functions AND user types. No type
    erasure, no runtime dispatch. Implementation note for the
    C backend: each generic instantiation emits a separate C
    function with the type parameters substituted, giving
    zero-cost specialisation without a template system — C
    handles this naturally as independent functions, same as
    Fuse2 did at the Cranelift IR level.

14. **String operations.** **[DECIDED]** Make `chars()` the
    default iteration API. Users iterate by character, not by
    byte: `for c in s.chars() { ... }`. `charAt(i)` and
    `byteAt(i)` remain available as explicit performance-path
    operations with documentation that `charAt` is O(n) for
    UTF-8 correctness and `byteAt` is O(1). `len()` returns
    byte length; `charCount()` returns character count. The
    goal: eliminate the Fuse2 footgun where
    `for i in 0..s.len() { s.charAt(i) }` was both slow AND
    incorrect for multi-byte UTF-8 sequences. See §2.4.

## Runtime and concurrency (mostly decided)

15. **What primitives does the Fuse3 C runtime expose, minimum
    viable?** The ~500 LOC target from §1.4 includes: malloc/free,
    stdin/stdout/stderr via libc, file I/O, process exit,
    OS-thread spawn (pthread_create / CreateThread), atomics
    (C11 `<stdatomic.h>`), mutexes and rwlocks, thread-local
    storage. The exact list of entry points is still to be
    enumerated in the Fuse3 guide alongside the emitted-C format.

16. **Scheduling.** **[DECIDED: OS threads first.]** §3.5
    recommends starting with OS threads via the C runtime and
    deferring green threads until profiling data justifies them.
    Same decision Fuse2's ADR-014 made for Stage 2, unblocked
    earlier.

17. **`Chan[T]` implementation.** **[DECIDED]** Fuse-owned ring
    buffer (for bounded) or linked list (for unbounded), guarded
    by a C mutex + condition variable. ~100 LOC of C for the
    primitive plus Fuse code for `.bounded(n)` / `.unbounded()`
    constructors.

18. **`Shared[T]` implementation.** **[DECIDED]** Wraps the value
    plus a `pthread_rwlock_t` / `SRWLOCK`. `.read()` / `.write()`
    call into the C runtime. `@rank(N)` is pure compile-time in
    the checker, emits no code.

19. **Memory model.** **[DECIDED]** Sequentially consistent across
    `Shared[T]` lock acquisitions and `Chan[T]` send/recv edges.
    Everything else is either statically forbidden by the ownership
    checker or explicitly undefined (and the checker rejects it).
    Document in the Fuse3 guide.

20. **`select`.** **[DECIDED: deferred to Fuse3 Wave 2.]** Day-one
    workaround is `spawn` per channel + collect results. Fuse2
    deferred this for the same reason; Fuse3 faces the same
    implementation cost (waiting on multiple condition variables
    simultaneously).

21. **`spawn` capture rules.** **[DECIDED]** Keep `spawn move` /
    `spawn ref` and reject raw `mutref` capture — same as Fuse2.
    The Fuse3 checker enforces these rules on top of the C-runtime
    thread primitives.

## Stdlib / tooling

22. **Tier structure.** **[DECIDED]** Keep Core / Full / Ext.
    Core is OS-free (no file I/O, no network, no process, no
    threading — those live in Full).

23. **Trait set for Core.** **[DECIDED]** `Equatable`, `Hashable`,
    `Comparable`, `Printable`, `Debuggable` in Core (day one).
    `Serializable`, `Encodable`, `Decodable` in Ext (need
    `Encoder` / `Decoder` types defined first — post-bootstrap).

24. **`Map[K: Hashable, V]`.** **[DECIDED: insertion order.]**
    §5.4 recommends insertion-order iteration by default, with
    explicit `sortedKeys()` / `unorderedKeys()` opt-outs. Prevents
    the Wave B1 HashMap→BTreeMap bug class entirely.

25. **C runtime interop.** Fuse3 needs an FFI mechanism for
    calling C functions (the runtime itself uses this heavily,
    and stdlib modules for `net`, `http`, `process` etc. will
    too). Design question: what does `extern fn` mean in Fuse3,
    given that emitted Fuse is *already* C? Probably trivial
    (emit the call verbatim), but the type checker needs a rule.
    Fuse2's `extern fn` mechanism is a good starting point.

26. **Testing.** **[DECIDED]** `@test` decorator. Fuse3 test
    runner invoked via `fuse test`. Cross-runner story (can Go's
    `go test` drive Fuse tests through the Stage 1 binary?) is
    deferred — not needed day one, revisit when Stage 2 is
    self-hosting.

27. **LSP, formatter, linter.** **[DECIDED]** LSP day one (it
    pays for itself — Fuse2 proved this in pre-Stage 2 Wave 7),
    formatter day one (tiny code), linter deferred until real
    Fuse code exists to lint.

## Meta / process

28. **Bootstrapping milestones.** **[DECIDED]** Fuse3 is Go-hosted
    Stage 1 from day one. Stage 2 self-hosting begins when Stage 1
    can compile the Fuse3 stdlib (~Wave 3 or 4 in the implementation
    plan). Three-generation reproducibility is the Stage 2
    completion gate.

29. **Rules doc.** **[DECIDED]** Single shared `rules.md` file
    referenced by every wave plan, *not* re-stated at the top of
    every doc. Fuse2's per-doc restatement was expensive.

30. **Learning log.** **[DECIDED: ordered append-only.]** Fuse2's
    `fuse2-learnings.md` / L001–L029 style. Chronological,
    numbered, never rewritten — mature bugs get a one-line
    summary, active bugs get a full entry. A searchable database
    was considered and rejected as overhead without clear payoff;
    `grep -n` over a flat file is fine at Fuse3's scale.

---

## Closing

This is the source material for Part 1 of the Fuse3 docs — the
Language Guide. It is deliberately wide. The next step is to answer
Part 10's questions in the guide itself, and that answer-pass is what
will shape the Fuse3 semantics.

The two meta-rules from Fuse2 that carry over verbatim:

1. **The guide precedes implementation.** If it's not in the guide,
   it doesn't exist.
2. **Every decision must serve the three pillars.** Memory safety
   without GC, concurrency safety without a borrow checker, DX as a
   first-class concern.

Everything else is negotiable.
