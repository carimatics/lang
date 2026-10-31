<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0056-registry-retained-checkpoints.md -->

# RFC 0056: 保持したregistry checkpointとreceipt検証

[English](../../rfcs/0056-registry-retained-checkpoints.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E04, M5-F01
- Supersedes: none; RFC 0045・0046・0048を具体化
- Superseded by: none

## Summary

明示的に開いたhost registryは、commit済み全履歴を検証した後で上限付きcheckpointをexportできる。これを独立して保持したhostは、後で現在履歴への包含を調べ、checkpoint以前のexport済み操作receiptを検証できる。Checkpointのparseは認証ではない。このAPIはmutation・source消費・実行権限を与えない。

2026-10-31 JST、RFC 0001の実験期委任により採択する。既存の到達可能履歴の証明を使い、別transactionやstorage namespaceを設けず包含境界を得られる。独立policy実験22 checkはrollback・fork・厳密なrecord対応・receipt順序を含む。PythonとJavaScriptで339-byte canonical例とdigestが一致し、全fieldの有効最大幅は652 byteとなる。Policy/encodingの証拠のみで、本番統合・敵対入力動作・両host実行はE04の受入条件に残る。

## Motivation

RFC 0045のR12/R16は独立保持した証拠を要求し、RFC 0046は検証済みreceipt/checkpointのexportをE04/F01に残す。`Receipt::as_bytes`は情報用の結果をexportするが、保持receipt byteの検証や、保持headと現在の完全な履歴を比較するhost APIは存在しない。Local hash chainだけでは、後の観測前にoperatorが全履歴を置換したことを検出できない。以前に独立保持したcheckpointは矛盾する置換を示せるが、operatorの誠実性や全体の完全性を保証しない。

## Goals

- 厳密なcommit済みcheckpointをexportし、独立保持checkpointが現在の検証済みheadの祖先であることを検証する。
- 既存の全receipt result variantを完全なcommit済みaudit recordとcheckpointのsequence上限に照合する。
- 権限外操作・local復旧・再開・未変更source実行と保持証拠検査をG5出口journeyで統合する。

## Non-goals

- 署名認証・公開checkpoint endpoint・transparency witness・全体の非分岐保証・信頼timestamp・自動保持。
- 欠落storageの復元・置換HEADの選択・孤立世代の採用・mutation/HTTP権限の変更。
- F01–F04向けsealed package消費証拠の付与、service memory budget変更。

## Guide-level design

Hostは明示的directory capabilityで`Registry`を開く。`Registry::checkpoint()`は不透明な`Checkpoint`を返し、`as_bytes()`でcanonical byteを取得してregistryから独立保持する。後のopen時に`Checkpoint::admit(bytes, identity)`はencodingと独立設定service identityを検査し、`Registry::verify_checkpoint(&checkpoint)`はcommit済み履歴への包含を検査する。Hostは信頼できる保持copyを選ぶ必要があり、未信頼の置換履歴と一緒に取得したbyteは独立anchorではない。

`Registry::verify_receipt(&checkpoint, receipt_bytes)`はcheckpointとreceiptを現在履歴に照合した後だけ、既存の情報用`Receipt`を返す。Checkpointより新しいreceiptは現在履歴内でも拒否する。そのeventまでの証拠が必要なら、明示的に新しいcheckpointをcaptureする。既存receipt exportと厳密request照合は不変。

## Reference semantics

### Checkpoint profile 1

順序付き閉recordを`{format:"lang-registry-checkpoint", schema:1, registry, origin, state_revision, time_floor, state_sha256, audit_sha256}`とする。既存の2-space pretty JSONと末尾LFちょうど1個を使い、1,024 byte・depth 16以下とする。Registry/originは`ServiceIdentity`の検証に従い、独立identityに厳密一致する。Revisionは1..=65,536、timeは0..=253402300799、両hashは64桁lowercase hexとする。重複/未知/欠落field・null・未対応format/schema・順序変更/非canonical JSON・後続入力・上限超過をdecoded data保持前に拒否する。`Checkpoint`はprivate recordと上限付きcanonical byteだけを保持し、public field constructorとpublic serde deserializerを持たない。

`checkpoint()`はregistryの既存nonblocking排他lock保持中に、read-onlyの全履歴validatorを呼ぶ。検証済み現在state hash・revision・永続time floor・最後のaudit hashと設定identityをcaptureする。Cleanup・transaction・時刻取得・event作成・filesystem書込みを行わない。Time floorはcommit済みfieldであり、現在wall-clock freshnessの主張ではない。

### Inclusion and receipt verification

検証は既存の有界inventory・archive identity・厳密遷移検査で現在全履歴を検証する。Checkpointのstate hashは到達可能state集合内に存在しなければならない。既存digest/no-follow境界でそのimmutable stateを読み、全checkpoint fieldと比較する。未来/rollback済みcheckpoint・孤立state・異なるidentity・誤ったtime/revision・保持stateを含まないforkは拒否する。真正な古い祖先は後続操作後も有効だが、後続操作が独立観測されたことは証明しない。

Receipt入力は既存の閉じた`{schema:1, audit_sha256, result}` encodingと2 KiB/depth 16上限を使う。一般public deserializerではなくprivate record型でdecodeする。そのaudit hashは同じ検証済み履歴内に必要である。Auditを読んで検証し、sequenceがcheckpointのstate revision以下であることと、export済みreceipt全体がauditのresult/hashに一致することを要求する。Initialization・publish・yank・quarantine・owner・revocation・principal・provision・recoveryの全result variantを含む。結果置換・未知variant/schema・同revision fork・未commit候補・後続receiptを拒否する。成功時に情報用receiptを構築し、現在bearer認可やmutation replayを実行しない。

高コストの履歴走査前に有界caller入力を検証する。部分検証結果を返さない。既存lockとstorage/request検査を保持する。Errorでfloorをresetせず、履歴を削らず、HEADを変えない。Filesystem pathやcredential secretをexportしない。

## Execution modes

Host registry APIであり、source syntax・MIR・bytecode・native ABI・JIT・embedded callback・WebAssemblyは不変。統合fixtureでは復旧sequence後に同じimmutable archiveをreopenし、元sourceをVM/nativeで実行する。

## ABI and compatibility

Checkpointは独立versionを持つ新しい情報用formatである。既存HEAD・state・audit・receipt・HTTP schemaは変えない。未知checkpoint schemaは安全側に拒否し、migration・履歴書換え・downgrade bypassを設けない。Rust APIはexperimentalのまま。F01は明示的signed formatで別途保持証拠を認証できるが、本parserから認証を推論せず、採択済み契約なしに本checkpointをpackage trust receiptへ流用しない。

## Diagnostics and tooling

既存の構造化`REG` errorを使う。Encoding/schema/field不正はmalformed、byte/depth超過はlimit、identity/包含/result矛盾はintegrity、managed storage不正は既存state/I/O errorとする。Error文に入力source・path・credential・record dumpを含めない。Editor・formatter・LSP・DAP・CLI flowの変更は不要。

## Performance and cost model

既存の全履歴検証1回に加え、上限付きimmutable state読込1回、receiptでは2 KiB audit読込1回を要する。検証済み到達可能集合を再利用し、別の全世代vectorを集めず、checkpointとreceiptで全走査を重複しない。Checkpoint encoding/保持は最大1 KiB、receipt受理/encodingは最大2 KiB。独立した明示directory host APIは既存どおり非計上とする。別途監査したworkspace予約なしに`MeteredRegistry`やHTTPへ追加しない。Service envelope拡大・benchmark速度の主張・自動background作業はない。

## Security and trust boundaries

保持copyはcallerが選んだ比較anchorである。Parse成功や現在履歴の自己整合性は、提供者を認証しない。侵害operatorはdataを出さず、anchorを比較しないhostごとに分岐履歴を提示できる。分岐前の古いcheckpointではその分岐を検出できず、独立copyを失う/置換されると保護も失う。検証は指定anchorとの矛盾を検出し、完全性・freshness・全体非分岐・sourceの無害性は保証しない。Filesystem/network/process/guest capabilityを与えず、消費済みpackageを構築できない。

## Alternatives

- **変更なし:** R12の保持証拠比較が未実装で、export receiptも未検証のまま。
- **外部file検査tool:** private formatと全遷移検証を重複し、孤立/不正履歴を受け入れる危険がある。Storeの検証済み境界を再利用する。
- **署名付き公開transparency log:** 外部帰属を示す強い証拠となるが、独立trust・更新・witness・可用性policyが必要。F01–F04がsigned package trustを扱い、ここでlogを加えると選択結果を拡大する。
- **HEAD/revisionだけ比較:** 正しい子孫を拒否するか、偽造した同revision stateを受け入れる。厳密な保持state包含と完全receipt比較を最小の有用な関係とする。

## Implementation plan

E08公開後にE04 branchで有界host APIを実装し、HTTPS/local復旧/保持証拠/source実行の統合fixtureを加える。敵対anchor/receipt・無副作用境界、既存fuzz target拡張、固定baselineと最新headの両host CIを行い、E01–E08に対しG5を監査する。実装前にE04を1 → 3 ptへ再見積りし、deliveryは追加しない。E08公開までは同項目だけをdoingとする。

## Test plan

独立canonical vector、有効最大field幅とbyte/depth境界、重複/未知/null/非canonical field、全receipt variant、真正な旧/現在checkpoint、未来/fork/rollback/孤立/cross-origin、結果偽造と後続receipt、成功/拒否時storage不変、実service停止/local復旧/再開後のimmutable byte保持と両host VM/native実行を確認する。既存mutation crash/競合suiteを必須とし、本read-only APIに試験用の書込みを追加しない。決定的入力変異corpusと既存registry fuzz targetで新入力境界を検査する。外部policy modelだけでG5を閉じない。

## Open questions

実装を妨げる未解決契約はない。実source統合、最大有効record、敵対storage/入力検査、両host検証は未検証である。公開signed evidenceの認証・保持policyはF01作業であり、本RFCで解決または暗黙bypassしない。

## Sources

- [RFC 0045](0045-registry-threat-model.md)、最終更新2026-10-31: R12/R16とlocal hash chainの限界。
- [RFC 0046](0046-registry-protocol-and-recovery.md)、最終更新2026-10-31: 有界audit/recovery、E04/F01保持証拠要件。
- [RFC 0048](0048-registry-state-and-publication.md)、最終更新2026-10-31: 完全な到達可能履歴とreceipt encoding。
- [RFC 0055](0055-registry-local-administration.md)、最終更新2026-10-31: local lifecycleと復旧receipt variant。
- [RFC 0001](0001-project-charter.md#experimental-rfc-decisions)、最終更新2026-10-28、実験期委任日2026-10-17 JST。

全てproject decisionであり、2026-10-31 JSTにE08実装`16a8e80`と照合した。22-check実験は設計証拠で、本番/platformの主張ではない。本具体化にweb researchを要する外部実装の主張はない。
