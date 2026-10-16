<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0006-source-frontend-and-portable-programs.md -->

# RFC 0006: Source frontend and portable VM programs

[日本語版](../ja/rfcs/0006-source-frontend-and-portable-programs.md)

- Status: accepted
- Authors: Codex (for repository-owner review)
- Created: 2026-10-17
- Last updated: 2026-10-17
- Tracking issue: Milestone 1 in `docs/roadmap.md`
- Supersedes: None; extends RFC 0002
- Superseded by: None

## Summary

Specify the Milestone 1 contracts: lexical rules, modules and calls, copy-only records/variants and exhaustive patterns, portable register-bytecode encoding, bounded VM frames, and transactional REPL sessions. RFC 0002 wrapping addition, signed comparison, statement ordering, and explicit returns remain in force. The repository owner accepted this RFC on 2026-10-17, satisfying the three source-design gates and the bytecode/REPL prerequisites. Acceptance does not complete their implementation tasks.

This is an accepted project design contract, not an account of implemented behavior or a claim about another language. The examples and compatibility fixtures below govern implementation; acceptance is not implementation or stabilization.

## Motivation

The accepted grammar leaves `IDENT`, comments, encoding, and signed-literal boundaries unspecified. It intentionally excludes calls, modules, and aggregates. The current bytecode `Program` represents one function and has no portable encoding. The VM has no guest call stack. Choosing these behaviors independently inside a lexer, CLI, or backend would create incompatible meanings. A single semantic contract lets the frontend, VM, formatter, and editor prototype share the same evidence.

## Goals

- Run small multi-function, multi-file scripts with useful diagnostics.
- Type-check and execute records, variants, and exhaustive patterns.
- Preserve source snapshots and spans through typed HIR, MIR, bytecode, and failures.
- Reject malformed source/artifacts and bound compiler and VM work before side effects.
- Keep a precise, versioned artifact independent of Rust layout and native calling conventions.

## Non-goals

Generics, methods, closures, strings, floats, implicit conversions, recursive data types, references, resource construction in source, destructors, exceptions, async, packages/registry I/O, a stable native ABI, or production sandbox claims. Native lowering of the new call/aggregate operations belongs to Milestone 2; it must report structured unsupported-feature errors until implemented. Existing RFC 0002 native differential conformance remains required.

## Guide-level design

```text
mod math;
use root::math::increment;

struct Pair { left: i64, right: i64 }
enum Answer { Missing, Found(Pair) }

fn total(answer: Answer) -> i64 {
    match answer {
        Answer::Missing => { return 0; },
        Answer::Found(Pair { left: a, right: b }) => {
            return a + b;
        },
    }
}

fn main() -> i64 {
    return total(Answer::Found(Pair { left: increment(20), right: 21 }));
}
```

`math.lang` contains `pub fn increment(value: i64) -> i64 { return value + 1; }`. Running the root file prints `42`. Function signatures and local types are explicit. `let mut name: Type = expression;` works for every type in this RFC; parameters and pattern bindings are immutable. Assignment replaces an entire local value. There is no field-assignment syntax.

## Reference semantics

### Lexical contract

