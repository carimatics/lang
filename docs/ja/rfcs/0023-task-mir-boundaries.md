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

既存 ownership MIR に Task/TaskError 型、ABI から独立した async effect、独立検証した task 命令/scope/claim flow/suspension cleanup を追加します。型/宣言だけの証明は task flow の証明ではありません。既存同期実行/保存経路はこの profile を拒否し、意味を消去できないようにします。

T11/T04 checkpoint（2026-10-21 JST）: 型/effect/宣言検証を実装しました。旧 place/body、runtime layout、bytecode writer は task metadata を明示拒否し、旧 decoder は同期 effect を復元して新 tag を割り当てません。特に汎用 runtime token layout の fallback で Task/TaskError の storage を作りません。T04 で同じ body table を使う別 task 検証 entry を追加し、scope/claim/return escrow 解析と move/loan/arena region/failure plan の証明を結合しました。async failure plan は取消と primary execution failure を区別し、loan 終了、scope cancel/join、owner retirement、escrow 破棄の順を明示します。VM/native task 実行は未実装であり、契約全体の完成まで本 RFC は accepted のままです。

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

## Execution modes

adapter 実装後は VM/JIT/AOT が同じ検証済み task 契約を使います。native task は continuation を compile し、MIR 解釈や VM fallback を使いません。それまでは既存同期 adapter が binding/allocation/副作用前に task profile を拒否します。現在の profile で定数実行/component task は未対応です。

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

手書きの妥当/不正な型/宣言 graph、canonicalization/property 伝播、scope と独立した storage/signature 制限、偽装 effect、C 拒否、資源上限、未使用 metadata を検証します。T04 で HIR 権限に依存しない raw MIR scope/claim flow、branch/loop 合流、local/prepared move、loan 境界、return escrow、取消/失敗順序を追加します。旧 place/body/実行/artifact 経路が意味を消去せず拒否することを証明し、既存 artifact round trip と compiler baseline を維持します。後続の VM/native differential fixture で実際の task 実行を証明し、compile-only fuzz を campaign と扱いません。

## Open questions

in-memory T04 証明に blocking question はありません。上記の命令/continuation と cleanup plan の判断は実装済みで、backend layout、実際の resume、serialized task revision は別の未検証 delivery です。serialized revision は公開前に採択が必要です。既存 ownership MIR の拡張が semantics 重複を減らすという仮説であり、compile/runtime 性能向上は主張しません。

判断: **RFC 0001 実験段階の委任で accepted、2026-10-21 JST**。段階的な拒否で既存 semantics を維持し、実行権限を作りません。必要な表現の前提であり、新 milestone gate や T04–T08 の縮小ではありません。

## Sources

- `crates/mir/src/unit/ownership/tasks.rs`、`ownership/failure.rs`、`crates/mir/tests/tasks.rs`、`tests/task_mir_boundary.rs`。埋込み更新日なし、実装と独立境界の証拠を 2026-10-21 JST に確認しました。静的 MIR 証明であり実際の task 実行適合ではありません。
- [RFC 0021](0021-bounded-structured-task-driver.md)、最終更新 2026-10-21。有界 driver の outcome と取消/cleanup 優先順位。
- [RFC 0022](0022-source-structured-tasks.md)、最終更新 2026-10-21。source task semantics と独立した source 証明。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、最終更新 2026-10-20。所有権/cleanup と段階的 C 対応。
- [RFC 0018](0018-native-compiler-records.md)、最終更新 2026-10-20 と `crates/codegen-clif/build.rs`（埋込み更新日なし、2026-10-21 JST に確認）。厳密な native compiler/SDK build identity。
- [Architecture](../architecture.md)、[roadmap](../roadmap.md)、最終更新 2026-10-21。`e4b17d0` を 2026-10-21 JST に確認。
- `crates/mir/src/unit/ownership.rs`、`ownership/declarations.rs`、`ownership/places.rs`、`crates/bytecode/src/ownership/`。埋込み公開/更新日はなく、`e4b17d0` を 2026-10-21 JST に確認。実装の根拠であり serialized task 形式の決定ではありません。
