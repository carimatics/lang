<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 00:39:25 JSTにclean commit `fc86889fed093a910d575ac47eca66239a7b47e8` から記録しました。Release buildで、harness schema 2、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 86.166 ns | 87.417 ns | 7.638% |
| Bytecode lowering：signed minimum | 1,000 | 76.000 ns | 77.000 ns | 0.535% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 22.583 us | 36.833 us | 57.043% |
| Cranelift JIT compilation：signed minimum | 1 | 26.167 us | 32.208 us | 15.909% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 16.041 us | 22.709 us | 15.231% |
| Cranelift AOT object emission：signed minimum | 1 | 23.042 us | 63.083 us | 50.846% |
| macOS system link：事前compile済みC harnessとadd／minimum object | 1 | 38.585 ms | 39.743 ms | 2.604% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.932 ns | 0.961 ns | 1.286% |
| VM wrapping `i64` add call | 100,000 | 42.638 ns | 44.422 ns | 1.807% |
| JIT wrapping `i64` add call | 100,000 | 1.867 ns | 1.932 ns | 1.500% |
| Direct Rust signed minimum call | 100,000 | 0.932 ns | 0.965 ns | 1.195% |
| VM signed minimum call | 100,000 | 42.483 ns | 44.651 ns | 1.777% |
| JIT signed minimum call | 100,000 | 2.664 ns | 2.852 ns | 2.946% |
| Direct Rust host callback | 100,000 | 0.932 ns | 0.979 ns | 1.311% |
| VM host callback | 100,000 | 40.401 ns | 41.765 ns | 2.028% |
| JIT host callback | 100,000 | 2.820 ns | 2.880 ns | 1.563% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、host callbackが40 byte含まれました。生成されたadd objectとminimum objectは352 byteと368 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。

## 生成コード検査

2つのnative object prototypeをclean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` から再生成し、2026-10-16 00:25 JSTに同じmacOS AArch64 hostで検査しました。

| Prototype | Export | Import | 生成body | Relocation |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | なし | `add x0, x0, x1`; `ret` | なし |
| 型付きhost callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer authentication付きframe、GOT load、間接 `blr`、authenticated return | `_lang_host_i64_0` に対する `GOTLDP` と `GOTLDPOFF` |

両方のfileは64-bit ARM64 Mach-O objectです。Add prototypeはrelocationなしの8-byte、2-instruction bodyへloweringされています。Callbackはlinkerが解決するexternal importのままであり、position-independentなGOT addressingを使用します。そのbodyは32 bytesです。これは意図したexport／import boundaryを確認し、callbackのcode sizeが大きい理由を示します。Control flow、allocation、debug metadata、unwind behavior、他targetは評価していません。

完全なcommandと、取得した `file`、`nm`、`otool` の出力は[raw inspection transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)に保存しています。

## 解釈と制限

- Harnessは計測前に、minimum functionの両方の結果を含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。System-link rowはC harnessを事前compileし、harness、add、minimumのobjectからexecutableを生成する `cc` driverとsystem linkerを計測します。
- JIT add compilationのRSDは57.043%で、medianが22.583 usである一方、1 sampleは105.542 usに達しました。Minimum AOT emissionのRSDは50.846%で、maximumが89.667 us、medianが23.042 usです。これらのmedianは方向性の証拠として有用ですが、compile-time variationが大きすぎるためstable performance budgetには使用できません。
- Runtime rowのRSDは3%未満でした。Conditional workloadではJIT call medianがaddの1.867 nsからminimumの2.664 nsへ増えますが、どちらも限定的microbenchmarkであり、代表的programのperformanceを予測しません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、minimum functionの生成コードを検査し、Windows x86-64上でschema 2を反復し、allocationと一般CFG workloadを追加し、startup、memory、executable size、debug qualityを計測します。

## 以前のbaseline

[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)はcommit `0808ce75f128dfb16bf9c6e6453200fea7749401` から2026-10-16 00:20:21 JSTに計測され、conditional workloadより前のresultであるためschema 2に置き換えられます。Commit `b55848cfd5d24678f09eb00a9edda987dc25b257` から2026-10-16 00:05:30 JSTに計測した[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)は、PIC object emissionとsystem-link measurementより前のresultです。両方をharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、2026-10-16 00:39:25 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
