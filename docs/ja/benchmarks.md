<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-11a1af7.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 13:00:19 JSTにclean commit `11a1af7cac32625ebfad5a6bde921ac5382f320d` から記録しました。Release buildで、harness schema 13、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

Schema 13は44個のtiming rowと決定的なhost-callback requested-payload heap profileをすべて維持し、link済みAOT processのmacOS `ru_maxrss` を31 sample追加します。Single callback profileは `(1, 8, 8, 0)`、burstは `(64, 512, 512, 0)` のままで、process-memory sampleはすべて1,671,168 byte（1.594 MiB）です。Heap profileはlogical payload lifetimeを測定し、process metricはallocator overheadではなくprocess全体のresident envelopeです。

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
| Bytecode lowering：wrapping `i64` add | 1,000 | 384.500 ns | 408.917 ns | 1.953% |
| Bytecode lowering：signed minimum | 1,000 | 1.079 us | 1.184 us | 3.431% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.702 us | 1.880 us | 3.571% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.794 us | 1.920 us | 2.694% |
| Bytecode lowering：conditional sum | 1,000 | 3.089 us | 3.293 us | 2.996% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.000 us | 28.375 us | 74.536% |
| Cranelift JIT compilation：signed minimum | 1 | 23.458 us | 28.333 us | 20.524% |
| Cranelift JIT compilation：countdown back-edge | 1 | 27.125 us | 32.708 us | 14.432% |
| Cranelift JIT compilation：2-value accumulator | 1 | 31.500 us | 43.792 us | 11.815% |
| Cranelift JIT compilation：conditional sum | 1 | 40.875 us | 47.667 us | 7.286% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.500 us | 16.500 us | 19.849% |
| Cranelift AOT object emission：signed minimum | 1 | 18.333 us | 21.292 us | 6.819% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.666 us | 27.958 us | 9.171% |
| Cranelift AOT object emission：2-value accumulator | 1 | 27.000 us | 58.167 us | 93.216% |
| Cranelift AOT object emission：conditional sum | 1 | 36.500 us | 46.375 us | 10.732% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator／conditional-sum object | 1 | 38.003 ms | 39.754 ms | 2.535% |
| macOS AOT process startup：single launch | 1 | 1.620 ms | 2.351 ms | 253.007% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.556 ms | 1.746 ms | 6.813% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.941 ns | 0.988 ns | 1.910% |
| VM wrapping `i64` add call | 100,000 | 185.060 ns | 187.286 ns | 0.714% |
| JIT wrapping `i64` add call | 100,000 | 1.894 ns | 1.983 ns | 3.298% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 1.012 ns | 3.757% |
| VM signed minimum call | 100,000 | 511.214 ns | 519.978 ns | 0.689% |
| JIT signed minimum call | 100,000 | 2.655 ns | 2.738 ns | 1.909% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.988 ns | 6.579% |
| VM countdown call | 100,000 | 1.612 us | 1.634 us | 0.416% |
| unset interruption flag付きVM countdown call | 100,000 | 1.626 us | 1.866 us | 4.298% |
| 期限前deadline付きVM countdown call | 100,000 | 3.345 us | 3.380 us | 0.508% |
| JIT countdown call | 100,000 | 12.879 ns | 13.428 ns | 2.097% |
| Direct Rust 2-value accumulator call | 100,000 | 1.756 ns | 1.816 ns | 2.792% |
| VM 2-value accumulator call | 100,000 | 1.686 us | 1.695 us | 0.377% |
| JIT 2-value accumulator call | 100,000 | 13.093 ns | 13.844 ns | 2.368% |
| Direct Rust conditional-sum call | 100,000 | 5.688 ns | 6.088 ns | 2.716% |
| VM conditional-sum call | 100,000 | 3.117 us | 3.303 us | 6.572% |
| JIT conditional-sum call | 100,000 | 20.590 ns | 21.317 ns | 1.825% |
| Direct Rust host callback | 100,000 | 0.958 ns | 0.982 ns | 1.025% |
| VM host callback | 100,000 | 182.690 ns | 185.032 ns | 0.797% |
| JIT host callback | 100,000 | 2.790 ns | 2.984 ns | 1.907% |
| Direct Rust host-allocation callback | 100,000 | 12.151 ns | 12.726 ns | 2.536% |
| VM host-allocation callback | 100,000 | 195.430 ns | 198.140 ns | 0.996% |
| JIT host-allocation callback | 100,000 | 14.087 ns | 14.364 ns | 1.981% |
| Direct Rust 64-allocation-burst callback | 100,000 | 783.238 ns | 787.139 ns | 0.448% |
| VM 64-allocation-burst callback | 100,000 | 965.470 ns | 973.100 ns | 0.421% |
| JIT 64-allocation-burst callback | 100,000 | 784.837 ns | 791.552 ns | 0.552% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、conditional sumが44 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator、conditional-sum objectは352、368、376、392、400 byteでした。Link済みAOT harnessは17,104 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

