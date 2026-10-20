<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャbaseline

[English](../architecture.md)

現在の実装を`e60e3d9`に対してレビュー、2026-10-20 JST。Milestone 0–2は記録されたscopeで完了しています。構文、native ABI、bytecode、SDK形式は実験段階です。意味論は受理済みRFCが定め、この文書は契約と実装の対応および将来の作業を示します。コマンドとM2の18ゲート監査は[機能ガイド](milestone-2-capabilities.md)、日付付きの開発checkpointは[architecture履歴](architecture-history.md)にあります。

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

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)はより広い所有権方針です。そのsource subsetと現在のruntime serviceは実装済みですが、source concurrencyの追加や、すべてのhost APIのSend/Sync化を意味しません。

## Failure、allocation、実行control

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)は、実際の初期化順の逆順によるexactly-once cleanup、return escrow、primary/secondary failure、poison/reentry、allocator authorityを定めます。回復可能な失敗には型付きResultを使い、[RFC 0014](rfcs/0014-explicit-source-panic.md)が終端となるi64 code付きsource panicを定めます。言語cleanupはnative unwindingではなく、明示的status/control flowを使います。

Guest memoryはprovider identityとlive/cumulative上限を保持するfallible accounting allocatorを使います。Host metadataとframe/retirement storageには別の準備上限があります。必須retirementはcontrol枯渇後も継続し、reportの打ち切りでcleanupを打ち切りません。信頼済みhost panicの封じ込めは文書化されたRust/C adapter境界に限ります。Foreign unwinding、abort-mode panic、不正native pointerは保証外です。

計測付き実行はinstruction fuel、interruption/deadline、host-call、frame/live-cell、[service-work](rfcs/0013-service-byte-work-budget.md)のaccountingを共有します。1つの論理invocation内でcallとservice viewがcounterを共有します。対応するeffectの前にadmissionし、未対応の有限controlは実行前に拒否します。Service workは指定した論理eventの計数であり、すべてのCPU時間、allocator処理、任意の信頼済みcallbackを測るものではありません。

通常のCopy native facadeは信頼済み・同期・unmeteredです。Ownership native実行は[RFC 0012](rfcs/0012-native-ownership-continuations.md)の準備済みmanaged frameとcompiled continuationを使い、host driverはMIR評価やVM fallbackをせずnative addressをdispatchします。Guest recursionは上限付き論理frameを使います。上限やsandboxを説明するときに、これらのprofileを混同してはいけません。

## Portable形式とnative形式

Versionの名前空間は独立しています。Component prototypeのschema番号は`LANGBC` source programのschema番号ではありません。

| 形式 | 現在の選択とauthority |
| --- | --- |
| Source program schema 1 | RFC 0006のCopy program |
| Source program schema 2 | RFC 0011のownership program。明示的panicとC metadataを含まない |
| Source program schema 3 | RFC 0014のownership program。明示的panicを含み、C metadataを含まない |
| Source program schema 4 | RFC 0017のC対応ownership program。明示的panicの有無は問わず、metadataはC実行authorityを与えない |
| Private ownership native revision 1/2/3 | RFC 0012・0014が定める通常／C対応／明示的panicの規約 |
| C profile 1とobject bridge 1 | 外部の有限C値／descriptorと、それとは別のprivate per-image binding slot |
| Native record schema 1 | RFC 0018の元objectとcompiler record。厳密なportable bytes/build/targetに結び付く |
| SDK/program manifest version 1 | RFC 0020の上限付きfile table、identity、role、size、SHA-256 |

Decoderは未対応version、不正shape、不正span、resource枯渇を拒否します。Ownership artifactは検証済みのcleanup前表現を格納し、必要なproof/elaborationを独立して再構築します。Native recordは元code metadataを保持し、startupでmachine codeを再生成しません。

Hash/build identityは不一致を検出しますが、悪意あるSDKやnative objectを認証しません。Portable artifact内のsource snapshotは診断用であり、file再parse、import解決、native code load、allocator取得のauthorityではありません。

## C ABIとnative SDK

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

[RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md)はstructured cancellation/isolation方針を定めます。[RFC 0003](rfcs/0003-versioned-component-contracts.md)はversioned component契約、capability、typed failure、resource所有権を定めます。State modelやscalar/resource相互運用prototypeは判断の根拠であり、production source async、scheduler/channel、IDL/adapter、汎用mixed-mode embeddingはM3の作業です。

M3 は[進行中の実装 Todo](milestone-3.md)で追跡します。[RFC 0021](rfcs/0021-bounded-structured-task-driver.md)は `crates/runtime/src/task.rs` の明示的な有界 task driver を定義します。FIFO の resume/cleanup dispatch、invocation 単位の handle、live/累積 task admission、直接の子の join、協調取消を扱います。`cargo run --example structured-tasks` は実際の host 所有 frame tree を実行し、42 を返し、通常終了/入れ子取消の両方で frame を 2,1,0 の順に破棄します。kernel 自身は guest code を実行しません。VM/native task adapter と実際の guest frame cleanup は後続作業です。独立 task MIR 証明は後述します。2026-10-21 JST に確認。M2 の実行経路や artifact 形式は変えません。

