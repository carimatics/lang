<!-- canonical-language: en; japanese-translation: ja/research.md -->

# Language and ecosystem research

[日本語版](ja/research.md)

Research baseline: **2026-10-15 22:08 JST**.

This document summarizes current language signals and extracts design lessons for this project. Survey popularity is treated as ecosystem evidence, not as proof that a language mechanism is technically superior.

## Developer sentiment

The [2026 Stack Overflow Developer Survey language dataset](https://survey.stackoverflow.co/2026/technology/data/da-language) covers 14,099 respondents. “Desired” means respondents want to use the language; “Admired” means respondents who used it want to continue using it.

| Language | Used in 2026 | Desired | Admired | Signal for this project |
| --- | ---: | ---: | ---: | --- |
| Rust | 18.0% | 26% | 67% | Strong demand for safe native performance, with substantial adoption |
| C++ | 24.3% | 16% | 46% | Native ecosystem and control remain important despite complexity |
| C | 21.8% | 14% | 46% | A stable C ABI remains the universal interoperability baseline |
| Kotlin | 10.8% | 9% | 43% | Productive syntax and tooling have a meaningful audience |
| Zig | 2.8% | 8% | 58% | Smaller ecosystem but strong enthusiasm for explicit systems design |
| Swift | 5.9% | 5% | 49% | Value semantics and approachable native development are attractive |
| Mojo | not highlighted in the usage table | 2% | 45% | Early interest in Python-like ergonomics plus ownership and native performance |

The survey is self-selected and ecosystem-wide. It should guide priorities such as ergonomics, tooling, and interoperability, not language semantics by itself.

Rust remains the closest validated reference point. The [2025 State of Rust Survey results](https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results/) report continuing organizational adoption, while also identifying slow compilation/storage usage, debugging, and concern about increasing language complexity. The [2025 compiler performance survey](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results/) reports an average build-performance satisfaction of 6/10, difficulty diagnosing compile bottlenecks, and dissatisfaction with source changes made solely to reduce build time. These are direct requirements for this project: compile-time observability, incremental architecture, and debugger quality cannot be postponed until after the language is mature.

## Relevant language designs

### Rust

The [Rust Book reference-cycle chapter](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html) documents that `Rc<T>` cycles can retain unreachable values and that `Weak<T>` does not contribute to target liveness. This is evidence for making shared ownership and weak back-edges explicit rather than treating reference counting as leak-free graph management.

What to adopt:

- Affine ownership, deterministic destruction, explicit unsafe boundaries, and strong data-race prevention.
- Traits, algebraic data types, pattern matching, zero-cost generics, and a strong package/build workflow.
- Compile-fail testing and diagnostics as part of the language experience.

What to improve:

- Keep lifetime syntax out of ordinary application code through lexical inference, value semantics, and non-escaping borrows.
- Avoid making generic sophistication or macro expansion the default solution for routine APIs.
- Design incremental compilation, build explanations, mixed-mode debugging, and stable interfaces from the beginning.

### Kotlin and Kotlin/Native

[Kotlin/Native](https://kotlinlang.org/docs/native-overview.html) demonstrates that an expressive language can use LLVM and produce binaries without a VM. Its [memory-manager documentation](https://kotlinlang.org/docs/native-memory-manager.html) specifies a shared heap and tracing collection from roots, so its runtime model does not satisfy the core no-mandatory-GC objective.

Adopt null safety, expression-oriented syntax, extension functions, exhaustive matching, coroutines, approachable generics, and IDE quality. Do not copy JVM-style object allocation as the universal abstraction.

### Swift

[Swift memory safety](https://docs.swift.org/latest/documentation/the-swift-programming-language/memorysafety/) and ownership work show the value of exclusivity checking and value-oriented APIs. The [Swift ARC guide](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md) documents strong-reference cycles and weak/unowned relationships. ARC is easier to approach than explicit borrowing, but it adds retain/release traffic, requires rules for cycles, and can hide performance costs.

Use Swift as evidence for value semantics and ergonomic access rules. Use reference counting only through an explicit shared-reference type whose cost and cycle behavior are visible.

### Zig

[Zig](https://ziglang.org/learn/overview/) is a useful reference for explicit allocators, predictable control flow, C interoperability, cross-compilation, and fast development builds. Its own documentation states that it is not fully memory safe. It is therefore a tooling and explicitness reference, not the target safety model.

### Mojo

[Mojo ownership](https://docs.modular.com/mojo/manual/values/ownership/) combines single ownership with `read`, `mut`, `var`, and `ref` argument conventions and value semantics by default. It supports the hypothesis that ownership can be presented with less routine syntax than Rust, but the language and ecosystem are still evolving.

### Hylo

[Hylo](https://hylo-lang.org/introduction/) is the strongest direct research reference for this project’s memory-model goal: mutable value semantics, AOT native code, in-place mutation, safety by default, and no lifetime annotations in the source model. Its maturity and ecosystem are not yet evidence of production viability, so the mechanism must be prototyped and measured independently.

### Pony and Koka

[Pony reference capabilities](https://tutorial.ponylang.org/capabilities/guarantees.html) demonstrate how aliasing guarantees can make actor communication data-race-free. [Koka](https://github.com/koka-lang/koka) demonstrates typed effects and Perceus reference-count analysis. Both are valuable semantic references, but neither should be adopted wholesale: Pony uses a GC, while Koka’s functional/effect-oriented model is farther from the desired Kotlin-like surface.

## Compiler and interoperability infrastructure

### GraalVM and Native Image

The [specification improvement study](spec-improvement-study.md) synthesizes Goose's storage/construction ideas, memory-domain safety contracts, and the GraalVM findings into independently readable decision criteria, historical probes, and reproducible lang fixtures. It preserves current semantics and separates recommendations from accepted or implemented work.

The [2026-10-30 GraalVM study](graalvm-study.md) compares Native Image reachability, build-time initialization, Truffle specialization, build reports and reproducibility with the current MIR/Cranelift pipeline. A small Copy-program experiment confirms retention of an unused native function before linking. The review recommends scoped reachability and cost-measurement prototypes, not a backend replacement or a change to ownership semantics; it records current release/edition distinctions and source dates.

### LLVM, Cranelift, and MLIR

[LLVM IR](https://llvm.org/docs/LangRef.html) remains the strongest route to mature optimization, object formats, debug information, and Windows/macOS target support. [LLVM ORC](https://llvm.org/docs/ORCv2.html) supports in-process JIT construction and host symbol integration.

[Cranelift through Wasmtime](https://docs.wasmtime.dev/stability-platform-support.html) supports x86-64 and AArch64 and can operate in AOT or JIT mode. It offers fast code generation and Rust-native integration, but has a smaller optimization surface than LLVM.

[MLIR](https://mlir.llvm.org/docs/LangRef/) supports extensible dialects, high-level and target-specific representations, conversion between dialects, and coexistence of several dialects in one module. It is a plausible future multi-language substrate. It is not recommended for stage 0 because it would add a large C++ dependency, another versioning surface, and significant infrastructure before the first language semantics are validated.

Recommended backend sequence:

1. Define a backend-neutral typed MIR and bytecode VM.
2. Prototype native object generation with Cranelift for fast iteration.
3. Add or replace with an LLVM release backend only when representative benchmarks justify the integration cost.
4. Re-evaluate MLIR when a second frontend or domain-specific optimization actually exists.

### WebAssembly Component Model

The [WebAssembly Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html) uses WIT interfaces and a [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html) to define language-independent contracts. It is the best current reference for versioned cross-language components and sandbox-oriented plugins.

The project should borrow the interface model and capability discipline. It should not promise zero overhead at every component boundary: strings, lists, resources, validation, and isolation can require lowering, checks, copies, or handles. Trusted statically linked code can use specialized direct calls after whole-program validation.

## Tooling standards

- Implement [LSP 3.18](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/) over the same incremental query database used by the compiler.
- Implement the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) so editors do not need language-specific debug UI.
- Emit DWARF for macOS and CodeView/PDB for Windows, while keeping a separate VM source map and mixed-stack model.
- Make the formatter’s syntax tree lossless and share parser recovery with the LSP.

## Strong additional requirements

The original requirements should be extended with the following:

1. **A written language and memory-model specification.** Safety cannot depend on compiler folklore.
2. **Stable C ABI interoperability.** C headers/imports are the practical bridge to operating systems and existing libraries.
3. **A versioned component ABI and IDL.** Native source-language ABI must not become the plugin compatibility promise.
4. **Plugin security and capabilities.** In-process execution removes IPC overhead but also removes process isolation.
5. **Resource limits and cancellation.** Scripts need fuel/time, memory, recursion, task, and host-call limits.
6. **Fast incremental builds with explanations.** Users need to know why a unit rebuilt and where compile time was spent.
7. **Compatibility policy and editions.** Syntax, packages, bytecode, ABI, and serialized IR need independent versioning.
8. **Reproducible and hermetic builds.** Lockfiles, checksums, offline mode, provenance, and deterministic outputs are supply-chain requirements.
9. **Diagnostics as an API.** Stable error codes, spans, fix-its, machine-readable output, and localization-ready messages support IDEs and CI.
10. **A complete unsafe/FFI model.** Pointer provenance, aliasing, atomics, unwinding, callbacks, and ownership transfer must be specified.
11. **Compile-time execution limits.** Macros and constant evaluation must be deterministic, cacheable, and resource-bounded.
12. **Profiling and observability.** Runtime profiles, allocation traces, VM/native mixed stacks, and compiler timing data are part of developer experience.
13. **Fuzzing and conformance suites.** Multiple backends and future frontends need differential and compatibility testing.
14. **Documentation generation and searchable package metadata.** A language ecosystem needs API docs from the first public packages.
15. **Migration tooling.** Editions and automated fixers are needed to evolve without fragmenting the ecosystem.

## Source registry

| Source | Published/updated | Verification note |
| --- | --- | --- |
| [Stack Overflow Developer Survey 2026 language data](https://survey.stackoverflow.co/2026/technology/data/da-language) | Dataset year 2026; exact page timestamp not published | Verified 2026-10-15 |
| [Stack Overflow Developer Survey 2026 usage data](https://survey.stackoverflow.co/2026/technology/data/languages) | Dataset year 2026; exact page timestamp not published | Verified 2026-10-15 |
| [2025 State of Rust Survey Results](https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results/) | 2026-03-02 | Verified 2026-10-15 |
| [Rust compiler performance survey 2025](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results/) | 2025-09-10 | Verified 2026-10-15 |
| [Rust Book: Reference Cycles Can Leak Memory](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html) | Source last changed 2025-10-03 | Verified 2026-10-16 |
| [Kotlin/Native overview](https://kotlinlang.org/docs/native-overview.html) | 2026-09-01 | Verified 2026-10-15 |
| [Kotlin/Native memory management](https://kotlinlang.org/docs/native-memory-manager.html) | Living documentation; date not published | Verified 2026-10-16 |
| [Zig overview](https://ziglang.org/learn/overview/) | Not published on page | Verified 2026-10-15 |
| [Mojo ownership](https://docs.modular.com/mojo/manual/values/ownership/) | Not published on page | Verified 2026-10-15 |
| [Hylo introduction](https://hylo-lang.org/introduction/) | Not published on page | Verified 2026-10-15 |
| [Swift memory safety](https://docs.swift.org/latest/documentation/the-swift-programming-language/memorysafety/) | Living documentation; date not published | Verified 2026-10-15 |
| [Swift Book: Automatic Reference Counting](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md) | Source last changed 2026-01-09 | Verified 2026-10-16 |
| [Swift Evolution SE-0304: Structured Concurrency](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0304-structured-concurrency.md) | Source last changed 2026-07-29 | Verified 2026-10-16 |
| [Pony Tutorial: Reference Capability Guarantees](https://tutorial.ponylang.org/capabilities/guarantees.html) | Living documentation; date not published | Verified 2026-10-16 |
| [MLIR language reference](https://mlir.llvm.org/docs/LangRef/) | Living documentation; date not published | Verified 2026-10-15 |
| [LLVM ORC design](https://llvm.org/docs/ORCv2.html) | Living documentation; date not published | Verified 2026-10-15 |
| [Wasmtime platform support](https://docs.wasmtime.dev/stability-platform-support.html) | Living documentation; date not published | Verified 2026-10-15 |
| [WebAssembly Component Model rationale](https://component-model.bytecodealliance.org/design/why-component-model.html) | Living documentation; date not published | Verified 2026-10-15 |
| [LSP 3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/) | Version 3.18.0, 2026-06-04 | Verified 2026-10-15 |
| [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) | Specification 1.71.0; date not published | Verified 2026-10-15 |
| [GitHub Docs: Building and testing Rust](https://docs.github.com/en/actions/tutorials/build-and-test-code/rust) | Living documentation; date not published | Verified 2026-10-16 |
| [GitHub Docs: Secure use reference](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions) | Living documentation; date not published | Verified 2026-10-16; used for read-only token permissions and full-SHA action pins |
| [GitHub Windows Server 2025 runner image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md) | Image version 20261004.281.1; page update timestamp not published | Verified 2026-10-16 |
| [actions/checkout v7.0.1 commit](https://github.com/actions/checkout/commit/3d3c42e5aac5ba805825da76410c181273ba90b1) | 2026-07-17 | Verified as the `v7` tag target 2026-10-16 |
| [ilammy/msvc-dev-cmd v1 commit](https://github.com/ilammy/msvc-dev-cmd/commit/0b201ec74fa43914dc39ae48a89fd1d8cb592756) | 2024-01-01 | Verified as the `v1` tag target 2026-10-16 |
| [GCC: Overall Options](https://gcc.gnu.org/onlinedocs/gcc/Overall-Options.html) | Living documentation; date not published | Verified 2026-10-16; defines `@file`, whitespace separation, quoting, escaping, and recursive expansion |
| [MSVC LINK: Specify a linker response file](https://learn.microsoft.com/en-us/cpp/build/reference/at-specify-a-linker-response-file?view=msvc-170) | 2022-05-18 | Verified 2026-10-16 |
| [rustc linker response-file implementation](https://github.com/rust-lang/rust/blob/364c4856ae0eb8e5b058a2c6699749d1e3efa7ef/compiler/rustc_codegen_ssa/src/back/link.rs#L2100) | File last changed 2026-10-08 | Verified 2026-10-16; used for GNU/Windows escaping and MSVC UTF-16LE encoding precedent |
| [GitHub Docs: Store and share data with workflow artifacts](https://docs.github.com/en/actions/tutorials/store-and-share-data) | Living documentation; date not published | Verified 2026-10-16; used for workflow-artifact upload and retention configuration |
| [actions/upload-artifact v7.0.2 commit](https://github.com/actions/upload-artifact/commit/cf430e030ddbb5b0abf93d22962f4752f3646cd9) | 2026-10-07 | Verified as the latest release tag target 2026-10-16 |

For living documentation without a published update time, the table intentionally reports “not published” rather than inferring a date from a CDN response or search index.

