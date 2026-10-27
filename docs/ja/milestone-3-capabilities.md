<!-- translation-of: ../milestone-3-capabilities.md -->

# Milestone 3: concurrencyとembedding

[English](../milestone-3-capabilities.md) · [完了delivery board](milestone-3.md) · [Roadmap](roadmap.md)

検証日: **2026-10-27 JST**、実行source **`9e78f2d`**。受理範囲で**全15 exit gate / 96 gate point**が成功しました。[出口記録](../evidence/2026-10-27-m3-exit-9e78f2d.json)で各gateと証拠を対応付けています。M2の完了記録と過去の計測は変更していません。

native hostは、検証済みuser scriptをloadし、typed native APIを供給してscriptを呼び、nested callbackを受け、IPCなしで明示capabilityを適用できます。structured task・channel・isolated serviceがVM・生成native実行・対応する別link objectで動作します。公開component C v1境界は安定化し、source構文とprivate Rust/SDK linkは実験段階のままです。

## 実装済みworkflowを試す

repositoryの固定Rust toolchain（`rust-toolchain.toml`）と対応native toolを使います。tier-1 targetはmacOS AArch64とWindows x86-64 MSVCです。repository rootで実行します。

```sh
cargo run -- run examples/scripts/tasks.lang
cargo run -- run tests/fixtures/task_owned_buffers.lang
cargo run -- run tests/fixtures/task_channels.lang
cargo run -- run tests/fixtures/task_service_state_replace.lang
cargo run -- run tests/fixtures/task_owned_transfer_all.lang
```

結果は順に**42、71、42、42、42**です。spawn/join、suspensionを越えるowned buffer、有限channel、owned service stateの置換、owning async引数/結果を実行します。sourceと独立MIR検査で、移動済みclaim・禁止suspension loan・非Send payload・不正service authorityを拒否します。

同じchannel fixtureを生成native codeで実行できます。

```sh
cargo run --example native-structured-tasks -- tests/fixtures/task_channels.lang
```

結果は**42**です。native差分/別link object検査では、値・event・失敗origin・所有権・cleanupを比較します。単一の戻り値一致だけを適合証拠にはしていません。

保存taskはartifactを独立検証してから実行します。

```sh
cargo run -- compile --output target/m3-tasks.lbc examples/scripts/tasks.lang
cargo run -- run --artifact target/m3-tasks.lbc
```

結果は**42**です。LANGBC schema 5がtyped task/channel/service/component情報・source span・独立に検査する所有権/effect/loan proofを保持します。不明/非互換versionは実行前に拒否します。component artifactには実際のchecked bindingとgrantが必要で、通常scriptのentry artifactとは異なります。

## Structured ownershipとisolation

taskはscopeを持つclaimで、detached workではありません。親のlocal owner退役前に子孫をjoinします。cancelは協調的で、決定的postorder cleanupを行います。cleanup失敗時もprimary failureと順序付きsuppressed証拠を保持します。未受領のowning結果は、移動または必須退役までescrowで保持します。

channelはqueue/backpressureを有限化し、未commit sendの拒否/cancelで所有権を保持します。FIFO wakeupは実scopeのwait authorityを使います。serviceは単一のisolated state ownerを持ち、producerとreceiver/stateのauthorityを区別し、state前にqueued messageを退役します。再帰Send/ShareはsourceとMIRで独立検査します。このprofileはguest taskを複数OS threadで実行しません。

## Typed native pluginとscriptをloadする

```sh
cargo run --example component-plugin-host -- vm
cargo run --example component-plugin-host -- jit
```

両modeで通常sourceのscriptと保持したnative increment providerを公開C API経由で実行します。script結果**42**、native call前のtyped grant欠落拒否、panic、結果**1**での正常再利用、fuel拒否、一度だけのnative owner解放、**最終管理割当ゼロ**を検査します。[plugin guideとsource/IDL](../../examples/plugins/README.ja.md)を参照してください。

providerの存在だけではauthorityを与えません。hostが検査済みcontract projectionと明示typed grantを供給し、grant欠落/不一致・非互換signature・不正metadataをprovider探索や副作用前に拒否します。native実装とimageはtrusted host codeです。script検証と有限controlで任意native codeをsandbox化するものではありません。

## 安定したC embedding境界

