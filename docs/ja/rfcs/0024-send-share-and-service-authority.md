<!-- informational-translation: en; canonical-source: ../../rfcs/0024-send-share-and-service-authority.md -->

# RFC 0024: Recursive Send/Share and service authority

[English](../../rfcs/0024-send-share-and-service-authority.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-21
- Last updated: 2026-10-22
- Tracking issue: M3-A01, M3-A02, M3-A05, M3-A06
- Supersedes: none
- Superseded by: none

## Summary

A02 service catalog 決定（2026-10-22 JST）: [RFC 0026](0026-source-isolated-services.md) は opaque ServiceSetup<S,M>/ServiceOwner<S,M>/ServiceHandle<M>、owned Send/storable state/message、owner の検証済み read/全体置換、message/取消専用 producer を受理します。receive で handler を開始し、finish/次 receive/退役で終了します。取消は冪等 request で、enqueue を止め、active handler 終了後に commit 済み message を FIFO、state の順に退役します。source/MIR は role・read origin・move・suspension を独立証明し、adapter と旧 artifact/foreign profile は各 delivery まで拒否を維持します。通常の composition 実験は [1,42,7] で退役し、別の managed 境界が必要な理由を示します。この決定だけでは delivery/gate を完了しません。

A03 実行 checkpoint（2026-10-22 JST）: 独立に証明した service authority は、Receiver/ServiceOwner・入れ子/再帰 ownership を含む完全な Send/storable state を private VM で実行します。producer は state authority や暗黙 join を得ません。既存 backing と有界 publication walk が実 state/queue edge を読み、置換では候補 state だけを prospective 検査し、再開 send を再検査します。動的な循環/資源拒否は commit 前の旧 state/generation と候補 ownership を保ちます。shared/weak producer publication は実 private identity を検証し、handle の message 型引数は strong ownership に対して phantom のままです。read generation・取消/closure・FIFO queue-before-state 退役は source/control/provider/quota と nested panic/report probe で inventory 0 を保ちます。service native/別リンク parity は A04 に残します。registry の追加・caller capability flag・wire/foreign 受理・M3 gate 完了は導きません。

C06 型 checkpoint（2026-10-21 JST）: 予約された opaque `core::ChannelSetup<T>`、`core::Sender<T>`、`core::Receiver<T>` の identity を source/HIR と独立検証する MIR に保持します。既存 graph で有限な Send/Share 依存を別々に導出し、Sender の両 capability は message の Send、Receiver は Share 不可、Setup は両方不可、shared/weak は target の両 capability を要します。全 role は non-Copy で退役が必要であり、未使用 message constructor も Send を要します。重点11件は再帰/宣言順 graph、alias、不正 message/arity、move、偽造 MIR、proof resource の上限境界/1 未満、未対応 profile の admission を検証します。schema-2/3/4 writer は constructor byte 前に拒否し、実行 layout、VM/native channel admission、両 target の native package は未使用 endpoint も guest/provider 副作用前に拒否します。これは型 capability の証明であり channel 実行ではありません。具体 call/error/MIR operation、suspension loan proof、guest allocator/cell 統合、VM/native fixture は C03/C04 に残り、wire/C layout や M3 gate は完了しません。

C01 follow-up 決定（2026-10-21 JST）: [RFC 0025](0025-bounded-channels-and-backpressure.md) で endpoint capability を選びます。Sender<T> は T が Send なら Send/Share、Receiver<T> は T が Send なら Send ですが Share ではなく、local Setup<T> は両方不可です。Sender は queued payload を所有せず identity metadata を保持し、receiver/service state projection を公開しません。handle-bearing message publication は強い所有循環を独立拒否します。新 constructor は以前の閉じた型族の Send/Share 同値を流用せず、source/MIR で独立導出します。具体 C03 catalog/proof と adapter が揃うまで未対応とし、wire/C/package profile を推論しません。

A06 実装 checkpoint（2026-10-21 JST）: source と MIR は独立に検証した推移 type fact から compiler-derived logical-task Send/Share を提供し、新 graph cache、trait 自己宣言、bit-copy transfer、wire tag を導入しません。async signature は owned Bytes/Text/Vec/Map、sum、再帰 record/enum、checked shared/weak を受理します。loan/arena/allocator/Task 権限は禁止し、通常 move 解析で spawn 引数を一度だけ消費します。重点 7 テストで両 graph、未使用/前方再帰 signature と alias、不正 MIR loan signature の type span での拒否、二重消費拒否、proof work 境界値/1 未満を確認しました。到達する owned async transfer は A05 の publication/await/disposal 実装まで VM/native で副作用前の Unsupported 拒否を維持します。source actor authority は具体的 channel handle 後の A02 に残ります。部分実装の証拠であり、RFC 0024 と全 M3 gate は未完です。

A05 実行 checkpoint（2026-10-21 JST）: VM と生成 native continuation は recursive Send 引数/結果を managed move で実行します。分離した result reservation は owner span を保持し、consuming await または破棄まで live cell として計上します。await は owned Ok payload を初期化して所有権を登録し、未回収結果は既存 mixed cleanup drain を通じて local owner より先に公開順の逆順で退役します。取消と未公開 return は一度限りの退役、primary failure の優先、有限 report、poison 証拠を維持し、必須 cleanup は work 課金しません。差分 15 テストで Bytes/Text/Vec/Map、record/whole enum、shared/weak identity、入れ子 task 世代と owned Result、公開前後の取消、cleanup panic/truncation、保持 cell による admission、resource 上限境界/1 未満、全 fuel/work/provider/interruption prefix を検証します。転送 fixture 2 本を加え linked-object は 13 profile になります。`cargo run -- run tests/fixtures/task_owned_transfer.lang` と `cargo run -- run tests/fixtures/task_owned_transfer_all.lang` は 42 を返します。logical task は同一 invocation 内に限り、source actor authority、channel、foreign/C/serialized/native-package profile、全 M3 gate の残り delivery を維持します。

検証済み source type から内部 capability を導出します。`Send` は同じ invocation の logical task 間で owned 値を移動する権限、`Share` は mutable alias 権限を与えず検査付き shared observation を行う権限です。async 引数/結果と channel message は Send を要求します。ユーザーによる宣言、trait/closure capture 構文、暗黙 clone、detached task、thread 間 host ABI は追加しません。

Service state は owner task に属します。service handle は interface で受理された message/取消操作だけを与え、mutable state projection を公開しません。具体的な handle type と操作は後続の channel/service 決定で選びます。この受理だけでは操作の実装や M3 gate は完了しません。

## Motivation

RFC 0022 は有限 Copy async transfer のみを先行対応しました。owned local は中断を越えて実行できますが、`async fn worker(value: core::Bytes) -> core::Bytes` は SRC0026 を報告します。channel と isolated owner の owned message 対応には再帰的 transfer 規則が必要です。従来の service 実験に手動指定する Sendable bit は source/独立 MIR の証明にはなりません。

## Goals

- 再帰 nominal/container graph を含む、独立かつ budget 付きの source/MIR 導出。
- 既存 cleanup、result escrow、共有 control に従う move-only owned async transfer。
- 通常の ownership/loan による owner state access。enqueue 権限による別 task state の変更は禁止します。

## Non-goals

thread、任意の host handle、public Rust Send/Sync 実装、ユーザー定義 trait、borrowed async 引数、Task claim transfer、channel/backpressure 構文、serialized format。

## Guide-level design

async worker は Send な値で構成した Bytes、Text、Vec、Map、record/variant を受け取り、返せます。non-Copy 引数は通常の source move 解析で消費します。await した成功結果は child escrow から一度だけ移動します。取消 child は payload を返さず、未取得の成功結果は retirement まで scope が所有します。source service は private な moved state を持つ async owner であり、producer は別途検証する message handle で通信します。対象が Share でも通常の shared/exclusive loan を別 task へ渡すことは禁止します。

実行例です（A06/A05 runtime admission test で検証）。0 を返します。

```text
async fn worker(value: core::Bytes) -> core::Bytes { yield; return value; }
async fn main() -> i64 {
    let mut input: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut output: core::Bytes = core::result_unwrap::<core::Bytes, core::TaskError>(await worker(input));
    return core::bytes_len(output);
}
```

## Reference semantics

### Derivation

| Type | Send | Share |
| --- | --- | --- |
| Unit, bool, i64, finite built-in error | yes | yes |
| Text, Bytes | yes | yes |
| Option/Result, record, variant | 全 field/payload が Send | 全 field/payload が Share |
| Vec<T>, Map<K,V> | 全 type argument が Send | 全 type argument が Share |
| shared T, weak T | T が Send かつ Share | T が Send かつ Share |
| ordinary loan, allocator authority, arena owner/reference, Task claim | no | no |
| future foreign/resource/service handle | 受理済みの明示 capability contract に従う | 受理済みの明示 capability contract に従う |

Share は owner を Copy にせず clone も供給しません。Bytes/container の変更には通常の exclusive loan が必要で、shared payload は既存 read/whole-replacement 境界を使います。いずれかの alias に read が残れば置換を拒否します。全 backend は RFC 0004/0007/0021 に従い invocation 内の managed 操作を直列化します。これは logical-task capability であり、現在の allocator/provider や private Rust runtime 値が OS thread を越せるという保証ではありません。parallel adapter は同じ profile を受理する前に同期を確立する必要があります。

検証済み type graph の最大不動点を導出します。候補 capability を true から始め、禁止 leaf または欠けた依存を逆依存へ伝播して安定させます。同じ規則を表す既存の推移的 type fact は再利用し、別 graph framework は要求しません。妥当な再帰 Vec/nominal graph は再帰だけを理由に拒否しません。到達可能な cycle に禁止 field があれば包含 capability を false にします。既存 layout/storage 検証は不正な by-value 再帰と stored loan を引き続き拒否します。正規化した alias の capability は一致させます。source/MIR は同じ規則を独立に検証し、work/storage を制限して枯渇を構造化拒否として報告します。呼出側の capability bit は信頼しません。

### Transfer, admission and ownership

async parameter/result は未使用関数を含め宣言 type span で Send を要求します。通常 loan の suspension 越え禁止、scoped Task claim、region 制限は維持します。引数は通常 move 規則で左から右へ一度だけ評価します。spawn は child 公開や所有権 transfer 前に task/frame/cell/allocation を事前検査し、拒否時も準備済み値を caller cleanup の管理下に保ちます。成功時は non-Copy root を一度だけ transfer し、owning ticket の bit copy は行いません。同期 loan は現在の実行 segment 内に限ります。

owned result は child cleanup と必須 join を終えるまで escrow に保持し、成功 await は Ok へ一度だけ移動します。取消または terminal/cleanup failure は T を捏造せず staging/result owner を退役します。scope 所有の未取得 result は実際の retirement まで cell/heap を計上します。必須 cleanup は RFC 0013 に従い非 preemptive・非計量です。静的 capability 導出と guest-frame marshalling は service-work event を増やしません。

### Service authority

通常 local state とその排他的に所有する子孫を borrow/replace できるのは owner async activation だけです。Send は所有権を移し、producer に state への loan を与えません。後続 service interface は別 task に typed message enqueue/取消権限だけを公開し、producer projection に mutable state を含めず、trust boundary で動的 identity/generation を検証する必要があります。明示 shared subgraph は別の checked shared 規則に従い、private owner state には含めず、その local state root への access も与えません。既存モデルの non-owner exclusive mutation という代替方針は受理しません。channel/message commit、FIFO backpressure、具体的 source 操作は channel 決定を受理してから実装します。

## Execution modes

source/HIR と独立 MIR は一致する必要があります。VM、生成 native、リンク済み object は同じ owner を移動/退役し、起点・使用量・取消結果を一致させます。未実装 owned-transfer adapter は実装 delivery まで副作用前の明示拒否を維持します。foreign/C/component、REPL、永続 task/package profile の現行制限も維持します。

## ABI and compatibility

以前拒否した owned async signature を拡張します。既存 Copy program の値・所有権・event cost は維持します。内部の導出 fact/proof のみを追加し、source trait annotation や public native/C 表現は追加しません。serialized task format は別の versioned 決定とし、tag 割当や既存 reader への暗黙対応は行いません。将来の handle は受理済み contract が capability 条件と検証を供給するまで未対応とします。

## Diagnostics and tooling

non-Send async type を source signature/argument で報告し、取得できる場合は最初の禁止依存も示します。通常の use-after-move と borrow-at-suspension 診断を保ちます。独立 MIR 拒否は typed error/span を保持します。導出 budget 枯渇は resource failure とし、受理へ fallback しません。LSP/formatter は既存 async 構文と同じ検証済み source fact を使い、未証明 capture を受理する別解析 mode は作りません。

## Performance and cost model

budget 付き graph worklist/SCC 相当の伝播を使い、再帰 host stack traversal は避けます。検証済み ownership layout、reservation、transfer/retirement を再利用します。Send は暗黙 deep copy、retain、graph scan を意味しません。安定化前に実際の transfer/result/shared-operation のコストを測定し、速度向上や thread scalability は主張しません。

## Security and trust boundaries

safe source は capability の自己宣言や task/service identity の捏造をできません。type proof は runtime shared/publication 検査、allocator identity、receiver authority、foreign resource 検証の代わりにはなりません。未信頼 serialized/foreign input は type 名だけで capability を得ません。この contract は trusted native code を sandbox にしません。

## Alternatives

Library/tool だけでは cross-task ownership transfer を認可できず、source/MIR の proof が必要です。library は受理済み async/channel 操作を組み合わせて service を構成できますが、その proof と authority 制約に従います。


Copy-only transfer の維持は単純ですが owned message を満たせません。loan 以外の全 owner を許す案では arena/allocator/Task/foreign 権限や shared operation の安全性を検査できません。ユーザー指定 Send は検証済み unsafe 境界なしに宣言を信頼します。無制限の thread-safe shared mutation は受理済み owner/loan contract と異なり、別の memory-model 設計が必要です。

## Implementation plan

規則を受理し、A06 で独立 source/MIR capability 導出と ownership compile-pass/fail、A05 で owned async 引数/結果実行を実装します。A02 は具体的 channel handle が利用可能になってから source actor authority の証拠を完成させます。owned async 引数/結果の実行は独立した観測可能な前提 delivery として実装前に audit/明示し、signature 受理だけで transfer/escrow 実装を完了扱いにしません。channel/source service delivery はこの proof を使います。follow-up 受理時に RFC 0005/0022/0023 と翻訳を改訂し、日付付き checkpoint は保ちます。実装と gate 完了は区別します。

## Test plan

再帰 record/variant/Vec/Map/Text/Bytes/shared/weak/Copy の compile-pass、直接/入れ子 loan、allocator/arena/Task 権限、non-Send な将来 resource、payload 再利用の compile-fail を検証します。宣言順、alias、再帰禁止依存、未使用 metadata、不正 MIR、導出 budget 境界を試します。後続実行では owned 引数/結果、admission 失敗、未取得 result 破棄、transfer 前後の取消、shared read/replace conflict、入れ子 join、panic、cleanup/report 枯渇を VM/native/リンク済み object で検証します。実際の全 fuel/work/provider 拒否境界と retained-result cell 上限も確認します。channel/service 実行時に実際の producer/owner isolation fixture を追加し、既存 host model だけでは完了としません。

## Open questions

A05 は既存 managed transfer/escrow で新 owner registry なしに owned async publication を実行することを検証し、A06 は proof 上限境界/1 未満で導出の有限性を確認しました。未検証の仮定は代表的な導出/転送費用の許容性、将来 channel handle の interface 権限のみの保持、両 host の動作適合です。実装 audit は不足する観測可能な前提を明示し、全 M3 受入条件を保つ必要があります。thread 間や foreign handle の安全性は推論しません。

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**。検証済み型の再帰的性質、通常 move/loan、既存 checked shared publication と直列 invocation 境界を組み合わせれば、追加の借用や thread 安全性を仮定せず owned task transfer を定義できます。service handle に state projection を与えず、既存 producer/owner 分離を維持します。上記の仮定は実装と両 host の検証まで未検証で、受理だけでは実装・安定化・gate 完了を意味しません。

## Sources

- [RFC 0004](0004-owned-values-and-cyclic-graphs.md) 最終更新 2026-10-17、[RFC 0005](0005-structured-tasks-and-isolated-services.md)/[0007](0007-source-ownership-and-core-values.md)/[0021](0021-bounded-structured-task-driver.md)/[0022](0022-source-structured-tasks.md)/[0023](0023-task-mir-boundaries.md) 最終更新 2026-10-21。2026-10-21 JST 確認。project contract であり外部性能の証拠ではありません。
- `crates/hir/src/tasks.rs`、`crates/hir/src/types.rs`、`crates/mir/src/unit/ownership/tasks.rs`、`crates/runtime/src/task/runner/`、実験 `crates/semantics/src/service.rs` を 2026-10-21 JST 確認。code に更新日の埋込みはありません。最小の owned Bytes worker 引数/結果例はこの決定の実装前に SRC0026 を再現します。

- A05 実行証拠: `crates/runtime/src/task/runner/`、`tests/task_owned_transfer.rs`、`tests/fixtures/task_owned_transfer.lang`、`tests/fixtures/task_owned_transfer_all.lang`、`tests/support/task_static.rs`。コードに更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で検証しました。RFC 0021/0022/0024（最終更新 2026-10-21）の既存 cleanup と transfer 契約を用い、host thread/foreign capability を与えません。
