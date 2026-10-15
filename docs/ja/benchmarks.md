<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在のbaselineは、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 03:08:08 JSTにclean commit `78fd3edff8d5dbeb38ff501999f7094a604404bd` から記録しました。Release buildで、harness schema 4、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

```bash
target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 419.750 ns | 470.791 ns | 5.168% |
| Bytecode lowering：signed minimum | 1,000 | 1,124.250 ns | 1,198.209 ns | 4.255% |
| Bytecode lowering：countdown back-edge | 1,000 | 1,664.708 ns | 1,805.333 ns | 3.746% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 20.916 us | 30.125 us | 67.711% |
| Cranelift JIT compilation：signed minimum | 1 | 23.375 us | 30.916 us | 21.480% |
| Cranelift JIT compilation：countdown back-edge | 1 | 27.166 us | 35.208 us | 13.675% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.375 us | 16.917 us | 20.476% |
| Cranelift AOT object emission：signed minimum | 1 | 17.666 us | 19.416 us | 4.242% |
| Cranelift AOT object emission：countdown back-edge | 1 | 21.459 us | 23.541 us | 3.617% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown object | 1 | 36.712 ms | 38.449 ms | 2.344% |
| macOS AOT process startup：add／minimum／countdown harness | 1 | 1.511 ms | 1.906 ms | 276.388% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 0.960 ns | 1.074% |
| VM wrapping `i64` add call | 100,000 | 177.461 ns | 178.857 ns | 0.473% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.960 ns | 2.836% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.957 ns | 2.533% |
| VM signed minimum call | 100,000 | 497.900 ns | 509.294 ns | 5.382% |
| JIT signed minimum call | 100,000 | 2.695 ns | 2.953 ns | 9.008% |
| Direct Rust countdown call | 100,000 | 0.971 ns | 0.996 ns | 1.620% |
| VM countdown call | 100,000 | 1,590.605 ns | 1,634.968 ns | 2.138% |
| JIT countdown call | 100,000 | 12.735 ns | 13.022 ns | 1.023% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.931 ns | 0.074% |
| VM host callback | 100,000 | 173.798 ns | 176.845 ns | 1.039% |
| JIT host callback | 100,000 | 2.790 ns | 2.979 ns | 2.702% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown objectは352、368、376 byteでした。Link済みAOT harnessは16,992 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 5 formatとmigration

Schema 5はtop-levelの `startup_iterations` fieldと `aot_process_startup_batch_add_min_countdown` measurementを追加します。既存のsingle-launch rowはsample当たり1 operationのままです。Batched rowは各sample内で設定数のprocess launchを順番に実行し、経過時間をその回数で割ります。Defaultは10です。これは1回のscheduling delayの影響を受けにくいamortized launch-cost estimateを提供し、single-launch rowはtail-sensitiveなlatency observationとして残します。

2つのrowは異なる問いに答えるため、mergeしてはいけません。Batchingはloader timeを分離せず、host noiseを除去せず、individual-latency percentileを置き換えません。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜4 JSON fileはimmutableなevidenceとして残します。

Schema 5の実装とWindows evidence pathは存在しますが、cleanなschema 5 resultはまだ記録していません。上のschema 4 tableが現在の測定済みbaselineです。

## Schema 4 formatとmigration

Schema 4は `executable_file_bytes.aot_add_min_countdown` と `aot_process_startup_add_min_countdown` measurementを追加します。Executable-size fieldは初期tier-1 hostでは正のbyte count、それ以外では `null` です。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜3 JSON fileはimmutableなevidenceとして残します。

Startup rowは、反復するprocess creation、dynamic loading、add／minimum／countdown checkの実行、output collection、process terminationを計測します。Harnessは計測前にlinkとexecutionを1回検証し、operating-system cacheをflushしません。そのため、これはcold-boot latencyではなくwarm-cache process-startup measurementです。Executableには事前compile済みC harnessとplatform linkerのdefaultが含まれますが、完全なruntime distributionやinstallation-size measurementではありません。

CleanなmacOS AArch64 schema 4 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 4 resultはまだ検査もcommitもしていません。

## Schema 3 formatとmigration

Schema 3は、bounded countdown workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、artifact size、system-link setへ追加します。Runtime sampleは、`i64` block parameterをback-edgeの周囲で運んでzeroへ到達するinput `64` と、immediate-return pathを取る `-7` を交互に使用します。Harnessは計測前に、`-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Schema 3は `jit_countdown` と `aot_countdown` size field、6つのcountdown measurement rowを追加します。また、`system_link_add_min` を `system_link_add_min_countdown` へrenameします。Consumerはこれらのnameをmergeせず、`schema_version` で分岐する必要があります。既存のschema 1／2 JSON fileはimmutableなhistorical evidenceとして残します。

## Windows x86-64 evidence path

Schema 5 harnessは、Windows x86-64 MSVCで `system_link_add_min_countdown`、両process-startup row、`executable_file_bytes.aot_add_min_countdown` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Windows Server 2025 workflowは31／1,000／100,000／10 parameterで実行し、schema 5、clean-checkout metadata、`windows`／`x86_64` identity、すべてのplatform measurement、startup batch size、正のexecutable sizeを検証し、そのJSONを30日間uploadします。

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
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add compilationのRSDは67.711%で、medianは20.916 usです。Countdown JIT／AOT medianは27.166 us／21.459 usで、RSDは13.675%／3.617%です。これらのmedianは方向性の証拠として有用ですが、add compile-time variationが大きすぎるためstable performance budgetには使用できません。
- AOT process-startup medianは1.511 ms、p95は1.906 msですが、49.824 msの1 sampleによりRSDが276.388%まで上がりました。これは現在の31-sample startup methodが、stable budgetにはschedulingまたはhost noiseの影響を受けすぎる証拠です。Methodを改善するときはmedianだけでなくraw distributionを使用します。
- VM／JIT runtime medianはschema 3と方向的に一貫していますが、複数のRSDが1.5%を超えました。64-iteration back-edgeとimmediate returnを交互に実行すると、JIT medianは12.735 ns、VM medianは1,590.605 nsになります。このbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。Sub-nanosecond direct-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、両tier-1 hostでschema 5を記録し、singleとbatched startup distributionを比較し、allocationとより広いCFG workloadを追加し、memoryとdebug qualityを計測します。macOS schema 4 executable-size resultはharness artifactであり、release packaging budgetではありません。

## 以前のbaseline

[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)はcommit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3` から2026-10-16 02:27:40 JSTに計測され、process-startupとexecutable-size evidenceより前のresultであるためschema 4に置き換えられます。[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、2026-10-16 03:08:08 JSTに計測。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
