<!-- translation-of: ../../rfcs/0014-explicit-source-panic.md -->

# RFC 0014: 明示的なソースpanic

[English](../../rfcs/0014-explicit-source-panic.md)

- Status: implemented
- Authors: Codex（所有者による実験的な委任に基づく）
- Created: 2026-10-19
- Last updated: 2026-10-20
- Accepted: 2026-10-19
- Implemented: 2026-10-19
- Tracking issue: M2-F13
- Supersedes: なし。RFC 0011のschema profileとRFC 0012のprivate revisionを拡張
- Superseded by: なし

## 現在の実装状態

2026-10-20 JSTに[M2 capabilityと検証根拠](../milestone-2-capabilities.md)と照合済みです。本RFCの範囲はimplementedであり、stabilizedではありません。以下の当初計画、decision時点の記述、日付付きcheckpointはrollout履歴として読みます。そこで別作業としていたnative／SDK／platformの現在状態は上記ガイドを参照してください。今回発見したRFC 0012の論理debugger検証不足は同ガイドの訂正で解消し、本RFCの非目標や規範的要求は変更しません。

## 概要

RFC 0008の`core::panic::<T>(code: i64) -> T`を実装する。呼び出しは復帰しない。動的なcode、呼び出しspan、論理stack、必須cleanupをVM/nativeで保持する。明示的panic命令を含むunitに限りownership schema 3とprivate native revision 3を導入する。

## 動機

最小のソース呼び出しは現在SRC0024となる。schema 2の命令表は閉じており、既存native panic helperは固定unwrap codeしか扱わない。無関係なunwrapの流用は受理済みの操作を曖昧にし、動的codeを失う。

## 目標

通常の引数評価と失敗cleanupで明示的panicを実行する。旧artifactとprivate native profileを保持する。ソースの制御フローと所有権解析に非復帰を伝える。

## 非目標

panicの捕捉、native unwind、新しいソース構文、ソースnever型、新runtime core-service tag、C export公開、const実行。

## 利用者向け設計

`let mut stopped: core::Unit = core::panic::<core::Unit>(code);`はi64を返す関数内でも現在の実行を終了する。後続ソースも型検査し、既存の到達不能診断を出す。外側の呼び出しはソース順に引数を評価し、非復帰以降の副作用は起きない。

## 参照意味論

通常の明示的generic引数と値引数の検査を使う。Tは通常のloanやallocator authorityを含む有効なソース結果型であり、Tの値は作らない。評価する子式と文の制御フローへ非復帰を再帰的に伝える。panic前に評価した所有値は必須cleanupの対象となる。非復帰後にしか評価しないoperandのmove/borrowを行わない。

typed MIRに結果のない`Panic(code_register)`を追加し、i64を要求する。Unreachableで終わるblockの最終命令でなければならない。既存のfuel pollingを行う通常のguest実行step一つで、host callや追加service workはない。動的な符号付きi64 codeを記録し、Panicとして失敗する。共有failure graphがcleanupを実行し、cleanup失敗でも先のpanicをprimaryとして保持する。新しい並行性、capability、借用、FFI authorityは導入しない。

## 実行モード

VM、native JIT、native AOTは同じ命令、span、stack、usage、cleanupを実装する。生成native codeは検査済みruntime helperを呼び、成功して続行できない。host callbackは既存の封じ込めと優先順位を維持する。component実行とコンパイル時実行は今回の対象外。

## ABIと互換性

schema 2の33命令tagと37runtime core tagは閉じたままとする。schema 3は同じframing/table順で、命令tag 33と後続のcode registerを加える。結果とspan fieldの符号化は変えない。writerはPanicを含むunitに限りschema 3、それ以外はschema 2を出す。readerは両方を受け入れ、schema 2のtag 33とPanicのないschema 3を拒否し、再符号化の正規性を守る。version dispatchは別profileへfallbackしない。既存のC identity serialization拒否も維持する。

Panicを含むunit全体はprivate symbol `lang_owned_r3_f<index>`を使う。revision 1/2のinput/output packet、statusの意味、helper index 0–33を保持する。動的guest panic用helper 34を末尾へ加え、private helper表を35要素にする。helperはcode cellを検査し、cleanup前にfaultを記録する。不正なcellでは実際の失敗を保持する。C対応unitも含めunit内全関数でrevision 3を使い、追加C capabilityは与えない。Panicのないunitはrevision 1/2を維持する。これらは外部C ABIや安定plugin interfaceではない。ソース、package、componentの移行は不要で、旧readerは新schema 3をversionで拒否する。