- Input is UTF-8 without BOM. Do not normalize Unicode or newlines. Invalid UTF-8 is input error `SRC0010` with the offending raw byte range; it is not converted into a fabricated valid-source span. NUL is invalid even inside comments. Valid source is stored as an immutable `lang-diagnostics` snapshot.
- Identifiers are `[A-Za-z_][A-Za-z0-9_]*`, except the lone `_`, which is reserved for wildcard patterns. Non-ASCII text is allowed inside comments but not identifiers. Keywords are exact, case-sensitive words: `fn let mut i64 bool if else while return mod use pub root extern struct enum match true false`. Other words are identifiers; there are no implicit future-reserved keywords.
- Whitespace is space, tab, LF, and CR. Only LF advances the compiler's line counter; CRLF therefore advances it once. A lone CR is whitespace within the same compiler diagnostic line. Byte spans are half-open; human columns count Unicode scalars, and IDE columns count UTF-16 code units. LSP adapters independently convert byte spans using protocol line endings, including lone CR, so editor line numbers follow LSP rather than copying compiler line numbers.
- `//` comments end immediately before LF or EOF. `/* ... */` comments may nest, with maximum depth 256. Both forms are trivia. An unterminated block comment spans from its opening delimiter to EOF and reports `SRC0009`. Exceeding the depth limit reports `SRC0011` and ends analysis without a panic.
- Punctuation is `(` `)` `{` `}` `:` `;` `,` `.` `=` `+` `<` `->` `::` `=>`. Prefer a matching two-character token. A minus immediately followed by an ASCII digit is part of a signed decimal token; there is no standalone subtraction or unary-negation operator.
- Decimal tokens are `-?[0-9]+`. Leading zeros are allowed, with no octal meaning. `-0` is zero. An immediately following identifier character makes the entire digit/identifier run one malformed token (`SRC0008`); separators and other integer bases are unsupported. Range checking reports `SRC0001` over the entire signed token without wrapping.
- Every other Unicode scalar is one invalid token (`SRC0008`), except the malformed-number rule above. `- 1` reports the minus; `a-1` tokenizes as identifier plus signed decimal and then fails syntax checking. Comments, whitespace, invalid tokens, and missing-token recovery nodes remain in the lossless tree. Concatenating real token text must reproduce the original valid UTF-8 bytes exactly.

### Grammar extensions

The RFC 0002 grammar remains, with these additions (`?` and `*` describe grammar, not source tokens):

```text
file        := item*
item        := "pub"? (function | record | variant | module) | import | host
function    := "fn" IDENT "(" (parameter ("," parameter)* ","?)? ")" "->" type block
parameter   := IDENT ":" type
module      := "mod" IDENT ";"
import      := "use" path ";"
host        := "extern" "fn" IDENT "(" IDENT ":" "i64" ")" "->" "i64" ";"
record      := "struct" IDENT "{" (IDENT ":" type ("," IDENT ":" type)* ","?)? "}"
variant     := "enum" IDENT "{" case ("," case)* ","? "}"
case        := IDENT ("(" type ("," type)* ","? ")")?
path        := ("root" "::")? IDENT ("::" IDENT)*
type        := "i64" | "bool" | path
primary     := SIGNED_DECIMAL | "true" | "false" | path
             | "(" expression ")" | call | construction | field
call        := path "(" (expression ("," expression)* ","?)? ")"
construction:= path "{" (IDENT ":" expression ("," IDENT ":" expression)* ","?)? "}"
field       := primary "." IDENT
statement  += "match" expression "{" (pattern "=>" block ",")* "}"
pattern     := "_" | IDENT | SIGNED_DECIMAL | "true" | "false"
             | path "{" (IDENT ":" pattern ("," IDENT ":" pattern)* ","?)? "}"
             | path ("(" pattern ("," pattern)* ","? ")")?
```

The local-declaration production replaces its RFC 0002 `"i64"` annotation with `type`. In this notation, `field` means a left-associative postfix chain, not left recursion in a parser. Calls are only to named functions or enum cases, not arbitrary values. Field access binds tighter than addition. A bare enum-case path constructs or matches a zero-payload case; parentheses on a zero-payload case are rejected. A bare single identifier in a pattern is always a binding. Payload cases require parentheses. Record literals/patterns list every field exactly once, in any written order. There is no shorthand, spread, guard, range, or or-pattern. Every match arm requires a trailing comma, including the last. Existing parameter lists gain an optional trailing comma. Empty enums are rejected; empty records are inhabited copy values.

To avoid the record-literal/condition-block ambiguity, an unparenthesized record construction is disallowed in the head expression of `if`, `while`, and `match`. Parentheses permit it. This does not restrict an ordinary name, call, field chain, or comparison in those heads. `match` is a statement, not a value expression. All reachable function exits still require `return`.

