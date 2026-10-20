<!-- translation-of: ../milestone-2.md; canonical-language: en; reviewed: 2026-10-20 -->

# Milestone 2実装Todo

[English](../milestone-2.md) · [完了条件](roadmap.md#milestone-2ownershipとnative-aot)

更新・検証：**2026-10-20 JST**。初回監査checkpoint：`0c2c012`。

**133 done · 0 doing · 0 todo · 0 remaining pt**

**次: none → Milestone 2の全成果が完了しました。後続milestoneは別途計画します。**

| 現在の成果 | 直近の検証済み完了 | Milestone出口 |
| --- | --- | --- |
| M2の進行中作業なし | **Z01完了**：日英capability guideと全出口監査（`e12c120`） | **18/18 gates、最終監査も完了** |

**最終成果checkpoint（2026-10-20 JST）：** [capability guideと18 gate監査](milestone-2-capabilities.md)に、現ownership／core／VM／native／C動作、実用SDK手順、正確な証拠と制約を記録しました。Z01ではREADMEの古い逐次statusを現機能の案内へ置き換え、整合性checkerが全件完了を`Next: none`で表せるようにしました。複合sourceのcheck／VM／artifact／公開SDK link・runを実行し、42を確認しています。

**今回：成果1件完了（−1 pt）、見積もり／task追加なし。** 残り **1 − 1 = 0 pt**、**133行／追加109点は不変**、[収支](#進捗と見積もり変更)は **134 + 109 − 243 = 0 pt**。完了したplanning単位であり、時間や計測済みの過去の超過工数ではありません。

最終local baseline：**1,593 tests／95 suites、失敗0、手動計測ignore 1件**、suite時間合計**439.90s**。fmt、workspace／all-target／all-feature警告error化Clippy（**18.52s**）、rustdoc（**18.31s**）、全11 fuzz targetのcompile／lint（**0.68s**）が成功しました。新guide／READMEのlocal link・anchor、全18監査行、日英command block一致も検証しました。最終board／roadmap整合性testは全件完了状態を検証し、計画のみの更新で変更のないcompiler全baselineは再実行しません。

最終native実装は`1496b8b`で、[両host実debugger run 39413674593](https://github.com/carimatics/lang/actions/runs/39413674593)が元sourceのbreakpoint、Copyの2→3行step／3段source caller、ownedのinstruction step／実runtime caller、42／正常終了に成功しました。Windows unwind metadataとlink後Mach-O addressを修正済みです。Z01の変更は文書と整合性testで、compiler／runtime動作は変更していません。

元の全受け入れ成果を維持しています。性能証拠は記載したB02／B03 revisionのままでRust／Cとの大きな差があり、現native debug probeは対応SDKの手動linkとowned／runtimeの物理frameを使います。実験的ABI／format、source変数位置、論理continuation step、VM／native混在debugger stack、後続milestone機能はguide記載の制約を維持します。

<details>
<summary>過去の成果とplatform検証記録</summary>

Z01成果（2026-10-20 JST）：`e12c120`で日英capability guideと元の全gate監査を公開し、README／SDK statusを更新しました。全行完了時のみ明示`Next: none`を認めます。Local全体1,593 tests／95 suites、失敗0、手動計測ignore 1件、fmt／Clippy／rustdoc／全11 fuzz targetとVM／artifact／nativeの実42 smokeが成功しました。最終1 ptを完了し、**132 → 133 done、残り1 → 0 pt**、収支は**134 + 109 − 243 = 0**。元18 gateと最終guide／監査が完了し、scopeは削っていません。

G03履歴checkpoint（2026-10-20 JST）：`1496b8b`の[run 39413674593](https://github.com/carimatics/lang/actions/runs/39413674593)、Mac job118597368496／Windows job118597368052で両hostの実debugger検証を完了しました。元のCDB stack途切れは`.pdata`／`.xdata`欠落が原因で、修正後も3段caller要件を維持しています。scope／見積もり追加なしで2 ptを完了し、**131 → 132 done、残り3 → 1 pt**、収支は**134 + 109 − 242 = 1**。G18を閉じ、Z01をdoingにします。

P07の履歴checkpoint：

**直近の実行checkpoint（2026-10-20 12:31 JST）：** SDK builderは成功Cargo artifact／build-script出力を正確に選び、Rust／proc-macro／native archiveを保持し、上限付きfileをstreamコピーしてP11で書込済みSDK全体を検証した後、exclusive公開します。Compilerの実行権限を保持します。同梱Copy／owned／C起動templateをSDK依存物だけでlinkでき、実C allocator／context clientを含めsource削除後も42を確認しました。Release SDK組立（105 component／logical size 197.2 MiB）とsourceなしowned entryの42も成功しました。Abort buildは公開前に拒否し、RFC 0020にunwind policyを記録してpanic cfgをbuild identityに含めました。

Mac検証：**1,577 tests / 93 suites**、最終失敗0、手動timing test 1件ignored。実行fileごとの照合です。初回全体実行で成功した不変の19 suiteを保持し、mtime順でabort／unwind libraryを混在させた手動rustc fixtureを修正して、影響したsource-native suiteと未実行の全root／workspace targetを成功させました。現在のCargo all-target／all-feature一覧93実行fileと一致し、未検証／余分な実行fileはありません。Source-native **10 / 13.42秒**、native ownership **76 / 268.87秒**、SDK **5 / 15.26秒**、C ABI **74 / 20.28秒**、snapshot **9 / 1.20秒**。Fmt、最終Clippy（**1.24秒**）、rustdoc（**24.95秒**）、全11 fuzz targetのClippy（**1.73秒**）が成功しました。初回に失敗した全体commandが成功したとは数えません。

検証の観測値：初回実時間 **164.10秒**／compile **26.06秒**、再開root batch **422.48秒**、非root workspace **77.24秒**、選択した成功suite報告合計 **398.96秒**。Release組立build **38.86秒**、最終abort拒否build **3.62秒**。別々の観測値であり、開発工数、単一全実行の時間、最小SDK size、性能改善ではありません。古いpackage check metadataはCargoのpackage単位cleanupで解消し、test加点はありません。ProductionのLF属性でcheckoutだけによる共通source digest変化を防ぎ、元header support byteは不変です。

P05 production入口の検証（`336a91b`）：Mac全1,547 tests／89 suites、native 76／279.32秒、C ABI 71／18.44秒とfmt／Clippy（2.18秒）／rustdoc（24.94秒）が成功。対応Windows [run 39336016918](https://github.com/carimatics/lang/actions/runs/39336016918)（job118345577536、head336a91b）は必須fmt／重点C・linker／Clippy／全test／rustdocを2026-10-19 23:02:02 UTCまでに成功。Workflow全体も2026-10-19 23:12:56 UTCに成功しました。後続P06やG17のRust／C比較の証拠には数えません。

P05完了と先行checkpointの監査（2026-10-20 JST）：

- [x] RFC 0018の上限付きnative記録でtarget／scope、正確なobjectとmetadataを保持し、checksumで意味論／objectのbyteを対応付け。Managed schema 2と明示的panicのschema 3をfileから読み直し、生成元破棄後に元span／work／cleanupを保つlinked実行を確認。
- [x] Copyの検証付き準備／bindでimageを保持し、JITとcanonical値検査を共有。読み直したfileからrecord／enum／scalarを呼出し。Count／symbol／UTF-8／target／budgetを呼出し前に拒否し、missing symbolではimageを解放。Code：`5e0d6a1`。
- [x] 通常owned用の具体的runtime入口。非公開の共通program viewとprovider対応primitivesを共有し、元entry、control、usage、failure、cleanup、poisonを保持。生成元破棄後の保存fileからproduction runtimeで実行し、全fuel／work／provider失敗位置をVMと比較。Code：`336a91b`。P05の残り3 ptを完了で除き、追加point／行はありません。

先行checkpoint検証：`5e0d6a1` はmacOS **1,539 tests / 89 suites**、失敗0、手動timingのみ1件ignored。Native **68件／276.12秒**、C ABI **71件／22.50秒**、fmt／Clippy（**3.16秒**）／rustdoc（**18.04秒**）成功。Suite合計377.01秒は検証時間で、開発工数ではありません。初回全体検証はstandaloneの直接rustcビルドがtest専用record helperまでcompileし、lang-bytecode externを持たず失敗したため、`cfg(test)`でfixtureを修正し、今回の全体再実行が成功しました。先行schema 4 snapshot `816b0b6` の[Windows run 39322772989](https://github.com/carimatics/lang/actions/runs/39322772989)、job118300781642：fmt／重点C・linker／Clippy／全test／rustdocが2026-10-19 21:16:54 UTCまでに成功し、workflow全体も成功（job完了21:26:54 UTC）。Prototype benchmarkはG17のRust／C比較証拠には数えません。Native記録版のWindows snapshot：[run 39328266216](https://github.com/carimatics/lang/actions/runs/39328266216)、head 5e0d6a1は成功（job118319603831、2026-10-19 22:08:42 UTC完了）。後続336a91bのsource runtime検証とは区別します。

P02配布監査（2026-10-20 JST）：metadata、意味論の保存、runtime起動、C context統合、SDK組立、両hostのclean installを既存3 ptのまま扱い、独立した成果が隠れていました。P02の最終clean-install検証は3 ptを維持します。P03独立metadata（初期2 pt、5324c64で完了）、P04 C対応artifact（初期1 pt、816b0b6で完了）、P05 runtime起動（3）、P06配布C context（3）、P07 SDK組立（2）を明示します。既存要件の再見積りであり、**scopeは削除しません**。**123 → 128行、追加11点、完了で3点を除き、24 + 11 − 3 = 残り32 pt**。P03／P04の登録を、そのcommit以降の新しい作業とは数えません。Source起動実験はP05／P06で完了しました。SDKの前提を下記で別途監査します。

- [x] **P03**：生成元IR／layouts破棄後に独立Rust記録から機械語生成なしでplanを再構築。Code size／source rangeと実linked実行での全work prefixのVM比較を維持。Code：`5324c64`。
- [x] **P04 / RFC 0017**：schema 4でHost／C、Guest／CExport識別、aggregate署名、明示的panicを保持。復元したsource fixtureで明示VM／native context経由の実C呼出しを確認し、CLIはsource削除後も消費ゼロでC実行を拒否。Code：`816b0b6`。
- [x] **P05**：Copy／ownershipのsource不要起動とnative metadataの保存・復元。Code：`336a91b`。
- [x] **P06**：元allocator／cleanupを保つ配布C context／import／export binding。Code：`2712700`。
- [ ] **P07**：toolchain要件を明示したcompiler／startup／runtime／headerのSDK組立。
- [ ] **P02**：repo／target/deps外で、Copy／ownership／Cを含む両tier-1のclean-install link/run。

先行schema 4検証記録：`816b0b6` はmacOS **1,533 tests / 89 suites**、失敗0、手動timingのみ1件ignored。C ABI **71件／16.98秒**、native **67件／278.18秒**、fmt／Clippy（**23.65秒**）／rustdoc（**21.70秒**）成功。Suite合計374.56秒は検証時間で、開発工数ではありません。先行metadata snapshot 479296bは[Windows run 39317655321](https://github.com/carimatics/lang/actions/runs/39317655321)、job118283210035を追跡：fmt／重点C・linker／Clippy／全test／rustdocが20:39:08 UTCまでに成功し、workflow全体も成功（job完了20:49:28 UTC、2026-10-19）。Prototype benchmarkはG17のRust／C比較には含めません。Schema 4の[Windows run 39322772989](https://github.com/carimatics/lang/actions/runs/39322772989)（head 816b0b6）はその後必須baselineに成功しました。上記の直近checkpointを参照してください。


先行checkpoint／成果の検証記録：

直近checkpoint検証：`5324c64` はmacOS全 **1,530 tests / 89 suites**、失敗0、手動timingのみ1件ignored。C ABI **71件／18.74秒**、native **67件／279.10秒**、fmt／Clippy（**32.93秒**）／rustdoc（**18.18秒**）成功。suite合計376.98秒は開発時間ではありません。P02はdoingのまま、**24 pt・123行・追加85点は不変**です。新しいwire形式やproduction配布物はまだありません。

- [x] **V17（3 pt）**：RFC 0016を実装。Vec、キー順Map、shared内容、weak生存状態をVM／実CLIで表示し、arena脱出禁止を維持。出力／host作業領域の境界、custody、weak cycle、cleanup／再利用を検証。Code：`72950d6`。
- [x] **P01（2 pt）／F07（2 pt）**：LINK直呼び、具体的なtoolchain／output失敗診断とWindows error縮小修正を両native hostで確認。P01 code：`0061045`／`2cf38ca`。[Windows run 39306998513](https://github.com/carimatics/lang/actions/runs/39306998513)、head `15a50bb`、job `118246548342` はfmt、重点C／linker、Clippy、全test、rustdocが成功。必須baselineは **2026-10-19 19:24:45 UTC** に完了。Workflow全体も **2026-10-19 19:35:39 UTC** に成功しました。さらにV17 snapshot `8b4ee47` の[全Windows run 39312412257](https://github.com/carimatics/lang/actions/runs/39312412257)（job118265129127）が **20:00:45 UTC** に成功しました。Prototype benchmarkはG17のRust／C比較証拠には数えません。
- [x] **V17 macOS検証**：最新suite結果を統合して **1,529 tests / 89 suites** 成功、失敗0、手動timingのみ1件ignored。初回全workspace実行は旧Vec拒否fixtureだけが失敗し、test入力をarenaへ修正後、`cargo test --workspace --exclude lang --all-targets --all-features`で修正suiteと停止後の未実行crateを再実行しました。以降production codeは不変で、成功済みroot／native証拠は保持。C ABI **71件／17.24秒**、native **66件／280.54秒**、owned実行 **155件／5.94秒**、REPL **28件／0.20秒**。fmt／Clippy（全workspace **30.41秒**、test修正後runtime **17.24秒**）／rustdoc（**24.25秒**）成功。重複を除くsuite時間合計377.91秒は開発時間ではありません。


F06の完了確認点（同じ2 pt内。taskの追加なし）：

- [x] 独立C11 header objectを生成。対象環境とpackingの不一致をcompilerが拒否。
- [x] 両object formatのslot／symbol／relocation、missing symbol 9通りの拒否、linkedの有限Copy caseと256混合importを確認。
- [x] Native macOS全baseline **1,519 tests / 88 suites**、失敗0、手動timingのみ1件ignored。C ABI **71件／20.09秒**、native **66件／279.70秒**、fmt／Clippy（**2.81秒**）／rustdoc（**13.72秒**）成功。Code：`adff5d9`。

F05のWindows失敗（2026-10-20 JST確認）：[run 39288239469](https://github.com/carimatics/lang/actions/runs/39288239469)、head 2f8772b、job 118181807851が **2026-10-19 16:53:47 UTC** に失敗しました。C ABIは **64件成功／4件失敗**、失敗はすべてresponse-file経由のprivate DLL slot欠落で、direct linkのcaseは成功しました。F06でCLの/linkをresponse file外へ置いてファイルをLINKへ渡し、slotをDATA exportし、全required exportとcompiler／linker診断を確認するよう修正しました。Native確認には修正版と一致する新F07 runが必要で、この失敗snapshotではCの完了条件を閉じません。

F05の完了確認点（1成果内の内訳。task／pointの追加なし）：

- [x] 実装前にprivate bridgeを受理（RFC 0015、949f3d8）。
- [x] 実C objectのdirect／response link／run、aggregate、owner付き再帰／元contextのimport／panic、256混合入力、全fuel／provider失敗prefix（対象test 6件）。
- [x] 全workspace **1,516 tests / 88 suites**、失敗0、手動timingのみ1件ignored。C ABI **68件／16.67秒**、native **66件／276.60秒**、fmt／Clippy（**34.11秒**）／rustdoc（**23.75秒**）成功。Code：`c0eee9a`。

F16完了（2026-10-20 JST）：4158655で再利用可能なNativeFactoryを実context allocator／cleanup Instanceへ接続しました。実Cからrecord・入れ子sum・全core error・空record・scalar極値を往復し、256混合引数を演算と128 guest callで処理します。Owner付き再帰・C callback・callee panicと、Bytes／Text／Vec／Map／shared／arenaの10 sourceで値／fuel／work／frame／cellのVM一致を確認しました。全fuel／provider拒否prefixでもoutputを保持し確保／解放数が一致します。必須検証：**1,510 tests / 88 suites**、失敗0、手動timingのみ1件ignored。C ABI **62件／14.13秒**、native **66件／276.96秒**、fmt／Clippy（27.48秒）／rustdoc（23.43秒）成功。初期 **3 pt** を取り除き **106 → 107 done、39 → 36 pt**。**123行・追加85点は不変、完了で取り除いた見積もり183点**。元の完了条件は **11/18** で、C object／platform／package／最終guideを維持します。

F15のWindows確定（2026-10-20 JST）：[全workflow 39272143078](https://github.com/carimatics/lang/actions/runs/39272143078)、head c81e5d2、job 118126939241が **2026-10-19 15:13:07 UTC** に成功しました。F15の全検証／benchmarkの証拠であり、後続F16 native runtimeの新snapshotは別途追跡します。

F15完了（2026-10-19 JST）：c0f9b68で実C callerから生成native source increment／panic、bool／Unitとcallbackを実行します。元opaque contextの保持・再入拒否・別Program・foreign failure・quota／OOM・host panic／poisonを5 testで検証しました。Contextは実C allocatorを保持し、result stagingも同じproviderとlive／累積quotaへ計上して成功・失敗とも解放します。必須検証は **1,504 tests / 88 suites**、失敗0、手動timing 1件のみignored。最後のstaging修正後に影響先C ABI全 **56件／12.37秒** を再実行し、他の実装pathは不変です。Native **66件／275.69秒**、fmt／Clippy（26.57秒）／rustdoc（18.01秒）成功。F15の初期 **3 pt** を取り除き、**105 → 106 done、39 → 36 pt**。下記F16の追加3点と分けて表示し、現在は **123行、追加85点、完了で取り除いた180点、残り39点**。元の完了条件は **11/18** で、C object完了とはしません。

F14のWindows確定（2026-10-19 JST）：[全workflow 39265484004](https://github.com/carimatics/lang/actions/runs/39265484004)、head 8aef3d6、job 118103831488が **2026-10-19 14:39:54 UTC** に成功しました。Raw context snapshotの全検証／benchmarkが完了し、後続F15 source exportの証拠とは分けます。

F14完了（2026-10-19 JST）：17da01bで実Programへ結び付けた既存C create／destroy／last_error入口を生成します。実C callerでopaque contextを作成／照会／破棄でき、stack descriptor／name上書き後もsnapshotを保持します。境界／状態5 testで不正要求・output保持・identity／再入／poison・address／重複拒否・有効なゼロ予算を検証しました。必須検証：**1,499 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。C ABI **51件／14.65秒**、native **66件／276.32秒**、fmt／Clippy／rustdoc成功。F14の **2点** を取り除き、**104 → 105 done、38 → 36 pt** とします。**121行、追加79点、完了で取り除いた見積もり177点** です。前提2点の明示と完了が相殺し、前回成果からの総点は同じでも実C supportが増えました。別途core完了条件を監査してG05／G07／G09／G10を完了とし、F05のsource export／objectは維持します。

<details>
<summary>残るFFI統合</summary>

F05着手前の監査（2026-10-20 JST）：typed value／owner bodyのruntime接続はF16で完了しました。残るobject入口はC unitを拒否し、JIT export／supportは実Program addressを捕捉し、NativeInvokerはJIT callback thunkを生成します。F05では明示version付きprivate bridge／symbol／metadata契約を先に記録し、linked code owner・同じruntimeのlayout参照・object内の型付きimport thunk・C support／exportを接続して実link／runします。JIT address埋込みや実行時JITへの置換でAOT完了としません。3 ptはobject実験前の暫定値で、新たな前提が判明した場合は実装前に可視化します。今回のscope／見積もり追加は0点です。

F15後の残作業監査（2026-10-19 JST）：実行済みfixture factoryはi64／bool／Unitで、aggregate exportのC → managed転送・aggregate return escrowとowner／guest call bodyのruntime接続が残ります。F16（初期3 pt）へ分離し、次は実Cのrecord／enum source exportを動かします。F05はlinked C object／bootstrapに絞り、明示的なprivate bridge契約とlink／runを維持します（暫定3 pt、着手前にobject実験で確認）。F15完了後の **36 → 39残点、122 → 123行、追加82 → 85点** で、完了による180点は不変です。新たなvalue／owner要件の追加ではなく、元のF05から残る実装を可視化しました。Runtime／body／aggregateの後続完了はF16に記録し、platform／package／最終guideも維持します。

F05実行後の範囲監査（2026-10-19 JST）：実C → 生成native source increment／panicの実験が成功しました。JIT source exportのcontext／callback／allocator／result／資源制限統合をF15（初期3 pt、doing）に分離し、F05はlinked C object公開・bootstrap・runtime／cleanup統合（暫定3 pt）として維持します。ポインタを埋め込むJIT wrapperはportable objectには使えず、object側の明示契約が未確定です。**121 → 122行、36 → 39残点、追加79 → 82点**、完了で取り除いた177点は不変です。Source実行の前に別の前提を追加しない条件は満たしました。Aggregate／owner body／platform／packageを含む元要件は残し、F15の成功をF05やC完了条件の完了としません。

F14時点のFFI監査（履歴、2026-10-19 JST）：F04 typed import、F09 drop、F13 panic、F14 raw C context supportはローカルで完了しました。F05にはnative source export実行・callbackでの元opaque context・実allocator／runtime／result staging・C対応object公開を維持します。次の実C source export実験まで3 ptは暫定とし、その実行前に別の前提作業を追加しません。F06／F07のtarget検証、P02 packaging、Q01／Q02 owner／C conformance、V17の間接preview、Z01 guideを維持します。

</details>

<details>
<summary>過去の成果・範囲監査・検証記録</summary>

Core完了条件の監査（2026-10-19 JST）：**G05／G07／G09／G10** を完了とし、**7 → 11 / 18**、履歴上の重み **50 → 68 / 113** とします。既存実装の完了確認であり、新たな工数見積もりの完了ではありません。各条件のsource／runtime全要件と実VM／native／tier-1根拠を下記で確認しました。Core実装pathは成功済みWindows snapshot 165ae7dから不変で、現macOS全baselineも通過しています。最終capability文書はZ01に維持します。

| Gate | 要件と確認した根拠 |
| --- | --- |
| G05 arena | [Source region失敗／call](../../tests/arena_calls.rs)、[独立region解析](../../crates/semantics/tests/region_analysis.rs)、[stale generation／逆順drain](../../crates/runtime/tests/arena.rs)、[入れ子managed検査](../../crates/runtime/tests/managed_arena.rs)。Source VM／native arenaとfailure prefixも通過。 |
| G07 Text／Bytes | [UTF-8・checked index・元allocatorのbuffer検査](../../crates/runtime/tests/buffer.rs)と[native source比較](../../tests/support/native_core.rs)。確保／fuel／work境界とcleanup／result拒否を含む。 |
| G09 Vec | [Move／growth／loan／OOM・逆順破棄](../../crates/runtime/tests/vector.rs)、[source実行](../../tests/support/owned_vec.rs)、[native差分／failure fixture](../../tests/support/native_vectors.rs)。 |
| G10 Map | [Runtime Map検査](../../crates/runtime/tests/map.rs)、[sourceの正確なUTF-8 byte key／置換oracle](../../tests/support/owned_map.rs)、[native owner／入れ子／failure fixture](../../tests/support/native_maps.rs)。 |

4 familyとも[standalone source matrix](../../tests/support/native_static.rs)に入り、direct／response-fileのlink／run、fuel／work／provider／interruption／cleanup全scenarioとVM比較を実行します。[Windows全run 39258215633](https://github.com/carimatics/lang/actions/runs/39258215633)、head 165ae7d、job 118078789294は2026-10-19 13:36:00 UTCに成功しました。G08はV17、G11／G12はC export／validity／target全体、G15–G18はpackaging・代表conformance・測定・実debug根拠のため未完了です。元の成果や最終guideは削除しません。

F13完了（2026-10-19 JST）：6eb9018で受理済みsource core::panicを、非復帰の型検査・動的な符号付きi64 code・元のspan／論理stack・owner cleanupとともにVM／native実行できます。RFC 0014の判断036bb97をschema 3／private revision 3より先に記録し、旧profileとC serialization拒否を維持しました。両sourceをstandalone AOTへ追加し、**35 source／direct・response合計70 executable** となりました。検証は **1,493 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。Native **66件／276.00秒**、C ABI **45件／12.45秒**、fmt／Clippy／rustdocも成功です。既存の **3点** を取り除き、**103 → 104 done、39 → 36 pt** とします。**120行と追加77点は不変、完了で取り除いた見積もりは175点** です。従来gateは **7/18** のままで、C export／objectとtarget検証はF05–F07に維持します。

F05の実装前監査（2026-10-19 JST）：受理済みincrement／panic C exportは解析できますが、通常object APIは両targetでUnsupportedCInterfaceとなります。Headerのcreate／destroy／last_errorにraw C実装がなく、現Program／Context APIは借用Rust descriptorだけを受け取ります。Export／objectより前に実C context support用F14（初期2 pt）を明示します。F05はraw export／value／runtime／objectの全要件を暫定3 ptで維持し、C source実行の実験後に監査します。**120 → 121行、36 → 38 remaining pt、追加77 → 79点**、完了で取り除いた見積もり175点は不変です。元の成果を削除しません。F14後は別の前提作業を追加する前にsource exportを実行します。

F13の検証記録（2026-10-19 JST）：最初のworkspace全体実行はdelivery件数／labelの整合性検査で停止しました。表示と既存core signature／bindingの期待値を修正し、repository_consistency・未実行だったroot 3 target・全non-root workspace targetを完了し、bindingも個別再実行しました。Test executable単位で照合し、**全88 suite／1,493 test成功** を確認しています。完了済みstandalone AOT matrixは繰り返していません。追加した最初のfaultの論理stack比較はnative panic 2 testを別途通過しました。最終Clippy **25.09秒**、rustdoc **36.48秒**、fmt成功。成功suiteの合計 **370.82秒** は検証の観測値であり、実作業時間やruntime benchmarkではありません。

F09のWindows evidence（2026-10-19 JST）：[全run 39251972828](https://github.com/carimatics/lang/actions/runs/39251972828)、head 0d43a41、job 118057726070は2026-10-19 12:59:33 UTCに成功しました。Fmt／Clippy／tests／docsとprototype benchmarkを含みます。Source drop snapshotの根拠であり、F13 panicと完全なC export／platform gateには各snapshotの検証が必要です。

F09完了（2026-10-19 JST）：59f0f08で受理済みsource core::dropを既存DropValue／Unitへ下げます。Copyの元の値を保持し、owner move・lexical loan・未対応loan特殊化の診断を維持します。Bytesと最後のstrong／weak期限切れsourceはVM／native・schema 2で42を返します。即時破棄順、cleanup失敗の一度だけの消費とsecondary、全有限fuel／work／provider拒否を検証し、両sourceを共通standalone matrixへ追加しました（33 source／direct・response合計66 executable）。必須検証：**1,486 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。Native **64件／267.49秒**、**C ABI 44件**、fmt／Clippy／rustdoc（**18.25秒**）も成功です。F09の **2点** を取り除き、**102 → 103 done、41 → 39 pt** とします。**120行と追加77点は不変、完了で取り除いた見積もりは172点** です。F04以降は分割で2点を明示し、dropで2点を完了したため、source機能が完成しても残り総点は同じです。従来gateは **7/18** のままで、source panicとversion付き契約／encoding判断はF13に維持します。

F04 Windows follow-up（2026-10-19 JST）：96593d5のrun 39239925813はC ABI 43／44件成功後、失敗fixtureの末尾return重複によるMSVC C4702（/WX）で停止しました。876b908でreturnを1つにし、macOSにも-Wunreachable-codeを追加しました。[後続の全run 39247826059](https://github.com/carimatics/lang/actions/runs/39247826059)、head 876b908、job 118043984510はfmt／Clippy／tests／docs・prototype benchmarkを含む全工程が成功しました。F04の型付きscalar／aggregate import snapshotの証拠であり、F09 source dropやF07の完全なC export／platform gateの証拠ではありません。

F09／F13の着手前監査（2026-10-19 JST）：source core::drop／panicは未認識です。既存MIR DropValue／Unitでdropは形式変更なく実装できますが、ownership Termには明示panicがなくschema 2の命令／core tableは閉じています。従来の暫定3点をF09 drop（2点、doing）とF13 panic（初期3点、暫定）へ分割し、新panic encoding／native契約の前にversion／互換性判断を記録します。Source非復帰、動的i64 code／span／stack、VM／native cleanupをすべて維持し、無関係なunwrapへの偽装や無version tag追加は行いません。F05は両方に依存します。119 → 120行、残り39 → 41点、追加分75 → 77点となり、完了分170点は不変です。元の要件は削除しません。

F04完了（2026-10-19 JST）：cfd0987でfinite Copy C importを実context allocator越しにVM／native実行します。Field単位の転送でrecord／空record、enum、入れ子Option／Result／core error、aggregate／scalar／Unit result、256混合引数に対応します。名前付きsourceは84を返します。不正output／status、全有限fuel／work境界、最初の3 allocation拒否でreport・usage・結果非公開・元allocatorによる解放が一致します。必須検証：**1,481 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。**C ABI 44件／12.15秒**、native **62件／267.18秒**、fmt／Clippy／rustdoc（**21.41秒**）も成功です。F04の **3点** を取り除き、**101 → 102 done、42 → 39 pt** とします。**119行と追加75点は不変、完了で取り除いた見積もりは170点** です。従来gateは **7/18** のままで、raw C factory／export／object公開、packaging、ownerとCの複合conformanceは未完了です。

F11のWindows evidence（2026-10-19 JST）：[全run 39234763047](https://github.com/carimatics/lang/actions/runs/39234763047)、head 1a8fcad、job 118001054052が成功しました。F11／revision 2のscalar source snapshotの証拠であり、F04 aggregateや残るF07／platform gateの証拠ではありません。

F11完了（2026-10-19 JST）：a37b053でC compiler産の実callbackをsourceからVM／生成native codeで実行します。名前付きscalar／Unit fixtureは59、直接callは42、上限256引数では32,896を返します。Context／code／実allocatorの結び付け、正規output、foreign失敗、共有control／workを強制します。新規7 fixtureには全有限fuel／work境界、OOM、interruption／deadline、同一ABIの別owner／allocator交換の作用前拒否を含みます。RFC 0012改訂43edb9bの後にC対応revision 2を生成し、通常のrevision 1 symbolと外部C profile 1を維持します。必須検証：**1,478 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。**C ABI 41件**、native **62件／262.13秒**、fmt／Clippy／rustdocも成功です（rustdoc **21.29秒**）。F11の **3点** を取り除き、**100 → 101 done、45 → 42 pt** とします。**119行と追加75点は不変、完了で取り除いた見積もりは167点** です。従来gateは **7/18** のままで、汎用aggregate、raw C factory／export／object公開、完全なpackaging／platform検証は未完了です。

F02のWindows evidence（2026-10-19 JST）：[全run 39222466737](https://github.com/carimatics/lang/actions/runs/39222466737)、head 40e6c34、job 117960765077が成功しました。F11／revision 2より前のsnapshotであり、新しいC source経路や残るF07／platform gateの証拠ではありません。

F02完了（2026-10-19 JST）：7d4fea7で共通config／context gate、code owner／allocator provenance、明示C import grant、F10受入れ／result検査、再入／poison拒否、callback／body／reader panicの上限付きcontainmentを完成しました。16件のfailure／state fixtureでforeign status、primary／callsiteの保持、cleanupの切捨て／件数、summary copyを検証します。必須検証：**1,471 tests / 88 suites**、失敗0、手動timing testのみ1件ignored。Nativeは **62件／266.07秒**、fmt、Clippy（**21.57秒**）、rustdoc（**20.93秒**）も成功です。F02の残り **2点** を取り除き、**99 → 100 done、44 → 42 pt** とします。**118行、追加72点は不変、完了で取り除いた見積もりは164点** です。従来gateは **7/18** のままです。Raw C descriptor read、正確なcallback呼出し、foreign allocator bridge、VM／native adapter、output公開はF04／F05、実platform実行はF06／F07に維持します。C実行／object／schema 2の拒否も保ちます。

<details>
<summary>F04／F11着手前・見積もり監査</summary>

F04／F11の着手前・見積もり監査（2026-10-19 JST）：CLIで受理済みC例を実行するとUnsupportedCInterfaceとなり、fuel／host call／service workはいずれも0です。Ownership VM／nativeにはOp::Import dispatchがなく、安全なcontext／header／value metadataだけではC呼出し、allocator provider、managed値とCの転送を供給できません。従来の合計3点見積もりを撤回し、実C code・共通context・VM／native越しに最初のscalar／Unit C source callbackを実行するF11を **初期3点** で追加します。全aggregate転送、allocator／config統合、failure conformanceを担うF04は、そのfixture後の **暫定3点** として残します。**118 → 119行、残り42 → 45点、追加分72 → 75点** とし、完了で取り除いた **164点** は不変です。F11をdoingとし、**100 done / 1 doing / 18 todo** です。F09／F05-F07を含む元のimport／export／platform全成果を維持し、次の区切りにはhelperだけでなくsource実行を必須とします。F04の残る範囲はF11後に再監査し、現時点で見積もり妥当とは断定しません。

</details>

F10のWindows evidence（2026-10-19 JST）：[全run 39218185835](https://github.com/carimatics/lang/actions/runs/39218185835)、head d4ddc65、job 117946926317が成功しました。F02より前のsnapshotであり、実C source実行の証拠ではありません。

F10完了（2026-10-19 JST）：5108069でF03 metadataを使ったpadding安全なC value／address事前検査を完成しました。入力read前にargument／result／context全体のshape、null／alignment／extent／overflow、output overlapを検証します。Readerは初期化済みfield／tag／reserved／unused unionだけを要求し、実C address／lifetimeの有効性はtrusted caller／adapterの義務を維持します。実union extentからenum末尾paddingを除外しunionの丸めを含め、生成C headerでも独立にassertします。正規tag zeroは受理し、未知tag、非正規bool／Unit／reserved／unused byteを拒否します。Padding readを禁止するReader、全address検査後のread、const inputのoverlap、output拒否、共有work、interruption／deadline／read failureを新規10 fixtureで検証しました。最終検証：1,455 tests / 88 suites, zero failures, one manual timing test ignored; native 62 in 265.39s。Fmt／Clippy／rustdocも成功です。**F02から配分した1点** をF10完了で取り除き、**98 → 99 done、45 → 44 pt** とします。118行と追加72点は不変で、完了で取り除いた点数は **162点** です。従来gateは **7/18** のままです。F02にはcontext／config／bindings／provenance／capability／poison／再入／callback containmentを残り2点で維持し、F04／F05で実VM／native C adapterにこの事前検査を強制します。統合まではC unit全体の実行／object／schema 2拒否を保ちます。

F03のWindows evidence（2026-10-19 JST）：[全workflow 39213444139](https://github.com/carimatics/lang/actions/runs/39213444139)、head 86cfa01、job 117931773193が成功し、native MSVCでの生成C11 header testも完了しました。F03のsnapshotでF10より前であり、C callback／exportの実行や残る代表性能gateの完了を示すものではありません。

過去のF02／F10の範囲配分（2026-10-19 JST）：Cのpaddingは未初期化の場合があるため、native adapterは入力C object全体をRustのu8 sliceへ変換できません。まずF10として、addressだけでnull／alignment／extent／overlapを事前検査し、通常paddingを要求しない上限付きread interfaceでfield／tag／reserved／unused unionを正規検証します。F03のC layoutと共有service-work計上を使い、raw addressの実際の生存・有効性はRFC 0008どおりtrusted native caller／adapterの義務とします。**F02初期3点から1点を配分**し、残る **2点** にcontext／binding／config全体の事前検査、owner／allocator provenance、poison／再入、callback containmentを維持します。F02はF10に依存し、F04／F05で両経路を実adapterに強制します。**117 → 118行、残り45点・追加72点・完了分161点は不変**です。F10をdoingとし、元の要件やplatform要件は削除しません。

F03完了／残る範囲の監査（2026-10-19 JST）：dee7f34でprofile 1のC layout、正確なfunction／type／artifact descriptorとSHA-256 fingerprint、callback／export／support宣言とlayout assertion付きC11 headerを完成しました。元のscalar vectorと独立aggregate／type／artifact vectorが両target・両方向で一致し、実C compilerでscalar／aggregate／Unit署名、headerの再読込み／共有型、両Clang targetの配置を検証しました。Resource／alias／depth fixtureで、artifact合計の早期上限とsource／MIR／展開後Cの一致した暗黙builtin payload境界も確認しました。最終検証：1,445 tests / 88 suites; zero failures, one manual timing test ignored; native 62 in 264.91s。Fmt／Clippy／rustdocも成功です。F03完了で **3点（45 → 42）** を取り除きます。同じ監査で既存要件の計上漏れも確認しました。受理済みRFC 0008のcore::drop<T>とcore::panic<T>は現行CLIでSRC0024となり、明示的な未完了成果がありません。Source型検査／発散、move／loan規則、VM／nativeのcleanupとpanic code／span検証をF09・暫定 **3点（42 → 45）** として追加し、実sourceのpanic／export fixtureのためF05の前提に加えます。受理済み要件の未実装であり、新言語機能やschema 1／2の変更ではありません。**97 → 98 done、116 → 117行、残り45点**。追加見積りは **69 → 72点**、完了で取り除いた点数は **158 → 161点** です。従来gateは **7/18** のままです。次は検証済みC metadataを使うF02で、F04-F07、F09、その他のM2要件も維持し、C実行／object／schema 2の拒否を保ちます。

F01のWindows evidence（2026-10-19 JST）：[全workflow 39207649118](https://github.com/carimatics/lang/actions/runs/39207649118)、head cdae1ee、job 117913869152が成功し、fmt／Clippy／全テスト／doc／prototype benchmarkを完了しました。F03より前のsnapshotであり、残るC相互運用／性能gateの完了を示すものではありません。

過去のF02／F03着手前監査（2026-10-19 JST）：runtimeのlayout.rsはmanaged／VM向けでC layoutではないと明記しています。製品用のLANGCABI descriptor／header生成器はなく、RFCのscalar fingerprintと手書きC layout fixtureだけがあります。F02には厳密な宣言digestの照合と実C field／reserved storageの検証が必要なため、F03をF02の前提に加え、先に実装します。範囲は上限付きprofile 1のfunction／type／artifact descriptorとSHA-256、target別C layout、layout assertion付きC11 headerで、固定RFC vectorと実C compilerで確認します。検証済みC対応MIRを再利用し、F02のcontext／capability／poison／unwind検査、F04 adapter、F05-F07の全要件を維持します。F03をdoing・3点とし、**97 done / 1 doing / 18 todo / 45 pt** とします。作業数、見積り、収支、元のgateは不変です。

F01完了と最初のC実行の監査（2026-10-19 JST）：819ee7cで、03239a1の明示的なC MIR検証（RFC改訂68240cc）へsource／HIRのC型検査とlexicalなunsafe scopeを接続しました。未使用scalar宣言を含む有効なC宣言は独立検証済みownership MIRへ到達し、unsafe呼出し、型、scope、move、loan、固定profile違反にはsource診断を出します。Unsafeはruntime branchやcapabilityを追加せず通常のcleanupを維持します。ローカル必須検証は **1,437 tests / 87 suites**、失敗0、手動timing testのみ1件ignored。Nativeは62件・255.76秒で、fmt／Clippy／rustdocも成功です。CLIでC例のcheck成功、unsafeなしのC callへのSRC0025と宣言span、C実行のfuel 0／host call 0での拒否を確認しました。**96 → 97 done、48 → 45 pt**。116行と追加分69点は不変で、完了で取り除いた点数は **158点** です。F01はdoneとし、従来gateは **7/18** のままです。最初の実C source callには、F02の明示的なC context／grant／validityとpoison／unwind事前検査、F03の正確なC layout／descriptor／header、続いてF04の型付きimport adapterが必要です。検証済み型graphと既存instance／allocation／controlを再利用し、通常host grantからC bindingは有効にしません。F05のexportとF06／F07のnative platform fixtureを維持し、schema 2もC識別を引き続き拒否します。この監査でscopeや見積りは追加しません。

F01着手前／F08分割の過去監査（2026-10-19 JST）：実行したprobeではunsafeという旧関数名が通り、extern "C"はSRC0008の字句エラーでした。ParserとHIRのHostImportは従来のi64引数1個のhost形式で、HIRにunsafe statementはありません。Accepted RFC 0008にC構文は定義済みで、SRC0024の共用もRFC 0007が明示するため、診断番号の再設計は不要です。Losslessなquoted-C import／exportとunsafe block、formatter、予約語移行、上限付きrecoveryをF08（初期2点、6c32322で完了）として追加します。未実装の意味解析は通常host importとして扱ったり無視したりせず拒否します。F01にはC型／unsafe／sourceとMIRの全要件を暫定3点で維持し、coding前に再度範囲を確認します。**115 → 116行、47 → 50 pt**。追加分は **66 → 69点**、完了で取り除いた **153点** は不変です。元のC／ABI／runtime／platform要件は削除しません。F08／F01のfrontend成果後、境界基盤を増やす前に最初のC source実行経路を確認します。

N07 native／CI上限の証拠（2026-10-19 JST）：516f663の[Windows全run 39198008813](https://github.com/carimatics/lang/actions/runs/39198008813)が33分56秒で成功しました。1,404 test／87 suite、native 60件（748.74秒）、31 source／62 executableのdirect／response-file standalone test、fmt／Clippy／rustdocと、cleanなschema-20 prototype benchmarkのuploadが成功しています。先行run 39194948229がbenchmark中に30分を超えた後、119f9ffの60分上限と5fe96deのbuild／実行分離を検証できました。Test build 4分49秒、実行16分20秒、docs 17秒、benchmark／release build 10分03秒で、いずれもCI時間です。N07／G14のnative証拠は完了とし、代表的M2の全体CI／性能の証明はQ02／B01-B04へ残します。F08は別の[Windows run 39200895976](https://github.com/carimatics/lang/actions/runs/39200895976)で検証し、N07のsnapshotで後続source変更の成功を主張しません。Task／見積もりの変更はありません。

F08の完了成果（2026-10-19 JST）：6c32322でF08を完了しました。正確なquoted-C import／export宣言とstatementのみのunsafe blockをlosslessにparse／formatし、comment、fixtureの全prefix、上限付きrecoveryを確認しました。型名を含む従来unsafe名はSRC0024となり、unsafeの自動fixは付けません。CLI checkは未対応のC意味境界に字句エラーではなくSRC0025を返し、従来のi64引数1個のhost importは維持します。**95 → 96 done、50 → 48 pt**。116行と追加分合計69点は不変で、完了で取り除いた見積もりは **162点** です。Frontendのfocused 352 test、workspace **1,421 test／87 suite**（native 61件、250.59秒、手動timing test 1件はignored）、fmt／Clippy／rustdocが成功しました。C bindingの意味規則、MIR接続、capability、実C adapterはF01-F07へ残し、従来gateは **7/18** を維持します。

N06着手前監査（2026-10-19 JST）：owned objectのbyte列と対応metadataは生成できますが、検証付きgatewayはJITのCompiledFunctionが保持するaddressだけをdispatchします。Code ownerの寿命を保持し、同じ検証付きprogram全体のgatewayを利用するtrusted linked-image境界と、実際のlinked owned codeの実行証拠をM2-N19（3点）として明示します。N06のmacOS direct／response-file全体conformance、P02のclean-install runtime packagingは維持します。VMへのfallback、packet／schema変更、sourceの部分対応で置き換えません。**114 → 115行、51 → 54 pt**、新たに判明した見積もり **63 → 66点**、完了で取り除いた146点は不変です。実装前にN19をdoingとして実装を開始しました。現在はd53aac0でN19を完了し、230予算でlinked Mach-OとVMが一致し、target／symbol／予算拒否とowner解放も検証済みです。**92 → 93 done、54 → 51 pt**、追加分66点は不変、完了で取り除いた見積もりは149点です。全workspace 1,414 test／87 suite、native 60件、fmt／Clippy／rustdoc成功（計測専用1件はignore）。N19完了時点では、次のN06のstandalone executable検証を暫定2点としていました。

N05完了／G06 exit監査（2026-10-19 JST）：`f6d9fc4`で現在の全managed service／payloadの計量をVM／nativeのcall、loan、viewへ接続しました。24件のsource programで有限予算の全prefixを比較し、他control／provider拒否、guest panic、secondary cleanup、source／artifact CLIの検証も成功しました。予算超過後も数値付きerrorと必須retirementを維持し、Rust contextの移行とserialized schemaが不変であることを文書化しました。**91 → 92 done、54 → 51 pt**。暫定3点を消化し、taskや見積もりは追加しません（**134 + 63 − 146 = 51**）。全baseline **1,410 test／87 suite**（手動計測1件は別途成功）、fmt／Clippy／rustdoc成功。D28–D34、V07／V08、C07、N05の生成cleanup・順序・returnの証拠と、owned execution 139件、native 56件に基づきG06を完了とします。従来gateは **6/18** となり、G14のmacOS／Windowsでのowned object実link／runは未完です。他gateはそれぞれの最終evidence reviewを維持します。

N05着手前監査／N18分離（2026-10-19 JST）：CLIで--service-work 37を指定するとunknown optionでexit 2となることを実行確認しました。Map／Vec／Shared／Arenaの4 runtime fileだけでも公開methodは74件あり、payloadの走査・copyとVM／nativeの接続、CLI／reportを暫定1点にまとめた見積もりは維持しません。N18のCLI／report監査を初期1点（doing）、N05のmanaged統合と全体conformanceを暫定3点とし、旧合計1 → 4点（+3）へ明示的に見直します。API数は工数の換算率ではありません。**113 → 114行、52 → 55 pt**、追加見積もり**60 → 63点**。N05の全service／payload、nativeのfailure／return、version付きreport移行、完全なVM／native conformance要件は維持し、managed eventの実装前監査を引き続き必要とします。

N17完了監査（2026-10-19 JST）：共通Bytes／Text計量とVM／nativeの有限予算programを実装（初期配分1点、085700c）。RFC 0013のunit全体の受理条件を先に明示改訂（143decc）し、未使用関数も含めた未対応eventをeffect前に拒否します。6 programの全予算prefixとguest callを検証し、fixtureは37単位で67を返します。Runtime境界test 3件、native 53件、全workspace 1389件／85 suite、fmt／Clippy／rustdoc成功。**89 → 90 done、53 → 52 pt**、追加見積もりは**60点のまま**、完了で取り除いた見積もりは**142点**。N05には全managed／payload・driver／CLI／report移行・完全なconformanceを残し、暫定1点は着手前に監査します。元のgateは5/18のままです。

N05縦slice配分（2026-10-19 JST）：既存2点をN17の共通Bytes／Text計量とVM／nativeのbuffer program実行（**初期見積もり1点**、doing）へ1点、残るN05のmanaged service・driver／report・全体conformanceへ **暫定1点** 配分します。**112 → 113行**、**53 ptは不変**、見積もり追加なしです。計量未完のserviceを使うprogramでは有限予算をeffect前に拒否し、RFC 0013の全成果を維持します。

N16完了／配分監査（2026-10-19 JST）：旧N05合計3点をN16のmeter／guard（**初期見積もり1点**、完了、`06c8e1d`）と、N05の全service接続／conformance（**暫定2点**）に配分します。**111 → 112行**、見積もり追加なし（追加分合計 **60点は不変**）、完了で **54 → 53 pt** です。共通APIで厳密／atomicなbatch課金とcleanup後のfailure数値を検証し、VM entry／preview／nativeで未完profileの有限予算をprovider effect前に拒否します。RFC 0013全event、driver／report移行、全VM／native service検証はN05に残します。

N15決定監査（2026-10-19 JST）：[RFC 0013](rfcs/0013-service-byte-work-budget.md) を実験段階の委任でacceptedとしました（`0a38beb`）。共通の累積event、構造化拒否、既知の依存costの一括確保、必須cleanupの課金免除、互換性規則を定義します。Map 20 testの成功は局所的な実現性の裏付けで、global実装／conformanceはN05の3点に維持します。N15の **初期見積もり1点** を同じ更新で追加・完了し、**110 → 111行**、**54 + 1 − 1 = 54 pt**、追加分合計 **59 → 60 pt**、完了分 **139 → 140 pt** です。元の要件は削除しません。

N05分割／N14完了監査（2026-10-19 JST）：着手前testでUTF-8の最大cell数がnative 49／VM 57と異なり、loan登録とreturn escrowが抜けていました。失敗の比較testではprimary spanも関数全体になっていました。N14（**初期見積もり3点**、完了）でcontrol／usageを共通化し、両不具合を修正し、未対応の準備済み容量設定をeffect前に拒否します。旧N05合計3点をN14制御3点と残るN05 service 3点へ分割し、完了前は **109 → 110行**、**54 → 57 pt**、追加分合計 **56 → 59 pt** です。N14の3点を完了し **57 → 54 pt** になりました。順序付きservice／payload全体のconformanceと設定可能なbyte-work admissionはN05に維持し、具体的なservice-work契約を実装前に設計文書で定義します。

N04完了監査（2026-10-19 JST）：字句arenaのowner・操作、collection／call越しのread custody、入れ子brand検査がnative／VM fixtureで成功しました。範囲は3点のままで、追加task・見積もり増はありません。**57 → 54 pt**、**109行は不変** です。順序付き失敗・制御とservice／payload全体のconformance、設定可能なbyte-work admissionはN05に残します。

N04分割・完了監査（2026-10-19 JST）：初回shared／arena sourceはVMで9を返し、nativeではshared core操作とarena字句開始が別々に未対応でした。旧N04合計3点をN13 shared／weak（**初期3点**、今回完了）とN04 arena 3点へ分割・再見積もりし、**108 → 109行**、完了前の残りを **57 → 60点**、追加分合計を **53 → 56点** としました。N13の3点完了で **60 → 57点** です。Sourceの排他アクセス規則は変えず、custody fixtureは所有clone経由で置換します。N04にはarenaの字句lifecycle、collection／call越しのread custody、入れ子brandを残し、service／payload全組合せと設定可能なbyte-work admissionはN05に維持します。

N03見積もり・依存関係監査（2026-10-19 JST）：keyの借用・消費、旧所有値の結果保護、切り離したkeyと拒否入力の解放の接続が必要なため、実装前にN03を **1 → 3点**、残りを **58 → 60点**、追加分合計を **51 → 53点** へ再見積もりしました。初回sourceはVMで49、nativeでUnsupportedとなり、現在はnativeでも成功します。最後の3点を完了し **60 → 57点**、108行は不変です。N04はN12／N03に依存し、collection／call越しのread custodyと入れ子brand検査を明示して維持します。N05と元のplatform／FFI／performance／debug・guide全要件を残します。

N12見積もり・依存関係監査（2026-10-19 JST）：当初のnative参照先はframe cellとallocator権限だけでした。Vector-backed projection／変更／置換と入れ子cleanupの接続が必要なため、実装前にN12を **1 → 3点**、残りを **59 → 61点**、追加分合計を **49 → 51点** へ再見積もりしました。初回sourceはVMで9、nativeでUnsupportedとなり、現在はnativeでも成功します。最後の見積もり3点を完了し **61 → 58点**、108行は不変です。N05にはN02／N11／V08に加えN12／N03／N04の依存を明示し、受理済みservice／payloadの全組合せ、順序付きexit、設定可能なbyte-work admissionを維持します。

**直近の完了 — N16**（`06c8e1d`、2026-10-19 JST）：Controlのservice-work設定、Usage counter、checkedなatomic batch、数値付き動的failureと共通変換を実装しました。Runtime 4 testで厳密／overflow／batchの拒否、polling／fuel不変、mandatory cleanup後のprimary数値とsecondaryを確認します。Entry／preview／nativeの有限予算0／1／u64::MAXは、service未接続の間はprovider effect・usageなしで拒否します。全 **85 suite・1385 test**、fmt／Clippy／doc成功。Service event接続はN05のままです。

N16検証時間：runtime重点4 testは報告値0.00秒、profile拒否の重点1 testは0.15秒、all-feature native suite 52 test／11.25秒。全体suiteのCargo compile 61秒、test報告時間合計88.29秒、ログ作成から最終書込まで752.13秒。Clippy 68秒、doc 28.71秒です。検証区間は実作業時間の計測値ではありません。

N17検証実測：Focused runtime 3件／0.00秒、有限buffer予算prefix 1件／0.69秒、全feature native 53件／11.29秒。全testのCargo compileは47.51秒、test報告時間の合計は88.20秒、log作成から最終書込みまで529.91秒。Clippy 45.34秒、rustdoc 19.28秒。検証の観測値であり、実作業時間やruntime benchmarkではありません。Build cacheの追加調査でdepsに645,863 entry／33.05 GiBがあり、一覧走査は12.26秒でした。Cargoのpackage単位の整理で生成物439,988 file／21.6 GiBを削除しました。同じ集計方法で整理後は228,369 entry／13.62 GiB、走査1.885秒です。各1回の走査はfilesystemの集計costを示し、検証実時間の残差すべての原因やsuite全体の高速化は示しません。起動のみの一覧表示2件は各0.00秒でした。Cache整理によるsource変更とfull baselineの再実行は不要で、plan更新後はrepository consistencyのみ実行します。

N07のローカル検証実測（2026-10-19 JST）：macOS baselineは1,415 test／87 suite成功、native 61件／255.64秒、計測専用1件ignore。全test実時間337.35秒、Cargo compile 0.60秒、suite報告時間合計334.29秒。Clippy実時間1.33秒、rustdoc 10.46秒、linked-image probe 1件／2.48秒。ローカル検証の時間で、Windows証拠・実作業時間・runtime benchmarkではありません。Planのみの変更はrepository_consistencyで検証します。

N07移植前の証拠確認（2026-10-19 JST）：当時の最新[Windows全workflow成功run](https://github.com/carimatics/lang/actions/runs/39007443435)はhead 88325a5、最終更新2026-10-17 05:54:49 UTCで、owned standalone matrix以前でした。この旧runをN07完了の証拠には使わず、上記の現行実装9589022のnative Test実行を確認して完了としました。

</details>

</details>

## 進捗と見積もり変更

詳細Todo導入時の `8605416`（2026-10-18 JST）以降、検証済み成果は **35 → 133件（+98件）**、作業一覧は **87 → 133件（+46件）** になりました。新たに可視化した作業は既存要件の実装であり、各件が同じ工数を表すわけではありません。

| 詳細計画導入時の残り | 追加・再見積もりの差分合計 | 完了により取り除いた見積もり | 現在の残り |
| ---: | ---: | ---: | ---: |
| 134 pt | +109 pt | −243 pt | 0 pt |

**134 + 109 − 243 = 0 pt**

G02完了／G03実装前監査（2026-10-20 JST）：`2297a07`で実Copy／owned／C-export source packageへunit全体のWindows CodeViewを追加し、linkしたPDBのfunction／位置を検証しました。local 1,592-test／95-suite baselineと同一commitのnative Windows必須baselineが成功しました。3 ptを完了し、**130 → 131 done、残り6 → 3 pt、retired 237 → 240 pt**、**133行／追加109点は不変**です。原出口は**17/18**を維持します。G03の両host実stepping／backtraceとlink後Mach-O relocation修正、Z01の最終日英guide／全監査を残します。

G01完了／G02実装前監査（2026-10-20 JST）：`4b0e902`でcanonical MIR snapshot／machine rangeを複数function／fileのDWARF 4へ変換し、実Mac Copy／owned／C-export packageへ接続しました。native LLVM、symbol relocation、UTF-8／CRLF／EOF／境界／cross-file test、新しい全baselineが通りました。既存3 ptを除き、**129 → 130完了、残り9 → 6 pt、完了で除いた234 → 237点、133行／追加109点は不変**。G02だけを既存3 ptで選び、共通source indexと現COFF writerを使い、private ABI parameterをsource variableとして扱いません。Windows metadataと両host実debugger証拠までG18は未完了で、最終Z01も不変です。

B04完了／G01実装前監査（2026-10-20 JST）：`990e422`で両hostの解釈と実行可能なread-only再計算toolを公表しました。両native captureのcompiler／runtime／workload／harness／build／toolchain blobは同一で、workload 270＋専用startup 90観測、parity／checksum／退役／provenance／ばらつきを独立に照合しました。tool実行で比率6行が再現し、限界とcommandを公表しています。既存1 ptを除き、**128 → 129完了、残り10 → 9 pt、完了で除いた233 → 234点、133行／追加109点は不変**。G17を完了し、**17/18条件、履歴上の重み105/113 pt（92.9%）**です。G01だけを既存3 ptで選び、source range／debug emitterを再利用しつつ、両platformのG02／G03と最終Z01を維持します。この実装前監査で見積もり追加はありません。

B03完了／B04実装前監査（2026-10-20 JST）：`02b348f`にrun 39385041497のcleanなnative Windows report `dced632`を保存し、workload 135＋startup 45観測、全checksum／退役／provenance／集計値、実際の非zeroなK32 memoryが通りました。既存2 ptを除き、**127 → 128完了、残り12 → 10 pt、完了で除いた231 → 233点、133行／追加109点は不変**。B04だけを既存1 ptで選び、両hostの解釈／再現手順をまとめます。計測には大きなscope内runtimeコスト差とreference／startupのばらつきがあり、限界を公表します。G17はB04まで未完了で、両hostのdebugger作業とZ01は不変です。

B02完了／B03実装前監査（2026-10-20 JST）：`95b97cb`にrun 39383945072のcleanなnative Mac report `ecc0fd1`を保存し、workload 135／startup 45観測、全checksum／退役／provenance／集計値の検証が通りました。既存2 ptだけを別に除き、**126 → 127完了、残り14 → 12 pt、完了で除いた229 → 231点、133行／追加109点は不変**。B03だけを既存2 ptで選び、Windowsの同一scopeを維持します。G17は速度閾値でなく計測が要件で、観測したコスト差／ばらつきと両hostの限界はB04で公表します。G17／G18／Z01は未完了です。

B01完了／B02実装前監査（2026-10-20 JST）：`41250a2`とprivate C alignment修正`fffe86c`でsource／Rust／C一致と計測harnessを完了しました。両native hostのcleanな`fffe86c`で21組oracle／unwindが通り、raw JSONを保存しています。既存2 ptだけを除き、**125 → 126完了、残り16 → 14 pt、完了で除いた227 → 229点、133行／追加109点は不変**。B02だけを既存2 ptで選び、macOSの全計測後に完了を計上します。B03／B04、両hostのdebugger作業、Z01は引き続き必須です。元のG17は代表native workloadと等価Rust／Cの比較で、実sourceの3 workloadが範囲を維持しています。新たな前提や見積もりは発生していません。

Q02完了／B01実装前監査（2026-10-20 JST）：`1d0c0dc` のMac限定branch CIと、同一実装／fixtureのWindows `3db077f` 全baseline成功を照合し、既存2 ptを完了しました。**124 → 125完了、残り18 → 16 pt、完了で除いた225 → 227点、133行／追加109点は不変**。Q01／Q02の実source・runtime・AOT／C・resource証拠でG16を完了し、従来gateは16/18。B01だけを既存2 ptで選び、G17／G18／Z01と元の全要件を維持します。

Q01完了／Q02実装前監査（2026-10-20 JST）：`3db077f` の複合source、末尾panic loan修正、通常／panic全resource prefix、実AOT／実C／保存C objectと全baselineで既存3 ptを完了しました。**123 → 124完了、残り21 → 18 pt、完了で除いた222 → 225点、133行／追加109点は不変**。Mac branch CI起動と対応両host証拠のQ02だけを既存2 ptで選びます。G16／B／G／Z01と元の全要件を維持します。

P02完了／Q01実装前監査（2026-10-20 JST）：`c54af31` の両host別consumer成功とJSON照合で既存3 ptの隔離installを完了しました。**122 → 123完了、残り24 → 21 pt、完了で除いた219 → 222点、133行／追加109点は不変**。Q01だけを既存3 ptで選びます。対応Windows全baselineも成功し、G15を完了、RFC 0020をimplementedとします。他のQ／B／G／Z01と元の全要件を維持します。

P12完了／P02実装前監査（2026-10-20 JST）：`4acd99f` で既存3 ptの公開linkを完了しました。**121 → 122完了、残り27 → 24 pt、完了で除いた216 → 219点、133行／追加109点は不変**。上記の成功checkと分離consumer方針に基づき、P02のみを既存の暫定3 ptで選びます。G15／Q／B／G／Z01と元の要件を維持します。

P07完了／P12実装前監査（2026-10-20 JST）：`3080d9e` で既存2 ptのSDK組立を完了し、正確なCargo選択Rust／native依存、startup／header／説明resource、上限付き検証済み公開、通常／release／unwind policyを実装しました。SourceなしSDK起動3経路が成功し、release owned起動とabort拒否は上記の別証拠で示します。旧手動test fixtureのvariant選択も追加加点なしで修正しました。**120 → 121完了、残り29 → 27 pt、完了で除いた214 → 216点、133行／追加109点は不変**。検証済みsnapshotと成功した手動SDK linkを前提に、P12の公開linkだけを暫定3 ptで選びます。P02／Q／B／G／Z01と元の全完了条件を維持します。

P10完了／P07実装前監査（2026-10-20 JST）：`6024931` で既存3 ptのscope付きproduction runtimeを完了しました。実sourceなしCopy／owned／C driverが成功し、identity／control／記録を解決前に拒否し、owned failureのsource／usage／allocator／poisonを保持して実C登録をhost unwind時にも解放します。Fixture Runtimeではなく元のP05／P06 runtimeを使います。**119 → 120完了、残り32 → 29 pt、完了で除いた211 → 214点、133行／追加109点は不変**。上記の正確なartifact実験後、P07の組立だけを暫定2 ptで選び、必要native archive、上限付きmetadata、exclusive公開、明示外部toolchainを保持します。P12／P02と元の全Q／B／G／Z01成果は残ります。全local baselineと前／現Windows snapshotは上記に記録し、SDK installや性能改善の完了は主張しません。

P11完了／P10実装前監査（2026-10-20 JST）：`01bf09c` は元の3 ptのmanifest／検証済みsnapshot成果を完了し、正確なobject／意味論／記録の境界をtool／native／provider起動なしに検査します。P09のWindows unused-mutと旧fuzz ABI初期化4箇所も追加task加点なしで修正しました。Parser fuzz targetはcompile成功し、campaignとは数えません。**118 → 119完了、残り35 → 32 pt、133行／追加109点は不変、完了で除いた208 → 211点**。P10はP05／P06上のscope付きproduction consumerだけを選びます。File検証はP11、SDK組立はP07、公開link commandはP12、隔離installはP02です。新前提や契約変更は特定されず、P10は暫定3 ptとし、拡張前に実sourceなし通常／C実行のcheckpointを示します。Mac: 1,570 tests / 91 suites, 0 failures, 1 manual timing test ignored; fmt, Clippy (27.02s), rustdoc (20.89s), all 11 fuzz-target Clippy checks pass. Snapshot tests 9 / 0.59s; native 76 / 280.80s; C ABI 74 / 20.27s。Windows：[run 39351953438](https://github.com/carimatics/lang/actions/runs/39351953438), head 01bf09c, in progress。元の全gateと最終guideを維持します。

P09完了／P10実装前監査（2026-10-20 JST）：`66ebb0c` でP09の元の3 ptのproducer成果を完了しました。完了分を分離し、**117 → 118完了、残り32 → 29 pt、完了で除いた205 → 208点**です。RFC 0020が要求する独立した2成果、非信頼SDK／program／fileの厳格検証と公開toolchain／linker commandが、P10の既存3 ptに隠れていました。Production readerと `link-native` は未実装です。検証済みsnapshotの **P11（初期3 pt）** と、正確なtoolchainでのlink／exclusive executable出力の **P12（初期3 pt）** を追加し、scoped production runtimeのP10は3 ptを維持します。既存accepted scopeであり、新しい言語featureではありません。**131 → 133行、追加103 → 109点、29 + 6 = 残り35 pt**。順序はP11 → P10 → P07 → P12 → P02とし、元の全gateを維持します。Mac baseline: 1,559 tests / 90 suites, 0 failures, 1 manual test ignored; fmt, Clippy (33.64s) and rustdoc (19.01s) pass; suite-time sum 450.31s; native 76 / 279.77s, C ABI 74 / 20.52s。Windows：[run 39347097096](https://github.com/carimatics/lang/actions/runs/39347097096), head 66ebb0c, in progress。残るpackaging／conformance／性能／debugの実装前判断と点数は暫定であり、実測時間と同一視しません。

SDK実装前監査（2026-10-20 JST）：P07の既存2 ptは、既存componentの組立に加えて公開source-native compilerコマンド、配布startup／runtime consumer、version／toolchain一致契約を隠していました。`src/main.rs`のcompileはportable artifactのみで、`prototype-object`は固定MIRだけを扱います。P08の契約判断（初期1 pt）、P09の製品用AOT compiler（初期3 pt）、P10の配布runtime／startup（初期3 pt）を前提として明示し、P07の最終組立2 ptとP02の両host隔離Copy／ownership／C install 3 ptは維持します。P06完了と分離して、**115 → 116完了、128 → 131行、追加96 → 103点、完了で除いた201 → 204点、29 − 3 + 7 = 残り33 pt**。元M2の成果は削除せず、P08受理前に未規定の公開動作を実装しません。

完了した作業と新たに判明した作業を分離しました。完了により見積もり216点分を取り除く一方、前提作業の可視化と再見積もりで109点増えています。これは計画上の単位で、時間や実工数の計測値ではありません。残り27点は暫定値で、FFI、packaging、debug、性能には引き続き実装前の実験が必要です。元の18完了条件は維持します。各成果の更新時にこの収支を検証し、追加分の合計は明示的なscope／見積もり監査がある場合のみ変更します。詳細計画の初期134点と、歴史的な完了条件の重み113点は別の指標です。

N13／N14の **初期見積もり各3点**、N15の **初期見積もり1点**、N16の **初期配分1点** を保持します。N16は既存N05の3点から1点を配分し、N05に2点を残したため見積もり追加はなく、**54 − 1 = 53 pt** です。N03／N12の1 → 3点監査、N11／N09の初期各1点も保持し、元の要件は削除しません。

以前のcode baselineの実測（2026-10-19 JST）：workspace Clippy **28.3秒**、全test **362.5秒**、rustdoc **14.2秒** の実時間（shell time）。Cargo報告のtest compileは **24.4秒**、suite時間合計は **87.2秒** です。Native境界suiteは **40 test・10.77秒** で成功し、service失敗matrixはcompile済みcodeを再利用してruntimeを毎回作り直します。検証所要時間であり、実稼働時間やruntime benchmarkではありません。文書だけの計画更新では `repository_consistency` を実行します。

現在のsource対応範囲：引数なしi64 mainでBytes／Text、arena allocation／read／replace／teardown、shared／weak allocation／clone／read／replace／upgrade、Vec new／len／push／pop／shared・mutable read、厳密なi64／Text keyのMap new／len／insert／get／removeを実行します。Storable payloadは入れ子collection、inline record／enum／Option／Result、再帰guest call越しに組み合わせられます。Shared publicationは推移的strong edge、arena storeは入れ子brandを検査し、projection／call越しにread custodyを保持します。Ownerはcleanupで保護したreturnとprepared replacementを通り、正常／失敗teardownを反復実行します。Ordinary loan、allocator authority、arena ownerは格納不可で、cross-region／shared-heap edgeとarena escapeを拒否します。Borrowed matchとpartial move禁止は維持します。V16／V15／V10は既存managed preview／推論／REPL profileに対応し、V13は明記したsource coverage内のprovisional-read loan終了を検証します。Indirectionを含むpreviewはV17です。V11はin-memory CFG argumentに対応しますが、現ownership source loweringは生成しません。Nativeの生成exitは累積workのconformanceまで検証済みで、両tier-1のstandalone実行はN06／N07で確認済みで、packaging／FFIの残要件はevidenceが必要です。上記source／runtime／tier-1根拠で4 core gateを完了とし、残る複合機能・preview・最終guideを明示します。

実装前・分割監査（2026-10-19 JST）：初回Bytes sourceはVMで1を返し、native allocator／Bytes接続の不足を確認しました。公開buffer APIがstorage操作を提供し、collectionには別途managed loan viewが必要です。N03の3点をN11 allocator／Bytes／Text（初期1点、今回完了）、N12 Vec（1点）、N03 Map（1点）へ分割しました。**106 → 108 task**、**N11の1点完了前の残り60点は不変** です。N05は設定可能なservice byte-work admissionを引き続き含み、exit conformance全体の前提としてN11に依存します。RFC 0012はVM／nativeで設定済み制御を一致させ、未対応設定を拒否することを要求します。具体的なservice-work契約は実装前に設計文書で定義する必要があります。Native／FFI／platform／conformance／performance／debug全要件、V17、Z01の英日guideを維持します。

現在のscope監査でG08／G11／G12を完了とし、従来の完了条件は **18件中14件完了**（重み84/113）です。V17は72950d6、P01／F07は修正済み15a50bbのWindows必須baselineを根拠とします。G15–G18のpackaging／conformance／性能／debugと最終guide Z01は未完了です。

この文書を実装Todoの正本とします。Roadmapには従来の受入条件とその履歴上の重みを残します。件数は独立して検証した成果の数であり、各件の作業量が等しいという意味ではありません。残りpointは積み上げによる暫定見積もりで、native／C／platform統合の確度は低く、時間や完成時期の予測ではありません。完了済み作業に後付けのeffort pointは与えません。完了行の *残り* pointは0です。

## 従来の数値が止まった理由

`367218e`（2026-10-18 00:14:21 JST）から `0c2c012`（03:24:22）まで、完了条件の重みは34 / 113のままでしたが、実装9commitとroadmap 9commitが追加されました。記録されたpassing-test baselineは840件から930件になりました。差分は48file、10,407行追加、78行削除です。Commit日時が示すのは3時間10分の区間であり、実測した稼働時間ではありません。Test数や行数は変更の裏付けであり、生産性や完成機能の指標ではありません。

原因は次のとおりです。

1. 大きなend-to-endの受入条件を日々のTodoにも使っていました。たとえばdrop項目には、cleanup engine、生成range、構築順序、入れ子report、failure CFG、allocator、layout、本番実行が隠れていました。前提作業が完了しても数値が変わりませんでした。
2. RFC 0007／0008で要件が確定した後も、残作業を積み上げずに113 ptを妥当と繰り返し判断しました。Ownership VM実行、fallible allocationの統合、ownership artifactのencodingなどは主に本文中にしか現れていませんでした。**113 ptを作業量予測として使うことを取りやめます。** 残り79の受入条件の重みは、実装残作業の妥当な見積もりではありません。
3. Compiler／runtimeの基盤を横方向に進め、ソース実行が未対応のままでした。必要な前提作業もありますが、利用可能な経路を試す前に基盤を追加し続けることへの明示的な制限がありませんでした。設計検討の長期化も最初の具体的な変更を遅らせました。
4. 各実装で長い履歴reportと別のroadmap commitを追加し、活動記録は増えても現在のqueueが見えませんでした。Code sliceごとにfull baselineを再実行していましたが、所要時間は計測していないため、経過時間に対するtestの比率は断定できません。

初回の残り134 ptの作業一覧は欠けていた詳細見積もりを置き換えたもので、完了済み34 ptの付け直しや新しい完成率の作成ではありません。現在の残りpointは成果の完了で減り、再見積もりでは変更を明示します。新旧の総計は粒度が異なり、過去のcost超過率の計算には使えません。新たな作業が判明したら、task数と残りpointの増減を含めて明示的に再見積もりします。

## 作業ルール

最初の実行fixture後の監査（2026-10-18 JST）：RFC 0007の受理済みmove／match規則では、非Copyのcase bindingは読み取りloanです。生成したBytesをownedなmutable localへbindingすると、`SRC0014`（expected Bytes, found &Bytes）で拒否されることを確認しました。Allocating Option／Resultから変更／消費core命令へ進む経路を妨げるため、V04前にS01〜S03（8 pt）を追加し、明示的で互換性を持つ取り出し設計を判断してからcompiler／runtime contractを実装します。受理済みborrowed matchingを暗黙に変更しません。また新VMはCFG block argumentを明示的に拒否するため、一般guest-call conformance前にV11（2 pt）を追加します。V03で3 ptを減らし、新規4行で10 ptを加えるため、残作業は129から **136 pt**、**53 todo** へ変わります。S02／S03はS01で設計を選ぶまで暫定とし、大きい形式を選んだ場合はcode前に分割／再見積もりします。実行できたsource fixtureは完了成果として維持します。

S01判断時の監査（2026-10-18 JST）：RFC 0009は新しい消費match構文を作らず、既存generic-call構文とwhole-value argument規則を使う有限core call 3個を選びました。2 ptの設計行を完了し、残作業は **136 → 134 pt**、todoは **53 → 52件** です。S02はsignature／source／MIR検査の3 pt、S03は対応Bytes／Copy移譲とpanic cleanupの3 ptを維持します。新parser構文、任意enum取り出し、collection、native実行はこの2行の対象外です。Transfer実験まで見積もりは暫定とし、新前提が判明したら実装前に明示／分割します。S02に続くS03で次のsource owner取り出しfixtureを実行してから基盤を拡張します。

- Roadmap先頭は作業中／次の作業、直近の完了成果、元の完了条件に絞り、履歴reportはこの表で折りたたみます。長いdoing行には実行／検証の確認点を見せますが、task加点やpointの前倒し完了は行いません。文書だけのfollow-upは整合性を検証し、不変のcode baseline全体は再実行しません。
- 実装前に1行を選んで `doing` にし、同時進行は最大1件とします。1行は観測可能な1成果、残り1〜3 ptです。それより大きい、または前提が不明なら着手前に分割します。
- 計画開始から10分経ってもpatchまたは実行可能な実験がなければ、設計を広げる前にtaskを狭めて最小限の有用な実験を実行します。抽象化やhardeningの追加は、選択した成果または受理済み要件に必要な場合に限ります。取得できる検証所要時間を記録し、commit間隔を稼働時間とはみなしません。
- Doneには行のoutcomeを満たすcommit（実装、または明示的に計画した設計判断）と対象を絞った検証evidenceが必要です。自分自身のcommitを参照できない場合は次の計画更新でcommitを記録し、evidenceが揃うまで仕掛かり状態を正直に保ちます。完了条件を満たすまで待たず、各成果の後にこの表を更新します。
- 対象を絞ったcheckから始め、まとまったcode成果ごとに必須full baselineを1回実行します。文書だけのfollow-upはrepository consistencyを確認し、追加code、失敗、未解決の懸念がある場合だけ広いcheckを再実行します。実装／roadmapのcommit順序は維持し、繰り返しの長文追記を行更新に置き換えます。監査記録の追記はscope、見積もり、依存、完了判断が変わった場合に限ります。
- ソース実行fixtureが増えない前提作業が2件続いたら、基盤の拡張を止め、次のvertical sliceを再検討します。漠然と「見積もりは妥当」とせず、具体的な阻害要因と残taskを報告します。
- 18件の完了条件を維持します。必要な実機tier-1実行を含め、全evidenceが揃った場合だけ完了にします。Localのobjectや設定済みCI jobはWindows native実行evidenceではありません。未完了のP1 debug作業はexit前に移動先と再見積もりを明示します。

V07着手前の監査（2026-10-18 JST）：[owned-call fixture](../../examples/scripts/owned_calls.lang) はsource解析が通りますが、実行はeffect前に `Call` を拒否します。現在の `FramePlan::reserve_after` は呼び出しごとに新しいhost wrapper vectorを確保し、最外guest entry前にhost poolを用意する既存実装contractを満たしません。V07前に再利用するwrapperのadmission／cancel用 **M2-V12（2 pt）** を追加します。**残り118 → 120 pt**、新規1行で **45 done／1 doing／46 todo** です。V07のowned／loan source call、再帰上限、return escrowは暫定3 ptで維持し、新たな前提はcode前に明示します。V11とV12で前提作業が2件続くため、その後は基盤を増やす前にV07のsource fixtureを実行する成果を必須にします。

V09着手前の監査（2026-10-18 JST）：provisional core readには未完了のVec／Map／shared／arena source adapterが必要ですが、C03／C06は既にV09へ依存しており、従来の複合行に依存cycleが隠れていました。Prepared replacementをV09へ残し、不足しているallocationなしのroot／field storage交換と、old cleanup前のpublication adapterを含めて **2 → 3 pt** へ再見積もりします。Provisional-readの成功／未使用／失敗armとVM／REPLのloan conformanceは、C03／C06／R04／R06／V10後の **V13（2 pt）** へ移します。両成果を維持し、**残り112 → 115 pt**、1行追加（計93行）、**48 done／1 doing／44 todo** です。V09では基盤を増やす前に実際のsource replacement fixtureを実行する成果を必須にし、元のexit gateは維持します。

V10着手前の監査（2026-10-18 JST）：pipeしたREPL実験で妥当なowning関数の定義が拒否され、その呼び出しがunknownになりました。後続のCopy式は `3` を表示します。`src/frontend.rs` はwrapper推論をCopy checkerへ固定し、`src/repl.rs` はitem commit前にownershipを拒否し、owned VM entryは引数なしi64 main専用です。RFC 0006にはCopy aggregate表示がありますが、RFC 0007／0008にはBytes／Textのpreview構文や表示後のresult retirementがありません。その判断を **V14（2 pt）**、上限付きmanaged-result preview／実行を **V16（3 pt）**、独立検証するownership式推論を **V15（3 pt）** として明示します。V10のsession／CLI／authority／error統合は **2 → 3 pt** へ再見積もりします。Bytes／TextをM2の全result familyと暗黙に扱わず、後続indirect-service resultのpreview設計／実装も実adapter後の **V17（3 pt）** へ明示します。**残り112 → 124 pt**、4行追加（計97行）、**49 done／1 doing／47 todo** です。順序はV14 → V16のsource実行 → V15 → V10とし、判断後はcompiler基盤を増やす前にowned-result fixtureを実行します。従来の成果とexit gateを維持し、この監査で新実装の完成は主張しません。

R01着手前の監査（2026-10-18 JST）：検証済みarena-tokenの `CopyPlace` だけを別のlive arenaの値へ差し替えた [source由来probe](../../tests/mir_regions.rs) が、type／move／loan-activity／loan／failure-CFG検証を通りました。同じsourceはHIRで拒否されますが、独立MIR region stageはありません。Loan topologyは意図的にnon-loan record fieldをopaqueに扱い、region value graphとcall-effect summaryを提供しません。3 ptのR01に4成果が隠れていたため、上限shape／path／owner／call境界の **R08（3 pt）**、関数内brand／loan／store／CFG転送の **R09（3 pt）**、再帰caller-projection／mutation／transient-store summaryの **R10（3 pt）** を明示します。3件の後に必須pipeline admission、構造化診断、偽造入力／resource／fuzz conformanceを行う **R01（2 pt）** を残し、元の拒否成果は維持します。**残り113 → 121 pt**、3行追加（計100行）、**53 done／1 doing／46 todo** です。R08は準備であり、region proofやarena実行／artifact有効化ではありません。R08／R09後は基盤拡張前にsource arena経路を再確認します。Probeではnested arenaでshared allocator parameterを再利用するとloweringでmoveされる不足も見つかったため、R06／R07のsource conformanceに修正を残します。新exit gateは完了扱いにしません。

R02 storage／次sliceの監査（2026-10-18 JST）：[型付きstorage実験](../../crates/runtime/tests/shared_storage.rs) で3つのOOM位置、quota境界、detach後のcleanupが成功しました。当初のhost Arc／opaque-edge engineでは型付きsource valueやguest-backed control／read metadataを直接扱えません。R03でtoken identity、上限付きgraph／read registry、cycle proof、反復child dispatchを新storageへ明示統合する必要があるため、実装前に **2 → 3 pt** へ再見積もりします。R02完了で **残り102 → 99 pt**、調整後は **100 pt**、**61 done／0 doing／39 todo** で、新規行はありません。R03の前提R02／D34は完了です。基盤2件のR02／R03後はR04のsource shared／weak fixtureを必須にし、新たな前提を追加する前にsource経路を見直します。G05／G06にはnative／source全体のevidenceが引き続き必要で、ownership artifact gateも未完了です。

R03 interaction監査（2026-10-18 JST）：実行できた型付きgraph probeはshared／weak fieldとBytes子孫を扱いますが、activeなVec／Map tokenは明示拒否します。Containerの再帰cleanup callbackから同じborrow中のheapへ安全に再enterすることもできず、入れ子edge／brand検証全体と共通の反復service drainには、実際のcollection adapter後に固有の統合が必要です。直接shared fieldを受理済みpayload全体とみなさず、C03／C06／R04／R06後の **C07（3 pt）** を明示します。V13／V17とN04もC07へ依存させます。既存のgeneric value scopeを維持し、**残り100 → 103 pt**、1行追加（計101行）、**61 done／1 doing／39 todo** です。C07は最初の実container／graph実験まで暫定です。直近はR03 → R04 source実行の順を維持し、元のgateは完了扱いにしません。

N01実装前監査（2026-10-18 JST）：既存native APIはCopyの `VerifiedUnit` cellのみを受け取り、ownershipはruntime `Layouts` と別管理の初期化／retirement metadataを使います。RFC 0008は2 pointer／statusのconventionを維持しますが、ownership context、返り値escrow、frame admissionを定めていません。明示的な決定と機械コード試作の **N08（2 pt）** を追加し、**残り66 → 68 pt**、**計103 → 104行**とします。元のN01〜N07の成果範囲はすべて維持します。今回と次の基盤成果の後は、ownership source fixtureを届けてからnative基盤を追加します。

## 完了済み実装

最初の35行はcommitからM2の実装成果を復元したものです。元のRFC 0007／0008判断は完了条件側に残します。以後の成果は、明示的に計画したfollow-up設計行を含め、計画時のIDを維持してdoneにし、各行に記したoutcomeだけを満たします。検証の詳細と限界は [architecture history](architecture-history.md) と [roadmapの実装履歴](roadmap-history.md#milestone-2ownershipとnative-aot) を参照してください。

`Gate` はroadmapのchecklist順にG01〜G18です。1行が複数条件に貢献しても、作業を二重計上しません。

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-D01 | done | 0 | — | G13 | Copy profile全体のnative loweringと差分fixture。 | `926fb3b` |
| M2-D02 | done | 0 | — | G04 | Shared／weakのlifecycle、cycle publication、rollback、race。 | `ec16f8a` |
| M2-D03 | done | 0 | — | G03 | Ownership構文とformatter往復。 | `eb3b16f` |
| M2-D04 | done | 0 | — | G03 | HIRのownership型特性と上限。 | `81fab15` |
| M2-D05 | done | 0 | — | G03 | HIRのplaceとlexical arena scopeのbinding。 | `f6e0edb` |
| M2-D06 | done | 0 | — | G03 | 有限core-call signatureとprovenance template。 | `9342175` |
| M2-D07 | done | 0 | — | G03 | Core sum case、Unit value、pattern。 | `0115a46` |
| M2-D08 | done | 0 | — | G03 | 文脈を用いたbody／core型検査と網羅性。 | `8eda046` |
| M2-D09 | done | 0 | — | G05 | Lexical arena-owner authorityの型検査。 | `7213ae3` |
| M2-D10 | done | 0 | — | G03 | HIRの初期化とaffine move flow。 | `9263966` |
| M2-D11 | done | 0 | — | G03 | HIRのlexical loanとmutable access。 | `7e3d43d` |
| M2-D12 | done | 0 | — | G03 | 入れ子reference cellと推移的provenance。 | `83b5e36` |
| M2-D13 | done | 0 | — | G05 | ソースのarena brandとstorage flow。 | `3e6563a` |
| M2-D14 | done | 0 | — | G05 | 再帰guest callのregion summary。 | `b39b96a` |
| M2-D15 | done | 0 | — | G05 | Arenaのretain付きread、generation、逆順teardown。 | `02f349d` |
| M2-D16 | done | 0 | — | G03 | MIR ownership型graphの独立検証。 | `c6847a6` |
| M2-D17 | done | 0 | — | G03 | MIRのcanonical typeとsource declaration。 | `97672e4` |
| M2-D18 | done | 0 | — | G03 | MIR place／projectionの型検証。 | `51759b3` |
| M2-D19 | done | 0 | — | G03 | Ownership命令の型検証とCFG検証。 | `a3c5dce` |
| M2-D20 | done | 0 | — | G03 | MIRの初期化とmoveの独立解析。 | `9cf81bd` |
| M2-D21 | done | 0 | — | G03 | RHS effectをまたぐ代入先の事前評価。 | `9a57680` |
| M2-D22 | done | 0 | — | G03 | MIR loanの明示的な有効化／終了解析。 | `fd5f050` |
| M2-D23 | done | 0 | — | G03 | MIR reference provenanceの独立検査。 | `4c6c13e` |
| M2-D24 | done | 0 | — | G03 | MIR core-call contractとallocator要件。 | `4619937` |
| M2-D25 | done | 0 | — | G03 | Ownershipソースbodyの自動lowering。 | `39bb20e` |
| M2-D26 | done | 0 | — | G03 | Matchとprovisional-loan armのlowering。 | `ad40b7a` |
| M2-D27 | done | 0 | — | G03 | Driver／queryのownership解析とlang check。 | `d25706c` |
| M2-D28 | done | 0 | — | G06 | 順序付きcleanup engineとreturn escrow。 | `c437dc1` |
| M2-D29 | done | 0 | — | G06 | ソース生成cleanup rangeとruntime順序の接続。 | `6216b43` |
| M2-D30 | done | 0 | — | G06 | 実際のrecord初期化順序をloweringで保持。 | `a984e5e` |
| M2-D31 | done | 0 | — | G06 | 上限付きの入れ子field failure report。 | `9678072` |
| M2-D32 | done | 0 | — | G06 | Ownership failure／return CFGの生成。 | `3c3131f` |
| M2-D33 | done | 0 | — | G06 | Fallible instance allocatorと会計。 | `d7f85de` |
| M2-D34 | done | 0 | — | G06 | Managed layoutと事前確保したretirement storage。 | `f68a4da` |
| M2-D35 | done | 0 | — | G07 | Allocator付きBytes／Textと検証済みtoken cleanup。 | `82ed11b` |

## ソース実行を先行

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-V01 | done | 0 | M2-D34 | G08 | Managed Unit／Option／Result／error codec：入れ子往復、tag検証、厳密な上限。 | `71ece57` |
| M2-V02 | done | 0 | M2-V01 | G06 | 上限付きowned frameの受け入れ時、effect前にvalue／retirement storageを確保し、失敗rollbackを検証。 | `86c23ce` |
| M2-V03 | done | 0 | M2-V02,M2-D35 | G06,G07 | lang runでBytes new／lenのソースと通常dropを実行し、到達可能な未対応命令はeffect前に拒否。 | `ece8690` |
| M2-S01 | done | 0 | M2-V03 | G07,G08,G09,G10 | RFC 0009でSome／Ok／Err全体取り出しを受理。Borrowed match維持、明示panic／cleanupと互換性規則。設計のみ。 | `c4cb8b9` |
| M2-S02 | done | 0 | M2-S01 | G08 | RFC 0009の3操作を型検査し独立検証済みMIRへlower。Copy／move、loan、region、入れ子sumのcompile pass／fail。Runtimeはgateを維持。 | `b562c0f` |
| M2-S03 | done | 0 | M2-S02 | G06,G08 | 有限Bytes／Copy sumと3種のwhole-payload取り出しをtransfer確保なしで実行。Source fixture、panic／fuel／OOM cleanup、managedな入れ子順序／span移譲を検証。 | `e584902` |
| M2-V04 | done | 0 | M2-V03,M2-S03 | G07,G08 | Source Bytes get／set／push、UTF-8変換、Text byte長を実行。Bounds／OOM、変換確保数、fuel／interruption cleanup、Copy cell loop再利用を検証。 | `8b24c94` |
| M2-V05 | done | 0 | M2-V04 | G07 | Text concat／clone／to-bytes／equalityとBytes cloneを実行。独立owner、allocator identity、typed OOM／quota error、fuel／loop cleanupを検証。 | `752a369` |
| M2-V06 | done | 0 | M2-V03,M2-S03 | G06,G08 | 入れ子Bytes／Text／Copy record／enum／sum ownerとreference projectionを実行。Pack用確保なし、記述逆順の1回限りdrop、OOM／panic／fuel、loop cleanupを検証。 | `92d225a` |
| M2-V12 | done | 0 | M2-V11 | G06 | Host拡張なしでmain／callee regionをadmit／release。厳密budget、size 0課金、OOM rollback／再試行、cumulative request、callback前拒否を検証。 | `05ebf0b` |
| M2-V07 | done | 0 | M2-V06,M2-V11,M2-V12 | G06 | Owned guest call／resultを再帰上限とreturn escrow付きで実行。 | `15436f0` |
| M2-V08 | done | 0 | M2-V07,M2-D32 | G06 | Single／callの生成failure exitで、body／fuel／OOM／interruptionとcleanup error／panic注入時の入れ子drop順序／span、保護return、順序付き上限report、poison／再利用、全解放を検証。 | `a7af470` |
| M2-V09 | done | 0 | M2-V04,M2-V06 | G07,G08 | Root／投影先置換でRHS effect越しに確定済みtargetを保持し、移譲用allocationなしでold cleanup前に公開。Caller所有権、field順序／span、失敗cleanup、再試行を維持。 | `0311bd0` |
| M2-V13 | done | 0 | M2-C03,M2-C06,M2-C07,M2-R04,M2-R06,M2-V10 | G08 | Provisional core readの成功／未使用／失敗armとloan終了をsource VM／REPL実行で検証。 | `9d9d2ac` |
| M2-V14 | done | 0 | M2-V08,M2-V09 | G08 | 委任でRFC 0010を受理。Owned REPL preview形式、result lifetime、明示allocator authority、上限出力、failure優先順位を決定しsource contractを検証。実装は未完了。 | `b09cc46` |
| M2-V16 | done | 0 | M2-V14,M2-V08,M2-V09 | G08 | Namedな引数なしowned resultを上限付きmanaged previewとresult全retirementで実行し、source fixtureと失敗根拠を示す。 | `040c082` |
| M2-V15 | done | 0 | M2-V14 | G08 | 生成ownership式wrapperの具体的resultを通常のHIR／MIR move／loan／region検証越しに推論し、user関数の推論は追加しない。 | `3ecef3b` |
| M2-V10 | done | 0 | M2-V08,M2-V09,M2-V15,M2-V16 | G08 | REPL ownership item／式、明示allocator authority、元sourceへのfailure mapping、cleanupで分離したcanonical resultをscript実行意味論へ統合。 | `834abe8` |
| M2-V17 | done | 0 | M2-V14,M2-V16,M2-V10,M2-C03,M2-C06,M2-C07,M2-R04,M2-R06 | G08 | 明示follow-upを判断してindirect-service resultの上限付きpreviewを実装し、Map順序とshared／weak／arena access制約を維持。VM／REPL根拠を示す。 | `72950d6` |
| M2-V11 | done | 0 | M2-V06 | G08,G14,G16 | 上限付きadmission済みsnapshotでCopy／affine CFG argumentを同時転送。Cycle／fan-out／enum／owner oracle、loan再束縛、cell／quota／OOM、fuel cleanupを検証。 | `1dcf260` |

## Shared、arena、collection

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-R08 | done | 0 | M2-D24 | G05 | 通算budget下で有限region type／slot／projection topology、arena-owner identity、region関連call境界をadmit。再帰shape edgeを展開せず保持し、region proofは行わない。 | `24fb835` |
| M2-R09 | done | 0 | M2-R08 | G05 | Local MIR操作、core value、CFG join／loopでarena brand、reference／capture path、storage lifetimeを独立転送し、直接のcross-region／escapeを拒否。 | `ee5b12e` |
| M2-R10 | done | 0 | M2-R09 | G05 | Caller projection、正常return時mutation、全transient storeの再帰guest-call summaryをHIR factsなしで導出し、推移的／循環call escapeを拒否。 | `28f2080` |
| M2-R01 | done | 0 | M2-D24,M2-R08,M2-R09,M2-R10 | G05 | Failure／実行admission前に独立MIR arena-brand／call-effect proofを必須化し、構造化span診断と偽造入力／resource／fuzz conformanceを維持。 | `dd0b006` |
| M2-R02 | done | 0 | M2-D02,M2-D33 | G06 | 型付きshared control／payload storageをguest fallible allocationに接続し、payload／retirement全領域のpreflight／rollbackを会計。Graph／source統合はR03／R04へ続く。 | `15834b9` |
| M2-R03 | done | 0 | M2-R02,M2-D34 | G06 | Production shared／weak token、上限付きgraph／read registry、cycle検証を型付きstorageへ統合し、test registryなしに子孫を反復retire。 | `20ade1f` |
| M2-R04 | done | 0 | M2-R03,M2-V08 | G06,G14 | Shared／weakソース命令、upgrade、cycle拒否、rollbackを実行。 | `830dc26` |
| M2-R05 | done | 0 | M2-D15,M2-D33 | G05,G06 | Arena payload allocationと子孫retirementにguest budgetと失敗rollbackを適用。 | `cb7a72b` |
| M2-R06 | done | 0 | M2-R01,M2-R05,M2-V08 | G05 | Arena allocation／read／replace／region再enter（reset）／teardownをソースから実行し、stale referenceと逆順dropを検証。 | `e0f6bb4` |
| M2-R07 | done | 0 | M2-R06,M2-V07 | G05 | Guest callを通じ、arena escape／call-brandのcompile-failとruntime結果を照合。 | `cecd6e8` |
| M2-C01 | done | 0 | M2-D33,M2-D34 | G09 | Vec storage／new／len／push growthをallocator-awareかつfailure-atomicに実装。 | `78a422a` |
| M2-C02 | done | 0 | M2-C01 | G09 | Vec read／mutable-read／take／replacementで移動／置換elementを正確に1回retire。 | `1271ad8` |
| M2-C03 | done | 0 | M2-C02,M2-V09 | G09 | Vecソース命令をmove／loan／growth／failure fixtureと副作用前のservice別payload preflightで実行。 | `3dca725` |
| M2-C04 | done | 0 | M2-D35,M2-D34 | G10 | Map storage／登録付きread lookupで受理済みの厳密key equalityとallocation budgetを実装。 | `348dc42` |
| M2-C05 | done | 0 | M2-C04 | G10 | Map insert／replacement／removalをfailure-atomicにし、同値の既存keyを保持して旧key／valueを1回だけretire。 | `677bb66` |
| M2-C06 | done | 0 | M2-C05,M2-V09 | G10 | Mapソース命令を独立したequality／replacement／failure oracleで検証。 | `fb13cfd` |
| M2-C07 | done | 0 | M2-C03,M2-C06,M2-R04,M2-R06,M2-C08,M2-C09 | G05,G06,G09,G10 | 上限付きcross-service edge／brand検証と反復cleanupで入れ子Vec／Map／shared／arena値を統合し、source cycle／escape、逆順drop、失敗caseを検証。 | `f89b323` |
| M2-C08 | done | 0 | M2-C03,M2-C06 | G05,G06,G09,G10 | 元initializer spanを保持する上限付きread-only入れ子Vec／Map走査を実装し、改ざん／work・depth制限／無effectを検証。 | `cb698d9` |
| M2-C09 | done | 0 | M2-C08,M2-R03,M2-R05 | G05,G06,G09,G10 | 再入・host再帰・guest中のhost pool拡張なしにサービス間の反復cleanupを実装し、逆順／失敗を検証。 | `559b176` |

## Ownership artifactとC境界

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-A01 | done | 0 | M2-V08 | G14,G16 | Ownership artifactのversion／encodingを互換性と検証上限付きでRFC受理記録にする。 | `6c9f12b` |
| M2-A02 | done | 0 | M2-A01 | G14,G16 | Ownership型／命令のencode／decodeに不正入力budgetとfuzz targetを追加。 | `1311acd` |
| M2-A03 | done | 0 | M2-A02 | G16 | Owned source artifactをload、独立検証、実行し、往復時のbehavior一致を確認。 | `2dfea13` |
| M2-F08 | done | 0 | M2-D27 | G11,G12 | AcceptedのC import／exportとunsafe blockをlosslessにparse／formatし、予約語を診断し、上限を維持して未完成の意味解析を拒否する。 | `6c32322` |
| M2-F01 | done | 0 | M2-D27,M2-F08 | G11 | 受理済みunsafe／source境界規則をsource診断とcompile-fail fixtureで強制。 | `819ee7c` |
| M2-F09 | done | 0 | M2-F01,M2-N07 | G08,G11,G12 | 既存MIRで受理済みsource core::dropの型・Copy／move／loan規則と即時VM／native cleanupを実装。 | `59f0f08` |
| M2-F13 | done | 0 | M2-F09 | G08,G11,G12 | Source panicの非復帰・動的i64 code・span／stack・VM／native cleanupをschema 3／private revision 3で保持。 | 6eb9018 |
| M2-F10 | done | 0 | M2-F03 | G11,G12 | C address／正規value検査でpaddingを読まず、alignment／extent／overlapと未知tag／非正規byteを作用前に拒否し、共有work計上とfailure fixtureを保つ。 | `5108069` |
| M2-F02 | done | 0 | M2-F01,M2-V08,M2-F03,M2-F10 | G11 | C境界のvalidity／capability／poison preflightとunwind containmentを失敗fixtureで検証。 | `7d4fea7` |
| M2-F03 | done | 0 | M2-F01 | G12 | 厳密なC descriptor／digest／headerを生成し、受理済みvectorでlayoutを検証。 | `dee7f34` |
| M2-F11 | done | 0 | M2-F02,M2-F03 | G12 | 実C codeと共通contextを使い、scalar／Unit importをsourceからVM／nativeで実行しfailureを検証。 | `a37b053` |
| M2-F04 | done | 0 | M2-F02,M2-F03,M2-F11 | G12 | C importでscalar／aggregate引数、allocation context、failure statusを扱う。 | `cfd0987` |
| M2-F14 | done | 0 | M2-F02,M2-F03 | G11,G12 | 生成raw C create／query／destroyを実C callerで実行し、descriptor copy・失敗時output保持・context寿命・artifact／再入gateを強制。 | 17da01b |
| M2-F15 | done | 0 | M2-F04,M2-F09,M2-F13,M2-F14 | G12 | 実Cからnative source exportを呼び、元opaque contextのcallback・allocator・result原子性・資源制限を検証する。 | c0f9b68 |
| M2-F16 | done | 0 | M2-F15,M2-N05 | G12 | 実Cからaggregate source exportを呼び、typed transfer／return escrowとowner／guest call bodyのruntime／cleanup接続を完成する。 | 4158655 |
| M2-F05 | done | 0 | M2-F16 | G12 | 明示private bridge契約でlinked C object／bootstrap／runtime統合を完成し、受理済みexportを実link／runで検証する。 | `c0eee9a` |
| M2-F06 | done | 0 | M2-F05 | G11,G12 | Headerを独立compileし、native macOSで境界validity fixtureを実行。 | `adff5d9` |
| M2-F07 | done | 0 | M2-F06 | G11,G12 | Native Windowsで対応するC境界fixtureを実行し、成功CI evidenceを記録。 | `15a50bb` |

## Native、packaging、最終evidence

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-N08 | done | 0 | M2-V07,M2-A02 | G14 | private native ownership call／context／公開契約を決定し、機械コードのABI試作を実行。 | `dc3df29` |
| M2-N01 | done | 0 | M2-V07,M2-A02,M2-N08 | G14 | Owned call／return conventionとmanaged layoutをemitし、source Bytes引数／返り値fixtureを含める。 | `0ff2618` |
| M2-N09 | done | 0 | M2-N01 | G14 | Nativeのowned record構築、入れ子field順、record全体のcallでcleanupを保持し、共有／VM fixtureと一致。 | `f199b19` |
| M2-N10 | done | 0 | M2-N09 | G14 | Nativeのscalar／controlとsum構築・検査・payload全体抽出をVM fixtureに一致させる。 | `5d335a9` |
| M2-N02 | done | 0 | M2-N10 | G14 | Nativeのplace／loan projection・準備済み置換と、owner move／record／sum／loanのVM総合conformance。 | `a3478bd` |
| M2-N11 | done | 0 | M2-N02,M2-C03 | G07,G08,G14 | Native allocator／Bytes／Text操作を共有runtimeへ接続し、source VM fixtureと一致させる。 | `1acb73a` |
| M2-N12 | done | 0 | M2-N11,M2-C03 | G09,G14 | Native Vec操作とscope付き要素loanをVM fixtureに一致させる。 | `649aa08` |
| M2-N03 | done | 0 | M2-N12,M2-C06 | G10,G14 | Native Map操作とscope付きvalue loanをVM fixtureに一致させる。 | `2e6a441` |
| M2-N13 | done | 0 | M2-N02,M2-N11,M2-N12,M2-N03,M2-C07,M2-R04,M2-R07 | G05,G14 | Native shared／weak操作でheap lifecycle、cycle拒否、collection／call projection越しのread custodyとcleanupを維持。 | `4e2c8d6` |
| M2-N04 | done | 0 | M2-N13,M2-N02,M2-N11,M2-N12,M2-N03,M2-C07,M2-R04,M2-R07 | G05,G14 | Native arenaの字句ownerと操作を接続し、collection／call projection越しのread custodyと入れ子brand検査を維持。 | `ea15b05` |
| M2-N14 | done | 0 | M2-N04,M2-N13,M2-N02,M2-N11,M2-N12,M2-N03,M2-V08 | G06,G14 | Nativeの実行上限・割込み・deadline・usageと所有値の失敗cleanupがVMの制御契約に一致。 | `a40e6eb` |
| M2-N15 | done | 0 | M2-N14 | G06,G14 | Service byte-workの課金・失敗・所有権・互換性をRFCで決定し、実現性を確認。 | `0a38beb` |
| M2-N16 | done | 0 | M2-N15,M2-N14 | G06,G14 | 共通service-workのcounter・batch課金・構造化failure・未対応profileの事前拒否を実装。 | `06c8e1d` |
| M2-N17 | done | 0 | M2-N16 | G06,G14 | 共通Bytes／Textのwork課金を接続し、対応programの有限予算・refusal・cleanupをVM／nativeで一致させる。 | 085700c |
| M2-N18 | done | 0 | M2-N17 | G06,G14 | Source／artifact／REPLのCLIでservice-work予算と数値付きfailureを扱い、report schemaの互換性を監査する。 | 419f6ac |
| M2-N05 | done | 0 | M2-N02,M2-N11,M2-N12,M2-N03,M2-N13,M2-N04,M2-V08,M2-N14,M2-N15,M2-N16,M2-N17,M2-N18 | G06,G14 | 設定可能なservice byte-work admissionを含め、受理済みservice／payloadの組合せ全体でnativeの生成failure／return exitをVMのcleanup順序とfuel behaviorに一致させる。 | `f6d9fc4` |
| M2-N19 | done | 0 | M2-N05 | G14 | Linked imageの寿命を保持した信頼済みbindingと既存gatewayによるowned program実行を、実objectで検証する。 | `d53aac0` |
| M2-N06 | done | 0 | M2-N03,M2-N04,M2-N05,M2-N19 | G14 | Native macOSでowned programのdirect／response-file object link／runを検証。 | `25de18a` |
| M2-N07 | done | 0 | M2-N06 | G14 | Native Windowsで対応owned programを実行し、成功CI evidenceを記録。 | `119f9ff` |
| M2-P01 | done | 0 | M2-N06 | G15 | 両tier-1でlinker discovery／出力path失敗に対処可能な診断を返す。 | `2cf38ca` |
| M2-P03 | done | 0 | M2-P01,M2-N07 | G15 | 元IR破棄後、独立native記録から機械語生成なしにplanを再構築し、linked実行をVMと比較。 | `5324c64` |
| M2-P04 | done | 0 | M2-P03,M2-F07 | G15 | C対応schema 4で識別を保持し、復元した実C callbackを明示VM／native contextで実行。 | `816b0b6` |
| M2-P05 | done | 0 | M2-P03,M2-P04 | G15 | 保存済みCopy／ownershipを検証付きimage bindingと具体的production入口で起動。生成元破棄後も元control、allocator、cleanupを保ち、frontend／機械語生成は不要。 | `336a91b` |
| M2-P06 | done | 0 | M2-P05,M2-F07 | G15 | 配布C context／import／export bindingで元allocatorとowner cleanupを保つlinked source実行。 | `2712700` |
| M2-P08 | done | 0 | M2-P05,M2-P06 | G15 | 実験段階SDKの配布／起動、compiler出力、version／toolchain一致、明示C権限の契約をRFCで決定。 | `bca5f12` |
| M2-P09 | done | 0 | M2-P08,M2-P04,M2-P06 | G15 | 製品用source-native compilerコマンドがCopy／ownership／Cのobject、意味論、記録、必要headerを検証・公開し、失敗時に既存出力を保持。 | `66ebb0c` |
| M2-P11 | done | 0 | M2-P09 | G15 | 上限付きSDK／program manifestと検証済みcomponent snapshotで、compiler／provider effect前にkey／path／hash／意味論／objectの不一致を拒否。 | `01bf09c` |
| M2-P10 | done | 0 | M2-P09,M2-P05,M2-P06,M2-P11 | G15 | 配布用runtime／startup consumerで保存Copy／ownership／Cを対応toolchainと起動し、source分析／backend生成／test runtimeに依存せず元control／allocator／context／grant／cleanupを維持。 | `6024931` |
| M2-P07 | done | 0 | M2-P05,M2-P06,M2-P09,M2-P10,M2-P11 | G15 | 対応toolchain要件を明示したRust／native依存closureを含むcompiler／startup／runtime／header SDKを組立。 | `3080d9e` |
| M2-P12 | done | 0 | M2-P11,M2-P10,M2-P07 | G15 | link-nativeがrustc／SDK／targetの完全一致を検証し、明示Rust／C hostと検証済み元byteをlinkして新しいexecutableをexclusive公開。 | `4acd99f` |
| M2-P02 | done | 0 | M2-P01,M2-N07,M2-F05,M2-P07,M2-P12 | G15 | Repo／target/deps外でCopy／ownership／Cを含む両tier-1のclean-install SDK link/run成功。 | `c54af31` |
| M2-Q01 | done | 0 | M2-A03,M2-N07,M2-F07,M2-V10 | G16 | 複合機能の代表ソースfixtureでVM／nativeの成功／失敗conformanceを検証。 | `3db077f` |
| M2-Q02 | done | 0 | M2-Q01,M2-P02 | G16 | 両tier-1 CIで代表conformanceとresource-limit scenarioの成功を記録。 | `1d0c0dc` |
| M2-B01 | done | 0 | M2-Q01 | G17 | 同等Rust／C workloadとharnessで比較前に意味的一致を確認。 | `fffe86c` |
| M2-B02 | done | 0 | M2-B01,M2-P02 | G17 | CleanなmacOSでruntime／startup／memory／sizeとばらつきを記録。 | `95b97cb` |
| M2-B03 | done | 0 | M2-B01,M2-P02 | G17 | 同等のclean Windows計測とばらつきを記録。 | `02b348f` |
| M2-B04 | done | 0 | M2-B02,M2-B03 | G17 | 結果の解釈、再現command、残るperformance上の限界を公開。 | `990e422` |
| M2-G01 | done | 0 | M2-N06 | G18 | Native owned-valueのsource span／locationをemitし、macOS debug fixtureで検証。 | `4b0e902` |
| M2-G02 | done | 0 | M2-N07 | G18 | 対応するWindows native debug metadataをemitし、source locationを検証。 | `2297a07` |
| M2-G03 | done | 0 | M2-G01,M2-G02 | G18 | 両tier-1実機でdebuggerのstep／backtrace evidenceを記録。 | `1496b8b` |
| M2-Z01 | done | 0 | M2-V17,M2-Q02,M2-B04,M2-G03 | G05,G06,G07,G08,G09,G10,G11,G12,G14,G15,G16,G17,G18 | 全exit evidenceを監査し、英日M2 capability guideを公開。 | `e12c120` |

## 情報源と確認日

- [最終capability guideと全出口監査](milestone-2-capabilities.md)（`e12c120`）：2026-10-20 JST更新・検証。元の全18 gate、実行手順、正確なrevision／計測上の制約を記録します。Compiler／runtime product `1496b8b`には両host実debugger証拠があり、最終成果の変更は文書とboard整合性testです。

- [RFC 0020](rfcs/0020-native-sdk-and-startup.md)と[保存したSDK準備実験](../experiments/sdk-owned-consumer.rs)（`bca5f12`）：2026-10-20 JST受理・確認。Producer／consumerを対応rustcとコピー済みlibraryで実buildし、元source移動後の42／allocation発生・全解放を確認。P08の初期1 ptを完了し、P09の製品実装は別途未完了です。Repository整合性3件が成功。

- SDK実装前監査：[CLIのcompile経路](../../src/main.rs)と[固定native prototype](../../src/bin/prototype-object.rs)、snapshot `2712700`を2026-10-20 JST確認。P08–P10は既存G15の不足した前提を明示し、SDK組立P07／両host隔離install P02を維持します。
- [P06復元API](../../crates/codegen-clif/src/owned/c_objects.rs)、[保存file／両形式／予算検査](../../tests/support/c_object_records.rs)、[実C caller](../../tests/support/c_objects.rs)（`2712700`）：2026-10-20 JST更新・確認。既存3 ptを完了し、全1,550 test／89 suitesとfmt／Clippy（3.21秒）／rustdoc（18.47秒）が成功。Suite合計382.61秒は検証時間で開発工数ではありません。

- [通常ownedのproduction入口](../../crates/codegen-clif/src/owned/runtime.rs)、[共通primitivesと完了処理](../../crates/codegen-clif/src/c_abi/runtime/source.rs)、[実VM／native／保存file比較](../../tests/support/native_entry_runtime.rs)、[RFC 0019](rfcs/0019-owned-native-entry-runtime.md)（`336a91b`）：2026-10-20 JST更新・確認。P05の既存3 ptを完了。全体baselineとWindowsの対応snapshotは先頭に記載。

- [Linker診断／事前検査](../../src/linker.rs)、[実native成功／失敗fixture](../../tests/linker_failures.rs)、[LINK直呼びC fixture](../../tests/support/c_imports.rs)、[Windowsの先行C／linker検証](../../.github/workflows/windows-x86_64.yml)（`0061045`）：**2026-10-20 JST更新・local確認**。Native macOSで上記全baselineが成功し、P01／F07の対応するnative Windows証拠を引き続き確認します。Local実装だけでeffort pointを前倒し完了しません。

- [独立C11 header object](../../tests/c_abi.rs)、[native object／bind／有限Copy fixture](../../tests/support/c_object_binding.rs)、[VM／JIT／AOT共有mixed import oracleとMSVC fixture](../../tests/support/c_imports.rs)（`adff5d9`）：**2026-10-20 JST更新・確認**。Darwin arm64／Apple Clang 21.0.0／固定Rust 1.99.0で上記の最終baselineをWindows fixture修正後に確認しました。対応するnative Windows成功はF07に維持します。Microsoftの[/link](https://learn.microsoft.com/en-us/cpp/build/reference/link-pass-options-to-linker?view=msvc-170)と[compiler response file](https://learn.microsoft.com/en-us/cpp/build/reference/at-specify-a-compiler-response-file?view=msvc-170)は最終更新 **2021-08-03**、[linker構文](https://learn.microsoft.com/en-us/cpp/build/reference/linking?view=msvc-170)は **2025-11-05**。すべて **2026-10-20 JST確認**。

- [C object生成／bind](../../crates/codegen-clif/src/c_abi/object.rs)、[保持するlinked metadata](../../crates/codegen-clif/src/owned/c_objects.rs)、[実C object検証](../../tests/support/c_objects.rs)（`c0eee9a`）：**2026-10-20 JST更新・確認**。Object fixture 6件と上記全baselineがlocalで成功。完全なtarget境界証拠はF06／F07、standalone packagingはP02、最終guideはZ01に維持します。実装前にRFC 0015を`949f3d8`で受理し、F05による行／見積もりの追加はありません。

- [Native factory](../../crates/codegen-clif/src/c_abi/runtime/factory.rs)、[typed transfer](../../crates/runtime/src/c_abi/transfer.rs)、[実C／VM比較](../../tests/support/c_exports_runtime.rs)、[全Copy case／256引数](../../tests/support/c_exports_copy_cases.rs)（4158655）：2026-10-20 JST更新・確認。上記全検証が成功。Linked C objectはF05、新Windows snapshotと残るplatform／package／guideはF06／F07／P02／Z01です。
- [Native source export](../../crates/codegen-clif/src/c_abi/exports.rs)、[実C caller](../../tests/support/c_exports_native.c)、[5実行／境界test](../../tests/support/c_exports_native.rs)（c0f9b68）：2026-10-19 JST更新・確認。全baselineと最後のC ABI再実行は上記。Typed runtime／object全体とWindows新snapshotはF16／F05／F07です。

- [Raw C context support](../../crates/codegen-clif/src/c_abi/context.rs)、[field reader／lifecycle](../../crates/codegen-clif/src/c_abi/context/raw.rs)、[実C／状態fixture](../../tests/support/c_context_native.rs)（`17da01b`）：2026-10-19 JST更新・確認。Local全baselineは上記のとおり成功し、Windows run 39265484004も成功しました。F15は後続のsource export snapshotで、C objectはF05に残ります。

- [Source panic](../../examples/analysis/explicit_panic_owned.lang)、[VM fixture](../../tests/support/owned_panic.rs)、[native fixture](../../tests/support/native_panic.rs)、[RFC 0014](rfcs/0014-explicit-source-panic.md)（`6eb9018`）：2026-10-19 JST更新・確認。全local baseline suiteは上記のとおり完了しています。[Windows run 39258215633](https://github.com/carimatics/lang/actions/runs/39258215633)、head 165ae7d、job 118078789294は2026-10-19 13:36:00 UTCに全workflow成功となりました。F14 raw context supportは後続の別実装として追跡します。

- [Source drop](../../examples/analysis/explicit_drop.lang)、[shared／weak期限切れ](../../examples/analysis/explicit_drop_shared.lang)、[lowering](../../crates/semantics/src/ownership_types/bodies/matching.rs)、[VM／native fixture](../../tests/support/native_drop.rs)（`59f0f08`）：2026-10-19 JST更新・確認。上記local必須検証に成功し、F09 Windowsはrun 39251972828で確認済みです。Source panicは別のF13成果です。

- [F04 finite Copy転送](../../crates/runtime/src/c_abi/transfer.rs)、[名前付きsource](../../examples/analysis/c_aggregate_imports.lang)、[実C fixture](../../tests/support/c_imports.rs)（`cfd0987`）：2026-10-19 JST更新・確認。上記local必須検証に成功し、修正後のWindows snapshot 876b908はrun 39247826059を通過しました。完全なraw C exportはF05です。

- [F11 scalar C source](../../examples/analysis/c_scalar_imports.lang)、[C／VM／native fixture](../../tests/support/c_imports.rs)、[revision 2 native gateway](../../crates/codegen-clif/src/owned/c_imports.rs)（`a37b053`）：2026-10-19 JST更新・確認。上記local必須検証を完了し、Windows snapshot 1a8fcadはrun 39234763047を通過しました。Raw C export／objectと完全なplatform検証はF05–F07です。
- [F02 context実装](../../crates/runtime/src/c_abi/context.rs)と[16件のfailure／state fixture](../../tests/support/c_context.rs)（`7d4fea7`）：2026-10-19 JST更新・確認。上記local必須検証を完了し、その後F11／F04でtyped C import adapterも完成しました。Raw C context supportはF14で完了し、source export／runtime／object公開はF05です。
- N02の過去のcheckpoint：[Native place／loan lowering](../../crates/codegen-clif/src/owned.rs)、[source fixture](../../examples/analysis/native_owned_loans.lang)、[place／VM test](../../tests/support/native_places.rs)（`a3478bd`）：2026-10-19 JST更新・検証。N02の既存見積もり3点を完了しました。これらはa3478bd時点で未完了でした。その後の完了はN03–N07／N11–N19行と上記snapshot根拠に記録しています。
- [進捗validator](../../tests/repository_consistency.rs)（`00cff53`）：更新・検証日2026-10-18 JST。見積もり収支と未完了行の一致、成果／task数の差分、roadmap集計、英日一致を検証します。全workspace 1326 testとfmt／Clippy／rustdocが成功しました。この追跡方法の保守をM2実装行の完了には数えません。
- 詳細Todoの履歴 `8605416..926ccfe`：2026-10-18 JST付の48 revisionを2026-10-18 JSTに再監査。行の状態変化だけでは完了107点／追加39点になります。V14とR08は最初のcommitでdoneでしたが、V10／R01の実装前監査に2／3点の見積もりが残っているため、両側に5点を加えて完了112点／追加44点としました。新たな成果41件をすべて含み、削除された行はありません。導入と完了が同じ更新でも、その見積もりを保持します。
- Local Git履歴 `926fb3b`〜`0c2c012` と差分 `367218e..0c2c012`：commit日は2026-10-17／18 JST、確認日は2026-10-18 JST。
- [Architecture](architecture.md)、[roadmap](roadmap.md)、そこから参照する実装／test：更新・確認日2026-10-19 JST。初回監査の930 testは過去のbaselineであり、現在の検証済みbaselineは上記に記載しています。
- 受理済み [RFC 0007](rfcs/0007-source-ownership-and-core-values.md)：最終更新2026-10-18、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)：最終更新2026-10-19、両方を今回のtask一覧のため2026-10-19 JSTに確認しました。Web調査や外部の生産性benchmarkは使用していません。
- [RFC 0012](rfcs/0012-native-ownership-continuations.md) と [native ABI実験](../../crates/codegen-clif/src/owned_abi_probe.rs)（`dc3df29`）：RFCの最終更新・確認は2026-10-19 JST、実験のcheckpointは2026-10-18 JSTです。N08の明示的見積もり2点は、そのcheckpointで追加・完了しました。
