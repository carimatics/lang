<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 12:24:45 JSTにclean commit `fa9a69b8650b6a2f4b15ea645ab0e18f522a8790` から記録しました。Release buildで、harness schema 12、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

Schema 12は44個のtiming rowをすべて維持し、決定的なhost-callback requested-payload heap profileを追加します。Single callbackは1 allocation、total／peak live 8 byte、終了後live byte 0を記録し、burstは64 allocation、total／peak live 512 byte、終了後0を記録します。これらのprofileはrequested payload lifetimeを測定するもので、allocator／process memoryではありません。

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
| Bytecode lowering：wrapping `i64` add | 1,000 | 387.000 ns | 444.250 ns | 4.921% |
| Bytecode lowering：signed minimum | 1,000 | 1.047 us | 1.145 us | 3.947% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.630 us | 1.730 us | 2.125% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.729 us | 1.844 us | 1.904% |
| Bytecode lowering：conditional sum | 1,000 | 2.982 us | 3.151 us | 2.786% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.166 us | 39.792 us | 78.766% |
| Cranelift JIT compilation：signed minimum | 1 | 23.875 us | 29.792 us | 20.127% |
| Cranelift JIT compilation：countdown back-edge | 1 | 27.042 us | 40.958 us | 15.224% |
| Cranelift JIT compilation：2-value accumulator | 1 | 31.334 us | 41.958 us | 10.191% |
| Cranelift JIT compilation：conditional sum | 1 | 40.833 us | 48.959 us | 8.694% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.667 us | 16.375 us | 18.600% |
| Cranelift AOT object emission：signed minimum | 1 | 18.417 us | 20.375 us | 4.883% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.125 us | 28.083 us | 7.200% |
| Cranelift AOT object emission：2-value accumulator | 1 | 26.166 us | 28.375 us | 3.537% |
| Cranelift AOT object emission：conditional sum | 1 | 35.791 us | 38.792 us | 3.082% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator／conditional-sum object | 1 | 37.325 ms | 39.520 ms | 3.276% |
| macOS AOT process startup：single launch | 1 | 1.527 ms | 2.446 ms | 270.642% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.678 ms | 1.890 ms | 6.180% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 1.026 ns | 2.830% |
| VM wrapping `i64` add call | 100,000 | 182.222 ns | 184.347 ns | 0.609% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 2.083 ns | 3.646% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.960 ns | 1.098% |
| VM signed minimum call | 100,000 | 499.119 ns | 503.471 ns | 0.567% |
| JIT signed minimum call | 100,000 | 2.662 ns | 2.863 ns | 2.932% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 1.010 ns | 4.932% |
| VM countdown call | 100,000 | 1.606 us | 1.629 us | 10.380% |
| unset interruption flag付きVM countdown call | 100,000 | 1.603 us | 1.608 us | 0.263% |
| 期限前deadline付きVM countdown call | 100,000 | 3.314 us | 3.325 us | 0.181% |
| JIT countdown call | 100,000 | 12.799 ns | 13.977 ns | 2.816% |
| Direct Rust 2-value accumulator call | 100,000 | 1.705 ns | 1.926 ns | 4.815% |
| VM 2-value accumulator call | 100,000 | 1.651 us | 1.657 us | 0.220% |
| JIT 2-value accumulator call | 100,000 | 12.750 ns | 13.379 ns | 1.616% |
| Direct Rust conditional-sum call | 100,000 | 5.605 ns | 6.068 ns | 3.529% |
| VM conditional-sum call | 100,000 | 3.039 us | 3.053 us | 0.237% |
| JIT conditional-sum call | 100,000 | 20.172 ns | 21.308 ns | 1.989% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.979 ns | 1.345% |
| VM host callback | 100,000 | 175.515 ns | 178.818 ns | 0.843% |
| JIT host callback | 100,000 | 2.795 ns | 3.116 ns | 4.188% |
| Direct Rust host-allocation callback | 100,000 | 11.857 ns | 12.503 ns | 2.064% |
| VM host-allocation callback | 100,000 | 187.240 ns | 190.214 ns | 0.699% |
| JIT host-allocation callback | 100,000 | 13.471 ns | 14.550 ns | 29.399% |
| Direct Rust 64-allocation-burst callback | 100,000 | 768.283 ns | 773.525 ns | 0.290% |
| VM 64-allocation-burst callback | 100,000 | 946.601 ns | 957.526 ns | 0.497% |
| JIT 64-allocation-burst callback | 100,000 | 771.625 ns | 777.079 ns | 0.369% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、conditional sumが44 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator、conditional-sum objectは352、368、376、392、400 byteでした。Link済みAOT harnessは17,104 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 13 formatとmigration

