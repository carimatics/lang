<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 11:05:30 JSTにclean commit `e390a5d66fefc8595ff88acb0d771978755a2ef7` から記録しました。Release buildで、harness schema 9、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

```bash
./target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 397.542 ns | 517.666 ns | 9.324% |
| Bytecode lowering：signed minimum | 1,000 | 1.076 us | 1.305 us | 9.120% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.703 us | 2.047 us | 6.634% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.833 us | 2.148 us | 7.183% |
| Bytecode lowering：conditional sum | 1,000 | 3.143 us | 3.730 us | 7.346% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.333 us | 33.750 us | 72.684% |
| Cranelift JIT compilation：signed minimum | 1 | 24.209 us | 29.542 us | 20.702% |
| Cranelift JIT compilation：countdown back-edge | 1 | 28.500 us | 54.041 us | 25.014% |
| Cranelift JIT compilation：2-value accumulator | 1 | 32.833 us | 113.792 us | 55.206% |
| Cranelift JIT compilation：conditional sum | 1 | 42.500 us | 96.750 us | 35.093% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 14.792 us | 37.375 us | 83.960% |
| Cranelift AOT object emission：signed minimum | 1 | 19.375 us | 26.750 us | 16.228% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.166 us | 27.834 us | 13.106% |
| Cranelift AOT object emission：2-value accumulator | 1 | 26.083 us | 31.250 us | 10.195% |
| Cranelift AOT object emission：conditional sum | 1 | 35.458 us | 52.792 us | 11.951% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator／conditional-sum object | 1 | 39.367 ms | 44.068 ms | 4.822% |
| macOS AOT process startup：single launch | 1 | 2.095 ms | 3.061 ms | 245.917% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.858 ms | 2.193 ms | 7.912% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.931 ns | 0.990 ns | 1.896% |
| VM wrapping `i64` add call | 100,000 | 180.487 ns | 184.458 ns | 0.963% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.897 ns | 0.769% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.960 ns | 1.795% |
| VM signed minimum call | 100,000 | 495.665 ns | 499.207 ns | 0.394% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.663 ns | 0.453% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 1.131 ns | 19.684% |
| VM countdown call | 100,000 | 1.588 us | 1.598 us | 0.324% |
| unset interruption flag付きVM countdown call | 100,000 | 1.592 us | 1.603 us | 0.317% |
| JIT countdown call | 100,000 | 12.781 ns | 13.345 ns | 3.296% |
| Direct Rust 2-value accumulator call | 100,000 | 1.705 ns | 1.775 ns | 6.449% |
| VM 2-value accumulator call | 100,000 | 1.641 us | 1.648 us | 0.267% |
| JIT 2-value accumulator call | 100,000 | 12.746 ns | 14.265 ns | 3.196% |
| Direct Rust conditional-sum call | 100,000 | 5.605 ns | 5.870 ns | 1.830% |
| VM conditional-sum call | 100,000 | 2.940 us | 3.093 us | 4.580% |
| JIT conditional-sum call | 100,000 | 20.511 ns | 23.565 ns | 4.517% |
| Direct Rust host callback | 100,000 | 0.941 ns | 1.661 ns | 19.314% |
| VM host callback | 100,000 | 177.398 ns | 203.175 ns | 5.537% |
| JIT host callback | 100,000 | 2.795 ns | 3.132 ns | 3.394% |
| Direct Rust host-allocation callback | 100,000 | 12.068 ns | 13.382 ns | 3.254% |
| VM host-allocation callback | 100,000 | 189.585 ns | 198.475 ns | 2.014% |
| JIT host-allocation callback | 100,000 | 13.931 ns | 14.399 ns | 2.429% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、conditional sumが44 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator、conditional-sum objectは352、368、376、392、400 byteでした。Link済みAOT harnessは17,104 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 10 formatとmigration

Schema 10は `vm_countdown_deadline_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介してharness準備の1時間後に設定した絶対monotonic deadlineを渡します。Harnessはwarm-up／計測前にresultの等価性を検証し、deadlineはcallごとに再計算せず全sampleで共有します。

このrowは、deadlineが期限前のままdeadline checkを有効化するsteady-state costを計測します。Expiration latency、scheduler accuracy、callback overrun、callback preemption、native／JIT enforcementは計測しません。Schema 10は既存row／artifact fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜9 JSON fileはimmutableなevidenceとして残します。Dirtyな3-sample smoke runは41行すべてが一意であることを検証しましたが、clean schema 10 resultはまだ記録していません。Windows Server 2025 workflowはschema 9 contractに加えてschema 10／新deadline rowを要求します。

## Schema 9 formatとmigration

Schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと同じ検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介して共有されたunset interruption flagを渡します。記録済み `e390a5d` artifactでは両rowがVMのoptional-interruption branchを含み、新rowは実行した各instruction／terminatorでflagのacquire loadも行います。Harnessはwarm-up／計測前にresultの等価性を検証します。

このrowは、requestが発生しないときにcooperative interruptionを有効化するsteady-state costを計測します。Interruption latency、cross-thread wake-up、set済みflag、callback preemption、native／JIT enforcementは計測しません。Schema 9はlowering、compilation、artifact-size、host-callback、link、startup fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜8 JSON fileはimmutableなevidenceとして残します。Clean macOS resultでは通常のVM countdownが1.588 us、unset interruption flag付きが1.592 usで、medianは3.409 ns、0.215%増加しました。両rowのRSDは0.4%未満ですが、この1つのpaired microbenchmarkは方向性のevidenceでありoverhead budgetではありません。このartifactはoptional deadline checkより前のもので、deadline-check overheadのevidenceを提供しません。Schema 10はこのrowを維持し、Windows workflow contractとしてschema 9を置き換えます。