別のlink済みAOT process-memory distributionは31 sampleで、minimum、median、p95、maximum、meanがすべて1,671,168 byte（1.594 MiB）、RSDが0.000%です。このpage粒度の一致は、このhost上の小さなallocation-free harnessについてのevidenceであり、普遍的なmemory floorではありません。

## Schema 14 formatとmigration

Schema 14はschema 13のtiming、artifact、heap、process-memory fieldをすべて維持し、`rust_host_allocation_churn_call`、`vm_host_allocation_churn_call`、`jit_host_allocation_churn_call` を追加します。各operationは64個のboxed `i64` valueをallocateし、1つおきのallocationをconsumeし、解放済みslotへ32個のreplacementをallocateしてから、最後の64個をconsumeします。同じgeneric heap pathは `(allocation_count, total_allocated_bytes, peak_live_bytes, live_bytes_after)` が `(96, 768, 512, 0)` の `host_allocation_churn_call` を追加します。

これらのrowはfree／reallocate interleavingを伴う固定sizeのsynthetic host-callback sequenceを計測します。Allocator metadata、fragmentation、reuse policyを分離せず、代表的applicationのevidenceではありません。VM／JIT差分test、macOS direct／response-file AOT integration、host非依存Windows COFF validation、cfg-gated MSVC pathがcallback／import contractを扱います。Nativeの3-sample smoke runは47個の一意なtiming row、3つすべての正確なheap profile、既存process-memory rowを記録しました。Windows CIはschema 14と正確なchurn row／profileを要求します。Consumerは `schema_version` で分岐する必要があり、schema 1〜13 JSON fileはimmutableなevidenceとして残します。Clean schema 14 baselineはまだ記録していません。

## Schema 13 formatとmigration

Schema 13はschema 12のtiming、artifact、heap fieldをすべて維持し、`process_memory_measurement_scope`、`process_memory_metric`、`process_memory_measurements` を追加します。Scopeは `linked_aot_executable_process` です。唯一のrowである `aot_process_peak_resident_memory_add_min_countdown_sum_to_sum_below` は、link済みAOT executableをsampleごとに1回起動した後、`bytes/process`、`sample_count`、minimum、median、p95、maximum、mean、relative standard deviationを記録します。

MetricはmacOSで `macos_rusage_ru_maxrss_bytes`、Windows MSVCで `windows_process_peak_working_set_bytes` です。前者は `/usr/bin/time -l` からparseし、後者はWindows PowerShell経由で `System.Diagnostics.Process.PeakWorkingSet64` を読み取ります。両方ともprivate／shared resident pageとloader／runtime stateを含むため、allocator-overheadや代表的applicationのmeasurementではなくplatform固有のprocess envelopeです。Consumerは `schema_version` で分岐する必要があり、schema 1〜12 JSON fileはimmutableなevidenceとして残します。上のclean macOS AArch64 resultは31 sampleすべてが1,671,168 byte、RSD 0.000%であることを記録します。Schema 14 Windows CIは正で順序付けされた31-sample distributionを1つ要求しますが、remote resultはまだ検査・commitしていません。

## Schema 12 formatとmigration

Schema 12はschema 11のtiming row／artifact fieldをすべて維持しながら、`heap_measurement_scope` と `heap_measurements` を追加します。Scopeは `host_callback_requested_payload_bytes` です。各profileは `allocation_count`、`total_allocated_bytes`、`peak_live_bytes`、`live_bytes_after` を記録します。Harnessはtimed loopの外側で、安全なmeasuring heapを通じて同じgeneric single／burst callback workloadを実行します。

正確なsingle profileは `(1, 8, 8, 0)`、burst profileは `(64, 512, 512, 0)` です。これらの値は要求された `i64` payloadとlogical cleanupだけを扱います。Allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。Consumerは `schema_version` で分岐する必要があり、schema 1〜11 JSON fileはimmutableなevidenceとして残します。過去のschema 12 resultは両profileと44個すべての一意なtiming rowを記録しました。Schema 13／14とWindows CIは同じheap contractを維持しますが、remote resultはまだ検査・commitしていません。

