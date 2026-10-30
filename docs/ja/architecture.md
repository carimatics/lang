<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャbaseline

[English](../architecture.md)

現在の契約を`28160e3`と照合し、2026-10-30 JSTにレビューしました。Milestone 0–4のexit gateは完了し、M5は[delivery board](milestone-5.md)で進行中です。[RFC 0043](rfcs/0043-editions-and-compatibility.md)がedition・非推奨方針を定めますが、実験sourceやformatの安定化ではありません。公開component C v1と旧生成clientはRFC 0033に従い安定化し、source構文・native Rust ABI・bytecode/SDKの独立formatは実験段階です。[M4機能ガイド](milestone-4-capabilities.md)と[M3機能ガイド・15 gate監査](milestone-3-capabilities.md)にcommand・両host証拠・制約をまとめます。M2の[機能ガイド](milestone-2-capabilities.md)と[過去のarchitecture checkpoint](architecture-history.md)も保持しています。

## 目標

1つの言語で、memory safety、明示的な割り当てと外部authority、有用なtooling、再現可能なself-hostingへの道筋を備えたnative applicationと埋め込みscriptを実現します。Native performance、生産的な高水準機能、data-race safety、複数frontendはプロダクトの目標であり、現在の実装がすべて提供済みという意味ではありません。

初期実装の対象はmacOS AArch64とWindows x86-64です。安定したnative言語ABI、tracing GC、任意のnative hot reload、分散実行、公開package registry、第二frontendは約束しません。

## コンパイラpipeline

```text
UTF-8 source + explicit source-root authority
  → lossless syntax / recovery / formatter
  → resolved source HIR
  → checked Copy or ownership HIR
  → independently validated typed MIR
      → versioned portable program → verifier → VM
      → Cranelift → JIT or native object → system linker
```

Typed MIRが意味論の契約です。Backendは評価順、整数の挙動、所有権、cleanup、failure identity、spanを保持します。実装にはCopyとownershipの別表現があり、「共通MIR」は単一の物理layoutやartifact versionを意味しません。Frontend APIはCranelift、LLVM、VM、platformの型に依存しません。

| 層 | 実装済みの責務 | 主なcode |
| --- | --- | --- |
| 診断とsyntax | 不変のsource snapshot、検証済みbyte span、構造化diagnostic/fix、lossless arena tree、上限付き回復、canonical format | `crates/diagnostics`, `crates/syntax` |
| Source semanticsとHIR | Root内のmodule、visibility、名前、有限の具体型、初期化／move、lexical loan、arena escape検査 | `crates/semantics`, `crates/hir` |
| MIR | Typed CFG、独立した型／move／loan／region検証、明示的failure／cleanup elaboration | `crates/mir` |
| Portable program | 上限付きencode/decode、version dispatch、source snapshot、独立検証 | `crates/bytecode` |
| 実行 | 共通のallocation/storage/core service、failure report、control、VM実行、native adapter | `crates/runtime`, `crates/vm`, `crates/codegen-clif` |
| Toolingとdriver | Query invalidation、UTF-16 diagnostic mapping、CLI/REPL/LSP、linker、SDK packaging | `crates/query`, `crates/lsp`, `src` |

Source名は診断データでありfilesystem authorityではありません。Source loaderはcache lookup/read前に明示的rootとoverlayを検査します。Snapshotは編集やREPL reset後も有効です。Parser、semantic、MIR、decoderの上限は別々のadmission境界であり、エラーを部分検証済みの実行可能プログラムに変えてはいけません。

## Sourceとmemory model

[RFC 0002](rfcs/0002-prototype-control-flow.md)と[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)はCopyの基礎を定めます。Wrapping `i64`加算、符号付き比較、左から右の評価、function/module、record/enum、pattern、制御フロー、明示的returnです。[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)が有限core型と所有権を、[RFC 0009](rfcs/0009-whole-sum-ownership-extraction.md)がOption/Result全体からの抽出を追加します。

