<!-- canonical-language: en; english-original: ../../rfcs/0040-bounded-debug-adapter-sessions.md -->

# RFC 0040: 上限付き debug adapter session

[English](../../rfcs/0040-bounded-debug-adapter-sessions.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-30
- Tracking issue: M4-D01–D05
- Supersedes: none
- Superseded by: none

## Summary

取得済み package source を使う上限付き stdio Debug Adapter Protocol adapter、`lang dap` を追加する。最初の実行 outcome は Copy VM program を準備し、replay せず実 source instruction の直前に止める。後続 delivery で step／inspection、両 tier-1 host の native launch、一つの実混在 VM／native stack を提供する。

## Motivation

source span と failure stack はあるが、Copy VM は現在 Machine を一括実行 call 内で消費する。ignored `target/m4-debug-vm-probe` は fuel 0 で main の元 call span、fuel 1 で main→answer の span を観測し、新しい通常実行は fuel 4 で 42 を返す。fuel failure は実行を破棄するため、この実験が証明するのは source／stack 情報であり pause／resume ではない。replay で効果を繰り返さず、実行状態を保持する。

## Goals

- 広い debugger 基盤の前に実 package の source breakpoint に到達する。
- stop 間で call 順、cell、program identity、source span、累積 control、元の効果を保つ。
- 上限付き read-only frame／variable／evaluate と、実 native／VM frame 対応を提供する。
- G7 完了前に D03／D04 の両 host native と混在 stack 要件を満たす。

## Non-goals

- 新 source 構文、bytecode format、component ABI、time travel、frame／value の書換。
- 任意 process attach、任意 debugger command、効果を持つ evaluate。
- JIT debugger registration、最適化済み variable の回復、native plugin sandbox。

## Guide-level design

stdio server は `cargo run -- dap` で起動する。client は initialize の後、明示的に許可する package を launch する。

```json
{"root":"/workspace/app","manifestPath":"Lang.json","mode":"vm","stopOnEntry":false}
```

`root` は明示した既存 local workspace directory。相対 manifest、任意 package／target 選択、全許可 dependency は既存 package graph guard に従い root 内に保つ。ancestor 検索、任意 executable、ambient source read は含まない。VM launch の初期範囲は既存 Copy source profile で、未対応 ownership／task／FFI profile は効果前に拒否する。この制限を capability／文書に明示しつつ、native launch と実混在 stack は M4 必須 outcome に保つ。

`initialize` は実装済み capability を返す。準備済み `launch` は応答して `initialized` を送り、configuration request が source breakpoint を設定する。`configurationDone` で実行を許可する。breakpoint 応答は検証済み実位置か明示的な unavailable 理由を返し、空行／comment 行は次の許可実行点へ進められる。path／URI、line／column の基数を negotiate し、IDE column は UTF-16 を使う。source request は disk 変更後も取得済み bytes を返す。

## Reference semantics

### Retained execution

既存 Copy VM の準備／run 経路を加算的 debugger Session に分ける。通常実行と debugger は同じ検証済み instruction 実装、argument／import preflight、call／return copy、並列 block 引数移動、control 順、failure 構築を使う。pause は次の instruction／terminator の課金・効果前に行い、同じ Machine を保持する。resume はその Machine を続け、fuel／host-call usage の reset、initializer 再実行、callback 重複を許さない。interruption／deadline 拒否は pause 公開より優先し、既存 deadline は pause 時間も含む。

通常 API の意味は変えない。debug session は clone 可能な snapshot ではない。完了／failure は終端で、後の resume は拒否する。Copy session の drop は buffer を解放する。新 guest allocation、算術、ownership、borrow、破棄、panic、並行性、compile-time 意味は導入せず、既存 instrumented profile の cleanup／accounting 契約を保持する。

### Stops and inspection

現在点は function／block／instruction identity、depth、元の任意 span を持つ。breakpoint は関数名の文字一致ではなく具体的実行点へ対応付ける。停止後の continue は現在点を一度だけ skip し、後の loop／再入では再度 hit できる。source step は許可済み位置と call depth を比較し、metadata がなければ位置を捏造しない。

frame は最内側を先にし、caller は進んだ PC ではなく child の元 call site を保持する。thread／source reference は上限付き target identity とし、source reference は target 終了まで不変の取得済み bytes を指す。frame／variable reference はさらに一つの pause epoch に束縛する。resume／restart／termination／関連 native 遷移で古い frame／variable inspection reference を無効化する。VM inspection は read-only。D02 はメモリ内・target 非依存 lowering sidecar で初期化済み・生存中の source local 位置を確立し、不明位置は推測せず unavailable にする。この metadata を無版の serialized field にしない。

evaluate の初期範囲は証明済み可視 identifier／field と上限付き scalar／Copy preview。guest／host function call、値変更、capability 発行、未初期化 cell の公開を許さない。paused state は所有 thread に置き、adapter は上限付き owned snapshot／command を交換し、同期 component／owner session を thread 間移動しない。別の pause flag は実行 safe point で止められ、cancel は元 control／cleanup 経路を使う。

VM 実装は検査済み lowering 環境を任意のメモリ内 sidecar に取得し、通常／debug lowering は同じ検証 MIR を生成する。初期化済み register の追跡は保持 debug frame だけで行い、caller の未返却 destination より前の停止 call 点を問い合わせる。scope／variable／evaluate query は元 worker で処理する。query ごとの上限は値 1,024 件、値ごとの copy cell 1,024、合計 copy cell 16,384、論理 preview text 1 MiB、visit 一千万回。評価 path は最大 32 component・4,096 bytes。source／frame／variable reference は target の reference 上限を共有し、生存 reference path も論理 debug index 32 MiB 上限に含める。拒否時に成功した variable list の一部を返さない。

### Protocol and lifecycle

DAP の Content-Length／ASCII header／UTF-8 body と seq／type／command／request_seq envelope を使い、LSP method envelope は使わない。全応答を対応付け、adapter message 番号を単調増加させる。configuration／step／inspection／restart／cancel／native capability は実装分だけ広告し、未対応 request は上限付き unsuccessful response にする。D01 は initialize／launch／configuration／source breakpoint／threads／stack／source／continue／disconnect を提供し、後続 capability は fixture と共に追加する。

launch は実行前に不変 source／semantic／portable state を取得する。別 target や古い pause の fact を返さない。disconnect／restart／cancel は所有 target／debugger resource を解放し、新実行を残さない。native process termination は明示的な trusted-process 契約に従い、言語 cleanup 保証は外部 process death からの回復を意味しない。

## Execution modes

- VM: 初期 Copy Session は実 interpreter と累積 control を共有する。source launch だけでは host callback 登録、FFI、capability を与えない。
- Native AOT: 明示 `mode: native` が一致 SDK／receipt と OS debugger を通じ、選択した compiled package の trusted 実行を許可する。D03 は macOS／Windows の実 source breakpoint を証明する。通常 Copy native は既存の trusted unmetered profile を保ち、VM instruction budget を帰属させない。
- Mixed stacks: D04 は実 trusted native↔VM 境界を実行し、一つの停止 invocation の frame／call-site 順を保持する。証明済み live descriptor／境界 identity と物理 frame を組み合わせ、別実行の結合や symbol 文字列による推測はしない。native backend／tool 選択と bridge は広告前に実行証拠を要する。
- 任意 JIT registration、広い ownership／task／source-variable profile は別 capability。要求した native session を黙って VM で代替しない。既存 component／C 実行動作は変えない。

### Native launch の具体化（2026-10-29 JST）

Windows LLVM 20.1.8 の必要 runtime は実 DLL import で特定した。host が `LLDB_PYTHON_RUNTIME` で対応 x64 Python 3.10 executable を選択し、debugger child だけに PATH／PYTHONHOME を適用できる。source/protocol input から runtime executable や debugger command は指定しない。

明示 native launch は同じ root／manifest 選択と、absolute `sdk` directory を受け取る。取得済みの許可 Copy program を私有 native package へ compile し、既存 package manifest／SDK snapshot と厳密 Rust release／commit／host を検証して、私有 executable と dSYM／PDB を debug link する。code・object・sidecar の owner は debugger 終了まで保持する。これは既存 format を使う一時的 host state で、新しい永続 artifact schema ではない。通常の `link-native` publication と再現可能 output は変更しない。

macOS の Xcode lldb-dap で実 source breakpoint／caller／step／正常終了が成功し、Windows には LLVM 20.1.8 の lldb-dap.exe が存在する。両 host 実行検証後にこの backend を広告する。adapter は許可 source breakpoint／thread／stack／step／continue／disconnect だけを転送し、shell／debugger command・任意 attach・memory write を受け付けない。source 応答は常に取得済み bytes を使い、物理 native frame の許可範囲外 source location は公開しない。初期 native scope／variable／evaluate は unavailable とし、native launch 時の capability 更新で hover 評価を無効にする。

この具体化は RFC 0001 の実験委任で採択する。既存の検証 snapshot と SDK／ABI 境界を再利用し、私有 debug file の寿命を明確にするためである。Windows の実行・tool 依存と混在 stack の対応は引き続き未検証条件であり、D03／D04 の gate 根拠に含める。

## ABI and compatibility

加算的・実験的 Rust／session と DAP API のみ。source、MIR 意味、portable schema、package／lock metadata、edition、公開 C／component ABI、native SDK 検証を変えない。frontend API に backend／process 型を露出しない。source-local sidecar は semantic／MIR identity と明示 host 上限を使う。後で永続 format を変える際は、先に版／移行を含む改訂を review する。既存一括 run API は結果、usage、failure 優先順を厳密に保ち、artifact 移行は不要。

## Diagnostics and tooling

compiler／package 診断と元 source を再利用し、stdout は framed DAP のみ。不正設定、権限外、古い ID、metadata 不在、未対応 profile、上限拒否、debugger／tool failure は ambient path を漏らさず報告する。LSP／formatter／linter 動作は変えない。source breakpoint／stack／value は現在 target の観測とし、文字一致や editor の過去成功 fact にしない。

## Performance and cost model

既存の有限 input／compiler／VM control を保つ。初期 adapter 上限は message body 4 MiB、header 8 KiB、breakpoint request 256、応答 frame 256、inspection reference 65,536、論理 debug-index bytes 32 MiB、query／index visit 10,000,000。複写 text／value preview と符号化応答を公開前に制限し、resource 拒否で部分成功 prefix を返さない。上限は host 方針／論理課金で、正確な heap／CPU 時間ではない。inspection は guest fuel を課金せず、resume で補充しない。trusted host callback は既存契約の協調的な境界を保つ。

## Security and trust boundaries

明示 launch root は許可 source read を与え、guest filesystem／network／environment／process／FFI 権限は与えない。native compile／debugger 実行は明示 trusted native mode、検証 package／SDK input、host 選択 tool を使う。protocol／source 文字列を shell／debugger command 断片へ渡さない。eval escape、任意 memory write、未検証 process attach は提供しない。frame／path／span／ID／件数／version／control 遷移を効果・出力前に検証する。取得後の source snapshot を保持し、editor overlay で権限を拡大しない。

## Alternatives

- 変更しないと一括 run failure と手作業 native debug のままで、G7 を満たさない。
- library observer だけでは stdio debugger の状態を保持できず、stop までの replay は callback と control を重複させる。
- 全て native debugger／DAP に委譲すると物理 frame は得られるが、この VM の source local／混在論理境界 identity を確立できない。対応箇所では上限付き host interface の背後で native tool を再利用する。
- 別 debug interpreter／文字由来 stack は初期実装が小さくても意味を重複し、効果／span／control parity を証明できない。既存 Machine の保持がより単純な十分設計となる。

## Implementation plan

D01 で本契約を採択し、最初の実 VM source breakpoint を提供する。D02 は step／初期化済み local／上限付き read-only evaluate／pause 権限。D03 は両 host native source debug、D04 は実混在 stack、D05 は lifecycle／cancel／不正／resource と両 host session の監査。doing は一つに保ち、新たな前提／scope 見積は実装前に公開する。compiler／runtime／source 意味は debugger vendor 選択から独立させる。

## Test plan

通常実行と停止／再開で、結果、usage、failure、callback 数を厳密比較し、効果前 stop、再帰、loop、block move、同一点の再 hit を含める。古い frame／source／variable ID、file 変更後 snapshot、private／workspace 外／未許可 path、不正 UTF-8／JSON／frame、有限上限、interruption／deadline 順、disconnect／restart／cancel を検証する。実 stdio client、macOS／Windows の実 native debugger と一つの混在境界 invocation を使う。既存 VM／native／C conformance と全 baseline を保ち、benchmark／fuzz campaign は推測しない。

## Open questions

RFC 0001 の実験委任で採択、2026-10-29 JST。検証済み Machine の再利用と明示 snapshot／権限境界は合理的で、有限 fuel 実験が効果前の元 span と入れ子 frame を示した。真の保持再開、source-local sidecar の費用、native debugger／tool、混在境界対応は未検証の delivery 要件として残る。採択で G7 を完了せず、未対応 profile を実装済みと扱わない。

## Sources

- [公式 DAP overview](https://microsoft.github.io/debug-adapter-protocol/overview) と [specification](https://microsoft.github.io/debug-adapter-protocol/specification): 公開／更新日なし。2026-10-29 JST に新規取得して確認。frame、message envelope、initialize／configuration、source／stack request は protocol fact で、初期 profile／上限は project の決定。
- RFC 0006（更新 2026-10-28）、RFC 0012（2026-10-28）、RFC 0013（2026-10-24）、RFC 0014（2026-10-28）、RFC 0020（2026-10-29）、architecture／roadmap、現 VM execution loop を 2026-10-29 JST に確認。code に埋込み更新日はない。
- ignored `target/m4-debug-vm-probe.log`、確認 2026-10-29 JST: fuel 0 は main bytes 50..58、fuel 1 は answer bytes 24..26 と親 call を観測し、新実行は fuel 4 で 42 を返す。有限の設計前証拠であり、保持 session／native debugger の結果ではない。