## Schema 11 formatとmigration

Schema 11は `rust_host_allocation_burst_call`、`vm_host_allocation_burst_call`、`jit_host_allocation_burst_call` を追加します。各operationは64個の異なる `Box<i64>` valueを作成し、512 payload byteすべてを固定長arrayで同時に保持し、wrapping sumを計算してからdropします。`black_box` は各boxとload済みvalueをoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべてのresultを検証します。

既存AOT link／startup seriesは変更せず、macOSと構成済みWindowsのintegration testが、64個の `malloc` allocationを同時に保持してから読み取り・解放する対応C callbackを実行します。これらのrowはallocator metadata、stack上のpointer array、process peak RSS、failure behavior、language heap、代表的applicationを計測しません。Schema 11はschema 10の既存row／artifact fieldをすべて維持します。Consumerは `schema_version` で分岐する必要があり、schema 1〜10 JSON fileはimmutableなevidenceとして残します。過去のclean schema 11 resultは44行すべてが一意であることを記録し、direct Rust／VM／JIT burst medianは782.624 ns／962.329 ns／786.270 nsです。RSDは0.331%／11.822%／0.378%で、VMのmaximum 1.620 usがp95 967.234 nsにもかかわらずRSDを押し上げているため、それらの値はperformance budgetではなく方向的evidenceのままです。Windows Server 2025 workflowは現在schema 14を要求し、burst 3 rowすべてを維持します。

## Schema 10 formatとmigration

