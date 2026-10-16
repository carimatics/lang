<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0002-prototype-control-flow.md -->

# RFC 0002: Prototype integer and structured control-flow semantics

[日本語版](../ja/rfcs/0002-prototype-control-flow.md)

- Status: proposed
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-16
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

Define the first source-level subset that can lower to the existing typed MIR without adding a second semantic path. The subset contains `i64` parameters and results, signed decimal literals, wrapping addition, signed less-than comparison, mutable local bindings, `if`/`else`, `while`, and explicit `return`.

```text
fn sum_below(value: i64, cutoff: i64) -> i64 {
    let mut current: i64 = value;
    let mut total: i64 = 0;

    while 0 < current {
        if current < cutoff {
            total = total + current;
        }
        current = current + -1;
    }

    return total;
}
```

For `sum_below(5, 3)`, the result is `3`: the loop visits `5`, `4`, `3`, `2`, and `1`, but adds only `2` and `1`. This matches the existing MIR, VM, JIT, and AOT prototype workload.

## Motivation

The repository already has backend-neutral typed MIR and bytecode for constants, wrapping `i64` addition, signed comparison, explicit branches, back-edges, block parameters, and returns. Those operations run equivalently in the VM and Cranelift paths, but no source program currently defines which structured constructs produce that MIR.

Milestone 0 exits only after the prototype subset has written semantics. A parser-only choice would be insufficient because operator overflow, evaluation order, mutation across control-flow joins, loop re-evaluation, and return requirements are observable across execution modes. A library or macro cannot define these core rules without becoming a second frontend.

## Goals

- Give the existing arithmetic and CFG prototypes one precise source-level interpretation.
- Define deterministic typing, evaluation order, overflow, mutation, and loop behavior.
- Preserve source spans while lowering structured control flow to typed MIR blocks and block parameters.
- Require equivalent VM, JIT, and AOT results for terminating programs in the subset.
- Leave a small grammar that a lossless parser, formatter, diagnostics engine, and LSP can share.

## Non-goals

- General integer types, implicit numeric conversions, floating point, bit operations, or user-defined operators.
- Immutable local declarations, boolean local declarations, `break`, `continue`, loop values, pattern matching, or recursion.
- Function calls, host calls, aggregate values, allocation, ownership, borrowing, destruction, async execution, or effects.
- Defining a stable native ABI, component ABI, serialized MIR, or bytecode format.
- Committing to the final language name or to syntax outside this narrow subset.

## Guide-level design

A function declares zero or more `i64` parameters and one `i64` result. Parameters are immutable. A mutable local must have an explicit `i64` type and an initializer:

```text
fn minimum(left: i64, right: i64) -> i64 {
    if left < right {
        return left;
    } else {
        return right;
    }
}
```

`+` performs two's-complement wrapping addition. It never panics or invokes undefined behavior. For example, `9223372036854775807 + 1` evaluates to `-9223372036854775808`.

`<` compares two `i64` values as signed integers and produces an internal `bool`. Only a `bool` may be used as an `if` or `while` condition. Comparison chaining is not part of this subset.

`if` executes exactly one branch. An omitted `else` behaves as an empty block. `while` evaluates its condition before every iteration and exits as soon as the condition is false. Assignments made in an executed branch or loop iteration are visible to following statements and the next loop condition.

Every reachable function exit must execute an explicit `return` with an `i64` value. Falling off the end of a function is a compile-time error. The compiler is not required to prove that a `while` loop is infinite.

## Reference semantics

### Syntax and precedence

The initial grammar is:

```text
function    := "fn" IDENT "(" parameters? ")" "->" "i64" block
parameters  := parameter ("," parameter)*
parameter   := IDENT ":" "i64"
block       := "{" statement* "}"
statement   := "let" "mut" IDENT ":" "i64" "=" expression ";"
             | IDENT "=" expression ";"
             | "if" expression block ("else" block)?
             | "while" expression block
             | "return" expression ";"
expression  := comparison
comparison  := addition ("<" addition)?
addition    := primary ("+" primary)*
primary     := SIGNED_DECIMAL | IDENT | "(" expression ")"
```