[RFC 0033](rfcs/0033-component-v1-compatibility.md)により、実両hostのcross-mode適合と旧client移行後、対応component C v1契約を安定化しました。規範[header](../component-abi-v1.h)と独立[layout vector](../component-abi-v1-layouts.json)は、**全32非null operation・139 field offset・38 signature**を定義します。API tableは両targetで**280 byte**です。

`lang_component_v1_get_api`へversion **1**と厳密table sizeを渡します。request/response headerには厳密size/versionとreserved 0が必要です。不明version・不正requestは公開前に失敗しoutputを変更しません。将来の非互換拡張には新version/namespaceを使い、v1へ暗黙にfield追加しません。

同じtargetで公開runtimeを更新するときも既存互換header/生成clientを保持できます。[旧client fixture](../../tests/fixtures/component-v1-40158d6/provenance.json)が最初の完全公開source `40158d6`を保持します。その実宣言/codecをcompileして現実装を実行します。旧scalar clientは実VM/JITのregistry/resource/task/失敗を検査し、旧counter bindingは厳密frameとtransfer commit結果を保持します。

bindingがencodeするのは値でありauthorityではありません。contextual resource indexは実table・generation・provider・holder・loanとの一致が必要です。encodeやcommit前拒否は入力所有権を変えず、committed/unknownを未移動扱いにしてはいけません。result byte/reportとresource ownerの寿命は別です。borrowed viewはcallback scope外へ逃がせず、元destructorがownerを一度だけ退役し、cleanup失敗は元実行をpoisonします。

実生成Cの検査はsource/artifact VM/JITと厳密SDK AOT Image、nested callback、保持した失敗、元code/resource ownerを検証します。

```sh
cargo test --test component_generation
cargo test --test component_embedding_inputs
cargo test --test component_native_image
```

## Installed native実行

[SDK guide](native-sdk.md)にassembly・明示host binding・`link-native`を記載しています。両hostの[macOS](../evidence/2026-10-27-m3-sdk-macos-8f41f44.json)と[Windows](../evidence/2026-10-27-m3-sdk-windows-8f41f44.json) consumerが、copy・owned・C・task・owned-task・component・component-gatewayの**7 profile**を実際に起動しました。

