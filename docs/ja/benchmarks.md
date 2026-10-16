<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Prototype benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## 現在のmacOS AArch64 baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 11:50:37 JSTにclean commit `664edb9d3a986614a8160d4d4adf9c1f0a8bb7bd` から記録しました。Release buildで、harness schema 11、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

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
| Bytecode lowering：wrapping `i64` add | 1,000 | 380.917 ns | 407.833 ns | 2.792% |
| Bytecode lowering：signed minimum | 1,000 | 1.091 us | 1.194 us | 4.704% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.746 us | 1.943 us | 5.151% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.825 us | 1.935 us | 3.177% |
| Bytecode lowering：conditional sum | 1,000 | 3.152 us | 3.402 us | 4.184% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.792 us | 27.917 us | 68.530% |
| Cranelift JIT compilation：signed minimum | 1 | 23.833 us | 29.542 us | 22.188% |
| Cranelift JIT compilation：countdown back-edge | 1 | 26.958 us | 32.541 us | 11.813% |
| Cranelift JIT compilation：2-value accumulator | 1 | 31.125 us | 36.833 us | 8.553% |
| Cranelift JIT compilation：conditional sum | 1 | 42.375 us | 55.583 us | 8.894% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.584 us | 18.542 us | 19.345% |
| Cranelift AOT object emission：signed minimum | 1 | 18.375 us | 20.375 us | 4.883% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.375 us | 28.375 us | 8.024% |
| Cranelift AOT object emission：2-value accumulator | 1 | 26.417 us | 30.791 us | 5.967% |
| Cranelift AOT object emission：conditional sum | 1 | 38.000 us | 117.584 us | 48.520% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator／conditional-sum object | 1 | 40.253 ms | 44.621 ms | 5.156% |
| macOS AOT process startup：single launch | 1 | 1.823 ms | 4.079 ms | 239.902% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.631 ms | 1.917 ms | 7.055% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 0.955 ns | 0.733% |
| VM wrapping `i64` add call | 100,000 | 181.037 ns | 184.516 ns | 0.794% |
| JIT wrapping `i64` add call | 100,000 | 1.859 ns | 2.465 ns | 13.750% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 1.001 ns | 2.676% |
| VM signed minimum call | 100,000 | 496.268 ns | 500.607 ns | 0.519% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.665 ns | 0.827% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.968 ns | 1.619% |
| VM countdown call | 100,000 | 1.602 us | 1.807 us | 4.793% |
| unset interruption flag付きVM countdown call | 100,000 | 1.616 us | 2.721 us | 22.662% |
| 期限前deadline付きVM countdown call | 100,000 | 3.324 us | 3.376 us | 0.618% |
| JIT countdown call | 100,000 | 12.844 ns | 13.335 ns | 1.583% |
| Direct Rust 2-value accumulator call | 100,000 | 1.705 ns | 1.896 ns | 6.577% |
| VM 2-value accumulator call | 100,000 | 1.678 us | 1.691 us | 0.747% |
| JIT 2-value accumulator call | 100,000 | 13.095 ns | 13.492 ns | 1.729% |
| Direct Rust conditional-sum call | 100,000 | 5.777 ns | 6.076 ns | 1.861% |
| VM conditional-sum call | 100,000 | 3.081 us | 3.097 us | 6.143% |
| JIT conditional-sum call | 100,000 | 20.469 ns | 20.963 ns | 1.665% |
| Direct Rust host callback | 100,000 | 0.958 ns | 0.959 ns | 0.038% |
| VM host callback | 100,000 | 178.337 ns | 180.564 ns | 0.828% |
| JIT host callback | 100,000 | 2.874 ns | 2.962 ns | 1.192% |
| Direct Rust host-allocation callback | 100,000 | 12.272 ns | 12.796 ns | 2.059% |
| VM host-allocation callback | 100,000 | 190.573 ns | 192.175 ns | 0.777% |
| JIT host-allocation callback | 100,000 | 14.123 ns | 14.572 ns | 1.775% |
| Direct Rust 64-allocation-burst callback | 100,000 | 782.624 ns | 787.409 ns | 0.331% |
| VM 64-allocation-burst callback | 100,000 | 962.329 ns | 967.234 ns | 11.822% |
| JIT 64-allocation-burst callback | 100,000 | 786.270 ns | 790.598 ns | 0.378% |

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、conditional sumが44 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator、conditional-sum objectは352、368、376、392、400 byteでした。Link済みAOT harnessは17,104 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 11 formatとmigration