Schema 13はschema 12のtiming、artifact、heap fieldをすべて維持し、`process_memory_measurement_scope`、`process_memory_metric`、`process_memory_measurements` を追加します。Scopeは `linked_aot_executable_process` です。唯一のrowである `aot_process_peak_resident_memory_add_min_countdown_sum_to_sum_below` は、link済みAOT executableをsampleごとに1回起動した後、`bytes/process`、`sample_count`、minimum、median、p95、maximum、mean、relative standard deviationを記録します。

MetricはmacOSで `macos_rusage_ru_maxrss_bytes`、Windows MSVCで `windows_process_peak_working_set_bytes` です。前者は `/usr/bin/time -l` からparseし、後者はWindows PowerShell経由で `System.Diagnostics.Process.PeakWorkingSet64` を読み取ります。両方ともprivate／shared resident pageとloader／runtime stateを含むため、allocator-overheadや代表的applicationのmeasurementではなくplatform固有のprocess envelopeです。Consumerは `schema_version` で分岐する必要があり、schema 1〜12 JSON fileはimmutableなevidenceとして残します。Native 3-sample smoke runはmacOS pathを検証し、Windows CIは正で順序付けされた31-sample distributionを1つ要求します。Cleanなschema 13 baselineもremote Windows resultもまだcommitしていません。

## Schema 12 formatとmigration

Schema 12はschema 11のtiming row／artifact fieldをすべて維持しながら、`heap_measurement_scope` と `heap_measurements` を追加します。Scopeは `host_callback_requested_payload_bytes` です。各profileは `allocation_count`、`total_allocated_bytes`、`peak_live_bytes`、`live_bytes_after` を記録します。Harnessはtimed loopの外側で、安全なmeasuring heapを通じて同じgeneric single／burst callback workloadを実行します。

正確なsingle profileは `(1, 8, 8, 0)`、burst profileは `(64, 512, 512, 0)` です。これらの値は要求された `i64` payloadとlogical cleanupだけを扱います。Allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。Consumerは `schema_version` で分岐する必要があり、schema 1〜11 JSON fileはimmutableなevidenceとして残します。上のclean macOS AArch64 schema 12 resultは両profileと44個すべての一意なtiming rowを記録します。Schema 13とWindows CIは同じheap contractを維持しますが、remote resultはまだ検査・commitしていません。

## Schema 11 formatとmigration

Schema 11は `rust_host_allocation_burst_call`、`vm_host_allocation_burst_call`、`jit_host_allocation_burst_call` を追加します。各operationは64個の異なる `Box<i64>` valueを作成し、512 payload byteすべてを固定長arrayで同時に保持し、wrapping sumを計算してからdropします。`black_box` は各boxとload済みvalueをoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべてのresultを検証します。