### Modules, names, and visibility

The caller supplies a root source file and an explicit source-root directory. The loader may read only files under its canonical source root. `mod math;` in the root loads `math.lang`; `mod inner;` in that module loads `math/inner.lang`. Paths follow module hierarchy, never the process working directory. Reject missing files, canonical paths outside the root, duplicate canonical files, and module cycles before analysis; never recursively follow a symlink cycle. File reads are compiler inputs supplied by the host, not runtime capabilities granted to the script.

Each module collects all item signatures before bodies are resolved, so forward function references and mutual recursion are allowed. All item kinds share one module namespace; duplicates and duplicate import aliases are errors. Imported aliases take the last path segment and cannot be renamed or re-exported. Import-alias cycles are errors. Unqualified paths resolve within the current module; `root::` paths begin at the root. Local values resolve before module values but cannot be called. A private item is visible only inside its declaring module. Traversing another module requires `pub` at each crossed child/item boundary; the current module may access its own private child module. A public record/enum exposes all its fields/cases. Reject a public signature containing a private type.

RFC 0002 lexical local scopes, declaration-before-use, and the function-wide prohibition on local/parameter name reuse remain. Pattern bindings join this rule, including across different arms, and exist only in their arm block. Module items are outside that local-name prohibition. HIR assigns stable-within-compilation module, item, local, and type IDs; display names and source spans remain available. IDs are not a persisted compiler API.

The root script entry is exactly `fn main() -> i64`, with no parameters. Libraries may be analyzed without an entry; running them reports a missing-entry diagnostic. A `main` in another module is ordinary code. No top-level statements or initialization effects are permitted.

### Types, aggregates, patterns, and evaluation

`bool` becomes a source type with `true` and `false`. Conditions require `bool`; addition/comparison retain RFC 0002 `i64` rules. Calls, assignments, returns, fields, and payloads require exact nominal types; no coercions, overloads, or inferred declaration types exist. The new general mismatch code is `SRC0014`; existing RFC 0002 contexts retain `SRC0005`/`SRC0006`. A return path is complete when it returns directly, both `if` branches return, or all arms of an exhaustive match return. A loop alone does not prove a return.

Records and enums are nominal types identified by their defining module and name. Their fields/payloads contain only `i64`, `bool`, and other records/enums. Reject every recursive type cycle, including cycles through enum payloads. An enum value contains exactly one declared case and its positional payload. All these values are copies; assigning, passing, matching, or returning one does not move or destroy another value. They have no resource identity, borrowing, aliasing, destructor, or language-visible heap allocation. Implementations may allocate bounded internal storage but must account for it as described below. Existing nominal `own<T>` MIR values are not admissible in these source aggregates.

Evaluate a call's arguments left to right, then enter the callee. Evaluate record fields in written order, then store them in declaration order. Evaluate enum payloads left to right. Field access evaluates its receiver once. A match evaluates its scrutinee once, tests arms in source order without effects, then copies bindings and executes the first matching arm. Literals test exact signed-integer or boolean values; a wildcard/binding matches any value of the expected type. Record patterns match the product of their fields; case patterns match that case and its payload patterns. All arm bodies are type-checked, including unreachable ones.

Exhaustiveness means the union of arm patterns covers every inhabited value of the scrutinee type; otherwise report `SRC0016` with a missing witness. An arm covered by earlier arms is warning `SRC0017`. An integer domain is partitioned into mentioned literal values and one representative of their complement; booleans have two constructors, records one product constructor, and enums their declared cases. Apply constructor specialization recursively over field columns. This defines nested/product coverage without enumerating every `i64`. Charge each specialization against a configurable work budget; exhausting it is `SRC0011`, never a false claim of exhaustiveness. No runtime match-failure path is emitted for valid exhaustive source.

### Host calls and guest frames

