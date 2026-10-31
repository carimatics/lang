<!-- translation-of: ../../rfcs/0054-registry-quarantine-transactions.md -->

# RFC 0054: 監査付きregistry quarantine transaction

[English](../../rfcs/0054-registry-quarantine-transactions.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E07, M5-E04
- Supersedes: none; RFC 0046–0048・0053を具体化
- Superseded by: none

## 概要

明示的なpackage scopeを持つ管理者が、既存の認証action routeで厳密releaseを隔離・復元する。Quarantineは不変archive・yank flag・owner・完全履歴を保持し、新規archive downloadを停止する。単一HEAD置換でflag・package revision・audit eventをcommitする。最新認可を受けたretryは、古いread世代へ戻さず元receiptを返す。

2026-10-31 JST、RFC 0001の実験期委任に基づき採択する。既存の`Operation::Admin` policy、transaction/replay guard、有界index modelがこの閉じた拡張を支える。独立した30-checkのpolicy/encoding実験で、replay前の権限確認、期限切れ、revision/no-op競合、逆action、archive/yank identity保持を確認した。Storage・HTTP・両hostの証拠ではない。実装と必須の敵対的/process/transport試験は未検証である。

## 動機

RFC 0046は管理quarantineを要求するが、現在のaudit variantでは記録できない。Yankとして扱うと厳密downloadが誤って利用可能なままになり、release flagの直接編集は全履歴検証を壊す。E04の残作業監査では独立したlocal provisioning/owner復旧要件も明らかになった。QuarantineをE07、local管理lifecycleをE08とし、E04は統合された復旧/権限外操作の出口監査として保持する。

## 目標

- 現在のscope付き管理者によるquarantine/unquarantineを、完全な監査済みstate再構成とともにcommitする。
- 全履歴byteと独立yank flagを保持し、隔離中のremote厳密archive readを拒否する。
- 認証HTTPS公開 → quarantine拒否 → 復元 → 未変更のVM/native実行を検証する。

## 非目標

- Quarantineへのowner権限、公開enrollment/credential管理route、local provisioning/owner復旧（E08）。
- 保持releaseの削除、archive identity変更、署名/鮮度証拠、cache消去、callerへ既に渡したbyteの回収。
- 破損履歴の拒否を迂回する修復やregistryのreset。

## ガイドレベル設計

`math@1.0.0`をrevision 1で公開した後、`math`の`admin` grantを持つbearerがexpected revision 1と理由を付けたcanonical `quarantine` actionを送る。Revision 2のindexは厳密releaseをquarantinedとし、そのobject GETを拒否する。Owner専用のpublish/yank/owners tokenでは操作できない。Revision 2への`unquarantine`はrevision 3を作り、以前のyank flagを保持して厳密archiveを復元する。本番実行向けにarchiveを信頼する根拠にはならない。

## 参照セマンティクス

### 閉じたactionと現在の権限

Schema 1の順序付きfield `schema`、`request_id`、`expected_revision`、`action`、`reason`、`version`を持つ`QuarantineAction`を追加する。Actionは厳密に`quarantine`または`unquarantine`とし、NAMEは別途受理するroute要素とする。2,048-byte、canonical 2-space JSONと末尾LF、正のpackage revision、32文字lowercase hex request ID、厳密version文法、control文字のない1–256-byte reasonという既存上限を再利用する。未知・重複・不適用field、別encodingを拒否する。

Request digestは`POST /v1/packages/NAME/actions`、expected package revision、厳密canonical bodyの長さ/hashからなるRFC 0048の既存HTTP request tupleを使う。他operationとrequest-ID identityを共有し、operation/bodyの変更は競合する。Caller指定actorは受け取らない。

既存の非待機排他writerを保持し、全履歴検証、最新host時刻取得、実bearer認証、`Operation::Admin(NAME)`要求を行う。Packageは存在しなければならない。現在の所有権は必要条件でも十分条件でもない。厳密admin grantは通常の公開・yank・owner変更を認可しない。Replay前と、staging後のHEAD置換直前に再認証し、元の要求deadlineとcancellation scopeを維持する。失効・期限切れ・inactive・誤scopeのactorは過去の成功もreplayできない。

### 原子的遷移とreceipt

新規requestでは現在のpackage revisionと存在する厳密versionを要求する。現在と同じquarantine値の要求は新eventなしで競合する。変更するのは当該releaseの`quarantined` flag、package revision（+1）、state revision（+1）、state/authority time floor、audit sequence/linkだけとする。Owner、credential、無関係release、不変descriptor/archive対応、独立yank flagを保持する。既存のchecked加算、state/index/物理/workspace上限をすべて書込み前に適用する。`false`への復元や10進revisionの増加で派生indexが大きくなっても上限内に収める。

`name`、`version`、`archive_sha256`を持つ閉じたaudit operation `quarantine`・`unquarantine`を追加する。閉じたresultの順序付きfieldは`state_revision`、`package_revision`、`name`、`version`、`archive_sha256`、`quarantined`とする。既存の有界audit envelopeで最新actor/credential、厳密request digest/ID、reasonを記録する。履歴検証はcanonical requestを再構成し、event時点の記録actorを検証して完全な次stateを再構成し、完全なresultも比較する。Quarantine recordで他state変更を隠せない。

HTTP成功応答はRFC 0046の6-field receipt、outcome `quarantined`または`unquarantined`、status 200を使う。認可済み厳密retryはflag再適用や世代rollbackなしに元receiptを返す。単独の計上receiptはleaseを保持し、HTTP body/receiptは解放までRFC 0053の既存予約を保持する。Commit/refresh/replyの結果不明では、必要な明示reopen後に同一要求で照合する。

### Read・中断・復旧の動作

完全な公開indexには厳密releaseと両flagを引き続き含める。現在世代は、commit済み非quarantine releaseが参照するobjectだけを提供する。隔離後のindex/archive readは古いservice世代を使えないが、並行readerへ既に渡したbyteは回収できない。履歴/物理recoveryでは隔離archiveを引き続き検証・保持する。不変archive対応の一時fingerprintを計算するときだけ、両mutable flagを正規化する。完全な遷移比較では両flagを引き続き別々に検証する。

既存のstagingによる不変配置、最終guard、単一HEAD commit点を使う。中断後は完全な旧stateまたは新stateとなる。既知の有界orphan cleanupはRFC 0048に従い、commit済み隔離objectを削除しない。破損・未知entryではrecoveryを拒否する。Lock queue、新filesystem/network capability、unsafe code、guest実行権限は追加しない。

## 実行モード

Source semantics、MIR、bytecode、JIT、native AOT、component契約を変更しない。Fixtureは公開sourceをVM/nativeで42として実行し、隔離中の新規download拒否を観測し、復元後に同一元byteを再び42として実行する。隔離されたunsigned fixtureは本番trust constructorではない。

## ABIと互換性

既に定義されたHTTP action/outcome variantを実装する。新Rust APIは追加である。実験段階のprivate schemaは1を維持し、閉じたaudit variantを追加する。古いreaderは未知履歴を推測・破棄せず拒否しなければならない。当該storeを開く前にprogramを明示upgradeする。履歴のin-place書換え、自動downgrade、edition、native/component ABI変更はない。

## 診断とツール

不正入力・認証・認可・不在/非公開object・競合・上限・busy・state・I/O失敗に既存の`REG0001`–`REG0008` classとHTTP対応を再利用する。Bearer/body textを反映しない。Clientはcanonical receiptをregistry・request・expected revision・quarantine outcomeと照合する。Parser・formatter・LSP・debugger変更は不要である。

## 性能とコストモデル

既存256 MiB論理budget、有界transaction workspace、重複read世代予約を維持し、RSSやthroughputは保証しない。Quarantineはarchive内容を複製/置換しないが、全履歴検証と不変audit/state世代保持には引き続き有界costとstorage増加がある。Quota枯渇時は履歴を削らず拒否する。

## Securityとtrust境界

Quarantineは厳密な管理scopeだけが許可する可用性判断である。まだ有効な管理bearerの盗難はlocal失効まで残留riskとなる。Source変更、証拠消去、署名trust付与、既に渡したbyteの失効はできない。Local host侵害とrollbackは明示的な残留riskに留める。Owner拒否だけでなく、最終時刻の期限切れ、inactive/失効権限、異なるscopeの管理者を検証する。

## 代替案

- **変更しない:** 採択済みの管理quarantine要件が未実装に残る。
- **Proxy/filter:** URLを拒否できるが、flag・index・audit・永続recovery履歴を原子的に結合できない。
- **物理削除/archive置換:** 履歴証拠を失い、不変identityとrecoveryを壊し得る。
- **Yank再利用:** 小さいが厳密downloadを誤って許可し、ownerに管理capabilityを付与する。
- **採用する閉じた遷移:** 検証済みtransaction境界を再利用し、権限とflagを独立させる。

## 実装計画

1. E05を固定したまま、本契約とpolicy実験を記録する。E05だけをdoingに維持する。
2. E05公開後にE07を選択する。有界action、完全audit/履歴遷移、計上/client/service dispatch、実source実行journeyを一緒に追加する。
3. 敵対的履歴、厳密上限、権限/replay、process中断、writer競合、両host実TLSを検証し、最新head CI経由で公開する。
4. E08で独立した監査付きlocal管理lifecycleを実装し、E04でG5全体の復旧・権限外操作journeyを監査する。E07でどちらも完了扱いしない。

## テスト計画

- 独立canonical action/request vector、重複/schema/UTF-8/size/reason/version/IDと入力変異。
- Replay前の現在scope、owner専用/別名admin拒否、最終期限切れ/失効/inactive、逆action後の旧ID retry。
- Yankと厳密archive identityを保持するquarantine/unquarantine、無関係な全authority/release/result fieldの完全再hash偽造。
- 実process-exit中断、両writer順序、旧/新reopen、既知/未知orphan、download拒否、結果不明照合、明示service stop/reopen。
- 拒否時無書込みの厳密派生index/論理memory境界、全local baseline、厳密fuzz lint、最新headのWindows/macOS CI。

## 未解決事項

実装を阻む未決契約判断はない。実storage/HTTPでの適用、全履歴の偽造遷移拒否、resource境界、両host動作は受入条件であり、policy modelからの結論ではない。

## 出典

- [RFC 0046](0046-registry-protocol-and-recovery.md)、2026-10-31更新: wire variant、scope付き管理、不変storage、復旧。
- [RFC 0047](0047-registry-authority-records.md)、2026-10-31更新: 厳密admin grant、最新権限。
- [RFC 0048](0048-registry-state-and-publication.md)、2026-10-31更新: 全履歴検証、request identity、orphan recovery。
- [RFC 0051](0051-registry-yank-transactions.md)、2026-10-31更新: 独立したmutable yank遷移。
- [RFC 0053](0053-registry-request-workspace.md)、2026-10-31更新: 共有workspace、応答寿命、cancellation。
