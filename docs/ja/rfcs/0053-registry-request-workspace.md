<!-- translation-of: ../../rfcs/0053-registry-request-workspace.md -->

# RFC 0053: Registry要求作業memoryと可変read世代

[English](../../rfcs/0053-registry-request-workspace.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E06, M5-E05
- Supersedes: なし。RFC 0046・0048・0051を具体化
- Superseded by: なし

## 概要

認証HTTPS mutationを有効化する前に、有界host作業memory受理と置換可能な検証済みread世代を準備する。Metadataのみのpreflightで保守的なallocation上限を導出し、body decode・recovery・state複製前に共有budgetから予約する。実store経路でも観測した上限を再検査する。Mutationはcommit前に置換read世代を予約し、commit後の更新に失敗したら古いservice readを拒否する。

以下の提案係数は確立済みallocation上限ではない。型・allocation・lifetime監査の完了まで本RFCをproposedとする。30-case modelはpolicy/計算だけを示す。本提案による実装・gate完了の主張はない。

## 動機

現read-only transportはconnection/TLS/worker、保持captured index/map、送信archive bufferを計上する。一方、host mutationは最大262,144物理entryのinventoryをscanし、最大16 MiB recordをdecodeし、replay/履歴tableを保持し、stage世代生成前にstate/authorityを複製する。これらに要求budgetの上限適用がない。HTTP入力bodyだけの計上では覆えない。また`CapturedReads`はregistryと不変1世代を所有するため、所有dataへallocation chargeを結び付けたままmutation後に更新できない。

旧E05・3 pt見積りは、このstore前提と認証/framing/client統合をまとめていた。Guard付きhost作業memoryと世代captureをE06・3 pt、その後の実HTTPS journeyをE05・3 ptへ分ける。これは既存必須scopeの+3 pt再見積りであり、新機能や受入条件の緩和ではない。

## 目標

- 要求所有のstore payload/mapを確保する前に、保守的な論理peak作業memoryを予約する。
- 実read/decode/collection/encoding境界でpreflight上限を再検査する。
- 単一commit済みHEAD、現在authority、完全履歴、厳密retryを維持する。
- 旧/新read世代と応答のchargeを実lifetimeへ結び付ける。
- E06でguard付きhost公開/read/source実行fixture、その後E05で認証HTTPSへ到達する。

## 非目標

- 256 MiBの集約要求budget、8 connection上限、既存storage/archive/record上限の引上げ。
- Process全体のRSS、allocator/runtime内部管理、throughput、availabilityの証明。
- 保存plan、cached authority、遠隔管理をcapabilityにすること。
- Pruning、pagination、第2 authority file、署名、本番消費の追加。

## 利用者向け設計

既存read-only service entry pointを保持する。別の明示mutable service entry pointがprivate registry capabilityと設定host clockを受け取る。Archive-format上限内でも、同時body・保持read・保守的store作業memoryが同じ有限budgetを共有するため、既存limit/busy診断で要求を拒否できる。

Mutable initial profileは要求処理から応答完了まで非blocking排他service guardを使う。競合処理はapplication queueで待たずbusyを受け取る。処理中readはcapture世代を完了してからwriterがcommitできる。Concurrencyを単純な有界lifetimeと引き換えにする設計であり、throughputを主張しない。現read-only entry pointの挙動は維持する。

Commit済みmutationは、別の成功応答がservice状態を使う前に一致する検証済み世代をinstallする。更新失敗でcommitをrollbackせず、旧世代を返さない。Mutable serviceをrecovery-requiredとし、明示停止/reopen後に同じrequest identityをcommit済み履歴と照合する。Local credential管理は引き続きwriter解放を要し、HTTP admin routeを使わない。

## 参照semantics

### Metadata preflightとguard付き作業memory

既存private directory/writer capability下で固定namespaceの有界metadata-only scanを行う。物理entryを数え、認識済みstaging candidateも含めstate/object最大長を観測する。有界名、通常file/no-follow/link制約、既存種別上限を検証する。Payload decode、entry数に比例するmap確保、file cleanup、HEAD変更を行わない。Scan中にcancel/deadlineを検査する。小さな固定scratchはscan前に計上済みでなければならない。

結果の上限はprivate・一時的で、そのlock中requestに結び付く。Deserialize可能な権限ではない。CaptureしたHEADを再検査し、対応payload確保・map挿入・decodeの前にentry/size上限を再適用する。File増大やentry増加が上限を超えたら、その追加allocation前に拒否する。既存hash・canonical format・全遷移・cleanup検証は必須であり、metadataだけでrecord/sourceを受理しない。

Archive/action decodeと完全recoveryの前に、保守的な作業memoryとcandidate世代を同じ共有budgetから予約する。Guard付きhost sessionはleaseと上限適用が有効な間だけ既存transaction semanticsを呼び出せる。Standalone host APIは明示local capability semanticsを維持するが、network adapterに非計上fallbackを設けない。返却receipt/read dataは一時作業memory解放前に自身の保持chargeを取得する。

### 提案allocation上限

最大観測state recordを`S`、観測object長と受理upload長の最大を`A`、物理entry数を`E`とする。`C = min(16 MiB, S + growth)`とし、growth案は公開で`2 × 128 KiB + 4 KiB`、owner/yank actionで`4 KiB`とする。物理state上限も独立に適用する。予約案は次のとおり。

```text
workspace = 48 × C + 6 × A + 8192 × (E + 8) + 2 MiB
generation = 2 × C + (1024 + 8192) × 1024 + 4096
```

全計算をallocation前にchecked演算で行う。入力上限はstate 16 MiB、archive 18 MiB、物理entry 262,144を維持する。`E + 8`はcandidate metadataのallocation allowanceであり、物理entry quota超過の許可ではない。Body・既存read data・connection/TLS/worker chargeは別のlive leaseで、本modelから差し引かない。残り256 MiB budgetを超える予約は書込みなしで拒否する。

係数監査の責務: 同時に生存する隣接/履歴/期待stateのdecode結果、authority複製/authenticator table、両prepared publication buffer、canonical再encoding capacity、archive入力/frozen複製、有界descriptor/binding scratchを上限化する。Inventory/binding/reachable/staging/replay構造を物理entry数で上限化し、新index/map保持量をstate byteと既存package/release件数上限から導く。Collection capacity増加と固定実装の要求allocation sizeを含める。`generation`の固定collection上限2個は、小さいregistryに対して意図的に余分に予約する。妥当な既存の大storeが保守的service budgetで拒否されても、不正storeになるわけではない。

本RFCのaccept前に、実際の固定型・codec・collection挙動・lifetimeから係数を確立し、実行可能な境界検査を添える。監査できなければ実装前に式を改訂し、計算modelの成功や観測RSSを証明と扱わない。後続record/layout/clone lifetime変更でも、無条件に上限を維持せず再監査する。

### Read世代の所有とcommit

内部でregistry所有と検証済み不変read世代を分離し、必要なら既存read-only APIをwrapperとして維持する。世代は厳密HEAD identity、有界index、commit済み非quarantine objectの厳密identity/lengthを持ち、cached bearer/authority判断を持たない。Captureはguard付き作業memory下で完全履歴を検証する。保持index/mapの各allocation前に世代allowanceを逐次適用し、見積り誤り時にleaseを超えて確保せず拒否する。

Mutation前に、旧世代を計上したまま一時作業memoryと置換世代allowanceを予約する。既存の書込み前導出index上限、物理quota、最新event時刻、最終最新認可、単一HEAD置換を維持する。Host結果後に実commit世代をcapture/検証し、排他service guard下で置換する。旧dataはdrop時にのみ解放する。厳密retryは現在世代を保持して元結果を返し、古いsnapshotへ戻さない。

Commit後の更新失敗では結果/HEADを保持してserviceをrecovery-requiredにする。明示reopenで現在commit状態を検証するまで通常readと新mutationを拒否し、復旧後に同じrequest IDで照合できる。最大candidateを選ぶ、欠落HEADを再作成する、authority/time floorを解除することはない。Commit前拒否はdefinitely uncommitted、曖昧なstorage/transport結果はunknownを維持する。

### E05へ維持するHTTPS adapter境界

RFC 0046のHTTP receipt `{schema:1, registry, request_id, revision, audit_sequence, outcome}`をhost audit receiptと分ける。元package/state revisionと受理request identityを対応させ、公開は新commitで201、replayで200、actionは200を返す。Host receiptでwire schemaを置き換えない。POST action条件/IDはcanonical body、PUTは必須conditional/idempotency headerから取る。重複/適用外header、route/archive identity不一致を拒否する。

認証headerには明示bearer入力だけを使い、public GETは引き続き拒否する。所有secret/header bufferを退役時にclearし、有界static errorだけを出す。TLS/origin/framing/body/deadline上限とredirect禁止を維持する。最終store clock guardはHEAD置換前にcancel/deadlineも検査し、commit後の応答失敗はcommitを取り消さない。Client failureは送信前の確定拒否と送信後の公開可能性を分け、unknownを同一identity照合へ案内する。実framing・receipt・現在owner/失効・停止/失効/reopenはE05の責務である。

## 実行mode

上限はhost受理を変え、guest実行やauthorityを変えない。E06はguard経路でpackageを公開/captureし、厳密archiveをVM/nativeで実行する。E05は両hostの実証明書検証loopback HTTPSで同じ流れを実行する。Compiler、VM、JIT、native ABI、component semanticsの変更はない。

## ABIと互換性

Archive・state・audit・lock・wire schema番号は不変。既存standalone host/read-only service callerを維持する。新mutable entry pointは明示的で、保守的共有budgetの圧迫時に拒否できる。Ambient capabilityやnetwork管理endpointを追加しない。明示busy直列化は新entry pointのavailability上の選択であり、既存read-only APIの回帰ではない。

## 診断とtooling

Resource拒否はREG0002/413、busyはREG0008/429、その他は既存malformed/authentication/authorization/conflict/integrity/state/I/O分類を使う。Unknown結果をrollbackと報告しない。Source formatter、parser、LSP、debugger変更は不要である。Test-only counterはcredentialや任意payloadをlogせず、要求予約量とlease lifetimeを公開できる。

## 性能・cost model

Metadata preflightは有界な線形directory passを追加し、既存完全inventory/history検証も保持dataに線形のままである。保守的過剰予約と排他応答はthroughputを下げ、他のallocation戦略なら収まる処理も拒否し得る。未証明peak上限より、明示したinitial-profile上の選択を優先する。監査済み上限を実装してから実実行を測定し、純粋modelからRSS・latency・scalabilityを推論しない。

## Securityと信頼境界

全payloadに完全受理と現在認可を適用する。Metadata上限は後続allocationを制限するだけで、cleanup/mutationを認可しない。実size/countを再検査し、preflight後のraceによる予約迂回を防ぐ。旧/新世代の重複や応答複製も含め、最後の所有dataが退役するまでchargeを保持する。Hash-chain/HTTP receiptは未署名host証拠であり、F01–F04 trustとは別である。Host侵害・外部rollback・電源断availabilityは既存の残余riskを維持する。

## 代替案

- **変更なし/bodyのみ計上:** 要求所有のrecovery/state allocationがaccepted共有budget外に残る。
- **全profile最大値を予約:** 単純だが256 MiBを超え、全mutationが使えなくなり得る。
- **Metadata由来の全operation上限:** 選択した提案。実境界で適用できれば、全valueへallocation hookを通さず保守的に監査できる。
- **Allocationごとの追跡wrapper:** より厳密になり得るが、lease付きrecord/container型とcodec/lifetime変更が増える。全operation監査で有用な上限を確立できなければ再検討する。
- **並行不変世代lease:** Read concurrencyは改善するが、遅いreaderの旧snapshot保持と重複計上が増える。本明示直列profileを実証してから扱う。
- **Commit/rebuild前に旧readを破棄:** Memoryは単純になるが、確定非commit拒否で妥当な現在世代も失う。重複分を明示予約する。

## 実装計画

1. 提案係数とlifetime証明を監査してから、RFC 0001委任でacceptまたは改訂する。固定baseline/CI中はE03を唯一のdoingに維持する。
2. E06: 有界metadata preflight、guard付き作業memory適用、置換世代予約/capture、実guard付きhost実行fixtureを実装する。確保前上限・race・cancel・拒否時HEAD不変を検証し、選択時監査後にのみ3 ptを維持する。
3. E05: 明示mutable HTTPS entry pointとclient、厳密framing/credential/receipt、直列世代公開、recovery-required挙動、両hostの実行/失効journeyを統合する。この独立した結果に3 ptを維持する。
4. 必須local baselineと最新head CIを実行し、証拠を公開して残G5作業を監査する。E04はE05後を維持し、作業memory準備だけでgateを完了にしない。

準備branch `codex/m5-registry-authenticated-service`は`carimatics/lang`の`codex/m5-registry-owner-control`・固定`18b2128`から作成し、#40 mergeまでorigin/codex/m5-registry-owner-controlを記録PR baseとする。新checkoutはcleanで、work unit開始branchはmainを維持する。このbranchで実装は未開始である。

## Test計画

- 両hostで型構造/capacity/clone lifetime上限を監査し、最大妥当collectionと有界malformed recordを含める。
- 厳密予約と1 byte不足を、対応payload/map確保・staging前に拒否する。Checked計算・物理上限も含む。
- Preflight後の実metadata変更、過大/未知/nonregular entry、完全検証前のcleanup禁止。
- Guard付きhost publish/action/private read/capture、保持sourceのVM/native実行、完全履歴、元結果retry。
- 旧/新世代重複、破棄/error応答、commit後更新失敗、明示reopen、厳密照合。
- E05の実TLS framing/認証/条件/receipt、8 connection、遅いpeer、cancel/deadline、無制限queueを持たないこと。

## 未解決事項

**Needs prototype/measurement:** accept前に提案係数と適用可能なstore上限を確立する。現在の`/private/tmp/lang-m5-service-policy-probe.js`は有限connection受理、排他service access、現在権限replay、旧/新lease重複、commit前拒否、commit後更新失敗のrecovery-requiredを含む30 policy/計算caseに成功した。独立JavaScript/Python計算は公開の`S=1024`、`A=3072`、`E=6`について一致し、candidate 267,264 byte、workspace 15,058,944 byte、generation 9,975,808 byteである。実allocation上限、TLS、bearer、storage、Windows挙動の証拠ではない。

2026-10-31のsource監査は要求guardと所有分離の欠落を特定したが、性能測定ではない。E05の既存scopeを3 → 6 ptへ再見積りし、3+3へ分割してE06を明示する。このplanning訂正にuser判断は不要であり、acceptance待ちは技術証拠の不足によるもので、permission待ちではない。

## 出典

- [RFC 0046](0046-registry-protocol-and-recovery.md)、更新2026-10-31: 共有budget、HTTP receipt、deadline、capability。
- [RFC 0048](0048-registry-state-and-publication.md)、更新2026-10-31: 完全履歴、quota、単一HEAD結果。
- [RFC 0051](0051-registry-yank-transactions.md)、更新2026-10-31: 明示mutation serviceと旧/新read memory要件。
- [RFC 0052](0052-registry-owner-and-credential-control.md)、更新2026-10-31: 現在owner/失効semanticsとlocal停止/reopen管理。
- `18b2128`でのlocal source監査と30-case model、2026-10-31 JST確認。Project証拠のみで、外部allocation/性能の主張はない。
