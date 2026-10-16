# macOS AArch64 AOT debug-information inspection

- Inspection date: 2026-10-16 12:06:01 JST
- Repository commit: `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d`
- Host: Apple M1 Max, macOS 27.0.1, AArch64
- Build profile: `release`
- Inspector: Apple LLVM version 21.0.0

## Reproduction

```bash
cargo run --release --quiet --bin prototype-object -- add /tmp/lang-debug-add-013da31.o
cargo run --release --quiet --bin prototype-object -- min /tmp/lang-debug-min-013da31.o
cargo run --release --quiet --bin prototype-object -- host-callback /tmp/lang-debug-callback-013da31.o
file /tmp/lang-debug-add-013da31.o /tmp/lang-debug-min-013da31.o /tmp/lang-debug-callback-013da31.o
xcrun nm -m /tmp/lang-debug-add-013da31.o /tmp/lang-debug-min-013da31.o /tmp/lang-debug-callback-013da31.o
xcrun otool -l /tmp/lang-debug-add-013da31.o | awk '/sectname|segname/ {print}'
xcrun otool -l /tmp/lang-debug-min-013da31.o | awk '/sectname|segname/ {print}'
xcrun otool -l /tmp/lang-debug-callback-013da31.o | awk '/sectname|segname/ {print}'
xcrun dwarfdump --debug-info /tmp/lang-debug-add-013da31.o /tmp/lang-debug-min-013da31.o /tmp/lang-debug-callback-013da31.o
xcrun dwarfdump --debug-line /tmp/lang-debug-add-013da31.o /tmp/lang-debug-min-013da31.o /tmp/lang-debug-callback-013da31.o
xcrun dwarfdump --statistics /tmp/lang-debug-add-013da31.o
xcrun dwarfdump --statistics /tmp/lang-debug-min-013da31.o
xcrun dwarfdump --statistics /tmp/lang-debug-callback-013da31.o
xcrun dwarfdump --verify /tmp/lang-debug-add-013da31.o /tmp/lang-debug-min-013da31.o /tmp/lang-debug-callback-013da31.o
```

## Captured output

### Generated objects

```text
object:   /tmp/lang-debug-add-013da31.o
symbol:   prototype_add
code:     8 bytes
artifact: 352 bytes
object:   /tmp/lang-debug-min-013da31.o
symbol:   prototype_min
code:     20 bytes
artifact: 368 bytes
object:   /tmp/lang-debug-callback-013da31.o
symbol:   prototype_host_callback
code:     32 bytes
artifact: 440 bytes
```

### Host and object format

```text
ProductName:		macOS
ProductVersion:		27.0.1
BuildVersion:		26A434
arm64

/tmp/lang-debug-add-013da31.o:      Mach-O 64-bit object arm64
/tmp/lang-debug-min-013da31.o:      Mach-O 64-bit object arm64
/tmp/lang-debug-callback-013da31.o: Mach-O 64-bit object arm64
```

### Symbols

```text
0000000000000000 (__TEXT,__text) external _prototype_add
0000000000000000 (__TEXT,__text) external _prototype_min
                 (undefined) external _lang_host_i64_0
0000000000000000 (__TEXT,__text) external _prototype_host_callback
```

### Section inventory

```text
/tmp/lang-debug-add-013da31.o:
  segname 
  sectname __text
   segname __TEXT
/tmp/lang-debug-min-013da31.o:
  segname 
  sectname __text
   segname __TEXT
/tmp/lang-debug-callback-013da31.o:
  segname 
  sectname __text
   segname __TEXT
```

No `__debug_*` section is present in any object.

### DWARF compile units

```text
/tmp/lang-debug-add-013da31.o:	file format Mach-O arm64

.debug_info contents:
/tmp/lang-debug-min-013da31.o:	file format Mach-O arm64

.debug_info contents:
/tmp/lang-debug-callback-013da31.o:	file format Mach-O arm64

.debug_info contents:
```

Each object reports an empty `.debug_info contents:` block.

### DWARF line tables

```text
/tmp/lang-debug-add-013da31.o:	file format Mach-O arm64

.debug_line contents:
/tmp/lang-debug-min-013da31.o:	file format Mach-O arm64

.debug_line contents:
/tmp/lang-debug-callback-013da31.o:	file format Mach-O arm64

.debug_line contents:
```

Each object reports an empty `.debug_line contents:` block.

### Selected DWARF statistics

```json
[
  {
    "file": "/tmp/lang-debug-add-013da31.o",
    "format": "Mach-O arm64",
    "#functions": 0,
    "#functions with location": 0,
    "#unique source variables": 0,
    "#source variables": 0,
    "#source variables with location": 0,
    "#params": 0,
    "#params with source location": 0,
    "#local vars": 0,
    "#local vars with source location": 0,
    "#bytes with line information": 0,
    "#line entries": 0
  },
  {
    "file": "/tmp/lang-debug-min-013da31.o",
    "format": "Mach-O arm64",
    "#functions": 0,
    "#functions with location": 0,
    "#unique source variables": 0,
    "#source variables": 0,
    "#source variables with location": 0,
    "#params": 0,
    "#params with source location": 0,
    "#local vars": 0,
    "#local vars with source location": 0,
    "#bytes with line information": 0,
    "#line entries": 0
  },
  {
    "file": "/tmp/lang-debug-callback-013da31.o",
    "format": "Mach-O arm64",
    "#functions": 0,
    "#functions with location": 0,
    "#unique source variables": 0,
    "#source variables": 0,
    "#source variables with location": 0,
    "#params": 0,
    "#params with source location": 0,
    "#local vars": 0,
    "#local vars with source location": 0,
    "#bytes with line information": 0,
    "#line entries": 0
  }
]
```

### Verification

```text
Verifying /tmp/lang-debug-add-013da31.o:	file format Mach-O arm64
Verifying .debug_abbrev...
Verifying .debug_info Unit Header Chain...
Verifying .debug_types Unit Header Chain...
Verifying non-dwo Units...
Verifying dwo Units...
Verifying .debug_line...
Verifying .debug_str_offsets...
No errors.
Verifying /tmp/lang-debug-min-013da31.o:	file format Mach-O arm64
Verifying .debug_abbrev...
Verifying .debug_info Unit Header Chain...
Verifying .debug_types Unit Header Chain...
Verifying non-dwo Units...
Verifying dwo Units...
Verifying .debug_line...
Verifying .debug_str_offsets...
No errors.
Verifying /tmp/lang-debug-callback-013da31.o:	file format Mach-O arm64
Verifying .debug_abbrev...
Verifying .debug_info Unit Header Chain...
Verifying .debug_types Unit Header Chain...
Verifying non-dwo Units...
Verifying dwo Units...
Verifying .debug_line...
Verifying .debug_str_offsets...
No errors.
```

## Interpretation


- The three emitted Mach-O objects retain external function/import symbol names, so a native debugger or symbolizer can identify the exported prototype boundary.
- They contain no DWARF compile units, source files, line entries, parameter/local-variable records, or variable locations. The current AOT prototype therefore does not preserve source spans into native debug metadata.
- This is negative but decision-relevant debug-quality evidence. It does not evaluate a linked executable or dSYM, unwind metadata, Windows CodeView/PDB output, JIT debugger registration, optimized stepping behavior, or a future source frontend.
- Backend selection must account for the implementation and maintenance cost of adding source-level debug metadata; this transcript does not by itself select or reject Cranelift.

No external web sources were used. The commands above are the primary local evidence and were run from the clean worktree at the recorded commit.

