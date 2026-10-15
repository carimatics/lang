# macOS AArch64 signed-minimum object inspection

- Inspection date: 2026-10-16 00:43:31 JST
- Repository commit: `dfc65f5c0f944abccaf24555ba3f6dbe77017483`
- Host: Apple M1 Max, macOS 27.0.1, AArch64
- Build profile: `release`

## Reproduction

```bash
cargo run --release --quiet --bin prototype-object -- min /tmp/lang-inspect-min.o
file /tmp/lang-inspect-min.o
xcrun nm -m /tmp/lang-inspect-min.o
xcrun otool -tvV /tmp/lang-inspect-min.o
xcrun otool -rv /tmp/lang-inspect-min.o
xcrun otool -hv /tmp/lang-inspect-min.o
```

## Captured output

```text
object:   /tmp/lang-inspect-min.o
symbol:   prototype_min
code:     20 bytes
artifact: 368 bytes

== FORMAT ==
/tmp/lang-inspect-min.o: Mach-O 64-bit object arm64

== SYMBOLS ==
0000000000000000 (__TEXT,__text) external _prototype_min

== DISASSEMBLY ==
/tmp/lang-inspect-min.o:
(__TEXT,__text) section
_prototype_min:
0000000000000000 cmp x0, x1
0000000000000004 b.lt 0x10
0000000000000008 mov x0, x1
000000000000000c ret
0000000000000010 ret

== RELOCATIONS ==
/tmp/lang-inspect-min.o:

== HEADERS ==
/tmp/lang-inspect-min.o:
Mach header
      magic  cputype cpusubtype  caps    filetype ncmds sizeofcmds      flags
MH_MAGIC_64    ARM64        ALL  0x00      OBJECT     4        280 SUBSECTIONS_VIA_SYMBOLS
```

No external web sources were used. The commands above are the primary local evidence and were run from a clean worktree at the recorded commit.
