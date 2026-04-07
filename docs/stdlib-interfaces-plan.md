# Stdlib Interfaces Implementation Plan (ADR-013)

> **Prerequisite:** Waves 0-8 complete. Interface system (W5) provides declaration,
> conformance checking, default methods, generic bounds, and parent resolution.
>
> **Goal:** Implement the 8 stdlib interfaces from ADR-013 with compile-time
> auto-generation from field metadata, operator-to-method dispatch, and
> evaluator support.
>
> **Stage 2 relevance:** Equatable and Hashable are blockers — the self-hosted
> compiler needs HashMap with custom-type keys and real `==` dispatch.

---

## Current State

**What exists (from W5):**
- Interface declaration parsing, HIR lowering, checker registration
- Conformance checking: verifies required methods exist as extensions
- Default method forwarding in codegen (synthetic extension entries)
- Parent interface resolution (transitive method collection)
- `Self` replacement in return types during codegen module loading

**What does not exist:**
- No stdlib interface `.fuse` files (Equatable, Hashable, etc.)
- No auto-generation of interface methods from field metadata
- No operator-to-method dispatch (`==` → `eq`, `<` → `compareTo`)
- No `Self` resolution in parameter types (only return types)
- `StructDecl` cannot declare `implements` (only DataClassDecl and EnumDecl can)

---

## Phase 0 — `Self` Type Resolution in Parameters

> Every ADR-013 interface uses `Self` in parameters:
> `fn eq(ref self, ref other: Self) -> Bool`.

- [ ] **0.1** Checker: resolve `Self` → concrete type in interface method params during conformance checking.
- [ ] **0.2** Codegen: resolve `Self` in extension function params during `load_module_recursive`.
- [ ] **0.3** Codegen: resolve `Self` in data class method params.
- [ ] **0.4** Codegen: resolve `Self` in struct method params.

**Test:** `self_type_in_params.fuse` — interface with `Self` param compiles and runs.

---

## Phase 1 — Struct `implements` Clause

> ADR-013: `@value struct` gets auto-generation, plain `struct` gets manual-only.
> Both need `implements`.

- [ ] **1.1** AST: add `implements: Vec<String>` to `StructDecl`.
- [ ] **1.2** Parser: parse `implements` after struct name (same pattern as data class).
- [ ] **1.3** Checker: register struct `implements` in `ModuleInfo.implements`.
- [ ] **1.4** Checker: pass struct `implements` through `as_data` conversion.
- [ ] **1.5** Codegen: include structs in implementor collection for default forwarding.
- [ ] **1.6** Evaluator: pass struct `implements` through struct→data conversion.
- [ ] **1.7** Test: `struct_implements.fuse` — `@value struct` with interface.

---

## Phase 2 — Stdlib Interface Declarations

> Create the `.fuse` files defining the interface contracts.

- [ ] **2.1** Create `stdlib/core/equatable.fuse` — `Equatable { fn eq(ref self, ref other: Self) -> Bool }` + default `ne`.
- [ ] **2.2** Create `stdlib/core/hashable.fuse` — `Hashable : Equatable { fn hash(ref self) -> Int }`.
- [ ] **2.3** Create `stdlib/core/comparable.fuse` — `Comparable : Equatable { fn compareTo(...) -> Int }` + defaults `lt`, `gt`, `le`, `ge`.
- [ ] **2.4** Create `stdlib/core/printable.fuse` — `Printable { fn toString(ref self) -> String }`.
- [ ] **2.5** Create `stdlib/core/debuggable.fuse` — `Debuggable : Printable { fn debugString(ref self) -> String }`.
- [ ] **2.6** Checker: auto-load stdlib interface module when `resolve_interface` fails for a known stdlib interface name.
- [ ] **2.7** Codegen: auto-load stdlib interface module during `load_module_recursive` when `implements` references an unresolved interface.

**Design decision:** No explicit `import core.equatable` required. The compiler auto-loads stdlib interfaces when referenced via `implements`.

---

## Phase 3 — Auto-Generation Infrastructure

> When `data class Foo(...) implements Equatable` does not provide `eq` manually,
> the compiler generates it from field metadata.

- [ ] **3.1** Create `fusec/src/autogen.rs` — shared module for AST generation.
- [ ] **3.2** Add `mod autogen;` to `lib.rs`.
- [ ] **3.3** Implement `TypeKind` enum: `DataClass`, `ValueStruct`, `PlainStruct`, `Enum`.
- [ ] **3.4** Implement `classify_type()` on Checker.
- [ ] **3.5** Implement `can_auto_generate(type_kind, interface_name) -> bool`.
- [ ] **3.6** Implement AST builder helpers: `make_self_param`, `make_other_param`, `make_member_access`, `make_binary`, `make_method_call`.
- [ ] **3.7** Implement `generate_eq(type_name, fields, span) -> FunctionDecl` — field-wise `self.f == other.f && ...`.
- [ ] **3.8** Implement `generate_hash(type_name, fields, span) -> FunctionDecl` — `h = h * 31 + self.f.hash()` loop.
- [ ] **3.9** Implement `generate_compare_to(type_name, fields, span) -> FunctionDecl` — field-wise compare, first non-zero wins.
- [ ] **3.10** Implement `generate_to_string(type_name, fields, span) -> FunctionDecl` — `f"TypeName({self.f1}, {self.f2})"`.
- [ ] **3.11** Implement `generate_debug_string(type_name, fields, span) -> FunctionDecl` — `f"TypeName {{ f1: {self.f1}, f2: {self.f2} }}"`.
- [ ] **3.12** Integrate into checker: in `check_interface_conformance`, when method missing and auto-generable, suppress error.