- Scalarと全fieldがCopyのaggregateはCopyです。Bytes、Text、Vec、Map、shared、weakは明示的な所有権を持ち、cloneも明示的な操作です。
- Loanはlexicalで、owner全体の粒度で追跡します。Partial move、通常loanのescape、reference fieldは導入しません。Mutable replacementは公開前に準備し、受理済みfailure/cleanup順序を保持します。
- Bytes/Text、有限Vec/Map特殊化、shared/weak/arena操作は提供されたallocatorを使用します。Allocation/growthは無効なownerを公開せず失敗できます。UTF-8検証、index検査、厳密なkey equalityはruntime操作です。
- Shared strong graphは公開前にcycleを検査し、weak edgeが所有権cycleを切ります。Lexical arenaは同一brandの循環storageを許し、escapeや異なるbrandへの置換を拒否し、古いgenerationを検査します。
- 有限のbuilt-in特殊化は実装済みです。User generics、trait、closure、lifetime parameter、通常functionのresult推論、suspensionを越えるborrowは将来の設計対象です。

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)はより広い所有権方針です。そのsource subsetとruntime serviceは実装済みです。logical-task concurrencyは別途RFC 0021–0026で実装しており、すべてのhost APIのSend/Syncを保証しません。

## Failure、allocation、実行control

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)は、実際の初期化順の逆順によるexactly-once cleanup、return escrow、primary/secondary failure、poison/reentry、allocator authorityを定めます。回復可能な失敗には型付きResultを使い、[RFC 0014](rfcs/0014-explicit-source-panic.md)が終端となるi64 code付きsource panicを定めます。言語cleanupはnative unwindingではなく、明示的status/control flowを使います。

Guest memoryはprovider identityとlive/cumulative上限を保持するfallible accounting allocatorを使います。Host metadataとframe/retirement storageには別の準備上限があります。必須retirementはcontrol枯渇後も継続し、reportの打ち切りでcleanupを打ち切りません。信頼済みhost panicの封じ込めは文書化されたRust/C adapter境界に限ります。Foreign unwinding、abort-mode panic、不正native pointerは保証外です。

計測付き実行はinstruction fuel、interruption/deadline、host-call、frame/live-cell、[service-work](rfcs/0013-service-byte-work-budget.md)のaccountingを共有します。1つの論理invocation内でcallとservice viewがcounterを共有します。対応するeffectの前にadmissionし、未対応の有限controlは実行前に拒否します。Service workは指定した論理eventの計数であり、すべてのCPU時間、allocator処理、任意の信頼済みcallbackを測るものではありません。

Component work（2026-10-24 JST、[RFC 0029](rfcs/0029-component-adapter-work.md)）: `Invocation` の準備時に作った元 ServiceWork を Machine と全 service へ渡します。scoped wire Session は canonical value の観測・copy・UTF-8 を同じ counter へ課金し、scope 終了/unwind で以前の対応を復元します。全 function/未使用 import の有限対応検査にも現在の準備 Budget を適用し、未対応 method flag は実行前に拒否します。root 準備中の work failure を latch し、再試行/後続 control で置換せず、新 root で counter を初期化します。codec/admission は既存累積上限も維持し、必須 cleanup は非計量、destructor 失敗は元 primary の後に保持します。Session は同期 counter を保持するため Send/Sync ではありません。

通常のCopy native facadeは信頼済み・同期・unmeteredです。Ownership native実行は[RFC 0012](rfcs/0012-native-ownership-continuations.md)の準備済みmanaged frameとcompiled continuationを使い、host driverはMIR評価やVM fallbackをせずnative addressをdispatchします。Guest recursionは上限付き論理frameを使います。上限やsandboxを説明するときに、これらのprofileを混同してはいけません。

## Portable形式とnative形式

Versionの名前空間は独立しています。Component prototypeのschema番号は`LANGBC` source programのschema番号ではありません。

| 形式 | 現在の選択とauthority |
| --- | --- |
| Source program schema 1 | RFC 0006のCopy program |
| Source program schema 2 | RFC 0011のownership program。明示的panicとC metadataを含まない |
| Source program schema 3 | RFC 0014のownership program。明示的panicを含み、C metadataを含まない |
| Source program schema 4 | RFC 0017のC対応ownership program。明示的panicの有無は問わず、metadataはC実行authorityを与えない |
| Source program schema 5 | RFC 0028 の Task/Component MIR・完全 IDL/map・元 snapshot/span・独立 task/binding proof。metadata は authority を与えない |
| Private ownership native revision 1/2/3 | RFC 0012・0014が定める通常／C対応／明示的panicの規約 |
| C profile 1とobject bridge 1 | 外部の有限C値／descriptorと、それとは別のprivate per-image binding slot |
| Native record schema 1 | RFC 0018の元objectとcompiler record。厳密なportable bytes/build/targetに結び付く |
| SDK/program manifest version 1 | RFC 0020の上限付きfile table、identity、role、size、SHA-256 |
| Native record schema 2 | RFC 0030のtask/component record、元schema-5 programと厳密なbuild/targetに対応 |
| SDK/program manifest version 2 | RFC 0031のinstalled task package |
| SDK/program manifest version 3 | RFC 0032の明示host bindingを持つinstalled component package |
| Component IDL / C ABI / LCMP / LCVL / LCSG | RFC 0027・0033: 1 / 1 / 3 / 1 / 1、別々のversion domain |

