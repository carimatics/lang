<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 2026-10-16 macOS AArch64 baseline

最初のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、clean commit `b55848cfd5d24678f09eb00a9edda987dc25b257` から記録しました。Release buildで、固定されたRust 1.99.0 toolchainを使用しました。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 37.666 ns | 38.083 ns | 9.368% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 29.791 us | 38.625 us | 19.754% |
| Direct Rust wrapping `i64` add call | 100,000 | 1.713 ns | 1.778 ns | 8.927% |
| VM wrapping `i64` add call | 100,000 | 38.105 ns | 55.598 ns | 14.605% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.910 ns | 1.873% |
| Direct Rust host callback | 100,000 | 0.959 ns | 1.023 ns | 7.332% |
| VM host callback | 100,000 | 36.919 ns | 38.520 ns | 1.811% |
| JIT host callback | 100,000 | 4.058 ns | 4.369 ns | 2.974% |

Craneliftのcompiled code bufferには、add functionについて8 byte、host-callback functionについて40 byteが含まれました。これらのsizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。

## 解釈と制限

- Harnessは計測前に、VM、JIT、direct Rustのresultが等価であることを確認します。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- JIT compile resultのRSDは19.754%、VM add resultのRSDは14.605%です。Medianは初期baselineに有用ですが、variationが大きすぎるためstable performance budgetには使用できません。
- この1 instruction microbenchmarkは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、Windows x86-64上で同じschemaを反復し、代表的なcontrol-flowとallocation workloadを追加し、generated codeを検査し、AOT object emission、system-linker time、startup、memory、debug qualityを計測します。Backendはhost-native object artifactを生成でき、macOS integration coverageはそれをC harnessへlinkしますが、現在のbenchmark harnessはobject emissionまたはlinkingをまだ計測しません。

## 情報源

- [Raw macOS AArch64 result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)、2026-10-16 00:05:30 JSTに計測。
- Repository commit `b55848cfd5d24678f09eb00a9edda987dc25b257`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

このlocal measurement baselineには外部Web情報源を使用していません。
