<!-- translation-of: ../../rfcs/0022-source-structured-tasks.md -->

# RFC 0022: Source の構造化タスク

[English](../../rfcs/0022-source-structured-tasks.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-T03, M3-T09, M3-T10, M3-T04–T08
- Supersedes:
- Superseded by:

## Summary

A06 実装 checkpoint（2026-10-21 JST）: source と MIR は独立に検証した推移 type fact から compiler-derived logical-task Send/Share を提供し、新 graph cache、trait 自己宣言、bit-copy transfer、wire tag を導入しません。async signature は owned Bytes/Text/Vec/Map、sum、再帰 record/enum、checked shared/weak を受理します。loan/arena/allocator/Task 権限は禁止し、通常 move 解析で spawn 引数を一度だけ消費します。重点 7 テストで両 graph、未使用/前方再帰 signature と alias、不正 MIR loan signature の type span での拒否、二重消費拒否、proof work 境界値/1 未満を確認しました。到達する owned async transfer は A05 の publication/await/disposal 実装まで VM/native で副作用前の Unsupported 拒否を維持します。source actor authority は具体的 channel handle 後の A02 に残ります。部分実装の証拠であり、RFC 0024 と全 M3 gate は未完です。

A05 実行 checkpoint（2026-10-21 JST）: VM と生成 native continuation は recursive Send 引数/結果を managed move で実行します。分離した result reservation は owner span を保持し、consuming await または破棄まで live cell として計上します。await は owned Ok payload を初期化して所有権を登録し、未回収結果は既存 mixed cleanup drain を通じて local owner より先に公開順の逆順で退役します。取消と未公開 return は一度限りの退役、primary failure の優先、有限 report、poison 証拠を維持し、必須 cleanup は work 課金しません。差分 15 テストで Bytes/Text/Vec/Map、record/whole enum、shared/weak identity、入れ子 task 世代と owned Result、公開前後の取消、cleanup panic/truncation、保持 cell による admission、resource 上限境界/1 未満、全 fuel/work/provider/interruption prefix を検証します。転送 fixture 2 本を加え linked-object は 13 profile になります。`cargo run -- run tests/fixtures/task_owned_transfer.lang` と `cargo run -- run tests/fixtures/task_owned_transfer_all.lang` は 42 を返します。logical task は同一 invocation 内に限り、source actor authority、channel、foreign/C/serialized/native-package profile、全 M3 gate の残り delivery を維持します。

A01 follow-up 決定（2026-10-21 JST）: [RFC 0024](0024-send-share-and-service-authority.md) は同一 invocation の logical task に対する再帰 Send/Share と mutable state projection のない interface-only producer handle を定めます。初期 Copy-only transfer subset を拡張し、loan/arena/allocator/Task transfer 禁止、checked shared 操作、独立 source/MIR proof を維持します。host thread の保証はありません。実行受理前に A06 導出と A05 owned async publication/await/disposal を検証し、A02 は具体的 channel handle 後の actor authority proof を維持します。以前の日付付き checkpoint は履歴です。channel/backpressure と foreign/C/serialized/package の決定は別途必要で、M3 gate は未完了です。

RFC 0005 の厳密な構造化並行性を RFC 0021 の driver で実行する source 構文を定めます。関数は `async` を宣言し、`spawn` が scope 付きの子を作り、`await` が handle を消費し、task scope は終了前に join します。borrow は suspension を跨げません。source semantics の採択であり、source task 実行や serialized coroutine 形式の完成ではありません。

T05/T06 checkpoint（2026-10-21 JST）: resolved async effect と有界 move/scope claim snapshot を、別段階の source loan 証明と結合します。await/yield/cancel_check/暗黙 join は生存 shared/exclusive loan を SRC0028 と loan 起点 span 付きで拒否し、終了する lexical loan を自身の join 前に終え、入れ子 join では外側 loan を維持します。TaskLoans だけでは実行できません。TaskProgram がこれらの source 証明と独立検証した ownership task MIR を保持し、task VM は明示 host allocator/task/control 権限でその証明を使います。`cargo run -- run examples/scripts/tasks.lang` は両 worker の yield 後に 42 を返し、CLI check と LSP は結合した source/MIR 証明を保持します。raw HIR 互換 API、REPL task submission、未対応実行/artifact adapter は各 profile を引き続き拒否します。async transfer は A01 まで scalar/finite Copy 値、初期 VM/native local は finite Copy/Task に限定します。実験段階の native continuation は JIT と別途リンクした host object で VM と値/event/cleanup が一致します。owned/loan/foreign task 実行、完全な共有 control、serialized task 形式は別の delivery に残し、契約全体の実装まで本 RFC は accepted のままです。

```text
async fn worker(value: i64) -> i64 {
    yield;
    return value;
}
async fn main() -> i64 {
    task_scope {
        let mut left: core::Task<i64> = spawn worker(20);
        let mut right: core::Task<i64> = spawn worker(22);
        let mut a: core::Result<i64, core::TaskError> = await left;
        let mut b: core::Result<i64, core::TaskError> = await right;
        return core::result_unwrap::<i64, core::TaskError>(a)
            + core::result_unwrap::<i64, core::TaskError>(b);
    }
}
```

T07 部分実行 checkpoint（2026-10-21 JST）: [RFC 0023](0023-task-mir-boundaries.md) に、実際の owned Text/Bytes、record、whole sum、同期 owned call、local loan を VM/生成 native/リンク済み object で実行する契約を記録します。取消/失敗は実 owner を retire し、有界で順序付きの report を保持して、cancelled join の抑制 cleanup 証拠も公開します。frame 間 loan、container/shared/arena の task service、広い統合は T07 に残り、この checkpoint で M3 gate は完了しません。async transfer は A01 まで既存 finite Copy profile を要します。

## Motivation

有界 host task driver は木と実 host frame の破棄を実行しますが、source の task 構文/effect、独立検証 task MIR は未実装です。既存宣言の `let mut name: Type = expression;` と明示型を維持します。候補 fixture は現在の parser で `async`、`yield`、task expression に失敗します。識別子扱いや未知 statement の無視では並行性の意味が失われます。

## Goals

- 明示 async effect と型付き join 取消による scoped spawn/await/yield/cancel。
- 厳密な loan、所有権、result escrow、決定的 cleanup、span の維持。
- VM/JIT/AOT/host 駆動に共通の意味と明示 scheduler 権限。
- parser 先行段階でも source async を同期実行しないこと。

## Non-goals

- closure/generator/pinning、借用 capture、detach、任意の first-class future、暗黙 thread/global executor。
- 最終 channel/service/Send/Share 導出、production component/C embedding profile。別の M3 判断を維持します。
- async C export/import、source 変数 debug location、一般推論拡張、安定 coroutine layout。

## Guide-level design

`async fn` と `pub async fn` は非同期 guest 関数です。async body は暗黙 task scope を持ち、`task_scope { ... }` は途中の lexical join 境界です。同期関数では spawn/await/yield/task 取消 check/task scope は使えません。async 関数から同期 guest 関数は通常通り呼べます。将来の script entry として `async fn main() -> i64` を認め、host が root scheduler を供給します。同期 entry は従来通りです。

`spawn worker(args)` は最も内側の task scope に直接の子を enqueue し、worker の戻り型 T に対する `core::Task<T>` を返します。async call は spawn/await の直下だけで使え、隠れた lazy future 型は作りません。`await worker(args)` は同じ scope で spawn/join します。`await handle` は handle を move し、`core::Result<T, core::TaskError>` を返します。初期の unit case は `TaskError::Cancelled` です。terminal execution failure は scope に伝播し TaskError に回復可能化しません。通常 guest の `Result::Err` は成功した task value なので、await の結果は入れ子 Result です。

`cancel handle;` は handle を消費せず冪等に取消要求し、suspend しません。`cancel_check;` は要求を観測し、保留がなければ yield しません。`yield;` は協調 suspension で ready queue 末尾へ移ります。await 中の親の取消は親を閉じ、子だけの取消は join の `Err(Cancelled)` になります。

```text
async fn cancelled() -> core::Result<i64, core::TaskError> {
    let mut child: core::Task<i64> = spawn worker(1);
    cancel child;
    return await child;
}
```

## Reference semantics

### Grammar and resolution

`async`、`task_scope`、`spawn`、`await`、`yield`、`cancel`、`cancel_check` を予約します。comment が隣接しても keyword は別 token です。

```text
function       := ["pub"] ["async"] "fn" signature block
task-scope     := "task_scope" block
yield-stmt     := "yield" ";"
cancel-stmt    := "cancel" expression ";"
check-stmt     := "cancel_check" ";"
unary          := "spawn" unary | "await" unary | existing-unary
```

spawn/await は加算/比較より強く call/field より弱く結合し、括弧を優先します。parser は operand tree を保持し、typing が spawn の direct async guest call、await の Task value/direct async call を要求します。既存の expression statement 制限を保ち、join Result の保持/破棄には型付き local を使います。modifier 順は `pub async fn`、C 宣言に async は付けません。

`core::Task<T>` と `core::TaskError` は予約 core namespace の型です。Task は直接の子の結果を取得する linear/non-Copy/non-Send/non-Share の claim で、生成 lexical scope の brand を持ちます。source は task/scope brand を作れません。初期 Task 型は async body 内の local のみで、field/container/関数引数・戻り値/host・C signature/入れ子 Task 結果を拒否します。生成 scope の外側に宣言された local への move/代入を拒否します。await/move 後の使用は既存の初期化/move 検査で拒否し、cancel は live task place を検査して clone/move しません。

Task claim は借用できません。既存の owned pattern matching は通常 loan を作るため、Task の直接 `match` を拒否します。型付き local に move して await してください。特別な move-pattern semantics は導入しません。T09 互換性監査（2026-10-21 JST）: opaque claim の制限を明示し、guide の Result unwrap に既存仕様で必要な core 型引数を補いました。新たな推論は導入しません。

### Effects, evaluation and admission

async は native ABI と独立した関数 effect です。call 検査前に effect を解決します。async 引数/戻り値は RFC 0005 の再帰的 sendability を要し、loan/arena reference/task handle/mutable alias/非 sendable host value は移せません。safe source が Send を宣言することはできません。最初の実行 fixture は scalar/有限 Copy 値だけでもよく、後続の accepted 導出で規則を緩めず対応を広げます。

spawn 引数は親で左から右へ一度評価します。子の公開前に scheduler capability、live/累積 task 上限、frame/result 容量、型、move を preflight し、その後にだけ引数所有権と queue entry を commit します。拒否時は子を公開せず、評価済み引数 owner を failure cleanup します。Task/frame quota 枯渇は既存 call/frame admission と同じ execution-control failure で、回復可能 TaskError ではありません。frame は invocation に明示供給した allocator/service を使い、source metadata から暗黙 executor/allocator grant を得ません。

await/yield/取消 check/暗黙 scope join は suspension/取消点です。live shared/exclusive loan をすべて拒否します。scope 終了では、その lexical scope とともに終了する loan を暗黙 join より先に終えます。内側 join でまだ live な外側 loan は拒否します。source と typed MIR の両方で独立に証明し、unsafe でも迂回しません。定数評価に async/task 操作はありません。

### Lifetime, result escrow and cleanup

handle とは独立して子は生成 scope に所属します。通常終了は全子を join し、terminal failure/取消は子孫へ先に取消要求します。return は scope を閉じる前に結果を一度 escrow へ評価し、必須 cleanup 後に公開します。子は scope/親 frame より長生きできません。

子の結果は子の cleanup 成功まで escrow に保持し、公開成功時に scope の result storage への初期化順を記録します。await は唯一の claim を消費して `Ok(T)` に一度 move します。取消した子は T を公開しません。Task の move は claim の移動です。handle の破棄/上書きは取得権を放棄しますが detach/暗黙取消はしません。未取得結果は scope が所有・計測し、全 join 後、通常の owned local cleanup より前に、結果公開の逆順で破棄します。子が live task でなくなった後も、保持 result の cell/heap は共有 invocation 上限に含めます。

frame local は loan 終了/子孫 join 後に RFC 0008 の実初期化逆順で破棄します。cleanup dispatch/確認は RFC 0021 に従い、entry 前の取消、active callback の非強制停止、明示的親 failure 優先、spawn 順で最初の失敗した子、副次 cleanup failure を維持します。guest fuel 枯渇や report 打切りでも必須 retirement を省略しません。

## Execution modes

typed MIR は async effect、scope identity、spawn 引数 move、result claim、suspension continuation、取消、cleanup edge と span を記録します。scope/handle liveness と suspension loan 状態を HIR と独立検証します。VM と compiled continuation は共通 driver を使い、native は MIR 解釈/VM fallback を行いません。同じ外部 event 順で値/失敗/観測/所有権/cleanup を一致させます。

CLI は有界 task 権限を供給する明示 host です。component host は scheduler 権限/上限を明示 grant し、source/package metadata は要求を宣言するだけです。C/native callback に暗黙の scheduling/再入権限を与えません。poll/callback 取消は production embedding 判断に従います。M2 同期 program の既存 control を維持し、task 取消が instruction fuel/interruption を無効化しません。

## ABI and compatibility

source syntax/core 型/effect の契約を追加しますが、native frame layout や既存 wire schema を変更しません。task 対応 MIR/bytecode/native record/startup adapter は、serialized 実行を認める前に明示 version 拡張と不正入力/互換性検証が必要です。未対応 artifact/profile は副作用前に拒否し、旧 program に暗黙の task を与えません。raw Task handle は C/component value ではなく、公開 component async signature は production profile を待ちます。

7 つの新 keyword と一致する既存識別子は、例 `async` → `async_value` のように改名が必要です。自動 rename は提供しません。安定 source edition はまだありません。C 宣言は同期のままで Task を公開できません。async effect の変更は互換性に影響し、将来の該当 profile の import/export に記録します。

T16 allocator/arena の判断（2026-10-21 JST、実装前に実験段階の委任で採択）: arena entry は invocation allocator 権限を捕捉し、その引数 temporary だけを直後に終えます。RFC 0007 で明確化した arena-entry 契約に従い、別 binding の通常 allocator loan と通常 arena-read loan は厳密な suspension 拒否を維持します。source lowerer は ArenaEnter 直後に対応 cleanup を出力し、独立 MIR は arena loan を例外扱いせず、loan が終わった状態を検証します。各 activation は payload admission 前に preparation budget 内で自身の arena 状態を準備します。登録済み arena read lease は owner の activation/slot と起点/generation を持ち、検査済み view、publication inspection、same-region 検証、mixed iterative retirement は既存 runtime service を使います。join 済み scope の owner retirement より先に lease を終え、同じ有界 report に failure/poison 証拠を保持し、cleanup 失敗後も独立した retirement を続けます。arena 値を task 間転送せず、serialized/C/package profile は変更しません。新 source fixture で owning cycle、中断、入れ子 descendant join、取消、失敗 cleanup を VM/生成 native/リンク済み object で検証します。temporary scope の短縮は以前拒否した arena task scope の互換性拡張であり、suspension 時の loan を許す例外ではありません。

## Diagnostics and tooling

専用 CST node で async 宣言/scope/prefix/statement を保持します。不正 async signature や operand 欠落でも全 token、囲む brace、後続宣言を回復で保ちます。format は空白だけを変え comment/優先順位/冪等性を維持します。LSP format は同じ CST formatter を使います。

段階導入で未対応の async 実行/解析 profile に `SRC0026`、task effect/scope/claim 違反に `SRC0027`、`concurrency.borrow-across-suspension` に `SRC0028` と loan 起点の副 label を使用します。既存 move/type 診断も使用します。source 型/claim と loan 証明だけで実行用 source check を成功させず、task MIR と adapter 接続までは executable checked program 作成前に未対応 profile を報告します。debug ancestry/lint は後続 tooling 作業で、source 変数/DAP 対応を主張しません。

## Performance and cost model

task 作成には admission 済み frame/result storage と bookkeeping が必要です。suspension は owned local、未取得結果は scope cleanup までメモリを保持します。await は結果を clone せず move します。並列性/latency/無確保 async は保証しません。両 tier-1 host で frame/result peak と spawn/join/取消費用を測定し、M2 workload baseline を保ちます。新構文にも既存 token/node/depth/work/output 上限を適用します。

## Security and trust boundaries

untrusted source は handle 構築/Send 宣言/scope 逸出/suspension を跨ぐ loan/暗黙 scheduler 権限を取得できません。foreign MIR/bytecode には source 証明だけでは不十分で、境界で証明と admission を反復します。task/result/heap/control 上限の callback 合算は残る M3 契約に従います。trusted host は adapter storage/cleanup 確認を正しく行う責任があり、native process 内 sandbox とはしません。

## Alternatives

- **変更なし:** accepted driver を source から利用できません。
- **library-only task 関数:** lexical brand、async effect、suspension loan、独立 MIR 証明を強制できません。
- **frame borrow/pinning 付き first-class lazy future:** 合成しやすい一方、RFC 0005 の初期規則にない寿命/アドレス安定/取消/所有権契約が必要です。
- **同期 spawn sugar:** lowering は容易ですが suspension/queued child 取消を表せず、目的の挙動を実装しません。
- **await が T を直接返す:** 短い一方、明示的な子の取消を terminal failure と混同せず型付きで表せません。Result で guest error とも分けます。

## Implementation plan

1. T03: 本判断の採択、lossless lexer/parser/recovery/formatter fixture、semantic admission の明示拒否。
2. T09: resolved async effect、Task/TaskError 型、scoped claim、call/move/escape/C surface 検査。
3. T10: 明示/暗黙 suspension の source loan 検査と関連 span。
4. T04/T05: 独立 task MIR/source VM fixture、T06: compiled native parity。serialization/startup の依存実装前に正確な artifact revision を決定。
5. T07/T08: 実 owned local/result、取消 cleanup、共有 control。channel/Send/Share/embedding は board の別判断を維持。

## Test plan

lossless CST span/回復/優先順位、comment/冪等 format、予約識別子/資源上限を検証します。未使用関数/import/REPL/LSP/virtual/preparsed 入力で profile discovery を検証します。対応 task 解析は source/MIR 証明を結合し、未対応 adapter は明示拒否を維持します。後続 compile-pass/fail で async call、scope 逸出、消費済み handle、両 loan、全 exit/C 境界を検証します。source/不正 MIR/bytecode の VM/JIT/AOT 差分で値、型付き取消、terminal failure 順、escrow/move、exactly-once cleanup を一致させます。suspension/claim 遷移を fuzz し、安定化前に両 host を測定します。

## Open questions

厳密な source 契約の blocking question はありません。未検証の仮定: local-only handle が初回 source slice に十分、保持 result escrow の費用が共有上限内で許容、より広い native profile とその費用。T06 は finite Copy/Task local で MIR 解釈を使わず managed-frame driver を再利用できることを示しました。一般 Task 引数/container/高度 future は別判断です。正確な serialized revision と production embedding は明示前提であり、暗黙に採択された形式ではありません。

判断: **RFC 0001 の実験段階の委任で accepted、2026-10-21 JST**。明示 effect/scoped claim/型付き子取消は RFC 0005/0008/0021 を維持します。実装/安定化は上記の証拠を要します。

## Sources

- T06 native の根拠: `crates/runtime/src/task/runner/`、`crates/codegen-clif/src/tasks.rs`、`tasks/object.rs`、`tasks/tests.rs`、`tests/native_task_execution.rs`、`tests/support/task_static.rs`、`examples/native-structured-tasks.rs`。埋込み更新日はなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。初期 VM/JIT/リンク済み object の一致であり、owned task、共有 control、platform gate の完了ではありません。

- T05 source/VM の根拠: `crates/semantics/src/analysis.rs`、`ownership_types/bodies.rs`、`crates/vm/src/unit/tasks/`、`tests/task_execution.rs`、`examples/scripts/tasks.lang`。埋込み更新日はなく、2026-10-21 JST に確認/実行しました。初期 VM の証拠であり native/owned task や M3 全体の適合ではありません。
- [RFC 0005](0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21: 厳密な task/borrow/取消規則と後続判断との対応。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、[RFC 0021](0021-bounded-structured-task-driver.md)、最終更新は順に 2026-10-20、2026-10-21: cleanup と実装済み driver。
- `crates/syntax/src/{lexer,parser,format}.rs`、`crates/hir/src/resolved.rs`、`crates/semantics/src/{modules,binding}.rs` を `b45a62d` で 2026-10-21 JST に確認。変更前の候補 source check は async/task 構文で SRC0012 を報告します。grammar 実験であり runtime/性能の証拠ではありません。
- 新しい構文/effect/result は project decision です。外部言語/性能の新たな事実は主張しません。

- A05 実行証拠: `crates/runtime/src/task/runner/`、`tests/task_owned_transfer.rs`、`tests/fixtures/task_owned_transfer.lang`、`tests/fixtures/task_owned_transfer_all.lang`、`tests/support/task_static.rs`。コードに更新日の埋込みはなく、2026-10-21 JST に macOS AArch64 で検証しました。RFC 0021/0022/0024（最終更新 2026-10-21）の既存 cleanup と transfer 契約を用い、host thread/foreign capability を与えません。