Parentheses bind most tightly, `+` is left-associative, and `<` has lower precedence and cannot be chained. `SIGNED_DECIMAL` includes an optional leading `-`; general unary negation is not included. Signed decimal values outside the `i64` range are rejected rather than wrapped while parsing.

### Name resolution and typing

- Parameters belong to the outer function scope, and each block introduces a nested lexical scope.
- A local is visible from its declaration to the end of its containing block. A name must be declared before use.
- No declaration may reuse any parameter or local name in the same function, including in a nested or disjoint block; this subset rejects both redeclaration and shadowing.
- Parameters are immutable. Only a `let mut` local may appear on the left side of assignment.
- Literals, parameters, mutable locals, addition operands, assignment values, and return values have type `i64`.
- `<` accepts two `i64` operands and has type `bool`.
- `if` and `while` conditions must have type `bool`.
- `bool` is an internal expression type in this subset; users cannot declare boolean parameters, results, or locals yet.

### Evaluation and control flow

- Statements execute in source order.
- Binary operands evaluate left to right. This is observable once calls or effects are added and must not change then.
- An assignment evaluates its right-hand side completely before replacing the local's value.
- An `if` condition is evaluated once, then exactly one selected block executes.
- A `while` condition is evaluated once before each possible iteration. A true condition executes the body and transfers control back to the condition; a false condition transfers control to the following statement.
- `return` evaluates its expression and immediately exits the function. Statements after an unconditional return are unreachable and should produce a warning, not alter runtime behavior.
- `i64` addition returns the low 64 bits of the mathematical sum, interpreted as two's-complement signed `i64`.
- Signed comparison uses the mathematical ordering from `i64::MIN` through `i64::MAX`.

Lowering creates a new SSA value for each assignment. Values that remain live across `if` joins or `while` back-edges are passed as typed block arguments to block parameters. This is a compiler representation rule, not source-visible aliasing or allocation.

### Ownership, errors, concurrency, and constant execution

`i64` and the internal `bool` are copy values. They allocate no language heap storage, have no destructor, carry no borrow, and introduce no aliasing or concurrency behavior. This RFC adds no `unsafe`, FFI, capability, panic, unwinding, cancellation, or recoverable-error boundary.

An unbounded loop may not terminate. The bytecode VM may stop it through the separately specified instruction-fuel interface, cooperative interruption flag, or absolute monotonic deadline; native and JIT execution currently have no equivalent enforcement. Fuel exhaustion, interruption, and deadline expiration are embedding/runtime outcomes, not values produced by source `return`.

The initial frontend performs no source-level constant evaluation. If constant evaluation is added, it must use the same wrapping addition, signed comparison, and control-flow semantics and must apply a deterministic resource limit.

## Execution modes

- **Bytecode VM:** structured control flow lowers to verified register bytecode. Every instruction and terminator keeps the existing fuel cost, the register count remains subject to the optional value-slot limit, and an optional shared interruption flag or absolute monotonic deadline is observed at VM safe points.
- **Native AOT:** the same validated MIR lowers to native object code. Wrapping addition and signed comparison must match the VM for every `i64` input.
- **Optional JIT:** uses the same native lowering contract as AOT and must pass the same differential input tables.
- **Embedding:** arguments and the result use the current typed `i64` prototype boundary. This RFC adds no host calls or stable ABI.
- **WebAssembly/components:** no mapping is defined here. A later component RFC must preserve these observable results where it exposes the same function.

Backend optimizations may restructure blocks or remove redundant work only when they preserve statement order once effects exist, wrapping results, branch selection, and termination behavior permitted by the source program.

## ABI and compatibility

The syntax and semantics remain proposed and carry no compatibility guarantee until this RFC is explicitly accepted. After acceptance, changes to wrapping arithmetic, condition typing, evaluation order, or loop/assignment behavior require the repository's language-compatibility process and, once editions exist, an edition or migration plan where source meaning changes.