Decoderは未対応version、不正shape、不正span、resource枯渇を拒否します。Ownership artifactは検証済みのcleanup前表現を格納し、必要なproof/elaborationを独立して再構築します。Native recordは元code metadataを保持し、startupでmachine codeを再生成しません。

Hash/build identityは不一致を検出しますが、悪意あるSDKやnative objectを認証しません。Portable artifact内のsource snapshotは診断用であり、file再parse、import解決、native code load、allocator取得のauthorityではありません。

## C ABIとnative SDK

Component C v1は[stabilized RFC 0033](rfcs/0033-component-v1-compatibility.md)に従い安定化しました（2026-10-27 JST）。公開32 entry・厳密size/version・status・所有権/失敗契約を固定し、旧header/生成clientは`9e78f2d`の実macOS/Windowsで成功しました。[移行記録](../evidence/2026-10-27-m3-migration-9e78f2d.json)が来歴と全検証を保持します。両host isolated SDK 7 profileも成功済みです。非互換C拡張には別version/namespaceを必要とし、private Rust/SDK ABIは厳密build一致と再buildを要求します。対応しない保存formatは元sourceと一致toolで再encode/compileします。

安定したcomponent-v1 query `lang_component_v1_get_api`は、厳密size/version検査済みで全32 entryが非nullの単一の不変tableを公開します。VM/JITのsource/artifact呼出しと厳密SDK AOT Imageは、実grantと保持ownerを伴ってこの公開surfaceを使います。callback資源退役は元の生きたholder/table/owning receiptを検査し、result/report snapshotを元preparation Budgetで事前計上し、元の事前確保済みcleanup Sessionを使います。制限されたretirement loanはparent bridge状態を保存・復元し、guest再入なしで割込み後の後始末を許可します。borrowed・移動済み・stale・使用中claimは副作用前に拒否し、異なる実装のownerもopaqueなまま元destructorで退役します。receipt退役で再試行を防ぎ、失敗後もbacking解放を続け、Sessionのpoisonでnativeの成功result公開を防ぎます。生成CはVM/JIT/AOTでこれらを実行します。E01の完全query、I08の両host隔離起動、B01の全適合、B02の旧client移行は完了し、RFC 0033の公開C v1契約を安定化しました。C/保存layoutは変更しません（RFC 0027、2026-10-27 JST更新）。

SDK所有component ImageはC gatewayのtrusted AOT mode 2を実装します。program-v3/component manifestの有界検査と厳密host/SDK一致を確認してからprivate Imageを参照します。factoryは不変のsemantic/native/bindings snapshotと実code resolver/ownerを保持し、instance identity・grant/provider・resource loan・元のcontrol/task/allocator/準備を別々に検査します。記録されたexportだけを実行し、VM/JITへfallbackしません。result bodyとinstance stateを退役してからexecutable ownerを解放します。完全C table、生成Cのscalar/task/reentry/resource fixture、両host適合、隔離起動は[M3完了監査](milestone-3-capabilities.md)で完了しています。

実装済みRFC 0008境界は、生成C11 headerとtyped adapterを介してscalar、Unit、有限Copy aggregateを許します。Managed tokenやprivate source layoutをC値として公開しません。C importにはlexical `unsafe`と明示的なhost binding/grantが必要であり、`unsafe`はownership/loan/region検査を無効化しません。

共通preflightはdescriptor全体、signature、grant、address、alignment、canonicalなactive fieldを検証します。未初期化であり得る通常のC paddingをRust byte sliceとして読みません。実際のnative objectの有効性、code/provider lifetime、同期owner-thread利用はcallerの責務です。成功した実行、cleanup、検証の後にだけoutputを公開します。Callbackは元contextを保持し、拒否されたreentry/poisonは以前のfailureを保持します。

[RFC 0015](rfcs/0015-c-native-object-bridge.md)は明示的private binding slotでlinked C export/importを接続します。[RFC 0018](rfcs/0018-native-compiler-records.md)、[RFC 0019](rfcs/0019-owned-native-entry-runtime.md)、[RFC 0020](rfcs/0020-native-sdk-and-startup.md)はsource-native compilation、元entryからのstartup、分離したSDK installationを提供します。