## 診断とツール

generic arity、i64不一致、到達不能診断を再利用する。返却loanを捏造しない。不正MIR continuationと誤ったartifact profileは構造化validation errorにする。artifact/native metadataでソースspanを保持する。parser、formatter、LSP、debuggerの構文拡張は不要。

## 性能とコストモデル

Panicに所有値の確保やcopyは不要。検査済みscalar read一回とfailure dispatchの後に既存必須cleanupを行う。非復帰以降をMIRから省く。測定前に性能を主張しない。

## セキュリティと信頼境界

実行前にopcode/version、operand型、空の結果、最終位置、Unreachable終端を検証する。decode入力はhelper pointerやcapabilityを供給できない。信頼済みgatewayが引き続きcontext identityと検査済みcontinuation dispatchを管理する。panicは必須cleanupを迂回せず、poison済みinstanceを再開しない。

## 代替案

変更なしでは受理済みソース機能を使えない。libraryだけではcompilerの終端制御フローを表現できない。新panic terminatorは既存の命令fuel/failure機構を重複させる。generic結果の捏造や無関係なunwrapは架空の成功経路を作るか動的codeを失う。版を変えないopcode/helper追加は閉じた契約に違反する。結果なし命令は検証可能な終端条件で既存failure graphを再利用する。

## 実装計画

ソースsignature/非復帰とMIR validation、VM/native helperとversion付きartifact、ソース・不正artifact・cleanup/control prefix・standalone AOTを実装し、日英の完了根拠を更新する。全実行経路とbaseline検査の成功までF13は未完了。

## テスト計画

generic、code型、loan、到達不能のcompile-pass/fail、入れ子式と引数順序、正負/境界値の動的codeと論理stack/span、cleanup失敗と有限fuel/work、VM/JIT/AOTの一致、schema 2のbyte安定性、schema 3の往復と不正version/tag/continuation拒否を検査する。全baselineを実行し、完成snapshotのWindows根拠を依頼する。

実装修正（2026-10-20 JST）：[複合source](../../../examples/conformance/owned_pipeline.lang) により、arenaのallocator loanがactiveな末尾panicをloan activity解析が誤って拒否することを確認しました。Typed MIRのPanicが最終instruction＋Unreachableである検証を前提に、その終了だけはloan終了を必須failure retirementへ委ね、primary panic記録後にowner／loanを解放します。通常return、bare Unreachable、不正な成功継続は引き続き拒否します。VM／実production native entryの通常値42・panic−42、全fuel／work／provider拒否prefixとframe／cell境界を比較し、元allocator／Instanceのretirementを検証します。実AOTと実C／保存C objectでも複合sourceを検証します。受理済み意味論の実装修正であり、構文・ABI・schema 1–4／native record形式は不変です。Compiler sourceが変わるため、SDKは既存の完全一致build方針どおり再buildが必要です。情報源：[loan解析](../../../crates/mir/src/unit/ownership/loan_activity.rs)／[回帰test](../../../crates/mir/tests/ownership_loan_activity.rs)／[VM-native比較](../../../tests/support/native_entry_runtime.rs)、2026-10-20 JST更新・確認。

## 未解決事項

ローカルでソースの非復帰、loan結果の特殊化、VM／nativeの失敗・cleanup比較、schema互換性、standalone AOTを検証した。このsnapshotのWindows実行と性能根拠はM2-F07とbenchmark作業で追跡する。初期の暫定3ポイントは時間の約束ではなく、実装済みは安定化を意味しない。

## 情報源

- RFC 0008、受理済みソースpanic契約。最終更新2026-10-19、確認2026-10-19 JST。
- RFC 0011、閉じたschema 2契約。最終更新2026-10-19、確認2026-10-19 JST。
- RFC 0012、private native continuation revision。最終更新2026-10-19、確認2026-10-19 JST。
- 0d43a41でのローカルソース実験、2026-10-19 JST。explicit_panic.langはSRC0024。

判断: 既存ソース契約と閉じたserialization/native境界を確認し、所有者の実験的委任によりacceptedとする。選択される追加profileは旧unitを維持し、明示的panicに検証可能な終端形を与える。受理は実装済みや安定化を意味しない。