MIR IDs, block layouts, bytecode registers, native calling conventions, object symbols, and debug encodings remain implementation details or separately versioned formats. This RFC does not make any of them a stable interface.

## Diagnostics and tooling

The first implementation assigns stable diagnostic identities to these conditions:

| Proposed code | Condition | Required primary span |
| --- | --- | --- |
| `SRC0001` | Decimal literal is outside the `i64` range | Literal |
| `SRC0002` | Name is unknown or used before its declaration | Offending name |
| `SRC0003` | Declaration reuses a parameter or local name | New declaration, with original declaration note |
| `SRC0004` | Assignment targets an immutable parameter | Assignment target, with parameter declaration note |
| `SRC0005` | `+`, assignment, or `return` receives a non-`i64` value | Offending expression |
| `SRC0006` | `if` or `while` condition is not `bool` | Condition expression |
| `SRC0007` | A reachable function exit lacks `return` | Closing brace, with function signature note |

The lossless parser retains comments, whitespace, delimiters, and erroneous tokens. Recovery synchronizes at semicolons and closing braces without discarding the remainder of the function. The formatter emits the brace and semicolon style shown above and is idempotent. LSP diagnostics reuse parser/type-checker results and remain available for incomplete blocks. The debugger should present source local names even when assignments lower to different SSA values; unavailable optimized values must be labeled honestly.

These diagnostic codes are proposals and become stable only when this RFC is accepted and the corresponding behavior is implemented.

## Performance and cost model

The subset requires no language-level allocation, reference counting, dynamic dispatch, or overflow branch. Mutable locals are source abstractions and may remain in SSA values or registers; users cannot rely on a stack slot or memory address. A `while` condition executes once per attempted iteration, and each executed `+` and `<` has fixed-width integer cost before backend optimization.

The existing countdown, two-value accumulator, and conditional-sum benchmarks provide lowering, compilation, code-size, VM, JIT, and AOT evidence for the corresponding MIR shapes. The clean schema 9 macOS AArch64 result measures the unset-flag cooperative-interruption safe-point overhead at a 3.409 ns or 0.215% median increase over the ordinary VM countdown. Schema 10 adds an unexpired-deadline row, but only a dirty smoke run is recorded so far. These microbenchmarks do not establish an overhead budget or performance guarantees for a future parser, type checker, optimizer, or representative application.

## Security and trust boundaries

Wrapping addition has defined behavior for every input and cannot trigger native signed-overflow undefined behavior. Source is still untrusted: the parser, resolver, type checker, MIR validator, and bytecode verifier must reject malformed structure without panicking. Deeply nested source and very large functions need explicit implementation limits before public exposure.

`while` permits denial of service through non-termination. The VM's opt-in fuel, value-slot, cooperative-interruption, and absolute-deadline controls provide partial containment; they do not cover heap use, preemption inside callbacks, native/JIT execution, or compiler resource exhaustion. This RFC does not widen plugin, filesystem, network, process, FFI, or host capabilities.

## Alternatives

### No change

Keep constructing MIR directly and defer source semantics. This preserves implementation freedom but fails Milestone 0's written-semantics exit criterion and leaves backend-equivalence tests disconnected from a language users can write.

### Library or tooling solution

Model loops and arithmetic through macros, builder APIs, or standard-library calls. Such a layer cannot independently define parser recovery, static typing, operator overflow, evaluation order, or backend-equivalent lowering without acting as another source language, so it does not solve the problem.

### Expression-oriented control flow with checked arithmetic

Make `if` and loops value-producing expressions, prefer immutable rebinding, support `break value`, and make `+` trap or return an error on overflow. This is the strongest competing design because it can improve compositionality and make accidental overflow visible. It also requires panic/error semantics, more complex typing for joins and non-local exits, and runtime checks not exercised by the current prototype. It remains a candidate for a later general RFC rather than this vertical slice.