`lang native`は上限付きprogram directoryを公開します。`lang link-native`はSDK/program snapshotと厳密なRust toolchainを検証し、同梱libraryでrustc/system linkerを駆動して、新規executableを排他的に公開します。Consumerのlink時にCargo、元repository、registryは不要です。明示的C hostは引き続きcontext、grant、allocatorを提供します。SDKには指定Rust release/commit、native linker、C入力用C11 compilerが必要であり、安定したC専用runtime配布ではありません。[SDK利用方法](native-sdk.md)を参照してください。

## Native backendとdebugging

`Cargo.lock`のCranelift 0.136.3が初期JIT/AOT backendです。Mach-O/AArch64とCOFF/PE/x86-64はtarget-neutralなlink request、決定的discovery、direct/response-file invocationを使います。JIT mappingのownerはcall/code borrowの終了まで存続します。LLVM、追加target、同梱LLDの導入には根拠と限定した判断が必要です。

Source function/instruction rangeはmacOS DWARF/dSYMとWindows CodeView/PDBまで保持されます。Windows x64 source objectはunwind tableも持ちます。[M2 debug監査](milestone-2-capabilities.md)は、両hostの実際のLLDB/CDB breakpoint、Copyのline step/caller、ownedのinstruction step/物理runtime callerを記録しています。

