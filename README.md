# Fuse

A compiled systems language with memory safety without GC, concurrency safety without a borrow checker, and developer experience as a first-class concern.

## Quick Start

```bash
# Compile and run
fusec src/main.fuse -o bin/main
./bin/main

# Type-check only
fusec --check src/main.fuse

# Interpret (no compilation)
fusec --run src/main.fuse
```

## Example

```fuse
data class Point(val x: Int, val y: Int)

fn distance(ref a: Point, ref b: Point) -> Float {
  val dx = (a.x - b.x).toFloat()
  val dy = (a.y - b.y).toFloat()
  (dx * dx + dy * dy).sqrt()
}

@entrypoint
fn main() {
  val p1 = Point(0, 0)
  val p2 = Point(3, 4)
  println(f"distance: {distance(ref p1, ref p2)}")
}
```

## Building from Source

### Prerequisites

- Rust toolchain (edition 2024)

```bash
cd stage1
cargo build -p fusec --release
```

The compiler binary is at `stage1/target/release/fusec`.

### WASM Target (optional)

To compile Fuse programs to WebAssembly:

```bash
# 1. Add the WASI compilation target to Rust
rustup target add wasm32-wasip1

# 2. Install the Wasmtime runtime (test runner)
cargo install wasmtime-cli

# 3. Verify
rustup target list --installed   # should include wasm32-wasip1
wasmtime --version               # should print version
```

Then compile and run:

```bash
fusec app.fuse -o app.wasm --target wasi
wasmtime run app.wasm
```

### LSP Server (optional)

```bash
cd stage1
cargo build -p fuse-lsp --release
```

The LSP binary is at `stage1/target/release/fuse-lsp`. It communicates via stdio JSON-RPC and supports diagnostics, hover, go-to-definition, and completion.

## Running Tests

```bash
cd stage1
cargo test -p fusec
```

## Project Structure

```
stage0/          Python interpreter (Stage 0 — complete)
stage1/          Rust compiler + runtime (Stage 1 — complete)
  fusec/         Compiler: lexer, parser, checker, Cranelift codegen, evaluator
  fuse-runtime/  Runtime: FuseHandle values, ASAP destruction, FFI functions
  fuse-lsp/      Language server: diagnostics, hover, completion
stdlib/          Standard library (43 modules)
  core/          Primitives, collections, errors (17 modules)
  full/          I/O, networking, concurrency, SIMD (15 modules)
  ext/           Testing, logging, crypto, serialization (11 modules)
tests/           Test suite (162+ fixtures)
docs/            Language guide, implementation plan, ADRs
```

## Documentation

- [Language Guide](docs/fuse-language-guide-2.md) — full specification
- [Pre-Stage 2 Roadmap](docs/fuse-pre-stage2.md) — implementation progress (Waves 0-8)
- [Post-Stage 2](docs/fuse-post-stage2.md) — deferred features

## License

See [CON](CON) for details.