**Key design:** Auto-generated functions are real `FunctionDecl` AST nodes. They compile, inline, and optimize identically to hand-written code. Zero runtime overhead.

---

## Phase 4 — Auto-Generation in Codegen

> Inject auto-generated methods as synthetic extensions during module loading.

- [ ] **4.1** Codegen: after default method forwarding in `load_module_recursive`, add auto-gen pass.
- [ ] **4.2** For each type with `implements`, collect required methods (own + inherited from parents).
- [ ] **4.3** For each required method not already in `extensions`: call `autogen::generate_*` and insert.
- [ ] **4.4** Auto-generated methods use the type declaration's span (synthetic).
- [ ] **4.5** Test: `autogen_eq_data_class.fuse` — `data class Point(x: Int, y: Int) implements Equatable`, verify `==` works.
- [ ] **4.6** Test: `autogen_hash_data_class.fuse` — `implements Hashable`, verify `hash()` returns consistent values.
- [ ] **4.7** Test: `autogen_comparable_data_class.fuse` — `implements Comparable`, verify sorting works.
- [ ] **4.8** Test: `autogen_tostring_data_class.fuse` — `implements Printable`.
- [ ] **4.9** Test: `autogen_debugstring_data_class.fuse` — `implements Debuggable`.
- [ ] **4.10** Test: `autogen_value_struct.fuse` — `@value struct` with `implements Equatable`.
- [ ] **4.11** Test: `autogen_plain_struct_error.fuse` — EXPECTED ERROR: plain struct without `@value` cannot auto-generate.
- [ ] **4.12** Test: `autogen_multiple.fuse` — `implements Hashable, Comparable, Debuggable` generates all methods.

---

## Phase 5 — Operator-to-Method Dispatch

> `==` on types implementing Equatable dispatches to `eq()`.
> `<` on types implementing Comparable dispatches to `compareTo()`.

- [ ] **5.1** Codegen: add `type_has_extension(type_name, method_name)` helper.
- [ ] **5.2** Codegen: add `compile_extension_call(receiver_type, method, args)` helper.
- [ ] **5.3** Codegen: `==` dispatches to `eq()` when operand type has `eq` extension.
- [ ] **5.4** Codegen: `!=` dispatches to `eq()` + boolean negate.
- [ ] **5.5** Codegen: `<` dispatches to `compareTo()` + `< 0` check.
- [ ] **5.6** Codegen: `<=`, `>`, `>=` dispatch similarly.
- [ ] **5.7** Evaluator: `==` on `Value::Data` checks for `eq` extension before structural compare.
- [ ] **5.8** Evaluator: `!=` on `Value::Data` dispatches through `eq`.
- [ ] **5.9** Evaluator: comparison operators on `Value::Data` dispatch through `compareTo`.
- [ ] **5.10** Test: `operator_eq_dispatch.fuse` — `Point == Point` uses auto-generated `eq`.
- [ ] **5.11** Test: `operator_compare_dispatch.fuse` — `emp1 < emp2` uses auto-generated `compareTo`.

---

## Phase 6 — Enum Auto-Generation

> Enums with `implements Printable` get `toString` from variant names.
> Enums with `implements Equatable` get tag + payload comparison.

- [ ] **6.1** `autogen.rs`: `generate_enum_to_string(enum_decl)` — match arms returning variant names.
- [ ] **6.2** `autogen.rs`: `generate_enum_debug_string(enum_decl)` — match arms with field detail.
- [ ] **6.3** `autogen.rs`: `generate_enum_eq(enum_decl)` — match on both self and other, compare tags + payloads.
- [ ] **6.4** `autogen.rs`: `generate_enum_hash(enum_decl)` — hash from variant tag ordinal + payload hashes.
- [ ] **6.5** Codegen: include enums in auto-gen pass.
- [ ] **6.6** Checker: include enums in `can_auto_generate`.
- [ ] **6.7** Test: `enum_implements_printable.fuse`.
- [ ] **6.8** Test: `enum_implements_equatable.fuse`.

---

## Phase 7 — Manual Override Precedence

> If the user writes `fn Point.hash(ref self) -> Int { ... }`, it overrides
> the auto-generated version. Auto-generated `eq` from Equatable parent is kept.

- [ ] **7.1** Verify codegen insertion order: user extensions registered before auto-gen pass (auto-gen checks `!extensions.contains_key` before inserting).
- [ ] **7.2** Verify checker conformance: user methods found before auto-gen check.
- [ ] **7.3** Test: `autogen_override.fuse` — override `hash()`, keep auto `eq()`.

---

## Phase 8 — Parent Interface Transitive Auto-Generation

