# macOS AArch64 source-mapped AOT debug-information inspection

- Captured: 2026-10-16 19:41:57 JST (+0900)
- Repository commit: `31e182ac27e324048597a5ebd0fda4d2d4934395`
- Commit timestamp: 2026-10-16 19:41:29 JST (+0900)
- Worktree before capture: clean
- Host: macOS 27.0.1 (26A434), AArch64
- Rust: `rustc 1.99.0 (b940084d7 2026-09-28)`
- Apple Clang: 21.0.0 (`clang-2100.3.34.2`)
- Apple LLVM `dwarfdump` / `dsymutil`: 21.0.0

## Scope

This inspection replaces the negative object-level result recorded at commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d` for the representative add, minimum, and host-callback MIR functions. It checks that:

1. their typed-MIR source spans name the checked-in `benchmarks/fixtures/native-debug.mir` fixture;
2. host-native emission produces relocatable Mach-O AArch64 objects with valid DWARF v4 compile units, subprogram entries, and line tables;
3. the normal macOS integration path links and executes the same objects through direct and response-file requests, creates dSYMs for both executables, verifies both dSYMs, and finds all three function names in linked debug information.

The third criterion is implemented by `tests/aot_macos.rs`; the recorded test result below exercises that path from the clean commit.

## Reproduction

```bash
date '+%Y-%m-%d %H:%M:%S %Z (%z)'
git rev-parse HEAD
git status --porcelain=v1

cargo run --release --quiet --bin prototype-object -- add /tmp/lang-debug-add-31e182a.o
cargo run --release --quiet --bin prototype-object -- min /tmp/lang-debug-min-31e182a.o
cargo run --release --quiet --bin prototype-object -- host-callback /tmp/lang-debug-callback-31e182a.o

file /tmp/lang-debug-add-31e182a.o /tmp/lang-debug-min-31e182a.o /tmp/lang-debug-callback-31e182a.o
xcrun dwarfdump --verify /tmp/lang-debug-add-31e182a.o /tmp/lang-debug-min-31e182a.o /tmp/lang-debug-callback-31e182a.o
xcrun dwarfdump --debug-info /tmp/lang-debug-add-31e182a.o /tmp/lang-debug-min-31e182a.o /tmp/lang-debug-callback-31e182a.o
xcrun dwarfdump --debug-line /tmp/lang-debug-add-31e182a.o /tmp/lang-debug-min-31e182a.o /tmp/lang-debug-callback-31e182a.o
xcrun dwarfdump --statistics /tmp/lang-debug-add-31e182a.o
xcrun dwarfdump --statistics /tmp/lang-debug-min-31e182a.o
xcrun dwarfdump --statistics /tmp/lang-debug-callback-31e182a.o

cargo test --test aot_macos system_linker_runs_exported_functions_and_host_callback -- --exact
```

## Object results

All three files were reported as `Mach-O 64-bit object arm64`. Every object contains non-empty `__DWARF,__debug_abbrev`, `__DWARF,__debug_info`, and `__DWARF,__debug_line` sections. `dwarfdump --verify` reported `No errors.` independently for all three compile units.

| Function | Machine code | Object | `__debug_abbrev` | `__debug_info` | `__debug_line` | Functions with location |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prototype_add` | 8 B | 944 B | 39 B | 131 B | 90 B | 1 / 1 |
| `prototype_min` | 20 B | 960 B | 39 B | 131 B | 91 B | 1 / 1 |
| `prototype_host_callback` | 32 B | 1,048 B | 39 B | 151 B | 92 B | 1 / 1 |

Each compile unit records producer `lang Cranelift prototype`, source `benchmarks/fixtures/native-debug.mir`, a relocated low PC, and a high-PC offset equal to its machine-code size. The subprogram entries record the expected linkage name and declaration start:

| Function | Declaration | Address range in object |
| --- | --- | --- |
| `prototype_add` | line 1, column 1 | `0x0..0x8` |
| `prototype_min` | line 6, column 1 | `0x0..0x14` |
| `prototype_host_callback` | line 16, column 1 | `0x0..0x20` |

Selected line-table rows are:

```text
prototype_add:
0x0000000000000000      2      3  is_stmt
0x0000000000000004      3      3  is_stmt
0x0000000000000008      3      3  is_stmt end_sequence

prototype_min:
line 9, column 3
line 13, column 3
line 11, column 3
line 11, column 3, end_sequence

prototype_host_callback:
0x000000000000000c     17      3  is_stmt
0x0000000000000018     18      3  is_stmt
0x0000000000000020     18      3  is_stmt end_sequence
```

The minimum comparison is fused into its branch, so the emitted machine instruction uses the branch span on line 9 rather than retaining a separate row for the eliminated comparison on line 8. The two return paths retain lines 11 and 13.

The objects deliberately contain no formal-parameter or local-variable DIEs: the current typed MIR has value identifiers and source spans, but no accepted source-language parameter names, lexical scopes, or variable-location contract.

## Linked artifact result

The focused macOS integration test completed successfully:

```text
running 1 test
test system_linker_runs_exported_functions_and_host_callback ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Inside that test, the same representative objects are verified with `dwarfdump`, linked and executed through both direct and response-file `SystemLinker` requests, processed with `xcrun dsymutil`, and verified again with `xcrun dwarfdump --verify`. Both linked dSYMs must contain the fixture path plus `prototype_add`, `prototype_min`, and `prototype_host_callback`; any missing mapping fails the test.

## Interpretation

- The macOS AArch64 prototype now carries validated MIR operation spans through Cranelift machine-code ranges into relocatable DWARF v4 and linked dSYMs.
- The representative direct and response-file link paths preserve the debug information while retaining executable behavior.
- This satisfies the Milestone 0 macOS native-debug task. It does not establish source-language debugging, parameter/local variable locations, unwind metadata, optimized stepping quality, JIT debugger registration, or Windows CodeView/PDB behavior.
- Windows x86-64 source-mapping evidence remains a separate explicit roadmap task and a prerequisite for backend selection.

No external web sources were used. The clean repository commit, checked-in fixture, generated objects, Apple LLVM tool output, and focused integration test are the primary evidence; their capture and version dates are recorded above.