`extern fn next(value: i64) -> i64;` declares an explicitly supplied host import. A host binds the qualified declaration name to an existing typed `HostFunctionRegistry` registration. Every declared import must be bound with the exact one-`i64` signature before any guest code executes; a declaration grants no filesystem, environment, process, network, or other authority. The CLI/REPL supplies an empty registry by default. Native callbacks remain trusted and non-preemptible; neither source syntax nor this RFC adds panic transport across `extern "C"`.

Guest calls use a VM-managed vector of frames, not recursive Rust calls. Each frame records the callee, registers, current block/instruction, return destination, and call-site span. Arguments are copied before the callee starts. Returning copies the result to the caller and releases that frame. Entry is frame depth one. Before frame/register allocation or any callee/host effect, check interruption/deadline, frame-depth limit, total live value-cell limit, and the relevant fuel/host-call budgets. Preserve the existing interruption-over-deadline precedence. A guest call instruction costs one fuel unit; callee instructions and returns have their normal costs. No additional hidden call/return fuel is charged. Tail-call elimination must not change these counters or stack traces.

Metering order preserves the prototype: check control and available instruction fuel, then charge the instruction once. A guest call then rechecks control and checks prospective frame depth before live cells, without allocating or entering the callee on failure; its instruction fuel remains charged. A host call rechecks control and the host-call budget, then charges one host call immediately before entering the callback. A refused callback therefore consumes instruction fuel but no host call. Initial entry frame/cell rejection consumes zero fuel; a callee that was never entered is absent from the stack trace. For example, with frame limit one, a main that emits constant `1` and then calls `leaf(1)` fails at the call site with fuel consumed two and host calls zero.

Each scalar/owned-resource cell counts one; a record counts one plus its fields' recursive counts; an enum register reserves one plus its largest case payload count. Check counts with overflow-safe arithmetic before allocation. Register copies cannot escape these static maximum charges. The frame limit includes entry and applies to direct/mutual recursion. Failures report the stable runtime reason, consumed counters, fault span, and guest stack ordered innermost first, with caller call-site spans. Failure releases all copy-only guest frames; it does not erase earlier host side effects.

The safe script/REPL defaults are 1,000,000 fuel units, 1,000 host calls, 256 live frames, and 1,000,000 live value cells. Explicit embedding APIs may choose different limits or existing unbounded convenience behavior. Execution errors are not source values. This RFC does not claim byte-based accounting for host callbacks, a production heap sandbox, native interruption, or async/task limits.

### Typed MIR and bytecode contract

Introduce backend-neutral compilation units containing nominal type definitions, named host imports, functions, and an optional entry ID. Existing one-function APIs may wrap a unit; old in-memory constructors are not a wire format. Extend the shared typed contract with boolean constants, guest calls, record construction/field projection, enum construction/case tests/checked payload projection, and internal integer equality for literal patterns. Assignment joins and loop back-edges still use typed block arguments. Copy-only aggregates pass as typed values through block arguments and calls. Do not smuggle frontend ASTs or backend types into MIR.

Validation resolves all IDs/signatures, rejects duplicate definitions, checks acyclic bounded type layouts, SSA definition/dominance, reachability, call arity/types, block-argument types, field/case bounds, return types, and source-map shape/ranges. Existing owned-resource linearity restrictions remain in force; the new copy operations reject owned-resource inputs, and guest calls reject signatures containing owned resources. This leaves the existing straight-line owner fixture intact without introducing a second ownership contract. A checked enum-payload projection traps with `InvalidVariantProjection` if the runtime case differs; well-typed source lowering only emits it inside the selected case path. Hand-built verified units may still encounter that defined trap.

### Portable bytecode schema 1

