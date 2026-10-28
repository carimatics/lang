<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャbaseline

[English](../architecture.md)

現在の実装を`9e78f2d`で2026-10-27 JSTに検証しました。Milestone 0–3のexit gateは完了しています。公開component C v1と旧生成clientはRFC 0033に従い安定化し、source構文・native Rust ABI・bytecode/SDKの独立formatは実験段階です。[M3機能ガイドと15 gate監査](milestone-3-capabilities.md)にcommand・両host証拠・制約をまとめます。M2の[機能ガイド](milestone-2-capabilities.md)と[過去のarchitecture checkpoint](architecture-history.md)も保持しています。

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

この根拠ではsourceを残した手動のmatching-SDK debug linkを使っています。既定の`link-native`はdebugger sidecarを公開しません。Source variable location、論理continuation step、最適化後のstep、JIT debugger登録、VM/native混在stack、DAPは別作業です。

[M2 source/Rust/C測定](benchmarks.md)は名前付きの不変revisionに結び付き、大きな性能差を示しています。測定完了の根拠であり、native同等性能や速度保証ではありません。今後の最適化判断にはこのworkloadを使い、過去のscalar microbenchmarkでsource programの費用を代用しません。

## 診断、REPL、editor tooling

CLI は check、source/artifact 実行、portable compile、native package/link、transactional REPL、同期 LSP prototype を備えます。format は共通 CST API と LSP document formatting から利用できます。[RFC 0010](rfcs/0010-owned-repl-previews.md) と [RFC 0016](rfcs/0016-indirect-repl-previews.md) が有界 result preview を定めます。output 公開前に result を retire し、session は source 定義を保持しても mutable guest owner を保持しません。

Query graphは推移的consumerを無効化し、編集、close/reopen、新しい処理の開始後に古い結果の公開を拒否します。LSPはsource analysis、不変overlay、UTF-16 mappingを再利用します。上限付きframing/outputでlifecycle、full synchronization、diagnostic、format、監視moduleのrefreshを実装しています。Navigation、completion、rename、code action、DAPは未提供です。

## Concurrency、component、将来のtoolchain

[RFC 0021](rfcs/0021-bounded-structured-task-driver.md)の有界FIFO driverと[RFC 0022](rfcs/0022-source-structured-tasks.md)・[0023](rfcs/0023-task-mir-boundaries.md)のsource/MIR契約を実装済みです。scope付きtask claim、join、所有引数/結果、suspension loan規則、取消、子から親のcleanupはVMと生成native continuation、受理済み別リンクobjectで共通です。未回収所有結果はescrowとlive-cell計上を保持し、子のjoinと結果退役が親localのcleanupに先行します。taskは1 invocation内の論理taskであり、複数OS threadでguestを実行しません。

[RFC 0024](rfcs/0024-send-share-and-service-authority.md)のSend/ShareはsourceとMIRで再帰的に独立導出します。[RFC 0025](rfcs/0025-bounded-channels-and-backpressure.md)の有界MPSC channelはFIFO backpressure、元のwait identity、未commit payloadの回復、世代検査と取消を保ちます。[RFC 0026](rfcs/0026-source-isolated-services.md)のserviceはproducer権限をstate ownerから分離し、置換候補の検査とqueueからstateの順での退役を行います。既存allocator、ownership、mixed retirementを共有し、別のowner registryは作りません。

[RFC 0027](rfcs/0027-production-component-interface.md)のIDL/codec、[0028](rfcs/0028-task-component-artifacts.md)のartifact、[0029](rfcs/0029-component-adapter-work.md)のadapter workを実装済みです。型付きgrantと選択済みproviderの照合は副作用前に行います。canonical resource indexは権限ではなく、元table・generation・provider・holder・loanを検証します。callbackは元allocator/control/準備台帳、task、source/caller spanを共有します。scope付き同期reentryを明示許可した場合だけ貸し出し、返却/失敗/unwind時に元scopeを復元します。thread確認前には不変identityだけをraw読出しし、可変StateのRust参照や有効capabilityを復元しません。期限切れ、別thread、競合loan、cleanup phaseからのguest再入場を拒否します。

root task exportのbindingはimportなし・export interface 1つ・その中のtask function 1つのLCMP3 projectionを検査します。実member、capability、厳密version、完全LCSG preimageを同じ準備台帳で照合します。root VM/JIT/AOTは元のstructured task treeを実行します。task importやcallbackからのtask export実行は受理しません。

[RFC 0030](rfcs/0030-native-task-component-records.md)–[0032](rfcs/0032-installed-component-packages.md)の保存record/packageは元のcode・grant・provider・SDK identityを要求し、欠落時に探索やVM/JIT fallbackで補いません。実macOS AArch64/Windows x86-64の7 profile隔離起動と旧C client移行は[M3 guide](milestone-3-capabilities.md)と日付付き証跡にあります。

M4 は 2026-10-29 JST に開始しました。[RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md)は厳密な version 付き manifest、明示 local dependency 権限、lock／offline 互換性、JSON CLI 診断を定義します。`lang package check-manifest [--message-format human|json] FILE` は依存を読まず最大 64 KiB を検証し、成功時は出力せず、エラー exit 1（usage 2）を維持します。host の `package::graph` API は整列した完全一致 local graph の不変 manifest／entry byte を保持し、canonical な dependency directory identity を共有して、循環、逸脱、名前／version／target 不一致、resource 超過を拒否します。capability 相対の通常ファイル読み取りは上限付きで、Unix では nonblocking。未検査 file は canonicalize しません。既定上限は 256 package、4096 active edge、16 MiB entry byte です。package source 実行と build／run／test、navigation／completion／rename、DAP、installer は [live board](milestone-4.md)に残ります。registry、WebAssembly component、追加 backend、self-hosting、別 frontend は後続 milestone で、実装済みではありません。async REPL、detached task、非同期 foreign callback stack も現在の実行範囲外です。

## 検証と保守

code変更は対象のbehavior regressionから確認し、workspace fmt/Clippy/test/docを実行します。source/format fixtureと、backend間の値、failure span、cleanup、resource accountingを維持します。native target実行、実debugger、sanitizer/coverage-guided fuzz campaign、性能測定は独立した証拠です。compileやCI時間からそれらを推測しません。

現在の契約はここ、詳細はRFC、dated checkpointは[M3履歴](architecture-m3-history.md)と[完了board履歴](milestone-3-history.md)へ置きます。新しいcheckpointをこの概要へ複製しません。[2026-10-27レビュー](review-2026-10-27.md)に今回の保守修正と検証を記録します。

情報源: repository実装・lockfile・RFC・[M3完了監査](milestone-3-capabilities.md)、2026-10-27 JST確認。実装ファイルには公開更新日の埋込みがないため、RFCの個別更新日と証跡の検証日を使用します。この整理は言語・ABI・serialized契約を変更しません。