Read-only revision 1のdebug descriptorで、dispatch時にallocationせずownedの生存論理frame IDとsource spanを公開します。[両hostの実機検証](https://github.com/carimatics/lang/actions/runs/39427222132)で3段のframeと親のcall siteを確認しました。旧G18根拠だけではRFC 0012のこの要件を満たしていませんでした。

このM2の根拠ではsourceを残した手動のmatching-SDK debug linkを使っています。既定の`link-native`はdebugger sidecarを公開しません。後述するRFC 0040のDAP profileはCopy VMのinspection／step、private sidecar付きnative launch、明示許可したVM／native混在stackを追加します。native value inspection、ownership continuation step、最適化後のstep、JIT debugger登録はこのprofileの範囲外です。

[RFC 0040](rfcs/0040-bounded-debug-adapter-sessions.md) は上限付き stdio framing と許可済み不変 package snapshot を使う `lang dap` を追加します。Copy VM debugger は通常 interpreter を所有 worker に保持し、instruction の効果・課金前で停止して、元 control と caller span を保ちます。初期 source breakpoint、stack／source query、entry stop、continue、disconnect を実装し、Copy native launchは実装・両host検証済みで、明示許可した混在host stackとlifecycle分離も実装・両host検証済みです。通常CLIのlaunchではhost importを許可しません。

VM adapterはsource step-in／over／outと、上限付きread-only scope／variable／identifier・field・scalar評価に対応します。通常loweringは検証MIRやportable bytesを変更せず、target非依存のlocal／register環境を追加取得できます。初期化済みregisterの追跡は保持debug frameだけで行い、callerは停止したcall点から検査します。queryは元workerで処理し、再開時にframe／variable referenceを無効化します。native／混在modeでのinspectionは、後述するより限定的な範囲に対応します。

native DAP backend は同じ package admission を使い、検証した private SDK／program／code／debug sidecar の owner を debugger 終了まで保持します。実 native package の意味 bytes と受理 program を比較し、検査済み source line breakpoint と実行 control だけを転送して、未受理の物理 frame source location を省きます。source 応答は取得済み bytes を保持します。native value inspectionはunavailableです。source 39473b1のproduction native DAPは両hostで成功し、source 759bae0はnative-debug CIの3環境で生存中の物理／論理frame対応とfailure cleanupを証明します。

信頼済みRust embeddingが混在native importを明示登録し、既定CLIはgrantしません。協調native pauseは元VMとcallbackを同じworkerに保持し、上限付き完全物理PC suffixを照合して、native prefixと元VM callerの順に表示します。Restartは旧targetを解放して明示launch入力を再captureします。異なるtarget reference blockで旧thread／source／frame／variable IDによる置換targetへのアクセスを防ぎます。Terminate／disconnectは元interruption／shutdown経路を使い、terminatedは一度だけ通知します。DAP cancelは完了済み／未知の要求の取消に応答し、実行を停止しません。同期準備中はqueueされた取消を処理できません。任意の信頼済みcallbackは協調的です。有限上限とlifecycle契約はRFC 0040を参照してください。

mixed bridgeは元VMのHostCallで内部停止し、source frameを保持して同じworkerで登録callbackへ入ります。協調的pauseから渡すのはowned物理／source frame dataだけで、借用中Machineはthreadを渡りません。callback return、割込み、scope退役で観測を失効させます。native pauseのvalue／stepはunavailableとし、元VMのcontrol・結果・effectを実行契約に保ちます。[完了済みM4ボード](milestone-4.md)にD04B／D04のbridge・両hostのfailure証拠とD05のlifecycle検証を記録しています。

[M2 source/Rust/C測定](benchmarks.md)は名前付きの不変revisionに結び付き、大きな性能差を示しています。測定完了の根拠であり、native同等性能や速度保証ではありません。今後の最適化判断にはこのworkloadを使い、過去のscalar microbenchmarkでsource programの費用を代用しません。

任意のhost point recorderはexecution Control／configuration identityに含まれ、Send／Syncを維持します。元Copy VM／ownership VM／通常native ownershipのcharge pointはfuel課金成功後だけ、予約済み有限storageと既存spanを使って記録します。`lang profile run`は頻度、決定的sampling、元usage、host entry経過時間を返し、CPU帰属とは主張しません。native engineはownership JITで、native Copy／task／C／component profileは拒否します。観測上限超過でもguestを中断／再実行せず、成功report公開を拒否します。閉じたruntime readerは4 MiB上限内で有限sample合計と取得済みsource範囲を検証します。

`lang profile allocations`はfreshな元allocatorに上限付きmetadata ledgerを付け、logicalな0-byte owner、admitされた失敗、resize確保、retirementをborrowed view経由でも一度だけ記録します。完全なentry-return snapshotを実counterと照合し、閉じたreaderもgeneration、request、全counterを独立にreplayします。要求guest storageには必要なguest-controlled metadataを含み、物理heap／RSSではありません。失敗entryは元診断と完全なfailed-outcome reportを保持し、観測超過はguestを変えず公開を拒否します。snapshotはlive ownerを保持でき、allocator teardownを意味しません。VMとownership native JITはimportをgrantせず一回実行します。完全local／両hostのreport検証をboardに記録しています。

`lang profile rebuild`は現在の不変package／workspace入力と過去のcanonical lockを比較します。native比較では現在の独立SDK snapshotと選択rustcも検証し、過去世代receiptを保持lock／SDK bytesと照合します。構造化相対keyでsource／dependencyの来歴、追加／削除、workspace選択、target／compiler identity、SDK／rustc変化を保持します。閉じた4 MiB readerは上限付きsnapshotから全差分を独立に導出します。この読取専用観測はpackageの型検査／実行／公開、過去recordの認証、cache出力再利用の保証を行いません。

## Signed toolchain distribution

[RFC 0042](rfcs/0042-signed-toolchain-installation.md)は固定local trust設定、上限付きthreshold署名channel catalogue、expiry、commit済みrollback floorを持つ明示root managerを採択します。metadata認証をpayload読取より先に行い、正確なfileとcompiler／SDK identityをactivation前に確認します。不変generationとatomic state置換は失敗時に旧active版を保持し、認証済みmetadata floorは転送前に進み得ます。root置換は新しい明示trust installationを要求し、初期formatはTUF互換を主張しません。F02の厳密verifier／読取専用CLIは両hostで検証済みです。F03のlocal installは保存receiptを再認証し、payloadをstream／hash、SDK表と上限付きcompiler identity subprocessを照合してatomic activationします。両hostの実SDK fixtureでinstall後のsource実行、失敗／process中断後の旧active保持と同sequence retryを確認しました。F03は`28bc8df`で完全local baselineと両host CIに成功し、F04は上限付き検証済みHTTPS、channel操作、検証付きrun／which、marker付きuninstall／復旧を追加し、実TLS、installed compiler中断fixture、完全local baselineに成功しました。F04の両host全体／native CIは`d2fb67b`で成功し、F05の配布生成／journeyは`bf14faf`で全体／native CIに成功してG9を完了しました。F06は`76c3698`で明示unlockによりlock所有を修正しました。最終統合journeyとM4終了監査は`0b6591d`で完了し、guest capabilityは変更しません。

信頼済みrelease producerは既存host SDK、別経路で信頼したroot、上限付き署名入力を受け取ります。正確なchannel payload bytesへ署名し、各source fileをstream／hash、private stagingで通常installerを再利用してSDK／compiler identityを確認してから非置換公開します。公開物はmetadataとobjectのみで、署名情報を生成・保存しません。同一compiler identityを保つ2種類のSDK直列化によって、実際にbundle generationを更新します。local journeyでは導入済みSDKでのnative link、3 channel間のgeneration共有、旧compiler実行中のupdate／uninstall、期限切れ保存receiptも検証し、Windows CIでは実行中exeのuninstall拒否／cleanup延期と終了後の復旧を検証しました。

## 診断、REPL、editor tooling

CLI は check、source/artifact 実行、portable compile、native package/link、transactional REPL、同期 LSP prototype を備えます。format は共通 CST API と LSP document formatting から利用できます。[RFC 0010](rfcs/0010-owned-repl-previews.md) と [RFC 0016](rfcs/0016-indirect-repl-previews.md) が有界 result preview を定めます。output 公開前に result を retire し、session は source 定義を保持しても mutable guest owner を保持しません。

Query graphは推移的consumerを無効化し、編集、close／reopen、新しい処理の開始後に古い結果の公開を拒否します。LSPはsource analysis、不変overlay、UTF-16 mappingを再利用し、上限付きframing／outputでlifecycle、full synchronization、診断、format、監視moduleのrefreshを実装しています。package profileはdependency item、case、parameter、local、record fieldのdefinition／referencesを追加します。[RFC 0039](rfcs/0039-package-editor-snapshots.md)が許可input／overlay epoch、元URI／span対応、応答上限、検証editの前提を定義します。実装済みDAP profileは前述の説明とRFC 0040に記録しています。

package completion は上限付きで決定的な module／import／case 名、lexical binding、宣言型 record field を追加し、不完全な修飾名／member slot は一時的な通常 parser／binder overlay で扱います。probe 診断／位置は公開せず、edit は元 UTF-16 範囲と epoch／version data を使います。曖昧／未解決／未対応 receiver 式の field は推測しません。

full semantic token は固定 legend と現在の binding index を使い、不正 source には字句 fallback を使います。CR／LF で分割し、非重複 UTF-16 delta と epoch result ID を返します。semantic-token refresh 対応を宣言した client には source／overlay／watch 無効化後に refresh を要求し、range／delta request は未対応です。

prepare／rename は dependency file を含む source 識別子の検証 edit を提案します。編集する全 file が open・version 付きであることと、client の `workspaceEdit.documentChanges`、複数 file では `transactional`／`textOnlyTransactional` の failure handling が必要です。許可済み全 input bytes の再確認、置換後の再解析、resolved HIR 全体と型の対応検証を行い、衝突、import alias、module、外部 ABI 宣言、古い input を拒否します。提案を disk に直接書き込みません。

literal code action は同じ input／version guard と package linter の rule を再利用し、client が宣言した kind family に限定して提供します。`source.format` は source 位置に依存しない HIR／型の証明後に文書全体を整形し、`quickfix` は一意に選択した未読 binding を rename 証明で `_` prefix にします。initializer と代入の効果を保持し、guest code を実行せず、曖昧／衝突する fix は edit を返しません。

RFC 0044の`package::fix`はentryと全宣言testの上限付き複数root rename証明で元spanを合成します。CLIはpreviewが既定で、`--apply`は証明と同じ固定graphから新しい非公開planを作ります。formatterと共有する公開境界は全source・manifestのidentityとbytesを固定し、別directory authorityを検証して選択source置換を全件stage化し、再検証後file単位でcommitします。依存は証明のみの対象です。previewと適用成功report両方のsizeを書込み前に検査し、後半I/O失敗時は適用済みprefixを報告して未公開stageをcleanupし、rollbackしません。旧source移行は未実装です。

RFC 0044の`package::migration::Preview`はbyteだけを受ける旧scalar readerで、通常compileから分離します。旧lexical scope・関数全体の名前一意性・scalar型・return経路を検証した後、全体でfreshな識別子へ対応付けます。元tokenのoffset検査と型付き解決構造の比較で、private呼出し・値・型・制御フロー順を保持します。previewはfileを開かずguest codeも実行しません。work・reportの上限と決定的fresh名は実装済みで、standalone CLI公開と両host native journeyは未完了です。

## Concurrency、component、将来のtoolchain

[RFC 0021](rfcs/0021-bounded-structured-task-driver.md)の有界FIFO driverと[RFC 0022](rfcs/0022-source-structured-tasks.md)・[0023](rfcs/0023-task-mir-boundaries.md)のsource/MIR契約を実装済みです。scope付きtask claim、join、所有引数/結果、suspension loan規則、取消、子から親のcleanupはVMと生成native continuation、受理済み別リンクobjectで共通です。未回収所有結果はescrowとlive-cell計上を保持し、子のjoinと結果退役が親localのcleanupに先行します。taskは1 invocation内の論理taskであり、複数OS threadでguestを実行しません。

[RFC 0024](rfcs/0024-send-share-and-service-authority.md)のSend/ShareはsourceとMIRで再帰的に独立導出します。[RFC 0025](rfcs/0025-bounded-channels-and-backpressure.md)の有界MPSC channelはFIFO backpressure、元のwait identity、未commit payloadの回復、世代検査と取消を保ちます。[RFC 0026](rfcs/0026-source-isolated-services.md)のserviceはproducer権限をstate ownerから分離し、置換候補の検査とqueueからstateの順での退役を行います。既存allocator、ownership、mixed retirementを共有し、別のowner registryは作りません。

[RFC 0027](rfcs/0027-production-component-interface.md)のIDL/codec、[0028](rfcs/0028-task-component-artifacts.md)のartifact、[0029](rfcs/0029-component-adapter-work.md)のadapter workを実装済みです。型付きgrantと選択済みproviderの照合は副作用前に行います。canonical resource indexは権限ではなく、元table・generation・provider・holder・loanを検証します。callbackは元allocator/control/準備台帳、task、source/caller spanを共有します。scope付き同期reentryを明示許可した場合だけ貸し出し、返却/失敗/unwind時に元scopeを復元します。thread確認前には不変identityだけをraw読出しし、可変StateのRust参照や有効capabilityを復元しません。期限切れ、別thread、競合loan、cleanup phaseからのguest再入場を拒否します。

root task exportのbindingはimportなし・export interface 1つ・その中のtask function 1つのLCMP3 projectionを検査します。実member、capability、厳密version、完全LCSG preimageを同じ準備台帳で照合します。root VM/JIT/AOTは元のstructured task treeを実行します。task importやcallbackからのtask export実行は受理しません。

[RFC 0030](rfcs/0030-native-task-component-records.md)–[0032](rfcs/0032-installed-component-packages.md)の保存record/packageは元のcode・grant・provider・SDK identityを要求し、欠落時に探索やVM/JIT fallbackで補いません。実macOS AArch64/Windows x86-64の7 profile隔離起動と旧C client移行は[M3 guide](milestone-3-capabilities.md)と日付付き証跡にあります。

M4は2026-10-30 JSTに完了しました。[RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md)は厳密な version 付き manifest、明示 local dependency 権限、lock／offline 互換性、JSON CLI 診断を定義します。`lang package check-manifest [--message-format human|json] FILE` は依存を読まず最大 64 KiB を検証し、成功時は出力せず、エラー exit 1（usage 2）を維持します。host の `package::graph` API は整列した完全一致 local graph の不変 manifest／entry byte を保持し、canonical な dependency directory identity を共有して、循環、逸脱、名前／version／target 不一致、resource 超過を拒否します。capability 相対の通常ファイル読み取りは上限付きで、Unix では nonblocking。未検査 file は canonicalize しません。既定上限は 256 package、4096 active edge、16 MiB entry byte です。

2026-10-30 JSTの最終契約監査ではRFC 0034–0042のtooling profileをimplementedとして照合した。新規開発者の導入済みtool journey、実行例、debug／profile／署名導入の範囲は[M4能力guide](milestone-4-capabilities.md)に記録する。安定化宣言ではなく、PR #26での最終公開、成功head `01f89ff`、merge `e7fa5ab`は[完了ボード](milestone-4.md)に記録する。

`package::source::analyze` が package 名前空間を共有 frontend へ供給します。root の `mod helper;` は宣言された有効な直接依存を選び、各 package の `root::` は自身の内部に留まり、共有 diamond は一つの nominal identity を保持します。最初の準備で宣言 entry／test の source directory を一度取得し、filesystem handle を解放します。後続の解析は削除／project rename 後も固定 byte、directory alias、source root ごとの所属を使います。開いた file の metadata で最終 filename を canonicalize せず source identity の重複を検出します。取得失敗も cache 化し、新しい filesystem 状態には新しい Graph を必要とします。未使用 input も含む上限は source file 256、16 MiB、directory entry 4096 です。原 source byte／span を既存 HIR／MIR／portable VM とプログラム全体の native JIT に渡し、dependency fixture は両 mode で 42 を返します。

`package::lock::Lock::capture` は同じ固定 inventory を追加 I/O なしで利用します。schema 1 は target／compiler identity、完全一致 package edge、元の manifest／source SHA-256、解決した entry／test／directory binding を固定し、長さ付き package checksum と UTF-8 辞書順を使います。inventory が重なっても全宣言の選択を保持します。`verify` は入力を 1 MiB に制限し、重複／未知 field、未対応 version を拒否して canonical byte の完全一致を要求します。`check` は明示 output-directory capability から `Lang.lock` だけを読み、書き込みません。`publish` は同 directory 内の排他的な一時 file、sync、handle close、atomic rename を使い、失敗 stage を除去します。呼出元は別の出力権限を保持し、操作全体が成功してから公開します。Graph の表示用 root を ambient に開き直したり network fallback したりしません。固定 serialization／checksum fixture と配置変更後の VM dependency replay は `tests/package_lock.rs` にあり、package command はこの公開境界を使います。

`lock::validate_encoding` は上限付きの構造／schema admission のみで、checksum や replay の正当性を保証しません。`package_metadata` fuzz target は filesystem access や code 実行なしで、この reader と `Manifest::parse` を検査します。実際の lock replay には引き続き、取得済み graph に対する `Lock::verify`／`check` が必要です。

`package::command` は排他的な `new` と package 対応の portable `build`／`run` を実装します。`Graph::resolve_in` は呼出元が output 用 clone を保持する同じ directory capability を消費し、source の表示 path を開き直しません。`package::output` は source／output alias を検査し、全 output を 64 MiB に制限して、不変な `target/lang/<lock-sha256>/build/{inputs.lock,main.lbc}` generation を stage 化し、再利用時は期待する全 byte を比較してから最後に `Lang.lock` を公開します。locked mode は型検査前に検証し、lock を書き換えません。compile は固定 provider、実行は既存の Copy／ownership／task VM engine と control を使います。schema 1 JSON の source 診断は元 code、path、UTF-8 offset を保持します。従来の file 明示形式は既存 parser と実行経路を使います。CLI fixture は 42 を返す実 dependency、古い input／改変 cache／runtime の拒否、output alias、profile 選択を実行します。

`package::source::analyze_test` は固定 binding inventory から宣言 test を選択します。その root は選択 package の直接 edge と、同 package を指す `_package` を持ち、元 package-root 名前空間、nominal identity、可視性、source span を保持します。別 test root は module 上限に算入し、package entry 自体が test なら同じ root を再利用します。`lang test` は選択 package の test だけを manifest 順に、同じ検証済み artifact／VM 経路と entry ごとに初期化する control で実行します。test 0 件でも main を要求せず graph を型検査します。最初の source／runtime／非 0 失敗では旧 lock／build／test output を保持します。保持 artifact と完全一致 lock copy は 64 MiB に制限し、`target/lang/<lock-sha256>/test/{inputs.lock,tests/<manifest-index>.lbc}` を公開します。test は package／test root 間の公開 nominal type、private UTF-8 span、入力削除／project 移動後の固定解析、isolated service を含む全 VM profile、test ごとの fuel、test 0 件の library、batch-output 拒否を検証します。test 名前空間による artifact／lock schema 変更はありません。

workspace選択、再現可能native executable、package／archive／fmt／lint／doc tooling、package editor navigation／検証edit、上限付きDAP profileは実装し、両hostで検証済みです。[完了ボード](milestone-4.md)にG1–G9の証拠を記録しています。`lang profile compile`は元のpackage capture／frontend／portable lower・verify・encodeを観測し、guest実行やbuild／lock公開を行いません。[RFC 0041](rfcs/0041-bounded-toolchain-profiles.md)が閉じたcompile／runtime／allocation／rebuild reportと計測制限を定めます。全4 profiling commandとRFC 0042の署名installerを両hostで検証しました。registry、WebAssembly component、追加backend、self-hosting、別frontendは後続milestoneで、実装済みではありません。async REPL、detached task、非同期foreign callback stackも現在の実行範囲外です。

## 検証と保守

code変更は対象のbehavior regressionから確認し、workspace fmt/Clippy/test/docを実行します。source/format fixtureと、backend間の値、failure span、cleanup、resource accountingを維持します。native target実行、実debugger、sanitizer/coverage-guided fuzz campaign、性能測定は独立した証拠です。compileやCI時間からそれらを推測しません。

現在の契約はここ、詳細はRFC、dated checkpointは[M3履歴](architecture-m3-history.md)と[完了board履歴](milestone-3-history.md)へ置きます。新しいcheckpointをこの概要へ複製しません。[2026-10-30レビュー](review-2026-10-30.md)に今回の保守修正と検証を記録します。

情報源: repository実装・lockfile・RFC・[M3完了監査](milestone-3-capabilities.md)、2026-10-27 JST確認。実装ファイルには公開更新日の埋込みがないため、RFCの個別更新日と証跡の検証日を使用します。この整理は言語・ABI・serialized契約を変更しません。