### Forward-only control flow

Support `if` and `return` but omit `while` and assignment. This is smaller, but it cannot express the tested back-edge, loop-carried values, fuel behavior, or representative bounded work that motivated the CFG prototype.

## Implementation plan

1. Add a lossless parser and syntax nodes for the exact grammar, with snapshots for valid, incomplete, and recovered input.
2. Resolve names and type-check `i64`, internal `bool`, mutation, conditions, and explicit returns while preserving source spans and proposed diagnostic identities.
3. Lower functions, `if`, and `while` to the existing typed MIR. Represent assigned values across joins and back-edges with block arguments and parameters.
4. Reuse the existing MIR-to-bytecode and MIR-to-Cranelift paths; do not add source-specific behavior to either backend.
5. Add formatter round-trip/idempotence tests, LSP diagnostics for incomplete programs, and source maps for VM/native debugging.
6. Measure parsing, analysis, and lowering separately before claiming frontend performance.

## Test plan

- Parser snapshots for precedence, negative literals, nested blocks, missing semicolons/braces, and recovery into following statements.
- Compile-pass tests for add, minimum, countdown, two-value accumulation, conditional sum, zero iterations, nested `if`, and assignments retained across joins.
- Compile-fail tests for every proposed diagnostic code, with primary/secondary spans and stable machine-readable fields.
- MIR snapshots proving signed comparison, wrapping addition, branch targets, back-edges, and typed block parameters.
- VM/JIT differential tables containing zero, negative values, `i64::MIN`, `i64::MAX`, wrapping overflow, both conditional paths, and bounded loops.
- AOT link-and-run coverage on macOS AArch64 and Windows x86-64 for representative source-generated objects.
- Fuel, interruption, and deadline tests proving that a non-terminating source loop stops at a VM safe point before the next guarded side effect.
- Formatter parse/format/parse equivalence and idempotence tests.
- Parser and verifier fuzzing plus explicit size/depth-limit tests before accepting untrusted source.

## Open questions

- Should immutable `let` bindings and user-visible `bool` locals join this first frontend slice or a follow-up?
- Which checked-arithmetic syntax should a later edition add while retaining wrapping `+`?
- Should `break`, `continue`, and value-producing loop expressions be designed together in a later control-flow RFC?

Review outcome: **ready for decision** for this narrow prototype subset. There are no additional prototype blockers because the corresponding MIR shapes already have VM/JIT differential, AOT, and benchmark evidence. Repository-owner acceptance is still required; immutable bindings, user-visible boolean storage, checked-arithmetic spelling, and loop expressions are optional follow-ups.

## Sources

- `docs/rfcs/0001-project-charter.md`, project charter and compatibility policy, last updated 2026-10-16 11:22 JST.
- `docs/architecture.md`, proposed compiler and execution architecture, last updated 2026-10-16 11:22 JST.
- `docs/roadmap.md`, Milestone 0 exit criteria and current evidence, last updated 2026-10-16 11:22 JST.
- `crates/mir/src/lib.rs`, typed CFG and validation prototype, last updated 2026-10-16 01:25 JST.
- `crates/bytecode/src/lib.rs`, verified register bytecode prototype, last updated 2026-10-16 01:25 JST.
- `crates/vm/src/lib.rs`, VM execution and resource-control prototype, last updated 2026-10-16 11:20 JST.
- `crates/codegen-clif/src/lib.rs`, Cranelift lowering prototype, last updated 2026-10-16 01:36 JST.
- `src/lib.rs`, differential prototype workloads, last updated 2026-10-16 10:17 JST.
- `src/bin/prototype-bench.rs`, schema 10 interruption/deadline-overhead harness, last updated 2026-10-16 11:22 JST.
- `benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json`, clean schema 9 macOS AArch64 evidence, measured 2026-10-16 11:05:30 JST.

No external factual claims are introduced by this RFC; the design proposal is derived from the repository's executable prototypes and project decisions.
