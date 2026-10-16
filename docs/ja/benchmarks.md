<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 09:37:56 JSTにclean commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6` から記録しました。Release buildで、harness schema 6、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

```bash
target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 387.375 ns | 457.750 ns | 5.394% |
| Bytecode lowering：signed minimum | 1,000 | 1.086 us | 1.128 us | 2.473% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.711 us | 1.885 us | 4.356% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.781 us | 1.833 us | 1.164% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 21.583 us | 113.000 us | 85.387% |
| Cranelift JIT compilation：signed minimum | 1 | 24.083 us | 30.458 us | 24.956% |
| Cranelift JIT compilation：countdown back-edge | 1 | 28.167 us | 32.334 us | 16.625% |
| Cranelift JIT compilation：2-value accumulator | 1 | 31.292 us | 37.458 us | 8.464% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.625 us | 21.833 us | 31.397% |
| Cranelift AOT object emission：signed minimum | 1 | 18.167 us | 20.459 us | 7.174% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.208 us | 26.750 us | 6.206% |
| Cranelift AOT object emission：2-value accumulator | 1 | 26.334 us | 28.959 us | 4.174% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator object | 1 | 36.834 ms | 40.020 ms | 3.516% |
| macOS AOT process startup：single launch | 1 | 1.486 ms | 2.005 ms | 263.849% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.415 ms | 1.638 ms | 5.510% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 0.982 ns | 1.913% |
| VM wrapping `i64` add call | 100,000 | 177.218 ns | 178.999 ns | 0.573% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.994 ns | 2.557% |
| Direct Rust signed minimum call | 100,000 | 0.931 ns | 0.992 ns | 2.091% |
| VM signed minimum call | 100,000 | 490.746 ns | 495.128 ns | 0.425% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.782 ns | 5.347% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.958 ns | 1.154% |
| VM countdown call | 100,000 | 1.562 us | 1.651 us | 1.855% |
| JIT countdown call | 100,000 | 12.730 ns | 13.217 ns | 1.276% |
| Direct Rust 2-value accumulator call | 100,000 | 1.704 ns | 1.732 ns | 0.989% |
| VM 2-value accumulator call | 100,000 | 1.631 us | 1.640 us | 0.302% |
| JIT 2-value accumulator call | 100,000 | 12.782 ns | 13.800 ns | 2.843% |
| Direct Rust host callback | 100,000 | 0.930 ns | 1.048 ns | 10.285% |
| VM host callback | 100,000 | 170.679 ns | 176.610 ns | 1.245% |
| JIT host callback | 100,000 | 2.789 ns | 2.920 ns | 1.465% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator objectは352、368、376、392 byteでした。Link済みAOT harnessは17,056 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 6 formatとmigration

Schema 6は2-value `sum_to` accumulator workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、generated-code size、object sizeへ追加します。Runtime sampleはinput `64` と `-7` を再利用し、64-iteration loop後に `2080`、immediate-return pathで `0` を返します。Harnessは計測前に `-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Tier-1 link／process-startup setにもaccumulator objectを追加します。そのためschema 6は3つのplatform rowを `system_link_add_min_countdown_sum_to`、`aot_process_startup_add_min_countdown_sum_to`、`aot_process_startup_batch_add_min_countdown_sum_to` へrenameし、executable-size fieldを `aot_add_min_countdown_sum_to` へrenameします。また、`jit_sum_to`／`aot_sum_to` size fieldと6つのaccumulator measurement rowを追加します。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜5 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 6 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 6 resultはまだ検査もcommitもしていません。

## Schema 5 formatとmigration

Schema 5はtop-levelの `startup_iterations` fieldと `aot_process_startup_batch_add_min_countdown` measurementを追加します。既存のsingle-launch rowはsample当たり1 operationのままです。Batched rowは各sample内で設定数のprocess launchを順番に実行し、経過時間をその回数で割ります。Defaultは10です。これは1回のscheduling delayの影響を受けにくいamortized launch-cost estimateを提供し、single-launch rowはtail-sensitiveなlatency observationとして残します。

2つのrowは異なる問いに答えるため、mergeしてはいけません。Batchingはloader timeを分離せず、host noiseを除去せず、individual-latency percentileを置き換えません。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜4 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 5 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 5 resultはまだ検査もcommitもしていません。

## Schema 4 formatとmigration

Schema 4は `executable_file_bytes.aot_add_min_countdown` と `aot_process_startup_add_min_countdown` measurementを追加します。Executable-size fieldは初期tier-1 hostでは正のbyte count、それ以外では `null` です。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜3 JSON fileはimmutableなevidenceとして残します。

Startup rowは、反復するprocess creation、dynamic loading、add／minimum／countdown checkの実行、output collection、process terminationを計測します。Harnessは計測前にlinkとexecutionを1回検証し、operating-system cacheをflushしません。そのため、これはcold-boot latencyではなくwarm-cache process-startup measurementです。Executableには事前compile済みC harnessとplatform linkerのdefaultが含まれますが、完全なruntime distributionやinstallation-size measurementではありません。

CleanなmacOS AArch64 schema 4 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 4 resultはまだ検査もcommitもしていません。

## Schema 3 formatとmigration

Schema 3は、bounded countdown workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、artifact size、system-link setへ追加します。Runtime sampleは、`i64` block parameterをback-edgeの周囲で運んでzeroへ到達するinput `64` と、immediate-return pathを取る `-7` を交互に使用します。Harnessは計測前に、`-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Schema 3は `jit_countdown` と `aot_countdown` size field、6つのcountdown measurement rowを追加します。また、`system_link_add_min` を `system_link_add_min_countdown` へrenameします。Consumerはこれらのnameをmergeせず、`schema_version` で分岐する必要があります。既存のschema 1／2 JSON fileはimmutableなhistorical evidenceとして残します。

## Windows x86-64 evidence path

Schema 6 harnessは、Windows x86-64 MSVCでaccumulator lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Windows Server 2025 workflowは31／1,000／100,000／10 parameterで実行し、schema 6、clean-checkout metadata、`windows`／`x86_64` identity、accumulator／platform measurement、startup batch size、正のexecutable sizeを検証し、そのJSONを30日間uploadします。

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

- Harnessは計測前に、minimumの両方の結果とcountdown／accumulatorのimmediate-return／loop pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.704 ns、1.631 us、12.782 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Schema 6後のconditional-sum prototypeは、同じbounded loop内へbranchとadd／skip mergeを追加します。Differential testとAOT integration testで扱いますが、このschema 6 baselineには含まれず、benchmark claimは未完了です。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulatorを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add compilationのRSDは85.387%、AOT add emissionのRSDは31.397%です。Countdown JIT／AOT medianは28.167 us／22.208 us、accumulator JIT／AOT medianは31.292 us／26.334 usです。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.486 ms、p95は2.005 msですが、45.908 msの1 sampleによりRSDが263.849%まで上がりました。10-launch batched rowは1.415 ms median、1.638 ms p95をRSD 5.510%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.730 ns／1.562 us、accumulatorは12.782 ns／1.631 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。Sub-nanosecond direct-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、Windows x86-64でschema 6を記録し、allocationと追加のCFG workloadを加え、memoryとdebug qualityを計測します。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)はcommit `180af8502341c4ed00d90a658193cfa14b5ce1da` から2026-10-16 04:50:50 JSTに計測され、accumulator evidenceより前のresultであるためschema 6に置き換えられます。[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、2026-10-16 09:37:56 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`、2026-10-16にcommit。
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`、2026-10-16にcommit。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
