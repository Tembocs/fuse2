# Fuse

> A compiled systems language with memory safety without a garbage collector, concurrency safety without a borrow checker, and developer experience as a first-class constraint.

Fuse programs are compiled ahead of time to portable C11, then compiled to native code by the system C compiler. There is no tracing garbage collector, no borrow checker, no hidden runtime, and no `async` / `await`. Safety comes from four ownership keywords, a single liveness analysis, a destructor protocol, and an explicit-`move` rule for escaping closures.

## The three pillars

1. **Memory safety without a GC.** Programs written in safe Fuse cannot use-after-free, double-free, dereference null (there is no null), or race on non-atomic memory. The mechanism is compile-time ownership analysis plus deterministic destructors, not a tracing collector.

2. **Concurrency safety without a borrow checker.** Concurrency is expressed with `Chan[T]` (channels), `Shared[T] + @rank(N)` (rank-checked mutexes with compile-time deadlock prevention), and `spawn` (OS threads). The compiler verifies lock ordering statically.

3. **Developer experience as a first-class constraint.** Every weakening of a rule is visible at the **call site**: `mutref x` tells the reader which arguments are mutated, `unsafe { }` marks every FFI call and raw pointer use, `?` marks every error-propagation point. Reading a function body is sufficient to predict its effects.

## Taste

```fuse
import core.list.List;
import core.string.String;

@value struct Point {
    x: F64,
    y: F64,
}

pub fn centroid(points: ref List[Point]) -> Option[Point] {
    if points.isEmpty() {
        return None;
    }
    var cx: F64 = 0.0;
    var cy: F64 = 0.0;
    for p in points {
        cx += p.x;
        cy += p.y;
    }
    let n = points.len().toFloat();
    return Some(Point { x: cx / n, y: cy / n });
}
```

## Status

Fuse is **pre-1.0**. The language specification is frozen at version 1, but the compiler is under active construction. Progress is tracked wave-by-wave in [`docs/implementation-plan.md`](docs/implementation-plan.md). Do not depend on anything here until Wave 15 (bootstrap gate) has landed.

## Documentation

The four normative documents live under [`docs/`](docs/):

| Document | What it is |
|---|---|
| **[docs/language-guide.md](docs/language-guide.md)** | The language specification. Read this if you want to know what Fuse *is*. |
| **[docs/implementation-plan.md](docs/implementation-plan.md)** | The wave-by-wave compiler build plan. Read this if you want to know what is done and what is next. |
| **[docs/rules.md](docs/rules.md)** | Discipline rules for contributors and AI agents. **Read this before touching the repository.** |
| **[docs/repository-layout.md](docs/repository-layout.md)** | Directory tree and per-directory purpose. Read this before adding a file. |

## Contributing

Before you write code, do three things:

1. Read [`docs/rules.md`](docs/rules.md). It is short and it governs everything.
2. Locate the current wave in [`docs/implementation-plan.md`](docs/implementation-plan.md) and pick a task. Work one task at a time.
3. Check the guide ([`docs/language-guide.md`](docs/language-guide.md)) for any feature you intend to use or implement. If the feature is not in the guide, it does not exist — update the guide first, then implement.

The project has **permanent prohibitions** listed in `rules.md` §13. They are closed questions; please do not re-litigate them.

## License

See [`LICENSE`](LICENSE).