Schema 10は `vm_countdown_deadline_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介してharness準備の1時間後に設定した絶対monotonic deadlineを渡します。Harnessはwarm-up／計測前にresultの等価性を検証し、deadlineはcallごとに再計算せず全sampleで共有します。

このrowは、deadlineが期限前のままdeadline checkを有効化するsteady-state costを計測します。Expiration latency、scheduler accuracy、callback overrun、callback preemption、native／JIT enforcementは計測しません。Schema 10は既存row／artifact fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜9 JSON fileはimmutableなevidenceとして残します。現在のclean schema 13 resultでは、通常のVM countdown medianが1.612 us、期限前deadline付きが3.345 usで、1.733 us、107.460%増加しました。0.416%／0.508%のRSDを持つこのpaired medianは方向性のevidenceであり、overhead budgetではありません。Schema 11はこのrowを維持し、schema 14が現在のWindows workflow contractです。

## Schema 9 formatとmigration

Schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと同じ検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介して共有されたunset interruption flagを渡します。現在のschema 13 artifactでは両rowがVMのoptional-control branchを含み、新rowは実行した各instruction／terminatorでflagのacquire loadも行います。Harnessはwarm-up／計測前にresultの等価性を検証します。

このrowは、requestが発生しないときにcooperative interruptionを有効化するsteady-state costを計測します。Interruption latency、cross-thread wake-up、set済みflag、callback preemption、native／JIT enforcementは計測しません。Schema 9はlowering、compilation、artifact-size、host-callback、link、startup fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜8 JSON fileはimmutableなevidenceとして残します。現在のclean schema 13 resultでは、通常のVM countdown medianが1.612 us、unset interruption付きが1.626 usで、14.078 ns、0.873%増加しました。これはinterruption rowの4.298% RSDより小さいため、この1つのpaired microbenchmarkはinterruption-check regressionを測定できず、overhead budgetも確立しません。Schema 10はこのrowを維持し、schema 14が現在のWindows workflow contractです。

## Schema 8 formatとmigration

Schema 8は `rust_host_allocation_call`、`vm_host_allocation_call`、`jit_host_allocation_call` を追加します。各operationは1回のRust `Box` allocate／read／drop round tripを通して `i64` を渡します。`black_box` によりallocated boxと戻り値をoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む既存の型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべての結果を検証します。

既存のAOT system-link、executable-size、process-startup seriesは変更しません。Launchされたharness内の1回のallocationはprocess creation／loader処理に埋もれるためです。別のplatform integration testが、link済みAOT objectを通して対応するC `malloc`／read／`free` callbackを実行します。したがって新rowが計測するのは、狭いhost-owned allocationとVM／JIT boundary costです。Language heap、allocator policy、peak memory、allocation failure、collection、代表的なapplication behaviorは計測しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜7 JSON fileはimmutableなevidenceとして残します。CleanなmacOS AArch64 schema 8 resultを上に記録しました。

Windows Server 2025 workflowは当初、変更のないplatform measurementに加えてschema 8と3つすべてのsingle-allocation rowを要求しました。Remote schema 8 resultはまだ検査もcommitもしておらず、workflowは現在、上記のschema 14を要求します。

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

Schema 7 harnessは、Windows x86-64 MSVCでconditional-sum lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Schema 8はそれらのplatform fieldと31／1,000／100,000／10 parameterを維持し、3つのsingle-allocation runtime rowを追加します。Schema 9は `vm_countdown_interruptible_call`、schema 10は `vm_countdown_deadline_call`、schema 11は3つのallocation-burst row、schema 12は正確な2つのheap profile、schema 13はlink済みprocess peak-memory distributionを追加し、現在Windows Server 2025 workflowはschema 14を要求します。Workflowはclean-checkout metadata、`windows`／`x86_64` identity、interruption、deadline、conditional-sum、single-allocation、allocation-burst、heap、process-memory、platform measurement、startup batch size、正のexecutable sizeも検証してから、そのJSONを30日間uploadします。

これはevidenceを生成するpathであり、記録済みWindows evidenceではありません。この変更によるremote runner結果はまだ検査もcommitもされていません。したがって、上のmacOS tableが現在唯一の測定済みbaselineです。

## 生成コード検査

Addとhost-callback objectをclean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` から再生成し、2026-10-16 00:25 JSTにmacOS AArch64 hostで検査しました。Signed-minimum objectはclean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` から00:43 JSTに最初に再生成し、その後、明示的CFGへの移行後にclean commit `f6eb016798c237412b59057fa32257ca2bc5d016` から01:08 JSTに再生成しました。

| Prototype | Export | Import | 生成body | Relocation |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | なし | `add x0, x0, x1`; `ret` | なし |
| Signed `i64` minimum | `_prototype_min` | なし | `cmp`、conditional `b.lt`、else側の `mov`、2つの `ret` path | なし |
| 型付きhost callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer authentication付きframe、GOT load、間接 `blr`、authenticated return | `_lang_host_i64_0` に対する `GOTLDP` と `GOTLDPOFF` |

3つすべてのfileは64-bit ARM64 Mach-O objectです。Add prototypeはrelocationなしの8-byte、2-instruction bodyへloweringされています。Minimumはrelocationなしの20-byte compareと実際のconditional branchへloweringされています。Less-than pathは最初のargumentを `x0` に保持し、もう一方のpathは `x1` をreturn registerへmoveします。明示的CFGへの移行後の再検査でも、同じcode／object size、symbol、instruction sequence、空のrelocation tableが得られました。Callbackはlinkerが解決するexternal importのままであり、position-independentなGOT addressingを使用します。そのbodyは32 bytesです。これは意図したarithmetic、control-flow、export／import lowering boundaryを確認します。Loop lowering、block parameter、loop-carried value、allocation、unwind behavior、他targetは評価していません。

完全なcommandと、取得した `file`、`nm`、`otool` の出力は[add／callback transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、[CFG移行前のminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、[明示的CFGのminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md)に保存しています。

その後のcleanな[debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)では、commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d` からadd、minimum、host-callback objectを再生成しました。すべてexternal symbolを保持しますが、`__debug_*` section、DWARF compile unit、line entry、source variable、parameter、localはいずれも含みません。これはbackend criterionを満たすものではなく、現在のdebug-quality floorを記録します。Source-span mapping、link済み／dSYM output、unwind metadata、Windows CodeView／PDB、JIT debugger registration、optimized steppingは未実装または未測定のままです。

## 解釈と制限

