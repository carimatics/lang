<!-- canonical-language: en; english-source: ../../rfcs/0013-service-byte-work-budget.md -->

# RFC 0013: Service byte-work budget

[English](../../rfcs/0013-service-byte-work-budget.md)

- Status: accepted
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-19
- Last updated: 2026-10-19
- Accepted: 2026-10-19
- Tracking issue: M2-N15, M2-N05
- Supersedes: None; elaborates RFC 0012's configured service admission
- Superseded by: None

## 概要

Instrumented実行contextに省略可能な累積service-work上限を追加し、VM／nativeで課金eventとusageを共有します。枯渇はterminalな制御failureとし、必須cleanupを続けます。N15で契約を決定し、N05で実装します。

## 動機

現在のControlはfuel・host-call・frame・live-cell上限を持ち、service-work設定はありません。Mapは候補観測・UTF-8検証・比較を実行前に制限し、bufferは可変長copy／scanを行います。命令fuelはpayloadの大きさを区別しません。これは実装の事実で、性能測定ではありません。

Mapの既存20 testが成功し、queryの保持／allocationやowner／出力の変更なしでworkを拒否する検証を含みます。Runtime admission境界の実現性を示し、global meterの実装完了を示すものではありません。

## 目標

論理guest callと両backendでmeterを共有し、拒否時の入力・span・cleanupを維持します。局所quotaとallocator会計を残し、host／driverから上限とusageを利用可能にします。

## 非目標

新構文／effect、compile-timeの計量、CPU時間の保証、任意のtrusted host codeの計量、命令fuelの変更、nativeのsandbox化。Allocator zeroingと余剰capacityはallocation上限で扱います。

## 利用ガイド

Control::with_limits(existing_limits).and_service_work(Some(64))、Usage.service_work: u64、実行CLIの--service-work Nを追加します。None／省略時は無制限です。未対応profileは有限予算をguest effect前に拒否します。

Entryが既に所有する3 byteの入力では、cloneのpayload-copy eventが3、続くUTF-8検証eventも3です。上限5では2つ目をServiceWorkLimit { limit: 5, used: 3, requested: 3 }で拒否します。そのbyteはscanせず、admit済み仮ownerと入力ownerを失敗cleanupで解放します。この例には無関係なservice eventを含みません。

## 参照意味論

### Contextとevent

外側のinvocationごとに0から開始し、論理call／core service callbackは同じmeterを引き継ぎます。独立instanceは別counterです。同期contextが所有し、ambientなglobal state、追加atomic／capabilityは導入しません。

Cost profile 1は論理event数で、実時間や物理memory trafficではありません。比較／検証が途中で終われる実装でもbulk eventの全長を課金します。

| Event | 単位 |
| --- | --- |
| Managed payload／graph node、Map lookup候補を1つ観測 | 1 |
| Core serviceの型付きdataを含むn byteのpayload copy | n |
| n byteのUTF-8検証 | n |
| 同じ長さnのbyte列の比較 | n |
| Headerで長さが異なると分かる列の比較 | 0 |
| 成功するBytes get／set、pushで追加するbyte | 1 |
| 固定header access、handle転送、allocator bookkeeping | 0 |

空のbyte eventは0です。Bytes pushでgrowする場合は、新storageへcopyする既存の論理byteも課金します。Clone／concatはcopyする論理長を課金し、allocator初期化と未使用capacityは除外します。Text lenは検査済みheaderを使うため0、Text変換は入力長の検証を課金します。Vec／Map／shared／arenaでは実際のpayload copy、観測、Text検査も課金します。Buffer handleの転送だけで参照先payload長を課金しません。

共通runtime境界で1回だけ課金し、wrapperで重複させません。別のservice走査で同じeventを再実行する場合は再課金します。既存Mapの観測／byte境界を再利用します。局所quotaを維持し、event前に局所／invocationの両checkを行い、両方成功後に両counterをcommitします。既存の局所拒否はerror分類を維持します。

同じcost-profile revisionで両backendのeventを一致させます。Event変更には明示的なRFC改訂と互換性注記が必要です。

### Admissionとfailure

既存guest-stepのpolling／fuel位置を維持し、service meterでpollを増やしません。先行する固定の入力／header／型／brand／bounds検査を行い、event単位とadmitted + requestedをchecked arithmeticで求め、有限上限と比較します。OverflowはCounterOverflow、上限超過はServiceWorkLimit { limit, used, requested }です。拒否したeventは一部も実行せず、service_workを増やしません。後続拒否、provider failure、recoverable data errorでもadmit済みworkは返金しません。無制限でも計数とoverflow検査を行います。

命令fuelはservice eventより先に課金し、その位置での割込み／deadline／fuelの優先順位を維持します。Admitしたscanが発見したdata errorは通常どおり分類し、拒否したscanで後のdata errorを発見することはありません。

