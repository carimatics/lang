<!-- canonical-language: en; japanese-translation: ja/milestone-1.md -->

# Milestone 1 capability guide

[日本語版](ja/milestone-1.md)

Last updated and verified: **2026-10-17 JST**. Milestone 1 is complete: **24/24 outcomes, 89/89 points**, including formatter and LSP integration. RFC 0006 is **implemented**, not stabilized. The project remains pre-alpha.

## Try it

Use the Rust 1.99.0 toolchain pinned by the repository. From the repository root:

```bash
cargo run -- run examples/scripts/answer.lang
cargo run -- repl
cargo build
```

The script prints `42` followed by a newline and exits 0. A compile/runtime failure prints diagnostics to stderr and exits 1; invalid command usage exits 2. The script's return value is the printed result, not the process exit code.

The executable is `target/debug/lang` (`target/debug/lang.exe` on Windows). Run `lang --help` for available commands. The old `lang 20 22` command still compares the original scalar VM and JIT prototypes.

## Write small typed programs

The source language now runs actual parsed programs through name resolution, type checking, typed MIR, and the portable VM.

| Area | Available behavior |
| --- | --- |
| Source | UTF-8 without BOM, ASCII identifiers, decimal signed `i64` literals, line comments, nested block comments; lossless syntax and recovery |
| Scalars | `i64`, `bool`, wrapping `+`, signed `<`, boolean literals; no implicit conversions |
| Functions | Explicit parameter/result types, forward and mutual calls, recursion, left-to-right argument evaluation, explicit `return` |
| Local state | `let mut name: Type = expression;`, whole-value assignment, lexical visibility; parameters and pattern bindings are immutable |
| Control flow | `if`/`else`, `while`, statement `match`; function exits and condition types are checked |
| Data | Nominal copy-only `struct` and `enum`, construction, field reads, nested patterns, bounded exhaustiveness and unreachable-code warnings |
| Modules | `mod`, `pub`, `use`, qualified paths and visibility; duplicates, cycles, root escapes, and private public-signature types are diagnosed |
| Diagnostics | Stable source codes, primary/related spans, notes, fix previews, incomplete-source recovery, original REPL locations, runtime call stacks |

Local names must be unique throughout a function, including separate branches and match arms. There is no shadowing. Record fields may be written in any order: expressions evaluate in written order, and storage/display uses declaration order. Records and enum payloads must form an acyclic type graph. Match is a statement; it does not produce an expression result.

The existing [answer.lang](../examples/scripts/answer.lang) demonstrates modules, calls, records, variants, and exhaustive patterns:

```text
mod math;

struct Pair { left: i64, right: i64 }
enum Answer { Missing, Found(Pair) }

fn total(answer: Answer) -> i64 {
    match answer {
        Answer::Missing => { return 0; },
        Answer::Found(Pair { left: a, right: b }) => { return a + b; },
    }
}

fn main() -> i64 {
    return total(Answer::Found(Pair { left: math::increment(20), right: 21 }));
}
```

Its sibling [math.lang](../examples/scripts/math.lang) exports `pub fn increment(value: i64) -> i64 { return value + 1; }`. A script requires a root `main() -> i64`. File modules resolve from the explicit source root, independent of the process working directory: `mod math;` selects `math.lang`, and a child `mod inner;` in that module selects `math/inner.lang`.

By default, `lang run FILE` uses FILE's parent as source root. Override it with `--source-root DIR`. Canonical containment is checked before reads, including symlink targets. This compiler file authority is separate from guest runtime capabilities.

## Explore with the REPL

Submit one declaration or one expression at a time:

```text
> fn increment(x: i64) -> i64 { return x + 1; }
> increment(41)
42
> struct Pair { left: i64, right: i64 }
> Pair { right: 21, left: 21 }
Pair { left: 21, right: 21 }
> :reset
> :quit
```