既存AOT link／startup seriesは変更せず、macOSと構成済みWindowsのintegration testが、64個の `malloc` allocationを同時に保持してから読み取り・解放する対応C callbackを実行します。これらのrowはallocator metadata、stack上のpointer array、process peak RSS、failure behavior、language heap、代表的applicationを計測しません。Schema 11はschema 10の既存row／artifact fieldをすべて維持します。Consumerは `schema_version` で分岐する必要があり、schema 1〜10 JSON fileはimmutableなevidenceとして残します。過去のclean schema 11 resultは44行すべてが一意であることを記録し、direct Rust／VM／JIT burst medianは782.624 ns／962.329 ns／786.270 nsです。RSDは0.331%／11.822%／0.378%で、VMのmaximum 1.620 usがp95 967.234 nsにもかかわらずRSDを押し上げているため、それらの値はperformance budgetではなく方向的evidenceのままです。Windows Server 2025 workflowは現在schema 13を要求し、burst 3 rowすべてを維持します。

## Schema 10 formatとmigration

Schema 10は `vm_countdown_deadline_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介してharness準備の1時間後に設定した絶対monotonic deadlineを渡します。Harnessはwarm-up／計測前にresultの等価性を検証し、deadlineはcallごとに再計算せず全sampleで共有します。

このrowは、deadlineが期限前のままdeadline checkを有効化するsteady-state costを計測します。Expiration latency、scheduler accuracy、callback overrun、callback preemption、native／JIT enforcementは計測しません。Schema 10は既存row／artifact fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜9 JSON fileはimmutableなevidenceとして残します。現在のclean schema 12 resultでは、通常のVM countdown medianが1.606 us、期限前deadline付きが3.314 usで、1.708 us、106.379%増加しました。10.380%／0.181%のRSDを持つこのpaired medianは方向性のevidenceであり、overhead budgetではありません。Schema 11はこのrowを維持し、schema 13が現在のWindows workflow contractです。

## Schema 9 formatとmigration

Schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと同じ検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介して共有されたunset interruption flagを渡します。現在のschema 12 artifactでは両rowがVMのoptional-control branchを含み、新rowは実行した各instruction／terminatorでflagのacquire loadも行います。Harnessはwarm-up／計測前にresultの等価性を検証します。

このrowは、requestが発生しないときにcooperative interruptionを有効化するsteady-state costを計測します。Interruption latency、cross-thread wake-up、set済みflag、callback preemption、native／JIT enforcementは計測しません。Schema 9はlowering、compilation、artifact-size、host-callback、link、startup fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜8 JSON fileはimmutableなevidenceとして残します。現在のclean schema 12 resultでは、通常のVM countdown medianが1.606 us、unset interruption付きが1.603 usで、3.424 ns、0.213%減少しました。これは両rowの10.380%／0.263% RSDより小さいため、この1つのpaired microbenchmarkはinterruption-check regressionを測定できず、overhead budgetも確立しません。Schema 10はこのrowを維持し、schema 13が現在のWindows workflow contractです。

## Schema 8 formatとmigration

Schema 8は `rust_host_allocation_call`、`vm_host_allocation_call`、`jit_host_allocation_call` を追加します。各operationは1回のRust `Box` allocate／read／drop round tripを通して `i64` を渡します。`black_box` によりallocated boxと戻り値をoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む既存の型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべての結果を検証します。

既存のAOT system-link、executable-size、process-startup seriesは変更しません。Launchされたharness内の1回のallocationはprocess creation／loader処理に埋もれるためです。別のplatform integration testが、link済みAOT objectを通して対応するC `malloc`／read／`free` callbackを実行します。したがって新rowが計測するのは、狭いhost-owned allocationとVM／JIT boundary costです。Language heap、allocator policy、peak memory、allocation failure、collection、代表的なapplication behaviorは計測しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜7 JSON fileはimmutableなevidenceとして残します。CleanなmacOS AArch64 schema 8 resultを上に記録しました。

Windows Server 2025 workflowは当初、変更のないplatform measurementに加えてschema 8と3つすべてのsingle-allocation rowを要求しました。Remote schema 8 resultはまだ検査もcommitもしておらず、workflowは現在、上記のschema 13を要求します。

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

Schema 7 harnessは、Windows x86-64 MSVCでconditional-sum lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Schema 8はそれらのplatform fieldと31／1,000／100,000／10 parameterを維持し、3つのsingle-allocation runtime rowを追加します。Schema 9は `vm_countdown_interruptible_call`、schema 10は `vm_countdown_deadline_call`、schema 11は3つのallocation-burst row、schema 12は正確な2つのheap profile、schema 13はlink済みprocess peak-memory distributionを追加し、現在Windows Server 2025 workflowはschema 13を要求します。Workflowはclean-checkout metadata、`windows`／`x86_64` identity、interruption、deadline、conditional-sum、single-allocation、allocation-burst、heap、process-memory、platform measurement、startup batch size、正のexecutable sizeも検証してから、そのJSONを30日間uploadします。

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
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.705 ns、1.651 us、12.750 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Conditional-sum prototypeは両方のbranch結果とimmediate-return pathを扱います。Direct、VM、JIT medianは5.605 ns、3.039 us、20.172 nsです。これはより広いCFG evidenceですが、代表的なprogram benchmarkではありません。
- Single-allocation host-callback medianはdirectが11.857 ns、VMが187.240 ns、JITが13.471 nsです。3 rowのRSDは2.064%／0.699%／29.399%で、JIT maximumはp95 14.550 nsを超える37.373 nsです。これは限定的なboundary-cost比較を支持しますが、1回の `Box<i64>` round tripと1つのoutlier-sensitive rowは代表的なallocator／memory workloadではありません。
- 64-allocation-burst medianはdirectが768.283 ns、VMが946.601 ns、JITが771.625 nsです。VM minus directは178.318 ns（23.210%）、JIT minus directは3.342 ns（0.435%）です。3 rowのRSDは0.290%／0.497%／0.369%で、このrunではstableなevidenceを提供しますが、一般的なboundary-overhead budgetを確立しません。
- Schema 12はtimed loopの外側で同じgeneric callback workloadをprofileします。Single callbackは1 allocation、total／peak live payload 8 byte、終了後live byte 0を記録し、burstは64 allocation、total／peak live payload 512 byte、終了後0を記録します。Scopeは意図的に `host_callback_requested_payload_bytes` としており、logical payload lifetime／cleanupを確認しますが、allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulator、schema 7はconditional sumを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add／minimum compilationのRSDは78.766%／20.127%です。Conditional-sum JIT／AOT medianは40.833 us／35.791 us、RSDは8.694%／3.082%です。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.527 ms、p95は2.446 msですが、RSDは270.642%です。10-launch batched rowは1.678 ms median、1.890 ms p95をRSD 6.180%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.799 ns／1.606 us、accumulatorは12.750 ns／1.651 usになります。Conditional-sum input `(64, 33)` と `(-7, 33)` を交互に実行すると、JIT／VM medianは20.172 ns／3.039 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。約1 nanosecondのdirect add／countdown／host-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、macOS AArch64とWindows x86-64でclean schema 13 resultを記録し、より広いallocation／memory workloadと追加のCFG workloadを加え、allocator overheadを分離し、両tier-1 targetでnative debug metadataを実装・検証します。Link済みharnessのpeak-memory rowはallocation-workload measurementではなくprocess envelopeです。64-allocation burstは1 boxより代表性がありますが、syntheticなhost callbackのままです。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 11 result](../../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json)はcommit `664edb9d3a986614a8160d4d4adf9c1f0a8bb7bd` から2026-10-16 11:50:37 JSTに計測され、requested-payload heap profileより前のresultであるためschema 12に置き換えられます。[Schema 10 result](../../benchmarks/results/2026-10-16-macos-aarch64-133172b.json)、[Schema 9 result](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)、[Schema 8 result](../../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json)、[Schema 7 result](../../benchmarks/results/2026-10-16-macos-aarch64-0865167.json)、[Schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 12 result](../../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json)、2026-10-16 12:24:45 JSTに計測。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource control、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、64-allocation burst callback prototype、最終更新 2026-10-16 11:42 JST。
- `src/bin/prototype-bench.rs`、schema 13 linked-AOT process peak-memory harness、最終更新 2026-10-16 12:50 JST。
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