- Harnessは計測前に、minimumの両方の結果、countdown／accumulatorのimmediate-return／loop path、conditional sumのimmediate-return／add／skip pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.756 ns、1.686 us、13.093 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Conditional-sum prototypeは両方のbranch結果とimmediate-return pathを扱います。Direct、VM、JIT medianは5.688 ns、3.117 us、20.590 nsです。これはより広いCFG evidenceですが、代表的なprogram benchmarkではありません。
- Single-allocation host-callback medianはdirectが12.151 ns、VMが195.430 ns、JITが14.087 nsです。VM minus directは183.279 ns（1,508.345%）、JIT minus directは1.936 ns（15.933%）です。3 rowのRSDは2.536%／0.996%／1.981%で、このrunについてstableで限定的なboundary-cost evidenceを提供しますが、1回の `Box<i64>` round tripは代表的なallocator／memory workloadではありません。
- 64-allocation-burst medianはdirectが783.238 ns、VMが965.470 ns、JITが784.837 nsです。VM minus directは182.232 ns（23.266%）、JIT minus directは1.599 ns（0.204%）です。3 rowのRSDは0.448%／0.421%／0.552%で、このrunではstableなevidenceを提供しますが、一般的なboundary-overhead budgetを確立しません。
- Schema 12はtimed loopの外側で同じgeneric callback workloadをprofileします。Single callbackは1 allocation、total／peak live payload 8 byte、終了後live byte 0を記録し、burstは64 allocation、total／peak live payload 512 byte、終了後0を記録します。Scopeは意図的に `host_callback_requested_payload_bytes` としており、logical payload lifetime／cleanupを確認しますが、allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。
- Schema 13は31個のmemory sampleごとにallocation-freeのlink済みAOT harnessを別processで起動します。macOSは `ru_maxrss` を通じてminimum／median／p95／maximumのすべてで1,671,168 byteを報告し、RSDは0.000%です。このmetricはshared pageとloader／runtime stateを含み、page粒度です。Allocator-overheadでも代表的なallocation evidenceでもなく、Windowsの `PeakWorkingSet64` metricは独立して解釈する必要があります。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulator、schema 7はconditional sumを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add／minimum compilationのRSDは74.536%／20.524%です。Conditional-sum JIT／AOT medianは40.875 us／36.500 us、RSDは7.286%／10.732%です。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.620 ms、p95は2.351 msですが、RSDは253.007%です。10-launch batched rowは1.556 ms median、1.746 ms p95をRSD 6.813%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.879 ns／1.612 us、accumulatorは13.093 ns／1.686 usになります。Conditional-sum input `(64, 33)` と `(-7, 33)` を交互に実行すると、JIT／VM medianは20.590 ns／3.117 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。約1 nanosecondのdirect add／countdown／host-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、macOS AArch64とWindows x86-64でclean schema 14 resultを記録し、variable-size／代表的なallocation-memory workloadと追加のCFG workloadを加え、allocator overheadを分離し、両tier-1 targetでnative debug metadataを実装・検証します。Link済みharnessのpeak-memory rowはallocation-workload measurementではなくprocess envelopeです。Allocation burst／churn rowはbounded lifetime evidenceを追加しますが、syntheticな固定size host callbackのままです。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 12 result](../../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json)はcommit `fa9a69b8650b6a2f4b15ea645ab0e18f522a8790` から2026-10-16 12:24:45 JSTに計測され、process-level peak-memory distributionより前のresultであるためschema 13に置き換えられます。[Schema 11 result](../../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json)、[Schema 10 result](../../benchmarks/results/2026-10-16-macos-aarch64-133172b.json)、[Schema 9 result](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)、[Schema 8 result](../../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json)、[Schema 7 result](../../benchmarks/results/2026-10-16-macos-aarch64-0865167.json)、[Schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 13 result](../../benchmarks/results/2026-10-16-macos-aarch64-11a1af7.json)、2026-10-16 13:00:19 JSTに計測。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource control、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、allocation burst／churn callback prototype、最終更新 2026-10-16 13:16 JST。
- `src/bin/prototype-bench.rs`、schema 14 allocation-churn／heap／process-memory harness、最終更新 2026-10-16 13:26 JST。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- [macOS AArch64 AOT debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)、2026-10-16 12:06:01 JSTに取得。
- Repository commit `0865167bfe75331a29e2d4ee9e509c9d17b4b4f1`、2026-10-16にcommit。
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`、2026-10-16にcommit。
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`、2026-10-16にcommit。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

- macOS 27.0.1にinstallされたmacOS `time(1)`／`getrusage(2)` manual page。公開・更新日は提示されず、2026-10-16確認。後者は `ru_maxrss` の単位をbytesと規定します。
- [.NET `System.Diagnostics.Process` API documentation](https://github.com/dotnet/dotnet-api-docs/blob/cfbdb94f711c6122622fd1f9cacc91bb6ee53e3c/xml/System.Diagnostics/Process.xml)、2026-10-15 02:27:32 UTC更新。`PeakWorkingSet64` をprocess開始後に使用した最大physical working-set memory（bytes、shared／private pageを含む）と定義します。