This is a new, incompatible artifact family. Its eight-byte magic is `LANGBC\0\0`, followed by little-endian `u16` schema `1`, `u16` flags `0`, and `u32` payload length. Reject unknown versions/flags, truncated input, length mismatch, and trailing bytes. All counts, lengths, indices, and offsets below are little-endian `u32`; signed constants are little-endian two's-complement `i64`; tags/booleans are `u8` (booleans must be 0 or 1). A vector is a count followed by elements. A string is a byte length followed by exact UTF-8 bytes, without NUL terminator. Empty identifier/name strings and embedded NUL are invalid. Source text is separately length-prefixed and may contain newlines, but not NUL. Encoding uses no Rust discriminants, pointers, native padding, varints, compression, or host registry addresses.

Payload fields, in order:

1. Source vector: normalized relative display path string, then source-text string. Paths use `/`, have no empty/`.`/`..` segment, and convey no file-read authority. Preserve text bytes; never reread a file while decoding.
2. Nominal definition vector: qualified name string; kind `0` record or `1` enum. Record body: vector of `(field-name string, type)`. Enum body: vector of `(case-name string, payload-type vector)`. Names are unique within their scope; enums have at least one case.
3. Host-import vector: qualified name string, parameter-type vector, result type. Schema 1 accepts only `(i64) -> i64` here. References use import indices, rebound by the host at instantiation.
4. Function vector: qualified name string, parameter-type vector, result type, register-type vector, optional function span, block vector. Entry arguments occupy the first registers in signature order. Each block contains a parameter-register vector, instruction vector, and one terminator. Block zero is the entry block, has no block parameters, and has no predecessors.
5. Optional entry: marker `0` for absent or `1` plus function index for present. No other marker is valid.

A type is tag `0` = `i64`, `1` = `bool`, `2` plus nominal resource ID = `own<T>`, `3` plus definition index = record, or `4` plus definition index = enum. A referenced definition must have the matching kind. No live resource value is serialized. A span is marker `0` for absent or `1` plus source index/start/end byte offsets for present; validate UTF-8 boundaries. Source-generated functions, instructions, and terminators always have spans. Hand-built fixtures may omit them.

Each instruction is its opcode, operands below, then an optional span. All register/function/import/definition/case/field operands are `u32`. All listed operands are present; no implicit destination or inferred type is serialized.

| Opcode | Meaning | Operands in order |
| --- | --- | --- |
| 0 | i64 constant | destination, i64 value |
| 1 | wrapping add | destination, left, right |
| 2 | signed less-than | destination, left, right |
| 3 | host call | destination, import, argument |
| 4 | bool constant | destination, u8 value |
| 5 | guest call | destination, function, argument-register vector |
| 6 | record construct | destination, definition, field-register vector in declaration order |
| 7 | record field | destination, record register, field index |
| 8 | enum construct | destination, definition, case index, payload-register vector |
| 9 | enum case test | destination, enum register, case index |
| 10 | checked enum field | destination, enum register, case index, payload index |
| 11 | internal i64 equality | destination, left, right |

A terminator is tag `0` plus return register; `1` plus block target; or `2` plus condition register, then-target, else-target; followed by an optional span. A block target is block index plus argument-register vector. Unknown tags/opcodes are errors. Functions/types/imports sort by qualified ASCII name for canonical encoding, and sources sort by normalized path. Remap their indices accordingly; field/case/block/instruction order is preserved. Duplicate names/paths are rejected. Decoding validates structural limits and the entire typed unit before returning an executable artifact. Encode/decode/re-encode must produce identical canonical bytes; noncanonical ordering is rejected rather than silently changing index meaning.

### Compiler and decoder bounds

Default compile limits are 4 MiB per source, 16 MiB total source, 256 modules/files, 1,000,000 tokens, 2,000,000 syntax nodes, syntax/comment depth 256, 4,096 functions/nominal definitions/imports each, 256 parameters/fields/cases/payload fields each, type depth 64, 4,096 cells per value, and 1,000,000 total registers/blocks/instructions each per unit. Exhaustiveness and verifier work budgets default to 10,000,000 charged steps each. A work step is a visited pattern-specialization state, or a visited verifier instruction/operand/CFG edge/dataflow-set member respectively; every repeated visit is charged. Implementations must document any extra charged operations and test small budgets. Structural bounds do not substitute for charging repeated dataflow work.

