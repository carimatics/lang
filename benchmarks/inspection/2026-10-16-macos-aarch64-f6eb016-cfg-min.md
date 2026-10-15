# macOS AArch64 explicit-CFG minimum object inspection

- Inspection date: 2026-10-16 01:08:20 JST
- Repository commit: `f6eb016798c237412b59057fa32257ca2bc5d016`
- Worktree: clean
- Host: Apple M1 Max, macOS 27.0.1 (26A434), AArch64
- Toolchain: Rust 1.99.0 (`b940084d7`, LLVM 23.1.1), Cargo 1.99.0
- Build profile: `release`

This inspection regenerates the signed-minimum object after replacing the specialized conditional-return representation with validated explicit MIR and bytecode basic blocks. It checks whether that structural change alters the compact native lowering for the existing workload.

## Reproduction

```bash
cargo run --release --quiet --bin prototype-object -- min /tmp/lang-inspect-cfg-min.o
file /tmp/lang-inspect-cfg-min.o
xcrun nm -m /tmp/lang-inspect-cfg-min.o
xcrun otool -tvV /tmp/lang-inspect-cfg-min.o
xcrun otool -rv /tmp/lang-inspect-cfg-min.o
xcrun otool -hv /tmp/lang-inspect-cfg-min.o
```

## Captured output

```text
object:   /tmp/lang-inspect-cfg-min.o
symbol:   prototype_min
code:     20 bytes
artifact: 368 bytes

== FORMAT ==
/tmp/lang-inspect-cfg-min.o: Mach-O 64-bit object arm64

== SYMBOLS ==
0000000000000000 (__TEXT,__text) external _prototype_min

== DISASSEMBLY ==
/tmp/lang-inspect-cfg-min.o:
(__TEXT,__text) section
_prototype_min:
0000000000000000 cmp x0, x1
0000000000000004 b.lt 0x10
0000000000000008 mov x0, x1
000000000000000c ret
0000000000000010 ret

== RELOCATIONS ==
/tmp/lang-inspect-cfg-min.o:

== HEADERS ==
/tmp/lang-inspect-cfg-min.o:
Mach header
      magic  cputype cpusubtype  caps    filetype ncmds sizeofcmds      flags
MH_MAGIC_64    ARM64        ALL  0x00      OBJECT     4        280 SUBSECTIONS_VIA_SYMBOLS
```

The 20-byte generated body, 368-byte object size, exported symbol, instruction sequence, and absence of relocations match the pre-CFG inspection. The explicit basic-block representation therefore introduces no generated-code regression for this minimum workload. This result does not cover loop lowering, block parameters, loop-carried values, allocation, or another target.

No external web sources were used. The commands above are the primary local evidence and were run from a clean worktree at the recorded commit.