Declarations commit only after the whole candidate program passes analysis and MIR verification. A failed definition or evaluation leaves earlier definitions available. Definitions persist as text; expression values and mutable globals do not persist. Each expression receives fresh execution counters. Diagnostics, fix previews, and stack traces point to the original numbered submissions, including earlier definitions.

Unclosed parentheses/braces or nested comments permit multiline input. Interactive input has `>`/`..` prompts; piped input has none. `:reset` clears definitions. `:quit` or ordinary EOF exits 0 after handled submission errors; incomplete EOF and fatal IO/input-limit errors exit 1. File modules are disabled unless the session receives `--source-root DIR`.

## Connect an editor and format source

Configure an LSP client with the built executable and argument `lsp`. Alternatively, `cargo run --quiet -- lsp` starts it from the repository root. Standard output carries only framed protocol messages.

| LSP feature | Prototype contract |
| --- | --- |
| Lifecycle | `initialize`, `initialized`, `shutdown`, `exit`; UTF-16 positions explicitly advertised |
| Synchronization | Full document changes, increasing version checks, open/close overlays |
| Diagnostics | Shared parser/module/type queries, incomplete source, cross-file locations, negotiated related information and versions, clearing on close |
| Workspace | At most one explicit local root; watched-file refresh and optional dynamic watcher registration |
| Formatting | `textDocument/formatting` returns a canonical four-space whole-document edit; malformed files are preserved |

Open buffers override disk contents. Imported open files are checked in their parent module context. Closing a buffer clears its published diagnostics and, when authorized, restores disk content for remaining dependent roots. Errors in a closed dependency attach to the open root's module declaration, with an original related location when supported. Without an initialization root, the server reads no source files from disk. The client must send file-change notifications for external disk edits to refresh cached closed files.

The formatter preserves real token/comment bytes and semantic syntax, is idempotent, and keeps invalid source unchanged. Formatting options are validated, but the language's canonical four-space policy is fixed. Embeddings may call `lang_syntax::format::format` directly. A standalone `lang fmt` command, code actions, completion, navigation, incremental text edits, and DAP are later tooling work.

## Embed the compiler and portable VM

The public APIs keep frontend semantics independent of Cranelift:

| API | Purpose |
| --- | --- |
| `lang::frontend::{analyze_file, analyze_virtual, compile_script}` | Retained source snapshots/diagnostics, typed programs, script entry validation, configurable compilation limits |
| `lang::repl::Session` | Transactional item definitions and expression evaluation with original-submission locations |
| `lang_semantics::database::Database` | Parser/module/diagnostic caching, reverse invalidation, immutable snapshots, stale-job publication guards |
| `lang_semantics::program_lowering::{lower, lower_library}` | Verified typed units, with optional script entry or library policy |
| `lang_bytecode::unit::{encode, decode}` | Canonical versioned schema-1 artifacts and validation under shared decoding/verifier budgets |
| `lang_vm::unit::{execute, execute_entry}` | Typed values, explicit host bindings, bounded guest frames, outcomes and structured failures |

Schema 1 has explicit little-endian fields and tags; it is independent of Rust layout. Decoding rejects unknown versions/opcodes, noncanonical tables, malformed UTF-8/spans, invalid types/control flow, and resource overruns. Canonical encoding remaps IDs deterministically. There is no CLI artifact writer/loader yet; use the library APIs. See [source VM tests](../tests/source_vm.rs), [codec tests](../crates/bytecode/tests/unit_codec.rs), and [database tests](../crates/semantics/tests/database.rs) for executable embedding examples.

Source host declarations use `extern fn name(value: i64) -> i64;`. Embeddings register an exact typed `extern "C" fn(i64) -> i64` and bind its host-function ID by qualified import name. Every declared import, including unused imports, is checked before guest execution. The script/REPL CLI provides an empty registry and therefore cannot execute programs declaring host imports. Registry callbacks are trusted native code; the source profile supplies no ambient filesystem, network, environment, or process access.

## Control resources and inspect failures

The portable VM uses explicit guest frames rather than Rust recursion. Its defaults are:

| Limit | Default |
| --- | ---: |
| Instruction/terminator fuel | 1,000,000 |
| Entered host callbacks | 1,000 |
| Simultaneous guest frames | 256 |
| Live guest value cells across frames | 1,000,000 |

```bash
cargo run -- run --fuel 10000 --frames 64 --cells 65536 examples/scripts/answer.lang
cargo run -- repl --fuel 10000 --frames 64
```

One instruction or terminator costs one fuel. A refused guest call still charges its call instruction, but its unentered callee does not appear in the failure stack. Initial frame/cell refusal consumes no fuel. Host-call budget is charged immediately before entering a callback. Failures report reason, consumed counts, the fault span, and entered calls from inner to outer frames. Earlier host effects remain observable after guest failure; they are not transactional.

Embeddings may select limits, interruption flags, and monotonic deadlines. Interruption takes precedence over an expired deadline at the same safe point. Trusted callbacks cannot be preempted; an overrun is observed at a later guest safe point. Cell accounting measures logical guest value storage, not total process memory or callback allocations.

Compiler, cache, formatter, and codec bounds are separate from VM fuel. Examples include 4 MiB per source file, nominal depth 64 and 4,096 cells per value, and a default 64 MiB portable artifact cap. Decoder and unit verifier share cumulative work/requested-allocation budgets. REPL value display is capped at 65,536 bytes. Hosts should inspect each API's limit type for the complete policy.

## Native support and later work

Scalar/CFG source programs have VM/JIT differential tests and native object/source-map conformance. The existing straight-line owned-resource identity fixture is also preserved. Native source guest calls, record/enum operations, and the other extended unit operations return structured unsupported-feature errors before code publication; the complete new source profile executes in the VM. The CLI does not yet offer an AOT build command.

Local 2026-10-17 source-native direct/response-file link/run evidence is macOS AArch64. Windows-gated integration tests are present, and earlier Milestone 0 Windows evidence remains documented separately; this completion does not claim a new Windows source-native CI execution.

There are no source strings/floats/arrays, generics, methods, closures, recursive data types, references, source resource construction/destructors, exceptions, async/tasks, production package manager, stable ABI, or self-hosting yet. Milestone 0 resource/component/concurrency models remain scoped experiments rather than these source-language features. Milestone 2 adds ownership and broader native AOT; later milestones cover concurrency/embedding, full tooling, ecosystem compatibility, self-hosting, and additional frontends. See the [roadmap](roadmap.md).

## Evidence and source dates

The later [whole-repository review](review-2026-10-17.md) records additional authority/handle/linker regressions and updated validation. It does not change RFC 0006 source semantics or portable wire bytes.

The 2026-10-17 completion audit records **497 passing tests**, `cargo fmt --all --check`, Clippy with warnings denied, and workspace documentation generation. Parser/formatter, typed-HIR, and portable-decoder/VM targets have uninstrumented 10,000-run smoke evidence; this does not claim coverage-guided or sanitizer verification. Reproduce the full baseline with the commands in [README](../README.md#development).

| Source | Publication/update or verification date |
| --- | --- |
| [Milestone 1 completion audit](roadmap.md#completion-audit-2026-10-17), implementation commits through `402a5dc` | Updated/verified 2026-10-17 JST; audit recorded in `5e69965` |
| [RFC 0002](rfcs/0002-prototype-control-flow.md), [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md), [architecture](architecture.md) | Last updated/verified 2026-10-17 JST |
| Official Microsoft [LSP 3.18](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md) and lifecycle documents linked in [architecture](architecture.md#diagnostics-lsp-and-debugging) | Living documents without stated publication/update dates; verified 2026-10-17 JST |
| [JSON-RPC 2.0](https://www.jsonrpc.org/specification) | Published 2010-03-26; last updated 2013-01-04; verified 2026-10-17 JST |

Language semantics and numeric limits are project decisions; supported behavior is established by repository implementation/tests. External protocol facts come from the official specifications. Future milestones are plans, not implemented guarantees.