Schema 11は `rust_host_allocation_burst_call`、`vm_host_allocation_burst_call`、`jit_host_allocation_burst_call` を追加します。各operationは64個の異なる `Box<i64>` valueを作成し、512 payload byteすべてを固定長arrayで同時に保持し、wrapping sumを計算してからdropします。`black_box` は各boxとload済みvalueをoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべてのresultを検証します。

既存AOT link／startup seriesは変更せず、macOSと構成済みWindowsのintegration testが、64個の `malloc` allocationを同時に保持してから読み取り・解放する対応C callbackを実行します。これらのrowはallocator metadata、stack上のpointer array、process peak RSS、failure behavior、language heap、代表的applicationを計測しません。Schema 11はschema 10の既存row／artifact fieldをすべて維持します。Consumerは `schema_version` で分岐する必要があり、schema 1〜10 JSON fileはimmutableなevidenceとして残します。上のclean macOS resultは44行すべてが一意であることを記録し、direct Rust／VM／JIT burst medianは782.624 ns／962.329 ns／786.270 nsです。RSDは0.331%／11.822%／0.378%で、VMのmaximum 1.620 usがp95 967.234 nsにもかかわらずRSDを押し上げているため、これらの値はperformance budgetではなく方向的evidenceのままです。Windows Server 2025 workflowはschema 11とburst 3 rowすべてを要求します。

## Schema 10 formatとmigration