The decoder additionally caps the entire artifact at 64 MiB. Check byte lengths, remaining-input minimums, counts, checked arithmetic, depth, cumulative allocations, and work budgets before reserving memory or following references. A count's individual bound is insufficient: charge cumulative element counts and an allocation budget (default 128 MiB of requested decoder/verifier buffers, excluding caller-owned input). Reject oversized requests before allocation. Limits may be configured explicitly; reaching one is an error, not malformed-source recovery or a partial successful artifact. Frontend APIs accept the same limit configuration as the CLI; library callers do not bypass checks accidentally.

## Execution modes

The VM implements all accepted Milestone 1 operations. JIT/AOT retain scalar RFC 0002 source differential coverage and reject unsupported calls/aggregates before code publication; Milestone 2 must add full lowering and equivalence. This staged support is an explicit restriction, not permission for different arithmetic or pattern behavior. Host imports retain the trusted typed callback boundary. No WebAssembly mapping or component ABI change is introduced. Constant evaluation is still absent.

## ABI and compatibility

Existing RFC 0002 programs keep their meaning. `bool` storage, calls, modules, record/enum declarations, match, and reserved words are additive within a pre-release source profile; there was no implemented source lexer to migrate. Once implemented, changing these meanings requires the RFC 0001 compatibility process. Nominal definitions are compilation-unit identities, not a stable native memory layout. Bytecode schema 1 rejects other versions; future incompatible encodings use a new schema and explicit rejection/migration fixtures. Do not repurpose component schema numbers or promise serialized MIR stability.

## Diagnostics and tooling

Keep `SRC0001`–`SRC0007` in their accepted contexts. New codes are `SRC0008` invalid token, `SRC0009` unterminated comment, `SRC0010` invalid source encoding/BOM, `SRC0011` resource limit, `SRC0012` expected syntax, `SRC0013` module/import/visibility error, `SRC0014` extended type mismatch, `SRC0015` invalid aggregate/pattern shape, `SRC0016` non-exhaustive match, `SRC0017` unreachable arm, `SRC0018` missing/invalid script entry, and `SRC0019` unreachable statement. Runtime and artifact errors use structured enums independent of these source codes. Every valid-source diagnostic carries a primary span; original declarations, signatures, and conflicting arms are related labels. Only unambiguous missing-delimiter fixes are machine-applicable; guessed expressions or branches are speculative.

Recovery synchronizes at a semicolon, the current block's closing brace, or the next item boundary, respecting nested delimiters and guaranteeing forward progress. Do not consume an enclosing closing brace while repairing a nested expression. Token/text and syntax snapshots cover incomplete files and later recovered items. The formatter preserves real comments and malformed text, uses four-space indentation, spaces around binary operators, and one declaration/statement per line. It is idempotent and preserves semantic parse structure; it does not format invalid files by discarding erroneous tokens. The incremental database keys parse/analysis results by immutable source snapshot plus dependency revisions, shares them with an editor/LSP prototype, and invalidates reverse module dependencies on edits. Closing a document clears its published diagnostics; old snapshot results must never overwrite a newer revision. The prototype need not implement completion or a production LSP server.

## Performance and cost model

Source resolution indexes module names; type layouts are acyclic and bounded. Aggregate copy cost is proportional to the bounded value size; field access and enum case tests do not run destructors. The bytecode representation is register-based and validates before execution. These are design cost bounds, not measured throughput guarantees. Measure lexing, parsing, resolution/type checking, lowering, decoding/verification, frame calls, and aggregate copying separately before publishing performance claims. No new benchmark result is claimed by this proposal.

## Security and trust boundaries