ServiceWorkLimitは構造化した共通execution／control failureで、core Result error、guest panic、invariant poisonにはしません。数値と最初のFunctionId／source操作spanをgateway／reportまで保持し、cleanup secondaryの順序を維持します。必須retirement・結果破棄・host resource解放はmeterを参照／課金せず、既存primary failureを置換しません。

Eventが依存するallocation／copy／scanより先にadmitします。複数段階のintrinsicでは、後の拒否より前にadmit済みprovider prefixが存在し得ます。Allocator要求／累積会計は巻き戻しません。仮resourceをすべて解放し、未admitのeventで結果公開、source placeの置換、入力token消費をしてはいけません。必要なfallible inspection／admission後にのみ入力をmoveし、準備済みpublication／retirementを使います。VM／nativeでprovider prefixと所有権の結果を一致させます。

後の拒否で入力handleが古くなったり仮resourceが未登録になったりする場合は、既知の依存costをmutation／allocation前にまとめてadmitします。特にgrowするpushの既存byte copyとappend、clone／concatのcopy全長はstorageの確保／置換前にadmitします。Batch全体の拒否では両counterを変えず、合計requested単位を報告します。可変inspectionでprefixを課金できるのは、入力の所有権と仮resourceのcleanup義務を維持できる間に限ります。

### その他の意味論

型、推論、評価順序、loan、arena brand、並行性、capabilityは変えません。Constant／compile-time実行は対象外です。外部入力にはstorage／UTF-8の検証が必要なままで、予算拒否によって未検査dataの観測権限を得ることはありません。

## 実行モード

Instrumented VM／native AOT／owned JITはeventとservice_workを共有します。有限設定では、effect前に検証済みunit全体の完全な計量を確認します。VM／native共通のcapability判定で、未使用関数を含む全関数を調べます。未計量のservice eventが必要なunitは拒否し、対応prefixだけの実行やVMへの置換をしてはいけません。動的service targetは解決して検査するか拒否します。既存のtrustedなopaque host callbackの適用除外は維持します。Uninstrumented Copyは別API／profileです。

段階的な実装では、完全に計量済みのservice familyだけを使うunitを受理できます。Backend全体の対応完了を意味せず、N05では全managed family、全payload、driver／reportの移行、完全なconformance matrixが引き続き必要です。Text tokenの復号はUTF-8検証を行うため、その後の操作がheader参照だけでもscanを課金します。

Opaqueなtrusted host callbackはhost-call課金を残し、引数から任意workを推定しません。同じcontextのowned serviceを使うcallbackはmeterを引き継ぎます。FFI／componentの伝播は明示的なversion付きcontextに限り、このRust APIで別component内部を自動計量しません。

## ABIと互換性

省略時の無制限実行は、定義済みcounter overflowを除きsource挙動／fuel位置を維持します。構文・型・package・edition・typed-source artifactの変更は不要です。上限はControlのprivate fieldに置き、既存Limits literalを維持します。全fieldを列挙するRust Usage literalはservice_workの初期化かDefault利用へ移行します。

計量付きbuffer entry pointは追加APIで、既存buffer methodは課金しないhost／cleanup用の挙動を維持します。BufferのFault::ServiceWorkが共通の構造化failureを保持するため、Faultを全列挙するRust matchにはこの分岐が必要です。この段階では新しいserialized表現を追加しません。

既存identityを流用せず、数値付きServiceWorkLimitを共通の動的failure domainへ追加します。現在のartifactはfailure elaboration前のdataを保存し、動的execution causeを含みません。N05でserializerを再監査し、新usage／errorをencodeするversion付きreport／interfaceにはformat更新かsupport negotiationと移行fixtureを要求します。同じ閉じたschema versionへtag／fieldを追加してはいけません。Private revision-1 native packetは不変で、Rust context layoutは公開C ABIではありません。FFI／component mappingのversionは独立です。

## 診断とツール

Failure identity、source span、上限、admit済み／拒否単位を報告し、論理guest frameとsecondary順序を維持します。Builder、CLI option、usage、Rust移行を文書化します。Parser／formatter／推論／LSP grammarは変えず、DAPは既存の論理chain／spanと新failure dataを表示します。

## 性能とコストモデル

Eventごとのcounter checkにguest allocation／registryは不要で、既存serviceの反復処理を境界に使います。Usage／failure型のsize増加はmetadata／report予算へ反映します。Overheadや速度の優位は主張せず、N05と既存performance gateで代表的な混合serviceを測定します。

## セキュリティと信頼境界

