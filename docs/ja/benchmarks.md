<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 04:50:50 JSTにclean commit `180af8502341c4ed00d90a658193cfa14b5ce1da` から記録しました。Release buildで、harness schema 5、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

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
| Bytecode lowering：wrapping `i64` add | 1,000 | 457.125 ns | 479.375 ns | 4.471% |
| Bytecode lowering：signed minimum | 1,000 | 1,064.542 ns | 1,216.125 ns | 5.064% |
| Bytecode lowering：countdown back-edge | 1,000 | 1,665.042 ns | 1,853.625 ns | 4.118% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 20.416 us | 34.959 us | 74.097% |
| Cranelift JIT compilation：signed minimum | 1 | 23.834 us | 31.292 us | 20.092% |
| Cranelift JIT compilation：countdown back-edge | 1 | 28.291 us | 36.834 us | 13.284% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.875 us | 36.750 us | 65.320% |
| Cranelift AOT object emission：signed minimum | 1 | 18.167 us | 25.959 us | 26.924% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.000 us | 27.209 us | 14.202% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown object | 1 | 36.837 ms | 39.596 ms | 2.958% |
| macOS AOT process startup：single launch | 1 | 1.487 ms | 1.846 ms | 256.272% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.499 ms | 1.639 ms | 4.688% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.958 ns | 0.974 ns | 0.659% |
| VM wrapping `i64` add call | 100,000 | 182.419 ns | 183.855 ns | 0.657% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.917 ns | 4.034% |
| Direct Rust signed minimum call | 100,000 | 0.958 ns | 0.991 ns | 1.624% |
| VM signed minimum call | 100,000 | 509.131 ns | 524.915 ns | 3.813% |
| JIT signed minimum call | 100,000 | 2.756 ns | 2.847 ns | 2.134% |
| Direct Rust countdown call | 100,000 | 0.958 ns | 1.207 ns | 38.693% |
| VM countdown call | 100,000 | 1,585.620 ns | 1,608.688 ns | 1.834% |
| JIT countdown call | 100,000 | 12.707 ns | 12.992 ns | 0.946% |
| Direct Rust host callback | 100,000 | 0.930 ns | 1.018 ns | 4.664% |
| VM host callback | 100,000 | 171.864 ns | 174.175 ns | 0.792% |
| JIT host callback | 100,000 | 2.789 ns | 2.868 ns | 0.898% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown objectは352、368、376 byteでした。Link済みAOT harnessは16,992 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 6 formatとmigration

Schema 6は2-value `sum_to` accumulator workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、generated-code size、object sizeへ追加します。Runtime sampleはinput `64` と `-7` を再利用し、64-iteration loop後に `2080`、immediate-return pathで `0` を返します。Harnessは計測前に `-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Tier-1 link／process-startup setにもaccumulator objectを追加します。そのためschema 6は3つのplatform rowを `system_link_add_min_countdown_sum_to`、`aot_process_startup_add_min_countdown_sum_to`、`aot_process_startup_batch_add_min_countdown_sum_to` へrenameし、executable-size fieldを `aot_add_min_countdown_sum_to` へrenameします。また、`jit_sum_to`／`aot_sum_to` size fieldと6つのaccumulator measurement rowを追加します。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜5 JSON fileはimmutableなevidenceとして残します。

Schema 6の実装とWindows evidence pathは存在しますが、cleanなschema 6 resultはまだ記録していません。上のschema 5 tableが現在の測定済みbaselineです。

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

- Harnessは計測前に、minimumの両方の結果とcountdownのimmediate-return／loop pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- Schema 6は別の2-value accumulator prototypeを既存の全workload dimensionへ追加しますが、現在のschema 5 baselineはそれらのrowより前のresultです。Cleanなschema 6 resultを記録するまで、accumulator benchmark claimはprovisionalです。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdownを追加します。どちらもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add compilationのRSDは74.097%、AOT add emissionのRSDは65.320%です。Countdown JIT／AOT medianは28.291 us／22.000 usで、RSDは13.284%／14.202%です。これらのmedianは方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.487 ms、p95は1.846 msですが、42.796 msの1 sampleによりRSDが256.272%まで上がりました。10-launch batched rowは、同程度の1.499 ms median、1.639 ms p95をRSD 4.688%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- VM／JIT runtime medianはschema 4と方向的に一貫しています。64-iteration back-edgeとimmediate returnを交互に実行すると、JIT medianは12.707 ns、VM medianは1,585.620 nsになります。このbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。Sub-nanosecond direct-call rowとその大きいrelative variationはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、両tier-1 hostでschema 6を記録し、allocationと追加のCFG workloadを加え、memoryとdebug qualityを計測します。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)はcommit `78fd3edff8d5dbeb38ff501999f7094a604404bd` から2026-10-16 03:08:08 JSTに計測され、batched startup evidenceより前のresultであるためschema 5に置き換えられます。[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、2026-10-16 04:50:50 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`、2026-10-16にcommit。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