Untrusted inputs have explicit compile/decode/verification budgets and malformed-input/fuzz tests. Source paths and debug names never authorize filesystem access; source loading stays under an explicitly supplied root. Host imports require explicit typed binding before effects. The VM uses bounded explicit frames and value storage. Trusted callbacks may block, allocate, or abort independently; the limits here do not turn them into sandboxed guest code. No new `unsafe`, native plugin loading, network authority, process access, or component-resource ownership rule is introduced.

## Alternatives

- **No change:** retain direct MIR construction; it cannot meet the existing source-script exit criteria.
- **Library/tooling layer:** translate source through an external language or macro library; this duplicates name/type/pattern semantics and loses the shared frontend contract.
- **Richer competing design:** Unicode-normalized identifiers, inferred immutable bindings, expression-valued match, generics, recursive boxed ADTs, and unrestricted host signatures. These can improve expressiveness but require unresolved ownership, normalization, error, and ABI decisions. They remain follow-ups rather than implicit implementation choices.
- **Simpler design:** ship only single-file `i64` functions without aggregates or guest calls. This is a useful intermediate slice but does not complete the current Milestone 1 scope; it may not replace the records/variants/module/call tasks or their exit evidence.

## Implementation plan

1. Record explicit repository-owner acceptance: completed on 2026-10-17. Implement the new source/wire behavior against this accepted contract.
2. Implement lossless tokenization/parser recovery using the diagnostic crate and bounded input limits. Add lexical and syntax snapshots.
3. Add module loading/resolved HIR, primitive typing, nominal layouts, pattern checking, and exhaustiveness in independently tested slices.
4. Lower typed HIR to validated compilation-unit MIR. Preserve spans on synthesized joins and call sites; keep scalar VM/JIT source differential tests.
5. Extend bytecode and VM with typed calls/aggregates, explicit frames, cell accounting, failures, and stack tests. Add schema-1 encoding/decoding and byte fixtures only against this accepted contract.
6. Expose `lang run <root-file>` and `lang repl`. Script success prints one signed decimal result and newline; exit 0 means successful execution, exit 1 compile/runtime failure, exit 2 invalid CLI usage. Diagnostics go to stderr; script return is not a process exit code.
7. A REPL accepts one complete item or expression per submission (balanced braces/parentheses permit continuation). Item submissions commit only after complete successful analysis; errors leave earlier definitions intact. Expressions run in a synthesized function against committed definitions with a fresh execution budget and print the canonical value. Calls may produce trusted host side effects that are not rolled back. No top-level mutable state exists. Duplicated items are errors until `:reset`; `:reset` clears definitions and diagnostics, `:quit` or EOF exits, and incomplete EOF diagnoses the submission. File modules are unavailable unless the host explicitly supplies a source root. Noninteractive input emits no prompt. Canonical aggregate display uses qualified type/case names, declaration-order record fields, comma-space separators, and recursively displayed scalar values; booleans print `true`/`false`.
8. Add formatter, incremental dependency invalidation, and an editor/LSP diagnostic adapter. Complete the roadmap's P1 tasks rather than silently postponing them. Write the final Milestone 1 guide only after all required implementation and exit evidence pass.

## Test plan

These are required compatibility fixtures, not existing passing tests:

| Area | Input/action | Required outcome |
| --- | --- | --- |
| Lexical | `return -9223372036854775808;` | One in-range negative literal |
| Lexical | `9223372036854775808` / `-9223372036854775809` | `SRC0001`, full token span |
| Lexical | `a+-1`, `a-1`, `- 1`, `12abc` | Add negative literal; syntax error; invalid minus; one malformed number respectively |
| Lexical | Nested `/* α /* b */ c */`, CRLF, EOF comment | Lossless bytes and correct following positions |
| Lexical | BOM, invalid UTF-8, NUL, Unicode identifier | Exact encoding/token error, no panic |
| Recovery | Missing `;`, `)`, or `}` before later code | Bounded recovery, retained tokens and subsequent diagnostics |
| Resolution | Forward call, mutual recursion, private import, symlink escape | First two resolve; latter two reject before execution |
| Scalars | RFC 0002 add/min/countdown/conditional sum over extrema | Correct results and scalar VM/JIT differential equality |
| Records | Reordered written fields with host calls, duplicate/missing fields | Written evaluation order; shape errors |
| Patterns | All enum cases, nested boolean product, integer literals without wildcard | Exhaustive; exhaustive only with full product; missing integer witness |
| Calls | Direct/mutual recursion, wrong arity/type, exact frame/cell/fuel boundary | Correct result or structured error before guarded effect |
| Metering | Constant then guest call at frame limit one; constant then host call at host budget zero | Fuel two in both; no callee frame/callback; host count zero |
| Stack | Three guest frames then a host/VM failure | Innermost-first fault/call-site spans and unchanged consumed counts |
| Bytecode | Golden bytes, re-encode, unknown tags/version, truncated/count bombs | Stable schema bytes; bounded rejection without allocation spikes |
| REPL | Valid definition, invalid definition, later call, reset, incomplete EOF | Transactional definitions, fresh budgets, deterministic output |
| Tooling | Format twice, edit dependency, close source, stale diagnostic completion | Idempotence, targeted invalidation, cleared/current diagnostics |

Add compiler/parser and bytecode fuzz targets with seed corpora, explicit depth/count/work/allocation-limit tests, compile-pass/fail fixtures for all codes, HIR/MIR/bytecode snapshots, and macOS/Windows workspace CI. Native tests cover the supported scalar source subset; unsupported new native operations must fail explicitly. Do not claim complete Milestone 1 from hand-built MIR tests alone.

## Open questions

Decision: **accepted on 2026-10-17** by the repository owner, explicitly stating “RFC 0006 を受理します”. This accepts the complete Milestone 1 source/execution profile, including ASCII identifiers, copy-only acyclic aggregates, statement match, explicit VM limits, schema 1, and transactional REPL state. No blocking design decision remains for this profile.

Optional follow-ups: Unicode identifiers, immutable ordinary bindings, import aliases, inferred types, expression match, generic/recursive data, richer host signatures, and native support for all new operations. None may silently change this profile during implementation.

The pre-decision review outcome was **ready for decision**. Implementation, compatibility fixtures, resource-limit evidence, and the Milestone 1 exit audit remain required after acceptance.

## Sources

- `docs/roadmap.md`, scope and dependency gates; last updated 2026-10-17 JST, commit `f596e2c`.
- `docs/rfcs/0001-project-charter.md`, accepted governance, backend restrictions, and compatibility; last changed 2026-10-17 JST, verified 2026-10-17 JST.
- `docs/rfcs/0002-prototype-control-flow.md`, accepted scalar/CFG grammar and codes; last changed 2026-10-17 JST, verified 2026-10-17 JST.
- `docs/architecture.md` and `crates/diagnostics/src/lib.rs`, source-snapshot and diagnostic implementation; last updated 2026-10-17 JST, commit `a4f135c`.
- `crates/mir/src/lib.rs` and `crates/bytecode/src/lib.rs`, current typed prototypes; last updated 2026-10-17 03:31 JST, commit `98bbb08`; verified 2026-10-17 JST.
- `crates/vm/src/lib.rs`, current typed VM; last updated 2026-10-16 23:23 JST, commit `b2ccdaf`; verified 2026-10-17 JST.
- `crates/runtime/src/lib.rs`, typed host registry; last updated 2026-10-16 15:05 JST, commit `a3e07f6`; verified 2026-10-17 JST.

- Microsoft LSP 3.18 [text-document position/line-ending rules](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocuments.md), living specification without a published update date; verified 2026-10-17 JST.

LSP position mapping follows the external protocol requirement. Numeric limits, grammar, wire tags, and staged backend support are accepted project decisions, not measured best-practice guarantees. No comparative performance claim is introduced.