## Schema 8 formatとmigration

Schema 8は `rust_host_allocation_call`、`vm_host_allocation_call`、`jit_host_allocation_call` を追加します。各operationは1回のRust `Box` allocate／read／drop round tripを通して `i64` を渡します。`black_box` によりallocated boxと戻り値をoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む既存の型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべての結果を検証します。

既存のAOT system-link、executable-size、process-startup seriesは変更しません。Launchされたharness内の1回のallocationはprocess creation／loader処理に埋もれるためです。別のplatform integration testが、link済みAOT objectを通して対応するC `malloc`／read／`free` callbackを実行します。したがって新rowが計測するのは、狭いhost-owned allocationとVM／JIT boundary costです。Language heap、allocator policy、peak memory、allocation failure、collection、代表的なapplication behaviorは計測しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜7 JSON fileはimmutableなevidenceとして残します。CleanなmacOS AArch64 schema 8 resultを上に記録しました。

Windows Server 2025 workflowは当初、変更のないplatform measurementに加えてschema 8と3つすべてのhost-allocation rowを要求しました。Remote schema 8 resultはまだ検査もcommitもしておらず、workflowは現在、上記のschema 10を要求します。

## Schema 7 formatとmigration

Schema 7は `sum_below` conditional-sum workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、generated-code size、object sizeへ追加します。Runtime sampleは、skipとaddの両pathを実行して `528` を返す `(64, 33)` と、immediate-return pathで `0` を返す `(-7, 33)` を交互に使用します。Harnessは計測前に `(-7, 33)`、`(0, 33)`、`(1, 1)`、`(1, 2)`、`(64, 33)` についてdirect／VM／JITの等価性を確認します。

Tier-1 link／process-startup setにも5番目のconditional-sum objectを追加します。そのためschema 7は3つのplatform rowを `system_link_add_min_countdown_sum_to_sum_below`、`aot_process_startup_add_min_countdown_sum_to_sum_below`、`aot_process_startup_batch_add_min_countdown_sum_to_sum_below` へrenameし、executable-size fieldを `aot_add_min_countdown_sum_to_sum_below` へrenameします。また、`jit_sum_below`／`aot_sum_below` size fieldと6つのconditional-sum measurement rowを追加します。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜6 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 7 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 7 resultはまだ検査もcommitもしていません。

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

Schema 7 harnessは、Windows x86-64 MSVCでconditional-sum lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Schema 8はそれらのplatform fieldと31／1,000／100,000／10 parameterを維持し、3つのhost-allocation runtime rowを追加します。Schema 9は `vm_countdown_interruptible_call`、schema 10は `vm_countdown_deadline_call` を追加し、現在Windows Server 2025 workflowはschema 10を要求します。Workflowはclean-checkout metadata、`windows`／`x86_64` identity、interruption、deadline、conditional-sum、allocation、platform measurement、startup batch size、正のexecutable sizeも検証してから、そのJSONを30日間uploadします。

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

- Harnessは計測前に、minimumの両方の結果、countdown／accumulatorのimmediate-return／loop path、conditional sumのimmediate-return／add／skip pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.705 ns、1.641 us、12.746 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Conditional-sum prototypeは両方のbranch結果とimmediate-return pathを扱います。Direct、VM、JIT medianは5.605 ns、2.940 us、20.511 nsです。これはより広いCFG evidenceですが、代表的なprogram benchmarkではありません。
- Host-allocation medianはdirectが12.068 ns、VMが189.585 ns、JITが13.931 nsです。3 rowのRSDは3.254%、2.014%、2.429%です。これは限定的なboundary-cost比較を支持しますが、1回の `Box<i64>` round tripは代表的なallocator／memory workloadではありません。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulator、schema 7はconditional sumを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add／minimum compilationのRSDは72.684%／20.702%、add AOT emissionのRSDは83.960%です。Conditional-sum JIT／AOT medianは42.500 us／35.458 us、RSDは35.093%／11.951%です。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは2.095 ms、p95は3.061 msですが、57.060 msの1 sampleによりRSDが245.917%まで上がりました。10-launch batched rowは1.858 ms median、2.193 ms p95をRSD 7.912%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.781 ns／1.588 us、accumulatorは12.746 ns／1.641 usになります。Conditional-sum input `(64, 33)` と `(-7, 33)` を交互に実行すると、JIT／VM medianは20.511 ns／2.940 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。約1 nanosecondのdirect add／minimum／countdown／host-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、macOS AArch64とWindows x86-64でclean schema 10 resultを記録し、代表的なallocation／memory workloadと追加のCFG workloadを加え、memoryとdebug qualityを計測します。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 8 result](../../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json)はcommit `6034dc8959a228c30aecb9bf29f125391f7759a2` から2026-10-16 10:23:41 JSTに計測され、interruption rowより前のresultであるためschema 9に置き換えられます。[Schema 7 result](../../benchmarks/results/2026-10-16-macos-aarch64-0865167.json)、[Schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 9 result](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)、2026-10-16 11:05:30 JSTに計測。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource control、最終更新 2026-10-16 11:20 JST。
- `src/bin/prototype-bench.rs`、schema 10 deadline-overhead harness、最終更新 2026-10-16 11:22 JST。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- Repository commit `0865167bfe75331a29e2d4ee9e509c9d17b4b4f1`、2026-10-16にcommit。
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`、2026-10-16にcommit。
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`、2026-10-16にcommit。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

これらのlocal measurement baselineには外部Web情報源を使用していません。
