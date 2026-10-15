<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 02:27:40 JSTにclean commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3` から記録しました。Release buildで、harness schema 3、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 452.917 ns | 537.375 ns | 10.463% |
| Bytecode lowering：signed minimum | 1,000 | 1,105.917 ns | 1,218.792 ns | 5.965% |
| Bytecode lowering：countdown back-edge | 1,000 | 1,659.458 ns | 1,738.083 ns | 2.702% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.375 us | 32.917 us | 72.237% |
| Cranelift JIT compilation：signed minimum | 1 | 23.792 us | 28.250 us | 16.980% |
| Cranelift JIT compilation：countdown back-edge | 1 | 27.541 us | 33.750 us | 11.217% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.375 us | 16.916 us | 19.445% |
| Cranelift AOT object emission：signed minimum | 1 | 17.834 us | 19.542 us | 3.779% |
| Cranelift AOT object emission：countdown back-edge | 1 | 21.625 us | 25.667 us | 5.546% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown object | 1 | 37.622 ms | 39.545 ms | 3.051% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.961 ns | 1.206 ns | 7.807% |
| VM wrapping `i64` add call | 100,000 | 178.762 ns | 182.893 ns | 0.867% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.864 ns | 0.206% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 1.214 ns | 8.554% |
| VM signed minimum call | 100,000 | 489.329 ns | 493.225 ns | 0.380% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.750 ns | 1.180% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.931 ns | 0.048% |
| VM countdown call | 100,000 | 1,558.118 ns | 1,567.146 ns | 0.396% |
| JIT countdown call | 100,000 | 12.729 ns | 12.992 ns | 0.655% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.930 ns | 0.544% |
| VM host callback | 100,000 | 171.447 ns | 173.440 ns | 0.532% |
| JIT host callback | 100,000 | 2.789 ns | 2.818 ns | 1.424% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown objectは352、368、376 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。

## Schema 3 formatとmigration

Schema 3は、bounded countdown workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、artifact size、system-link setへ追加します。Runtime sampleは、`i64` block parameterをback-edgeの周囲で運んでzeroへ到達するinput `64` と、immediate-return pathを取る `-7` を交互に使用します。Harnessは計測前に、`-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Schema 3は `jit_countdown` と `aot_countdown` size field、6つのcountdown measurement rowを追加します。また、`system_link_add_min` を `system_link_add_min_countdown` へrenameします。Consumerはこれらのnameをmergeせず、`schema_version` で分岐する必要があります。既存のschema 1／2 JSON fileはimmutableなhistorical evidenceとして残します。

## Windows x86-64 evidence path

Schema 3 harnessは、Windows x86-64 MSVCでも同じ `system_link_add_min_countdown` rowを記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証してから、反復linkだけを計測します。Windows Server 2025 workflowは同じ31／1,000／100,000 parameterで実行し、schema 3、clean checkout、`windows`／`x86_64` identity、拡張済みsystem-link rowの存在を検証し、そのJSONを30日間uploadします。

これはevidenceを生成するpathであり、記録済みWindows evidenceではありません。この変更によるremote runner結果はまだ検査もcommitもされていません。したがって、上のmacOS tableが現在唯一の測定済みbaselineです。

## 生成コード検査

Addとhost-callback objectをclean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` から再生成し、2026-10-16 00:25 JSTにmacOS AArch64 hostで検査しました。Signed-minimum objectはclean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` から00:43 JSTに最初に再生成し、その後、明示的CFGへの移行後にclean commit `f6eb016798c237412b59057fa32257ca2bc5d016` から01:08 JSTに再生成しました。

| Prototype | Export | Import | 生成body | Relocation |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | なし | `add x0, x0, x1`; `ret` | なし |
| Signed `i64` minimum | `_prototype_min` | なし | `cmp`、conditional `b.lt`、else側の `mov`、2つの `ret` path | なし |
| 型付きhost callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer authentication付きframe、GOT load、間接 `blr`、authenticated return | `_lang_host_i64_0` に対する `GOTLDP` と `GOTLDPOFF` |

3つすべてのfileは64-bit ARM64 Mach-O objectです。Add prototypeはrelocationなしの8-byte、2-instruction bodyへloweringされています。Minimumはrelocationなしの20-byte compareと実際のconditional branchへloweringされています。Less-than pathは最初のargumentを `x0` に保持し、もう一方のpathは `x1` をreturn registerへmoveします。明示的CFGへの移行後の再検査でも、同じcode／object size、symbol、instruction sequence、空のrelocation tableが得られました。Callbackはlinkerが解決するexternal importのままであり、position-independentなGOT addressingを使用します。そのbodyは32 bytesです。これは意図したarithmetic、control-flow、export／import lowering boundaryを確認します。Loop lowering、block parameter、loop-carried value、allocation、debug metadata、unwind behavior、他targetは評価していません。

完全なcommandと、取得した `file`、`nm`、`otool` の出力は[add／callback transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、[CFG移行前のminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、[明示的CFGのminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md)に保存しています。

## 解釈と制限

- Harnessは計測前に、minimumの両方の結果とcountdownのimmediate-return／loop pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdownを追加します。どちらもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- JIT add compilationのRSDは72.237%で、medianが19.375 usである一方、1 sampleは116.333 usに達しました。Countdown JIT／AOT medianは27.541 us／21.625 usで、RSDは11.217%／5.546%です。これらのmedianは方向性の証拠として有用ですが、add compile-time variationが大きすぎるためstable performance budgetには使用できません。
- VM／JIT runtime rowのRSDは1.5%未満でした。64-iteration back-edgeとimmediate returnを交互に実行すると、JIT medianは12.729 ns、VM medianは1,558.118 nsになります。このbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。Sub-nanosecond direct-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、両tier-1 hostでschema 3を記録し、allocationとより広いCFG workloadを追加し、startup、memory、executable size、debug qualityを計測します。

## 以前のbaseline

[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)はcommit `fc86889fed093a910d575ac47eca66239a7b47e8` から2026-10-16 00:39:25 JSTに計測され、countdownより前のresultであるためschema 3に置き換えられます。[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)と[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はconditional control flowより前のresultです。すべてをharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、2026-10-16 02:27:40 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
