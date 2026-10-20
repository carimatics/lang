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

既存 ownership MIR に Task/TaskError 型と ABI から独立した async effect を追加します。型/宣言の証明は task flow の証明ではありません。task 命令と独立 flow 証明までは place/body 境界でこの profile を拒否し、既存の実行/保存経路が意味を消去できないようにします。

T11 checkpoint（2026-10-21 JST）: 型/effect/宣言検証を実装しました。旧 place/body、runtime layout、bytecode writer は task metadata を明示拒否し、旧 decoder は同期 effect を復元して新 tag を割り当てません。特に汎用 runtime token layout の fallback で Task/TaskError の storage を作りません。task 命令/flow/cleanup と VM/native task 実行は未実装であり、契約全体の完成まで本 RFC は accepted のままです。

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

## Execution modes

adapter 実装後は VM/JIT/AOT が同じ検証済み task 契約を使います。native task は continuation を compile し、MIR 解釈や VM fallback を使いません。それまでは既存同期 adapter が binding/allocation/副作用前に task profile を拒否します。現在の profile で定数実行/component task は未対応です。

## ABI and compatibility

Rust MIR API は実験段階です。enum variant/effect field の追加に伴い内部 builder/match を移行します。内部 ordinal を使う箇所では既存 constructor/case の順を維持します。Rust enum layout は wire ABI ではありません。既存 schema の byte/意味を維持し、writer は未対応 task 型/effect を消去せず拒否します。reader は名前や scalar signature から async を推測しません。将来の encoded task profile は公開前に明示 revision 採択と不正入力/移行テストを要します。

native package は RFC 0018 に従い厳密な compiler/SDK build ID に結び付きます。semantic wire byte が同じでも compiler source の変更後は対応する SDK で native package を再 build します。混在 build のために startup mismatch 拒否を弱めません。

## Diagnostics and tooling

不正 task 型/宣言と未対応 profile は検証済み span を持つ構造化 MIR error を使います。source の SRC0027/SRC0028 は維持します。型だけの成功で source check や debugger task 対応を許可しません。formatter/parser は RFC 0022 のままです。

## Performance and cost model

既存の有界 ownership 型 graph、canonicalization、property 伝播、宣言 budget を再利用し、追加 traversal/storage を既存上限へ計上します。この型だけの変更で runtime cost/frame size/code size を主張しません。task/frame/result 費用は M3 の実行測定要件です。

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

手書きの妥当/不正な型/宣言 graph、canonicalization/property 伝播、scope と独立した storage/signature 制限、偽装 effect、C 拒否、資源上限、未使用 metadata を検証します。旧 place/body/実行/artifact 経路が意味を消去せず拒否することを証明し、既存 artifact round trip と compiler baseline を維持します。後続の flow/不正入力/VM/native differential fixture で実際の task 実行を証明し、compile-only fuzz を campaign と扱いません。

## Open questions

型/宣言境界に blocking question はありません。命令/continuation encoding、task 対応 cleanup、serialized revision は未選択で実装前の記録が必要です。既存 ownership MIR の拡張が semantics 重複を減らすという仮説であり、compile/runtime 性能向上は主張しません。

判断: **RFC 0001 実験段階の委任で accepted、2026-10-21 JST**。段階的な拒否で既存 semantics を維持し、実行権限を作りません。必要な表現の前提であり、新 milestone gate や T04–T08 の縮小ではありません。

## Sources

- [RFC 0022](0022-source-structured-tasks.md)、最終更新 2026-10-21。source task semantics と独立した source 証明。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、最終更新 2026-10-20。所有権/cleanup と段階的 C 対応。
- [RFC 0018](0018-native-compiler-records.md)、最終更新 2026-10-20 と `crates/codegen-clif/build.rs`（埋込み更新日なし、2026-10-21 JST に確認）。厳密な native compiler/SDK build identity。
- [Architecture](../architecture.md)、[roadmap](../roadmap.md)、最終更新 2026-10-21。`e4b17d0` を 2026-10-21 JST に確認。
- `crates/mir/src/unit/ownership.rs`、`ownership/declarations.rs`、`ownership/places.rs`、`crates/bytecode/src/ownership/`。埋込み公開/更新日はなく、`e4b17d0` を 2026-10-21 JST に確認。実装の根拠であり serialized task 形式の決定ではありません。
