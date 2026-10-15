<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、clean commit `0808ce75f128dfb16bf9c6e6453200fea7749401` から記録しました。Release buildで、固定されたRust 1.99.0 toolchainとApple Clang 21.0.0を使用しました。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 35.250 ns | 42.458 ns | 11.394% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 20.583 us | 34.000 us | 66.169% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 15.792 us | 20.083 us | 13.802% |
| macOS system link：事前compile済みC harnessとAOT object | 1 | 38.031 ms | 39.093 ms | 1.665% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.988 ns | 0.990 ns | 1.425% |
| VM wrapping `i64` add call | 100,000 | 38.567 ns | 39.618 ns | 2.093% |
| JIT wrapping `i64` add call | 100,000 | 1.916 ns | 1.962 ns | 0.714% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.995 ns | 3.368% |
| VM host callback | 100,000 | 37.450 ns | 38.214 ns | 1.686% |
| JIT host callback | 100,000 | 2.874 ns | 2.928 ns | 0.751% |

Craneliftのcompiled code bufferには、add functionについて8 byte、host-callback functionについて40 byteが含まれました。生成されたadd objectは352 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。

## 生成コード検査

2つのnative object prototypeをclean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` から再生成し、2026-10-16 00:25 JSTに同じmacOS AArch64 hostで検査しました。

| Prototype | Export | Import | 生成body | Relocation |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | なし | `add x0, x0, x1`; `ret` | なし |
| 型付きhost callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer authentication付きframe、GOT load、間接 `blr`、authenticated return | `_lang_host_i64_0` に対する `GOTLDP` と `GOTLDPOFF` |

両方のfileは64-bit ARM64 Mach-O objectです。Add prototypeはrelocationなしの8-byte、2-instruction bodyへloweringされています。Callbackはlinkerが解決するexternal importのままであり、position-independentなGOT addressingを使用します。そのbodyは32 bytesです。これは意図したexport／import boundaryを確認し、callbackのcode sizeが大きい理由を示します。Control flow、allocation、debug metadata、unwind behavior、他targetは評価していません。

完全なcommandと、取得した `file`、`nm`、`otool` の出力は[raw inspection transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)に保存しています。

## 解釈と制限

- Harnessは計測前に、VM、JIT、direct Rustのresultが等価であることを確認します。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。System-link rowはC harnessを事前compileし、2つのobject fileからexecutableを生成する `cc` driverとsystem linkerを計測します。
- JIT compile resultのRSDは66.169%で、medianが20.583 usである一方、1 sampleは112.333 usに達しました。Object-emission resultのRSDは13.802%です。これらのmedianは初期baselineに有用ですが、variationが大きすぎるためstable performance budgetには使用できません。
- この1 instruction microbenchmarkは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、Windows x86-64上で同じschemaを反復し、実装済みconditional-return workloadをbenchmarkし、allocationとより広いcontrol-flow workloadを追加し、startup、memory、executable size、debug qualityを計測します。

## 以前のbaseline

Commit `b55848cfd5d24678f09eb00a9edda987dc25b257` から2026-10-16 00:05:30 JSTに計測した[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)は、harness履歴として残します。PIC object emissionとsystem-link measurementより前のresultであり、設計判断では現在のbaselineに置き換えられます。

## 情報源

- [現在のraw macOS AArch64 result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、2026-10-16 00:20:21 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- Repository commit `0808ce75f128dfb16bf9c6e6453200fea7749401`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
