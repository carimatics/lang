<!-- translation-of: ../../rfcs/0023-task-mir-boundaries.md -->

# RFC 0023: Task MIR boundaries

[English](../../rfcs/0023-task-mir-boundaries.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-T11, M3-T04–T08
- Supersedes:
- Superseded by:

## Summary

A06 実装 checkpoint（2026-10-21 JST）: source と MIR は独立に検証した推移 type fact から compiler-derived logical-task Send/Share を提供し、新 graph cache、trait 自己宣言、bit-copy transfer、wire tag を導入しません。async signature は owned Bytes/Text/Vec/Map、sum、再帰 record/enum、checked shared/weak を受理します。loan/arena/allocator/Task 権限は禁止し、通常 move 解析で spawn 引数を一度だけ消費します。重点 7 テストで両 graph、未使用/前方再帰 signature と alias、不正 MIR loan signature の type span での拒否、二重消費拒否、proof work 境界値/1 未満を確認しました。到達する owned async transfer は A05 の publication/await/disposal 実装まで VM/native で副作用前の Unsupported 拒否を維持します。source actor authority は具体的 channel handle 後の A02 に残ります。部分実装の証拠であり、RFC 0024 と全 M3 gate は未完です。

A01 follow-up 決定（2026-10-21 JST）: [RFC 0024](0024-send-share-and-service-authority.md) は同一 invocation の logical task に対する再帰 Send/Share と mutable state projection のない interface-only producer handle を定めます。初期 Copy-only transfer subset を拡張し、loan/arena/allocator/Task transfer 禁止、checked shared 操作、独立 source/MIR proof を維持します。host thread の保証はありません。実行受理前に A06 導出と A05 owned async publication/await/disposal を検証し、A02 は具体的 channel handle 後の actor authority proof を維持します。以前の日付付き checkpoint は履歴です。channel/backpressure と foreign/C/serialized/package の決定は別途必要で、M3 gate は未完了です。

既存 ownership MIR に Task/TaskError 型、ABI から独立した async effect、独立検証した task 命令/scope/claim flow/suspension cleanup を追加します。型/宣言だけの証明は task flow の証明ではありません。既存同期実行/保存経路はこの profile を拒否し、意味を消去できないようにします。

T11/T04–T06 checkpoint（2026-10-21 JST）: 型/effect/宣言検証を実装しました。旧 place/body、runtime layout、bytecode writer は task metadata を明示拒否し、旧 decoder は同期 effect を復元して新 tag を割り当てません。特に汎用 runtime token layout の fallback で Task/TaskError の storage を作りません。T04 で同じ body table を使う別 task 検証 entry を追加し、scope/claim/return escrow 解析と move/loan/arena region/failure plan の証明を結合しました。async failure plan は取消と primary execution failure を区別し、loan 終了、scope cancel/join、owner retirement、escrow 破棄の順を明示します。T05 で source task をこの契約へ lower し、明示 task VM driver 経由で初期 finite Copy/Task-local profile を実行します。通常 source check/LSP は結合した TaskProgram 証明を保持し、CLI run が source fixture を実行します。T06 で初期 profile を native continuation にコンパイルし、JIT と別途リンクした host object の一致を確認します。owned/loan/foreign task 実行と完全な共有 control は未実装であり、契約全体の完成まで本 RFC は accepted のままです。

## Motivation

`9f7012d` に source 型と suspension loan 証明がありますが、ownership MIR に該当型や async effect はありません。型、宣言、place、body、move/loan/region、cleanup wrapper の証明対象は異なります。source 証明を MIR の妥当性とみなすと外部入力/backend が不健全になります。この境界は library では強制できません。

## Goals

- span と独立検証を保ち、既存 ownership 型/宣言 graph を拡張します。
- 同期 program と既存 encoded byte/ABI identity を維持します。
- 別 task 言語や実行 fallback を作らず、次の task flow/VM/native 作業を明示します。

## Non-goals

wire tag、artifact revision、native frame layout、component async ABI は決めません。channel、Send/Share 導出、source 実行、安定化は別の M3 成果です。型だけの証明で coroutine は実装されません。

## Guide-level design

source 構文は RFC 0022 の `async fn`、`core::Task<T>`、`core::TaskError::Cancelled` です。MIR の backend 非依存 async effect は Guest/CExport ABI と独立です。既存実行/公開経路は、未使用 metadata や scalar 値だけの pure async 関数も unsupported task profile として拒否します。

## Reference semantics

TaskError は Cancelled という unit case だけを持つ通常の finite Copy error です。Task(T) は retirement が必要な non-Copy claim であり、通常の子/frame owner ではありません。claim の drop は結果取得を放棄しますが子を detach/cancel しません。RFC 0022 に従い task scope が join と保持結果を所有します。

型検証で loan/arena/allocator と独立して `contains_task` を伝播します。field/container/loan/parameter/result/入れ子 payload の Task を拒否します。Task payload は loan/arena/allocator 権限/別 Task を含めません。直接の Task local は async 関数を要します。async C export と既存 C profile の Task/TaskError を拒否します。一般の transfer sendability は source/flow profile の責務であり、型だけで全 owned 値を送信不能と判断しません。

async effect span を関数 signature 内で検証します。既存 decoder は宣言を明示的に同期として復元します。型/宣言検証は claim 起点、suspension、cleanup、capability、実行を証明しません。初期 place verifier は Task/TaskError/async 宣言を body 証明作成前に拒否します。T04 は task 対応命令と独立 type/move/loan/region/scope/cleanup 証明が揃ってからこれを置き換えます。通常の同期 call が async effect を消去して呼ぶことを認めません。

命令/continuation と scope exit の表現は実装前に本 RFC に記録します。scoped spawn、消費 await、非消費 cancel、yield/check point、return escrow、暗黙 join、失敗/取消 cleanup を扱い source invariant を独立して再検証する必要があります。この境界判断でそれらを弱い代替に置き換えません。

### T04 instruction and continuation decision

実装前の 2026-10-21 JST に実験段階の委任で採択: ownership `Op` に TaskScopeEnter/Exit、TaskSpawn、TaskAwait、TaskCancel、TaskYield、TaskCheck、TaskSaveReturn を追加し、terminator に ReturnSaved を追加します。各操作は source span を持ち、継続先は source AST ではなく次の命令 site（または block terminator）です。await/exit はその site で suspend でき、admission/move と結果初期化は一度だけ行い、resume で先行副作用を繰り返しません。通常 guest call は同期のままです。

各 async body は有界 scope tree（entry で暗黙に active な root 0、先行 parent ID、span）と、全 Task local/SSA value に一つずつ lifetime scope binding を宣言します。通常 slot に claim binding はありません。binding は意味上の lifetime metadata であり source range から権限を推測しません。enter は active parent と一致し、exit は最上位 scope を閉じます。exit ではその scope に束縛された初期化済み claim slot を retire し、handle の残存とは独立に全ての子を join して未取得結果を破棄します。root claim を内側 binding に move した場合、その binding の終了で取得を放棄しますが root の子は detach されません。

claim の起点を move、local 初期化/置換、CFG 引数経由で独立に追跡します。宛先 lifetime scope は起点自身か子孫であり、初期化時に active でなければなりません。claim の copy/borrow/捏造/逸出/消費・retire 後の再利用を拒否します。CFG 合流は active scope stack と return escrow 状態の整合を要し、起点集合を共有 work/storage 上限内で結合します。通常 ownership move/loan/arena region 証明も必要です。await/yield/check/scope exit で生存する通常 loan を HIR とは独立に拒否します。

TaskSpawn は async guest だけを呼び Task(T) を生成します。TaskAwait は claim を消費し、成功/取消の join 後だけ Result(T,TaskError) を初期化し、子の terminal failure は失敗経路を通ります。TaskCancel は claim を観察し、消費/suspend しません。初期 async transfer は採択済み finite Copy profile に限定し、一般 owned Send/Share は A01 に残します。owned local は別の値言語ではなく既存 ownership MIR 規則を使います。

TaskSaveReturn は join 前に結果を一つの保護 escrow へ move します。async の正常終了は保存済み結果一つ、空の scope stack、ReturnSaved を要し、二重保存/未保存 return を拒否します。失敗/fuel/interrupt cleanup は frame の通常 loan を終え、子孫を cancel し、内側から active scope を join して結果を retire してから frame owner を retire し escrow を破棄します。この必須 cleanup は guest fuel を消費せず、RFC 0008/0021 に従い副失敗があっても継続します。同期 body の既存 return/failure は維持します。task failure plan にこれらの phase を明記し、型/body wrapper だけで task 実行 plan を公開しません。

task verifier は同じ ownership body table を使う別 entry とし、既存同期検証/実行/writer の拒否を維持します。検証済み task wrapper は task/ownership の責務を証明しますが、host grant、物理 layout、backend 対応を証明しません。T05/T06 が明示 host service 経由でこの契約を実行し、本決定では tag/encoded artifact revision を割り当てません。

T04 cancellation plan の補足（2026-10-21 JST）: async failure plan に、primary failure 記録を飛ばして必須 cleanup に入る cancellation entry を持たせます。EndTaskLoans、CancelTaskScopes、JoinTaskScopes の後に owner retirement と結果破棄を行います。FinishTask は task driver の pending outcome と記録済み失敗で cleanup を終え、通常取消を execution fault に偽装しません。cleanup failure と RFC 0021 の優先順位を維持します。TaskCancel は local を含む whole place を観察し、claim を copy/move しません。

T16 allocator/arena の判断（2026-10-21 JST、実装前に実験段階の委任で採択）: arena entry は invocation allocator 権限を捕捉し、その引数 temporary だけを直後に終えます。RFC 0007 で明確化した arena-entry 契約に従い、別 binding の通常 allocator loan と通常 arena-read loan は厳密な suspension 拒否を維持します。source lowerer は ArenaEnter 直後に対応 cleanup を出力し、独立 MIR は arena loan を例外扱いせず、loan が終わった状態を検証します。各 activation は payload admission 前に preparation budget 内で自身の arena 状態を準備します。登録済み arena read lease は owner の activation/slot と起点/generation を持ち、検査済み view、publication inspection、same-region 検証、mixed iterative retirement は既存 runtime service を使います。join 済み scope の owner retirement より先に lease を終え、同じ有界 report に failure/poison 証拠を保持し、cleanup 失敗後も独立した retirement を続けます。arena 値を task 間転送せず、serialized/C/package profile は変更しません。新 source fixture で owning cycle、中断、入れ子 descendant join、取消、失敗 cleanup を VM/生成 native/リンク済み object で検証します。temporary scope の短縮は以前拒否した arena task scope の互換性拡張であり、suspension 時の loan を許す例外ではありません。

T08 control の判断（2026-10-21 JST、実装前）: 全 task、同期 activation、managed service/view で RFC 0013 の invocation ServiceWork handle と既存 runtime event をそのまま使います。有限 work control の admission 前に未使用 metadata を含む全 unit を調べ、非計量 family を黙って実行したり VM に置き換えたりしません。固定の guest-frame marshalling/tag/header 検査は event に加えません。既存 failure/report domain を通じて拒否の数値と最初の operation span を保ち、上限枯渇後も retirement、result discard、cleanup view を非計量とします。frame retirement 後も detached return reservation を 1 live cell として計上し、実際に move/release したときだけ計上を外します。retained result によって後続 frame admission を副作用前に拒否できます。VM/生成 native/リンク済み object で zero/exact/one-below と複合 task/frame/cell/fuel/work control、cleanup failure、実際の全拒否境界を検証します。以前拒否した有限 task control を拡張するもので、source 意味論、cost-profile event、C/wire/package format、一般 async transfer 規則は変えません。

T08 実装 checkpoint（2026-10-21 JST）: T08 は invocation 共通の ServiceWork counter を各 task、同期 activation、managed service/view へ接続します。未使用 foreign metadata を含む unit 全体の有限 control 検証と RFC 0013 の既存 event を適用し、guest-frame marshalling に新 event を加えません。Buffer 拒否時は owner token を保ち、数値付き work failure は function/span と cleanup 証拠を保持します。必須の lease 終了、retirement、result 破棄、heap close は非計量です。分離した task result は移動/解放まで 1 live cell と数え、保持中の別 frame admission を拒否できます。ゼロ/境界値/1 未満と統合 work/fuel/provider prefix は cleanup panic/report truncation を含め VM/生成 native で一致し、同期実行と task の flat-buffer cost も一致します。リンク済み object の差分検証も有限 work prefix を実行します。一般 async transfer と foreign/C/serialized/package profile の境界は維持します。2026-10-21 JST 確認、code に更新日の埋込みはありません。

## Execution modes

T15 service の判断（実装前、2026-10-21 JST）: 到達可能な service operation が必要とする場合だけ、invocation 状態に既存 iterative retirement drain と managed shared heap を preparation budget 内で準備します。型付き core descriptor は検証済み generic 型引数を保持します。frame の loan entry が既存 Vec/Map/shared read lease を所有し、Copy な target は起点 activation/loan/generation と検査済み構造 location を指します。reborrow は起点を保ち、失敗/取消 frame を含め、owner retirement 前に lease 登録を消費して終了します。既存 service view、mutation/publication 検証、shared custody 規則を使い、observation から owning ticket を作りません。frame/temporary の全 owner cleanup は同じ iterative drain と共通 report を使います。通常 loan を suspension/async transfer に持ち越さず、arena/foreign/artifact の未対応 profile を明示拒否します。owned source pipeline と失敗/取消 prefix を両 backend で実行してから T15 を完了します。

T14 参照先の判断（実装前、2026-10-21 JST）: admission ごとに invocation 内で単調増加する activation identity を付けます。通常 reference sidecar は起点 activation/loan/generation、cell target は所有 activation を保持します。一時的な access view が現在の同期 activation と生存 ancestor だけを解決し、Rust reference、native address、新しい ownership token を保存しません。同期引数は通常 reference を渡せますが、async transfer 制限は維持します。field access、borrowed case payload、buffer service、replacement は解決した owner reservation と初期化 ledger を使います。独立した source/MIR loan 証明は通常 loan が live な suspension を引き続き拒否します。VM と生成 native callback は同じ型付き access service を使い、serialized/C layout は選択しません。再帰 shared/exclusive call、owner replacement、borrowed payload、失敗 cleanup、suspension 境界を検証してから T14 を完了します。

T07 owned adapter の判断（実装前、2026-10-21 JST）: managed reservation、token service、iterative retirement、既存 cleanup Context/Report を再利用します。有界の task-owned cleanup session が同期 call-stack entry を中断越しに借用せず report/poison 状態を持ち、事前確保した frame 記録に実際の owner 初期化順と通常 loan の参照先/generation を保持します。non-Copy root/payload は move して元の obligation を終え、claim と allocator 権限は別表現を維持します。join 前に通常 loan を終え、保護結果/owner を一度だけ retire し、RFC 0021 の primary/secondary 優先順位で task report を結合します。native callback は準備済み metadata 上の型付き storage/service/cleanup を処理し、MIR 命令 dispatch は行いません。owned source fixture を先に接続し、対応 owned-local family、同期 move/loan、取消/失敗 prefix を検証してから T07 を閉じます。一般 async transfer は A01、完全な共有 control は T08、foreign/production embedding は各判断に残します。source/C/wire 形式は導入しません。

T12 の順序明確化（実装前、2026-10-21 JST）: RFC 0022 の既存契約を維持し、source の task scope 終了では exiting loan/reference の cleanup を join 前、通常 owner/value cleanup を当該 join と未回収 result retirement 後に lower します。return は result を一度 escrow に置き、入れ子 scope を内側から閉じ、各 scope の local をその join 後に retire します。既存 MIR 命令を並べ替え、新しい source semantics/wire tag は導入しません。runtime は scope に公開順の result chain を保持し、全 child の join 後に公開逆順で未回収結果を破棄します。await 済みの穴を再破棄せず、失敗/取消でも同じ inventory を使います。T07 の owner admission 前に source/MIR と VM/native region 解放でこの順序を検証します。

T06 native の判断（実装前、2026-10-21 JST）: invocation driver、型付き frame/claim/result 状態、scheduling/storage primitive を runtime で共有し、backend ごとに明示 resume/cleanup entry を持ちます。MIR node dispatch は VM backend に残します。Cranelift が native block、演算、比較、分岐、失敗 edge、有界 continuation 選択を生成し、native callback は型付き storage/scheduling/control/failure primitive だけを処理して MIR 命令/body を評価しません。private invocation packet は有効範囲内の context と検査済み callback entry を保持し、C embedding API や serialized interface ではありません。native 呼出しは通常/中断/取消 continuation の生成 function address を選択します。初期 finite Copy/Task profile で VM と値/event/失敗起点/cleanup を一致させ、厳密に一致する生成 code/metadata と compiler build を信頼する native object 実行も確認します。別の serialized revision 採択までは既存 task artifact/native package の拒否を維持します。managed frame/result layout を再利用し、scheduler/IDL framework や別 IR を追加しません。

T06 は `runtime::task::runner` で invocation、managed frame/claim/result 状態、scheduling/storage service を共有し、backend ごとに resume/cleanup entry を持ちます。VM に MIR dispatch を残し、`codegen_clif::tasks` が演算、比較、分岐、失敗 edge、検査付き continuation 選択を生成します。native callback は MIR 命令/body を評価しません。`cargo run --example native-structured-tasks` は 42 を出力します。JIT と別途リンクした host object は finite Copy/Task local で VM と値/event/失敗起点/使用量/storage retirement が一致します。unsafe object binding は完全に一致する信頼済み code/metadata/compiler/SDK と生存 code owner を要し、公開 C embedding ABI や task package 形式を定義しません。既存 artifact/native package の拒否と後続 owned/loan/foreign/shared-control gate は維持します（2026-10-21 JST、macOS AArch64 検証）。 コンパイルは上限内で proof/layout/native metadata を累積し、invocation は別の preparation/guest control を維持します。private callback は unwind を封じ込めて source span の失敗証拠を保持し、allocation provider は別の no-unwind 契約を維持します。範囲外 continuation は callback 前に拒否し、standalone fixture は再生成 object byte とリンク入力を照合します。より広い性能/platform は主張しません。

T05 adapter の判断（実装前、2026-10-21 JST）: 証明を受け取る task layout/frame entry で managed finite Copy layout と FrameStorage を再利用します。TaskError は通常の 1 case の有限 error layout を持ちます。TaskClaim layout は payload 0 の marker とし、汎用 token fallback を使いません。invocation 内の sidecar が scheduler の偽造不能な TaskId を保持し、claim も logical cell に計上します。runtime scope は loop 再入場を区別する activation identity を持ち、追加の scheduled task は作りません。cleanup で frame storage を解放し、分離した型付き return reservation を取得/破棄まで生成 scope に保持します。これらの private layout は native/C/wire ABI を割り当てません。初期 VM adapter は到達可能な finite Copy/Task local と対応操作を副作用前に検査し、owned/loan/foreign 実行は静的規則を弱めず T07/T08 の明示的な後続作業に残します。host が allocator 権限、task 上限、execution control を渡します。source 昇格は source claim/loan と独立 MIR 証明を一緒に保持し、未対応 backend/artifact の拒否を維持します。

adapter 実装後は VM/JIT/AOT が同じ検証済み task 契約を使います。native task は continuation を compile し、MIR 解釈や VM fallback を使いません。それまでは既存同期 adapter が binding/allocation/副作用前に task profile を拒否します。現在の profile で定数実行/component task は未対応です。

T12 実装（2026-10-21 JST）: source lowering は通常 scope join と入れ子 return の各境界で cleanup を分割し、既存の独立 task/move/loan 証明で MIR を再検証します。runtime scope は割り当て不要の公開 link stack を保持し、結果を全 join まで維持して新しい順に retire します。取消/失敗でも RetireFailure より前に処理します。await 済みの穴は一度だけ飛ばし、mandatory abort でも同じ inventory を保持します。provider region の実行で公開順の両方向と遅い sibling を VM/native で検査し、source test で通常/入れ子 return 終了の owned local と loan の順序を検査します。owned 実行は T07 まで拒否し、wire/packet revision は割り当てません。

T07 実行 checkpoint（2026-10-21 JST）: VM/生成 native continuation で Text/Bytes、owning record、whole Option/Result extraction、同期 owned 引数/戻り値、field replacement、同一 frame 内の通常 loan を実行します。事前確保した owner list が実際の初期化逆順を保持し、move は後続 cleanup 前に元の登録を外します。有界 cleanup session が既存 Context/Report と reservation の iterative retirement を使います。task report は選択された terminal failure を先頭にし、完了 task の report を完了順に結合します。frame report は callback 順を保持します。RFC 0021 に従い取消を primary に保ち、成功した invocation outcome でも抑制された cleanup 証拠と poison 状態を公開します。CLI は poisoned outcome を拒否します。callback 失敗/panic や report truncation 後も cleanup を継続し、allocator provider の unwind 禁止契約は維持します。native service callback は MIR を解釈しません。owned 取消 source は 7、flat-buffer/whole-sum source は VM/JIT/リンク済み object で 71 を返します。T07 の部分 checkpoint であり、同期 frame 間の loan/borrowed pattern payload、container/shared/arena service、広い owned-variant 統合は引き続き副作用前に明示拒否します。async transfer は A01 まで finite Copy とし、foreign service、完全な共有 control、serialized task profile は各未完判断に残します。

T14 実装 checkpoint（2026-10-21 JST）: activation ID と起点 loan/generation sidecar を、一時的な access view で同期 ancestor に解決します。VM と生成 native service は所有 reservation を使い、scalar/field read、borrowed Option/Result payload、buffer mutation、exclusive replacement を処理します。reference binding の置換は空の payload byte を交換せず sidecar を移します。引数は入れ子 reference と allocator 権限を含め、通常の reborrow 契約を維持します。source fixture は VM/JIT/別プロセスのリンク済み object で 12 を返します。source テストは再帰 call/frame vector の拡張、loop generation 更新、正確な owner retirement、call 前後の取消、panic 起点、全 fuel/provider 拒否 prefix を検証します。独立 source/MIR 証明に従い通常 loan は suspension 前に終え、caller 所有 reference cell への loan 格納は SRC0022 のままです。async transfer、foreign、C、serialized profile は拡張しません。container/shared/weak と arena task service は T15/T16、全 owned-family 統合は T07、完全な共有 control は T08 に残します。

T15 実装 checkpoint（2026-10-21 JST）: task invocation が既存 managed shared heap と mixed retirement drain を準備/使用します。generic native core descriptor と frame 所有 Vec/Map/shared lease が既存 mutation/publication/shared custody 検証を維持します。起点/generation sidecar は compiler-known get/read 結果を pattern、extraction、同期 call に渡し、owning ticket を捏造しません。collection lease を shared read より先に終え、全 owner より先に lease を終えます。失敗/取消や有界 report の枯渇後も cleanup は必須です。owned pipeline は VM/生成 native/別プロセスのリンク済み object で 42 を返します。source テストは可変 element の置換/loop 更新、Map の置換/削除、入れ子 shared observation/置換、取消、全 fuel/provider 拒否 prefix、包含 cleanup panic/truncation を検証します。arena は T16、全 owned 統合は T07、完全な共有 control は T08 に残り、async transfer と foreign/C/serialized/package profile は維持します。

T16 実装 checkpoint（2026-10-21 JST）: source analysis/lowering は allocator 権限を捕捉する arena entry の直後に引数 temporary を終えます。独立 MIR テストは cleanup を yield 後へ移すと suspension-loan error で拒否します。明示的 allocator/arena-read loan binding は引き続き suspension で SRC0028 になります。各 task activation が既存 managed arena 状態を準備し、native descriptor は MIR 評価なしで ArenaEnter/Exit と arena cleanup target を保持します。登録済み read lease は所有 activation、検査済み構造 location、generation を解決し、同期 call forwarding にも対応します。VM/native は arena publication inspector と mixed retirement drain を共用します。owning-cycle source は suspension をまたいで 42 を返し、arena retirement 前に descendant を join します。scope 再入場、collection payload、shared publication 拒否、取消、panic 起点、全 fuel/provider 拒否 prefix、panic/report truncation を検証します。entry temporary を exit まで保持する既存同期 artifact は引き続き有効で、新 serialized/C/package profile は導入しません。一般 arena task transfer を禁止したまま、全 owned 統合は T07、完全な共有 control は T08 に残ります。

T07 統合 checkpoint（2026-10-21 JST）: 複合 task source は対応する全 owned-local family と nominal payload pattern を同期再帰、中断、入れ子 join を通じて実行します。source enum match は BorrowCaseField で 2 つの owning payload を partial move なしに借用し、provisional loan sum は単一 payload の UnpackVariant を使います。成功 source は 42、panic −42 は元の source/function identity を保持します。VM/native 差分テストは正確な cleanup inventory、primary/secondary report 順、使用量、全 fuel/provider 拒否 prefix、複数世代の取消と suppressed cleanup failure を照合します。保存した return は child failure や owner cleanup failure 後に公開せず、有界/省略 report でも必須 cleanup を省略しません。secondary task report は cleanup 完了順であり、terminal instruction の発生順と異なる場合があります。リンク済み object の全 11 profile に複合成功/失敗を含めます。完了するのは owned-source 統合受入条件であり、任意の owning 複数 payload MIR extraction、一般 async Send/Share、共有 service-work control、foreign/C/serialized/package profile は、引き続き未対応または既存 delivery に残します。

## ABI and compatibility

Rust MIR API は実験段階です。enum variant/effect field の追加に伴い内部 builder/match を移行します。内部 ordinal を使う箇所では既存 constructor/case の順を維持します。Rust enum layout は wire ABI ではありません。既存 schema の byte/意味を維持し、writer は未対応 task 型/effect を消去せず拒否します。reader は名前や scalar signature から async を推測しません。将来の encoded task profile は公開前に明示 revision 採択と不正入力/移行テストを要します。

native package は RFC 0018 に従い厳密な compiler/SDK build ID に結び付きます。semantic wire byte が同じでも compiler source の変更後は対応する SDK で native package を再 build します。混在 build のために startup mismatch 拒否を弱めません。

## Diagnostics and tooling

不正 task 型/宣言と未対応 profile は検証済み span を持つ構造化 MIR error を使います。source の SRC0027/SRC0028 は維持します。型だけの成功で source check や debugger task 対応を許可しません。formatter/parser は RFC 0022 のままです。

## Performance and cost model

既存の有界 ownership 型 graph、canonicalization、property 伝播、宣言 budget を再利用し、追加 traversal/storage を既存上限へ計上します。静的 MIR 証明で runtime cost/frame size/code size を主張しません。task/frame/result 費用は M3 の実行測定要件です。

## Security and trust boundaries

raw MIR の ID/span/cycle/storage/signature/上限を HIR と独立して検証します。source metadata は scheduler capability を付与しません。型/宣言 wrapper は実行/serialization/native 公開を許可しません。未使用 metadata も拒否し、backend に async effect を捨てさせません。

## Alternatives

- 変更なし: source task が全て未対応のままで必要な実行経路が進みません。
- library/tool: 外部 MIR の検証や backend/artifact 境界での async 意味の保持ができません。
- 別 task MIR/型系: 変更を隔離できますが最初の source fixture 前に所有権 semantics/adapter が重複します。
- 旧 executable profile に直接 task 命令を追加: 配線は単純でも未証明の suspension/cleanup や serialized 意味の変更を許します。

既存 ownership MIR を拡張し、task flow 証明までは厳密な証拠境界を維持します。

## Implementation plan

1. T11: 型/effect/宣言検証と旧 profile の明示拒否、内部 builder/exhaustive match の移行。
2. T04: 命令/continuation 設計を本 RFC に記録してから task scope/claim/suspension/所有権/cleanup edge を独立検証。
3. T05/T06: RFC 0021 driver 経由の source VM fixture と compiled native continuation 一致。
4. T07/T08: owned local、取消/失敗/cleanup、共有 control。全 M3 exit gate を維持します。

## Test plan

手書きの妥当/不正な型/宣言 graph、canonicalization/property 伝播、scope と独立した storage/signature 制限、偽装 effect、C 拒否、資源上限、未使用 metadata を検証します。T04 で HIR 権限に依存しない raw MIR scope/claim flow、branch/loop 合流、local/prepared move、loan 境界、return escrow、取消/失敗順序を追加します。旧 place/body/実行/artifact 経路が意味を消去せず拒否することを証明し、既存 artifact round trip と compiler baseline を維持します。T06 で finite Copy/Task local、aggregate、取消、loop/call、失敗起点、fuel/trace/provider 拒否、private packet 境界の VM/JIT/リンク済み object 比較を追加します。より広い owned/loan/foreign/shared-control 適合は未完了とし、compile-only fuzz を campaign と扱いません。

## Open questions

in-memory T04 証明に blocking question はありません。上記の命令/continuation と cleanup plan の判断は実装済みで、初期 task VM/native の layout/resume は実装済みで、owned task 実行、完全な共有 control、serialized task revision は別の未検証 delivery です。serialized revision は公開前に採択が必要です。既存 ownership MIR の拡張が semantics 重複を減らすという仮説であり、compile/runtime 性能向上は主張しません。

判断: **RFC 0001 実験段階の委任で accepted、2026-10-21 JST**。段階的な拒否で既存 semantics を維持し、実行権限を作りません。必要な表現の前提であり、新 milestone gate や T04–T08 の縮小ではありません。

## Sources

- T14 の根拠: `crates/runtime/src/task/runner/frame/access.rs`、activation/loan metadata と VM/native adapter、`tests/task_loan_execution.rs`、`tests/fixtures/task_sync_loans.lang`、リンク済み object fixture。code に更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。[RFC 0007](0007-source-ownership-and-core-values.md) は通常 reborrow、partial move 禁止、loan escape 規則の根拠で、最終更新 2026-10-20、確認 2026-10-21 JST です。reference cell へ逃がす実験は SRC0022 を維持し、source/MIR suspension テストは独立した証明を維持します。

- T07 owned checkpoint の根拠: `crates/runtime/src/cleanup/session.rs`、`crates/runtime/src/task/runner/frame/`、`machine/retirement.rs`、VM/native task adapter、`tests/task_owned_execution.rs`、`crates/runtime/tests/cleanup_session.rs`、`tests/fixtures/task_owned_buffers.lang`、`tests/support/task_static.rs`。更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。[RFC 0007](0007-source-ownership-and-core-values.md) と [RFC 0009](0009-whole-sum-ownership-extraction.md) は既存 core/whole-sum 契約の根拠で、最終更新は 2026-10-20、確認日は 2026-10-21 JST です。意図的に cleanup provider を panic させるのは containment 試験であり、provider からの unwind を許可しません。

- T12 順序の根拠: `crates/semantics/src/ownership_types/bodies.rs`、`crates/runtime/src/task/runner/`、`crates/vm/src/unit/tasks/machine.rs`、`tests/task_retirement.rs`、`tests/fixtures/task_owned.lang`、`examples/scripts/task_result_retirement.lang`。code の埋込み更新日はなく、2026-10-21 JST に確認/実行しました。owned source fixture は lowering のみを証明し、provider region の実行順検証は対応済み finite Copy task を使います。

- T06 native の根拠: `crates/runtime/src/task/runner/`、`crates/codegen-clif/src/tasks.rs`、`tasks/object.rs`、`tasks/tests.rs`、`tests/native_task_execution.rs`、`tests/support/task_static.rs`、`examples/native-structured-tasks.rs`。埋込み更新日はなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。初期 VM/JIT/リンク済み object の一致であり、owned task、共有 control、platform gate の完了ではありません。

- T05 実装: `crates/runtime/src/layout/`、`crates/runtime/src/task.rs`、`crates/semantics/src/analysis.rs`、`ownership_types/bodies.rs`、`crates/vm/src/unit/tasks/`、`tests/task_execution.rs`。埋込み更新日はなく、2026-10-21 JST に確認/実行しました。初期 source VM の値、取消、scope instance、失敗起点、資源拒否は native/owned task 適合と区別します。
- `crates/mir/src/unit/ownership/tasks.rs`、`ownership/failure.rs`、`crates/mir/tests/tasks.rs`、`tests/task_mir_boundary.rs`。埋込み更新日なし、実装と独立境界の証拠を 2026-10-21 JST に確認しました。静的 MIR 証明であり実際の task 実行適合ではありません。
- [RFC 0021](0021-bounded-structured-task-driver.md)、最終更新 2026-10-21。有界 driver の outcome と取消/cleanup 優先順位。
- [RFC 0022](0022-source-structured-tasks.md)、最終更新 2026-10-21。source task semantics と独立した source 証明。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、最終更新 2026-10-20。所有権/cleanup と段階的 C 対応。
- [RFC 0018](0018-native-compiler-records.md)、最終更新 2026-10-20 と `crates/codegen-clif/build.rs`（埋込み更新日なし、2026-10-21 JST に確認）。厳密な native compiler/SDK build identity。
- [Architecture](../architecture.md)、[roadmap](../roadmap.md)、最終更新 2026-10-21。`e4b17d0` を 2026-10-21 JST に確認。
- `crates/mir/src/unit/ownership.rs`、`ownership/declarations.rs`、`ownership/places.rs`、`crates/bytecode/src/ownership/`。埋込み公開/更新日はなく、`e4b17d0` を 2026-10-21 JST に確認。実装の根拠であり serialized task 形式の決定ではありません。

- T15 の根拠: `crates/runtime/src/task/runner/services.rs`、frame の Vec/Map/shared/target adapter、`tests/task_heap_execution.rs`、`tests/fixtures/task_owned_pipeline.lang`、リンク済み object fixture。code に更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。既存 service 契約の根拠は [RFC 0007](0007-source-ownership-and-core-values.md) と [RFC 0009](0009-whole-sum-ownership-extraction.md)（最終更新 2026-10-20、確認 2026-10-21 JST）です。

- T16 の根拠: `crates/hir/src/loans.rs`、source arena lowering、frame arena/read adapter、`crates/runtime/src/arena/managed/publication.rs`、`tests/task_arena_execution.rs`、`crates/semantics/tests/task_loans.rs`、名前付き source/リンク済み object fixture。code に更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。[RFC 0007](0007-source-ownership-and-core-values.md) と [RFC 0022](0022-source-structured-tasks.md)（最終更新/確認 2026-10-21 JST）が allocator 権限捕捉と、維持する通常 loan の suspension 規則を記録します。

- T07 統合の根拠: `tests/fixtures/task_owned_integrated.lang`、`tests/task_owned_integration.rs`、`tests/support/task_static.rs`。code に更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。契約は [RFC 0007](0007-source-ownership-and-core-values.md)、[RFC 0021](0021-bounded-structured-task-driver.md)、本 RFC に基づき、最終更新/確認は 2026-10-21 JST です。外部性能や fuzz campaign の主張ではありません。