各consumerはcheckoutなしでrepository path 12件の不在を確認し、PATHにCargoなし・空Cargo home・起動前に元link入力を除去した状態です。taskは42を返し、componentは42/panic −42/再利用1/fuel拒否と最終在庫ゼロを保持します。[Actions run 40456240378](https://github.com/carimatics/lang/actions/runs/40456240378)でproducer/consumerが成功しました。その後`9e78f2d`までの変更はtest/docsだけなのでproduction/SDK証拠は有効です。

C v1の安定性はprivate Rust/native source ABIを安定化しません。AOT Imageはprivate pointer参照前に対応host/buildの厳密SDK一致、元の選択code、実grant/providerを必要とします。探索やVM/JIT fallbackで不足authorityを補いません。identityや独立versionの保存formatが異なるときは、一致toolでpackageを再buildします。

| Version領域 | 対応revision |
| --- | --- |
| IDL / 公開C ABI | 1 / 1 |
| LCMP contract/report | 3 |
| LCVL frame / LCSG signature preimage | 1 / 1 |
| LANGBC task/component artifact | 5 |
| LANGNAT task/component record | 2 |
| Native component program / SDK | 3 / 3 |

これらは独立version領域であり、共通の互換番号ではありません。旧copy/owned/Cとtask packageも、それぞれ記録した受入規則を保持します。

## 上限と失敗動作

単一の元allocator/control/preparation履歴をguest・native adapter・nested callbackで共有します。heap byte/live allocation、frame/cell/task admission、fuel/host call、service/adapter workを副作用前に検査します。numeric failureは実kind・上限・要求量、source/task/caller context、順序付きcleanup証拠を保持します。

例えば次のcommandは、元source spanと`ServiceWorkLimit { limit: 0, used: 0, requested: 1 }`で拒否します。

```sh
cargo run -- run --service-work 0 tests/fixtures/task_owned_buffers.lang
```

割込み/拒否後も受理済みownerのcleanupを行います。上限は対応する管理/計上対象workであり、process RSSや任意native実行の上限ではありません。協調pollではblocking foreign callbackを中断できません。async native callback stack・detached task・第二frontend・WebAssembly component実行はこのmilestoneの対象外です。async REPL submissionも未対応で、上記CLI/artifactと明示task APIが実行workflowです。

## Cost観測

[benchmark report](benchmarks.md#milestone-3-concurrencyとcomponent-entryのcost)と[clean release sample](../../benchmarks/results/2026-10-27-m3-macos-c9ffd19.json)で有限VM/JIT 6 workloadとC/script/native callback境界を、3 warmup・21計測sample、生分布、source/compiler来歴、同一論理counter、最終管理在庫ゼロで記録しました。準備/実行区間とcomponent JITの各call内compileを明示しています。

macOS限定の観測であり、Windows/AOT throughputやprocess memoryの主張はしません。[M2再実行](../../benchmarks/results/2026-10-27-m2-parity-macos-c9ffd19.json)は21組のsource/Rust/C/oracleとunwind退役/再利用で成功しました。parity証拠であり過去M2 timingを置き換えません。test/CI時間をbenchmark sampleには使いません。

## 全exit監査

全行がsource `9e78f2d`で完了しています。[機械記録](../evidence/2026-10-27-m3-exit-9e78f2d.json)に対応を保持します。完了のためにP0/P1項目を先送り・削除していません。

| Gate | Weight | 受理した成果と実行証拠 |
| --- | ---: | --- |
| G01 | 5 | Scheduler・async・Send/Share・channel/backpressure・isolation契約: RFC 0021–0026。 |
| G02 | 5 | Versioned IDL/wire/C・error・reentry・cancel契約: RFC 0027。 |
| G03 | 8 | Owning structured task・join/cancel・決定的退役: task integration/transfer・native task suite。 |
| G04 | 5 | 有限channel・wakeup・所有権回復・cancel: source/native channel suite。 |
| G05 | 8 | Isolated service state・capability role: source/native service・type capability suite。 |
| G06 | 8 | 有限IDL/codec/generation・独立検証済み保存artifact: IDL/wire/generation/artifact suite。 |
| G07 | 8 | Owned/borrowed resource来歴・移動・loan・poison: source resource/scoped root/C suite。 |
| G08 | 5 | Typed grantと副作用前の不正/非互換拒否: grant/selection/registration・installed package。 |
| G09 | 8 | Typed failure/owner保持付き実VM/JIT/AOT call/callback: C host/native Image/installed SDK。 |
| G10 | 5 | 元mixed stack・task/caller span・cancel/cleanup: scoped root/C host、LF/CRLF faultを含む。 |
| G11 | 5 | 元heap/allocation・callback計上: foreign allocator/scoped root/native Image。 |
| G12 | 5 | 複合recursion/frame/cell/live task admissionと有限control: task control/scoped root。 |
| G13 | 8 | VM/JIT/AOT/callbackの協調割込みと必須cleanup: native task/channel/service/Image/record suite。 |
| G14 | 8 | 両host適合と旧C/生成client移行: stabilized RFC 0033と移行記録。 |
| G15 | 5 | 明示authority付きtrusted native/verified script plugin実行例とguide。 |
| **Total** | **96** | **15/15 complete** |

## 検証と出典

**2026-10-27**記録の[移行記録](../evidence/2026-10-27-m3-migration-9e78f2d.json)で、macOS **153 suite / 2,321成功 / 失敗0 / 既存ignore 1**、workspace fmt/Clippy/test/rustdocとfuzz 16 targetのformat/build/lint成功を確認しました。fuzz campaignは実施していません。固定入力の前後hashは一致しています。[Windows CI 40471721760](https://github.com/carimatics/lang/actions/runs/40471721760)は**2026-10-27 09:01:42 UTC**に全段階と**153 suite / 2,306成功 / 失敗0 / 既存ignore 1**で完了し、旧client 2ケースも成功しました。15件の差は記録済みのplatform固有filesystem/linker/DWARF/dynamic image検査で、等価なplatform別置換もあります。

一次project判断の最終更新日: RFC 0021 **2026-10-25**、RFC 0022–0026 **2026-10-27**、RFC 0027 **2026-10-27**、RFC 0028 **2026-10-24**、RFC 0029 **2026-10-25**、RFC 0030–0033 **2026-10-27**。SDK・出口・cost記録の日付は**2026-10-27**です。test/example sourceには公開/更新日の埋込みがないため、推測した日付を付けず、厳密sourceと実行確認日を**2026-10-27 JST**として記録しています。