> `implements Hashable` must also generate `eq` from Equatable parent.
> `implements Debuggable` must also generate `toString` from Printable parent.

- [ ] **8.1** Codegen: expand default forwarding to include parent interface defaults from other loaded modules.
- [ ] **8.2** Auto-gen: collect ALL required methods (own + inherited from parents) when generating.
- [ ] **8.3** Checker: validate parent interface methods are auto-generable too.
- [ ] **8.4** Test: `hashable_generates_eq.fuse` — `implements Hashable` auto-generates both `hash` and `eq`.
- [ ] **8.5** Test: `comparable_generates_eq.fuse` — `implements Comparable` auto-generates `compareTo` and `eq`.
- [ ] **8.6** Test: `debuggable_generates_tostring.fuse` — `implements Debuggable` auto-generates both.

---

## Phase 9 — Evaluator Integration

> The `--run` mode must support all new interfaces for testing and REPL use.

- [ ] **9.1** Evaluator: auto-load stdlib interface modules when `implements` references unknown interface.
- [ ] **9.2** Evaluator: add default method forwarding during module load (mirror codegen logic).
- [ ] **9.3** Evaluator: add auto-generation pass during module load.
- [ ] **9.4** Evaluator: operator dispatch `==` on Data values via `eq()`.
- [ ] **9.5** Evaluator: operator dispatch `!=` on Data values.
- [ ] **9.6** Evaluator: operator dispatch `<`/`<=`/`>`/`>=` on Data values via `compareTo()`.
- [ ] **9.7** Test (--run): `equatable_run.fuse` — verify `==` works in interpreter.
- [ ] **9.8** Test (--run): `comparable_run.fuse` — verify `<` works in interpreter.

---

## Phase 10 — Serializable, Encodable, Decodable (Deferred)

> Depends on `Encoder`/`Decoder` type definitions and mature `json`/`toml` stdlib.
> Not needed for Stage 2. Implement post-self-hosting.

- [ ] **10.1** Define `Encoder` interface/struct.
- [ ] **10.2** Define `Decoder` interface/struct.
- [ ] **10.3** Create `stdlib/core/serializable.fuse`.
- [ ] **10.4** Create `stdlib/core/encodable.fuse`.
- [ ] **10.5** Create `stdlib/core/decodable.fuse`.
- [ ] **10.6** `autogen.rs`: `generate_to_json(type_name, fields)` — field-name-keyed JSON object.
- [ ] **10.7** `autogen.rs`: `generate_from_json(type_name, fields)` — JSON parser to type.
- [ ] **10.8** `autogen.rs`: `generate_encode(type_name, fields)`.
- [ ] **10.9** `autogen.rs`: `generate_decode(type_name, fields)`.
- [ ] **10.10** Add `toJSON` methods to primitive types (Int, Float, String, Bool).
- [ ] **10.11** Test: `serializable_data_class.fuse`.
- [ ] **10.12** Test: `serializable_roundtrip.fuse`.

---

## Dependency Graph

```
Phase 0 (Self in params) ───────┐
                                 ├──→ Phase 2 (Stdlib .fuse files)
Phase 1 (Struct implements) ────┘           │
                                            ▼
                                   Phase 3 (Auto-gen infra)
                                            │
                                            ▼
                                   Phase 4 (Codegen auto-gen)
                                            │
                               ┌────────────┼────────────┐
                               ▼            ▼            ▼
                          Phase 5      Phase 6      Phase 7
                          (Operator    (Enum        (Override
                           dispatch)   auto-gen)    precedence)
                               └────────────┼────────────┘
                                            ▼
                                   Phase 8 (Parent transitive)
                                            │
                                            ▼
                                   Phase 9 (Evaluator)
                                            │
                                            ▼
                                   Phase 10 (Serializable — deferred)
```

---

## Task Count Summary

| Phase | Name | Tasks | Stage 2 Blocking? |
|-------|------|-------|-------------------|
| 0 | `Self` type resolution in params | 4 | Yes |
| 1 | Struct `implements` | 7 | Yes |
| 2 | Stdlib interface `.fuse` files | 7 | Yes |
| 3 | Auto-gen infrastructure | 12 | Yes |
| 4 | Codegen auto-gen + tests | 12 | Yes |
| 5 | Operator-to-method dispatch | 11 | Yes |
| 6 | Enum auto-generation | 8 | No (nice to have) |
| 7 | Manual override precedence | 3 | No (safety) |
| 8 | Parent transitive auto-gen | 6 | Yes |
| 9 | Evaluator integration | 8 | No (--run mode) |
| 10 | Serializable/Encodable/Decodable | 12 | No (deferred) |
| **Total** | | **90** | **59 blocking** |
| **Non-deferred total** | | **78** | |

---

## Critical Path for Stage 2

Phases 0 → 1 → 2 → 3 → 4 → 5 → 8 (59 tasks) are the minimum needed before the
self-hosted compiler can use `HashMap<CustomKey, Value>` and `==` on AST nodes.
Phases 6, 7, 9 improve correctness and developer experience but are not blockers.
Phase 10 is deferred to post-Stage 2.