[RFC 0022](rfcs/0022-source-structured-tasks.md) で明示 async effect、scope 付き Task claim、型付き join 取消、厳密な suspension loan 規則を採択しました。lossless syntax を resolved async effect、Task/TaskError 型、有界 move/scope claim snapshot に束縛します。不正 call、consume 済 handle、scope escape、未対応 C surface を拒否し、Task の borrow/match/入れ子/container 格納を禁止します。一般の Send/Share 導出までは async transfer を scalar/finite Copy 値に限定します。対応 source 解析は TaskProgram 証明を保持し、CLI run/LSP が接続済み task profile を使います。raw HIR 互換 API と async REPL submission は未対応 profile の明示拒否を維持します。lexical loan engine は await/yield/cancel_check/暗黙 join 時の生存 shared/exclusive loan を SRC0028 と loan 起点で拒否します。branch 状態の合流前に return/失敗 edge を検査し、終了する local loan を join 前に終え、入れ子 join では外側 loan を維持します。結合した TaskLoans snapshot に source claim/loan 証明を保持します。独立 MIR と初期 VM の証明は後述し、native task 実行は後続作業です。task 実行権限や artifact schema の変更は導入しません（2026-10-21 JST）。

[RFC 0023](rfcs/0023-task-mir-boundaries.md) で既存 ownership MIR 型/宣言 graph に Task/TaskError と ABI から独立した async effect を追加します。storage/signature 制限と effect span を独立検証し、通常の call query に async を消去させません。型/宣言の成功は task flow 証明ではありません。旧 place/body 境界、runtime layout compiler、bytecode writer は未使用 metadata も含めこの profile を拒否します。旧 reader は同期 effect を明示復元し、task tag は割り当てません。別 entry の `ownership::tasks::verify` が同じ body table の task 命令を検証します。有界 scope stack/起点集合、local/prepared destination/CFG edge 経由の move-only claim、suspension loan 拒否、保護された return escrow を扱います。既存 move/loan/arena region 証明も要し、owner retirement 前の取消/失敗 cleanup phase を明示化します。`VerifiedTasks` は layout/service/実行権限を付与せず証明を保持します。旧 VM/native API は scalar async 証明も副作用前に拒否し、旧 codec tag は維持します。TaskProgram が source claim/loan 証明と独立に lower した task MIR を結合します。明示 VM task API は managed finite Copy layout、TaskClaim sidecar、再入場を区別する scope identity、分離した return reservation を使い、通常 call と task suspension の continuation が failure plan を実行します。`cargo run -- run examples/scripts/tasks.lang` は 42 を返し、取消、失敗起点、loop scope、fuel/trace/provider 拒否のテストで task/storage を解放します。初期の到達可能 local は finite Copy/Task とし、owned/loan/foreign task 実行、native continuation、完全な共有 control は T06–T08 の作業です。source 解析は host allocator/scheduler/artifact 権限を付与しません（2026-10-21 JST）。

配布tierを明示します。信頼済みnative codeにsandboxの主張はなく、検証済みscriptにはcapability/resource enforcementが必要で、portable WebAssembly componentは後のmilestoneです。Native in-process pluginは非信頼codeのsandboxにできません。将来のfrontendがMIRを共有できるのは意味論が一致するときだけで、それ以外はcomponent境界を必要とします。

Package-aware build/test/lint/doc、package manager/installer、拡張LSP/DAP、registry/provenance、LLVM評価、self-hosting、追加frontendは[roadmap](roadmap.md)に残ります。現在のSDK packagingはpackage managerやinstallerを完了させません。Bootstrapには固定した対応subset、stage 0→1→2のconformance、両hostでの再現性が必要です。第二frontendは第一frontendとABIの安定後です。

## 検証と保守

Code変更では、対象のbehavior regressionを先に確認し、repositoryのfmt/Clippy/test/doc baselineを実行します。Source/format fixtureと、backend間の値、failure span、cleanup、resource accountingを保持します。Native target実行、debugger実行、sanitizer／coverage-guided fuzz campaign、性能測定は別々の根拠であり、compileだけでは成立しません。

現在の契約はこの文書とRFCに保持します。時系列の実装記録は該当Todoや日付付きレビューへ置き、所定interfaceと検証が完了したら実装状態を更新します。古い「残作業」の段落をこの文書へ複製しません。

情報源: repository実装、lockfile、受理済みRFC、[M2完了監査](milestone-2-capabilities.md)。`e60e3d9`で2026-10-20 JSTに確認しました。個別RFCの更新日と過去の外部情報源の日付はリンク先に保持しています。この整理では言語、ABI、serialized contractを変更しません。
