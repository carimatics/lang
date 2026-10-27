<!-- canonical-language: en; japanese-translation: README.ja.md -->

# Explicit-authority component plugins

[日本語版](README.ja.md)

Run from the repository root with the pinned Rust toolchain:

```bash
cargo run --example component-plugin-host -- vm
cargo run --example component-plugin-host -- jit
```

The host links a trusted native math plugin and loads a verified script plugin through the public component-v1 API. [verified.lang](verified.lang) is ordinary source: it calls the native `increment` import or panics for a negative input. Native increment uses saturating i64 addition. The two IDL files define the exact typed boundary; the host explicitly maps the source names and grants only `example.math.increment`. It compiles the source to the canonical component artifact, then independently revalidates that artifact at `module_load` before execution. No plugin discovery or implicit grant is involved.

Both commands check these observable results:

- Omitting the grant refuses instance creation with an untouched output and zero native calls.
- Input 41 returns 42. Input −1 reports a panic; a later input 0 returns 1 on the same instance.
- Fuel 0 produces a structured limit failure before the native plugin runs.
- Exactly two native calls occur. A retained failure keeps the actual native owner alive after instance/provider close; final result release releases it once.
- Runtime-managed allocations drain to zero.

Expected final output starts with `VM:` or `JIT:` followed by:

```text
script=42; native calls=2; grant denied; panic; reuse=1; fuel refused; owner released; live allocations=0
```

The native implementation runs trusted code in the host process. It cooperatively checks the original context and returns buffers allocated by that context. Runtime limits include 10,000 fuel/service work, 8 host calls, 1 MiB managed heap, 256 live allocations, 32 frames and one live task; preparation and canonical decoding have separate finite bounds. Its fixed scalar native codec has a separate bounded scratch Session. These limits do not sandbox arbitrary native code. The script receives only the declared typed import; the example supplies no filesystem, network, environment or process import.

This is the experimental RFC 0027 interface. Windows execution, both-host conformance and C stabilization remain on the [M3 board](../../docs/milestone-3.md). For installed exact-SDK AOT binding, see the [native SDK guide](../../docs/native-sdk.md).

Sources: [host](../component-plugin-host.rs), [script IDL](verified.idl.json), [native IDL](native.idl.json), and [RFC 0027](../../docs/rfcs/0027-production-component-interface.md). Code has no published update date; this example and its VM/JIT execution were verified **2026-10-27 JST**. RFC 0027 was updated **2026-10-27 JST**.