Fuel・memory・局所inspection quotaを補完し、CPU work全体、allocator初期化、opaque host callback、必須cleanupまでは制限しません。長さはcheckedとし、allocator／host grantを明示します。Native callbackはtrustedかつpanic-containedのままです。枯渇でcleanupを妨げたり、poison後の再開を許してはいけません。

## 代替案

- 変更なし：N05の設定可能なwork要件が未実装のまま。
- Libraryだけのcounter：context統合なしではinvocation上限／usage／cleanupを共有できない。
- Byteに重みを付けた命令fuel：acceptedな1 stepのfuel／poll契約を変更する。
- Intrinsic単位の保守的見積もり：単純だが入れ子Map／graphを見落とすか、未使用capacityを課金しruntime policyを重複させる。
- 全体の事前plan：provider admission全体をatomicにできるが仮plan／走査が増える。本案はadmit済みprefixを許し所有権を維持してcleanupする。

## 実装計画

1. N15で既存service／failure境界と互換性のevidenceを使って本RFCを決定する。
2. N05でmeter／builder／usage／failure dataとdriver設定を追加し、bufferと全managed serviceを局所quotaを残して接続する。未完のprofileは有限設定を拒否する。
3. N05で全成功／拒否／panic／cleanupと移行を検証する。N06／N07の実target object link／runも必要なままで、frontend／backend結合や新serialized IRは導入しない。

## テスト計画

- 0／厳密／1不足／u64 overflow。拒否したeventでcounter／入力を変えない。
- 複数段階prefix、grow OOM、guest call間のmeter共有、必須cleanupの課金免除。
- Buffer／Vec／Map／shared／arena全payload、入れ子Text、Copy／所有値／置換、provider prefix。VM／nativeのusage、failure数値／span、cleanup順序、allocation漏れ0を比較する。
- 局所拒否、不正外部入力、割込み／deadline／fuelの優先順位、panic／poison。
- 省略時の無制限互換性、Rust literal移行、閉じたschema、未対応profileの事前拒否。
- 既存parser／MIR／artifact／platformと代表性能検証。このhost optionだけでparser snapshotは変更しない。

## 未解決事項

任意の後続課題はevent check overheadの測定と将来のcost profileです。N05で全eventとrollbackを検証してから実装完了とし、Map 20 testは実現性の裏付けに限定します。

## 判断レビュー

2026-10-19 JST、実験段階の委任に基づきacceptedとしました。Blockingな設計課題はありません。共通runtime eventで式の分岐を防ぎ、局所check後にcounterをcommitし、既知のgrow／copy依存costをhandle変更前にまとめてadmitし、terminal failure後も必須cleanupを行えます。Rustの移行と閉じたschemaの規則により、source型やfuelを変えずに互換性を扱います。

未検証の仮説は全eventの網羅、全仮resource経路の安全な統合、overheadの測定です。N05で検証し、実装完了／安定化は主張しません。既存Map testは実現性の裏付けに限定します。後で契約を変更する場合は、実装前に明示的なRFC改訂／後続RFCを行います。

### 受理条件の改訂 — 2026-10-19

Buffer統合prototypeのレビュー後、実験段階の委任に基づきacceptedとしました。Unit全体で受理を判断することで、未対応programをeffect前に拒否する規則を保ち、独立して検証可能な進捗を得られます。最後のfamilyの計量まで全unitを拒否する案と比較して、cost契約を弱めずに受理するprogram集合を限定します。全関数の検査により、実行された経路に依存しません。共通判定と有限予算・未使用関数・failure span・cleanupのend-to-end testを実装で要求し、prototypeの実現性だけではN05を完了しません。Source構文、serialized format、cost-profile eventは変更しません。この明示的な改訂をproduction統合commitに先行させます。

## 情報源

規範部分はprojectの決定で、以下はlocal実装の事実です。すべて2026-10-19 JST確認。外部言語／性能の主張は使いません。

- [RFC 0007](0007-source-ownership-and-core-values.md)：更新2026-10-18、[RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)：2026-10-17、[RFC 0012](0012-native-ownership-continuations.md)：2026-10-18。
- [共通control](../../../crates/runtime/src/execution.rs)：最終commit 2026-10-19。[Buffer](../../../crates/runtime/src/buffer.rs)、[Map work](../../../crates/runtime/src/map.rs)：最終commit 2026-10-18。
- [Map storage test](../../../crates/runtime/tests/support/map_storage.rs)、[mutation拒否test](../../../crates/runtime/tests/support/map_mutation_failures.rs)：最終commit 2026-10-18。着手前command cargo test -p lang-runtime --test map：20件成功、0.06秒。
- [Failure domain](../../../crates/mir/src/unit/ownership/failure.rs)、[artifact codec](../../../src/artifact.rs)：最終commit 2026-10-18。N05でserializerを再監査します。