Schema 10は `vm_countdown_deadline_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介してharness準備の1時間後に設定した絶対monotonic deadlineを渡します。Harnessはwarm-up／計測前にresultの等価性を検証し、deadlineはcallごとに再計算せず全sampleで共有します。

このrowは、deadlineが期限前のままdeadline checkを有効化するsteady-state costを計測します。Expiration latency、scheduler accuracy、callback overrun、callback preemption、native／JIT enforcementは計測しません。Schema 10は既存row／artifact fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜9 JSON fileはimmutableなevidenceとして残します。現在のclean schema 11 resultでは、通常のVM countdown medianが1.602 us、期限前deadline付きが3.324 usで、1.723 us、107.558%増加しました。4.793%／0.618%のRSDを持つこのpaired medianは方向性のevidenceであり、overhead budgetではありません。Schema 11はこのrowを維持し、Windows workflow contractとしてschema 10を置き換えます。

## Schema 9 formatとmigration

Schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと同じ検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介して共有されたunset interruption flagを渡します。現在のschema 11 artifactでは両rowがVMのoptional-control branchを含み、新rowは実行した各instruction／terminatorでflagのacquire loadも行います。Harnessはwarm-up／計測前にresultの等価性を検証します。

このrowは、requestが発生しないときにcooperative interruptionを有効化するsteady-state costを計測します。Interruption latency、cross-thread wake-up、set済みflag、callback preemption、native／JIT enforcementは計測しません。Schema 9はlowering、compilation、artifact-size、host-callback、link、startup fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜8 JSON fileはimmutableなevidenceとして残します。現在のclean schema 11 resultでは、通常のVM countdown medianが1.602 us、unset interruption付きが1.616 usで、14.056 ns、0.878%増加しました。これは両rowの4.793%／22.662% RSDより小さいため、この1つのpaired microbenchmarkはinterruption-check regressionを測定できず、overhead budgetも確立しません。Schema 10はこのrowを維持し、schema 11が現在のWindows workflow contractです。

## Schema 8 formatとmigration

Schema 8は `rust_host_allocation_call`、`vm_host_allocation_call`、`jit_host_allocation_call` を追加します。各operationは1回のRust `Box` allocate／read／drop round tripを通して `i64` を渡します。`black_box` によりallocated boxと戻り値をoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む既存の型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべての結果を検証します。

既存のAOT system-link、executable-size、process-startup seriesは変更しません。Launchされたharness内の1回のallocationはprocess creation／loader処理に埋もれるためです。別のplatform integration testが、link済みAOT objectを通して対応するC `malloc`／read／`free` callbackを実行します。したがって新rowが計測するのは、狭いhost-owned allocationとVM／JIT boundary costです。Language heap、allocator policy、peak memory、allocation failure、collection、代表的なapplication behaviorは計測しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜7 JSON fileはimmutableなevidenceとして残します。CleanなmacOS AArch64 schema 8 resultを上に記録しました。

Windows Server 2025 workflowは当初、変更のないplatform measurementに加えてschema 8と3つすべてのsingle-allocation rowを要求しました。Remote schema 8 resultはまだ検査もcommitもしておらず、workflowは現在、上記のschema 11を要求します。

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

Schema 7 harnessは、Windows x86-64 MSVCでconditional-sum lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Schema 8はそれらのplatform fieldと31／1,000／100,000／10 parameterを維持し、3つのsingle-allocation runtime rowを追加します。Schema 9は `vm_countdown_interruptible_call`、schema 10は `vm_countdown_deadline_call`、schema 11は3つのallocation-burst rowを追加し、現在Windows Server 2025 workflowはschema 11を要求します。Workflowはclean-checkout metadata、`windows`／`x86_64` identity、interruption、deadline、conditional-sum、single-allocation、allocation-burst、platform measurement、startup batch size、正のexecutable sizeも検証してから、そのJSONを30日間uploadします。

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
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.705 ns、1.678 us、13.095 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Conditional-sum prototypeは両方のbranch結果とimmediate-return pathを扱います。Direct、VM、JIT medianは5.777 ns、3.081 us、20.469 nsです。これはより広いCFG evidenceですが、代表的なprogram benchmarkではありません。
- Single-allocation host-callback medianはdirectが12.272 ns、VMが190.573 ns、JITが14.123 nsです。3 rowのRSDは2.059%、0.777%、1.775%です。これは限定的なboundary-cost比較を支持しますが、1回の `Box<i64>` round tripは代表的なallocator／memory workloadではありません。
- 64-allocation-burst medianはdirectが782.624 ns、VMが962.329 ns、JITが786.270 nsです。VM minus directは179.705 ns（22.962%）、JIT minus directは3.646 ns（0.466%）です。VM rowの11.822% RSDはp95 967.234 nsを超えるmaximum 1.620 usに左右されているため、この1回のrunはstableなboundary-overhead budgetを確立しません。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulator、schema 7はconditional sumを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add／minimum compilationのRSDは68.530%／22.188%です。Conditional-sum JIT／AOT medianは42.375 us／38.000 us、RSDは8.894%／48.520%です。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.823 ms、p95は4.079 msですが、RSDは239.902%です。10-launch batched rowは1.631 ms median、1.917 ms p95をRSD 7.055%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.844 ns／1.602 us、accumulatorは13.095 ns／1.678 usになります。Conditional-sum input `(64, 33)` と `(-7, 33)` を交互に実行すると、JIT／VM medianは20.469 ns／3.081 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。約1 nanosecondのdirect add／countdown／host-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

初期native backendを選定する前に、Windows x86-64でclean schema 11 resultを記録し、より広いallocation／memory workloadと追加のCFG workloadを加え、peak memoryを計測し、両tier-1 targetでnative debug metadataを実装・検証します。64-allocation burstは1 boxより代表性がありますが、syntheticなhost callbackのままです。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 10 result](../../benchmarks/results/2026-10-16-macos-aarch64-133172b.json)はcommit `133172b383b80b05f5cf2d576d2eba5f2fbc2851` から2026-10-16 11:28:39 JSTに計測され、allocation-burst rowより前のresultであるためschema 11に置き換えられます。[Schema 9 result](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)、[Schema 8 result](../../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json)、[Schema 7 result](../../benchmarks/results/2026-10-16-macos-aarch64-0865167.json)、[Schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 11 result](../../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json)、2026-10-16 11:50:37 JSTに計測。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource control、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、64-allocation burst callback prototype、最終更新 2026-10-16 11:42 JST。
- `src/bin/prototype-bench.rs`、schema 11 allocation-burst harness、最終更新 2026-10-16 11:42 JST。
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

これらのlocal measurement baselineには外部Web情報源を使用していません。
