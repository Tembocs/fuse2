# Stage 2 Learning Log

Issues discovered and resolved during Stage 2 self-hosting work.
Each entry captures what went wrong, why, and how it was fixed.

---

## L001: Stage 0 is not a test harness for Stage 2

**Phase:** W1.1 (Token Definitions)
**What happened:** When building `stage2/src/token.fuse`, the Stage 0
Python evaluator was used to test the module. It lacked enum runtime
support, so enum types, module-scoped data class constructors, and
module path resolution were added to the Stage 0 evaluator and values
system.

**Why it was wrong:** Stage 0 is a completed prototype. Stage 2 code
is compiled and tested by Stage 1 (the Rust compiler). Modifying
Stage 0 to support Stage 2 features creates maintenance burden in the
wrong codebase and blurs the boundary between stages.

**Resolution:** Added Rule 8 ("Fixes Go Forward, Not Backward") to
the plan. All future fixes land in Stage 1. Stage 2 code is validated
using Stage 1's `--check` and `--run` modes.
