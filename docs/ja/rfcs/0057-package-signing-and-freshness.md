<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0057-package-signing-and-freshness.md -->

# RFC 0057: Package署名root・provenance・freshness

[English](../../rfcs/0057-package-signing-and-freshness.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-F01–F04
- Supersedes: none; RFC 0045・0046・0049・0050・0056を具体化
- Superseded by: none

## Summary

独立設定したregistry署名root、認証付きroot更新、既存package indexへの署名binding、正確に範囲を限定した公開provenance、永続freshness/失効検査を経て、本番`ConsumedArchive`証拠を構築する方式を提案する。署名から未署名fallbackやcapability付与は導かれない。

本稿は準備草案で、採択済み契約や実装済みtrust pathではない。既存の固定Ed25519実装で、独立した二重threshold/domain分離実験13件が成功した。純粋なfreshness/yank policyと最大field encoding modelの22 checkも成功し、提案する公開観測128件はpayload 54,463 byte・envelope 60,807 byteとなる。これらは本番storage・transport・source統合・platform動作を検証しない。採択/実装前に以下の未決定事項を解決する。

## Motivation

RFC 0049は本番consumed-archive constructorを意図的にsealedのままにしている。RFC 0050のimmutable byte cacheは認証済み消費を許可できない。RFC 0042のtoolchain installerはimmutable bootstrap rootを持ち、自動更新を明示的に先送りしている。そのpolicyをそのまま流用するとregistryの鍵更新/失効要件を満たせない。Registry owner権限・transport TLS・byte hash・provenance主張・署名trustを分離する。

## Goals

- 独立提供bootstrap policyをregistry/origin 1組へ結合し、認証付き鍵更新でもidentityを保持する。
- Source消費前に署名metadata・厳密index byte・immutable archive/descriptor identity・観測公開recordを結合する。
- 障害やoffline replayを跨ぎ、root/metadata/time floorと観測済み失効/隔離情報の単調性を保つ。

## Non-goals

- TUF wire互換・SLSA level・独立検証build provenance・source無害性・即時の全体失効の主張。
- 暗黙trust discovery・署名metadata中URL・floorをresetするorigin移動・packageによる鍵選択・未署名CLI/cache bypass。
- Provenance/署名によるfilesystem・network・process・FFI・guest capability付与。

## Guide-level design

Trusted hostはregistryと独立した初期root byte、明示service identity、privateな永続trust-state capability、別package-cache capability、clock、明示transport accessを与える。Bindingのroot digestは元policyを示し、後継rootでidentityを新設したり履歴を消したりしない。期限切れrootには認証付き更新が必要でも、過去に信頼されていたことだけでrelease消費を許可しない。

別host producerが完全検証したregistry世代を読み、厳密indexと観測公開事実をcaptureして、明示提供したin-memory keyで署名する。通常のpublish/yank/owner HTTP handlerは署名秘密を受け取らない。Consumerはpackage payloadを開く前に現在trustと署名metadataを検証し、sealed source証拠取得前に厳密byteと最新の受入条件を再確認する。Offlineではnetwork/credential lookupを行わず、保持した認証証拠と厳密cache byteを必要とする。

## Reference semantics

以下は候補profileであり、未解決事項を後述する。

### Root and signature boundary

閉じたversion付き有界recordと、弱鍵拒否を含むEd25519 strict verificationを使う。Root payloadはformat/schema・registry・origin・正version・issued/expires・公開鍵・明示thresholdを持つ独立root/release roleを結合する。各roleは最大8つの異なる鍵を持ち、key IDは厳密public byteのSHA-256とする。Root/release roleを非共有とする。候補rootは連続した次versionで、旧/新root role双方の異なる認可鍵によるthresholdを満たす場合だけ成功する。後継root role間で共有するsignerは各roleに1回ずつ寄与できるが、重複signer recordでthresholdを増やさない。適用するどちらのroot roleにも認可されない署名を拒否する。

Envelope候補は`{schema, payload, signatures}`とし、payloadは署名する厳密UTF-8 string、signaturesはkey IDと署名を含む。Domain prefix `LANGREGROOT1\0`と`LANGREGRELEASE1\0`でroot/release署名相互とtoolchain profileを分離する。署名前のbyteを厳密検証し、検証前に正規化しない。閉じた受理後に既存canonical pretty JSON+LFを要求する。初期root policy identityはenvelope署名順序と独立してcanonical初期payloadをhashする案とし、採択前に確定する。

上限候補はroot payload 32 KiB/envelope 64 KiB、後継root署名最大16件、release payload 128 KiB/envelope 1 MiB、release署名最大8件である。Integer/timeは有界unsigned versionとRFC 0047の整数Unix秒を再利用する。Root chain件数・期限切れ中間root・永続進捗境界は未決定。

### Signed index binding and provenance

Packageごとのsnapshotにformat/schema・registry/origin/name・root version・単調snapshot sequence・issued/expires・厳密index SHA-256/長さ・完全なRFC 0056 checkpoint・versionをkeyとする最大128件の公開観測を含めて署名する。既存indexは元の有界canonical artifactを維持し、最大2 MiBのJSONを別の署名JSON stringへ埋め込まない。独立最大幅実験で、短いpackage名やtimestampに依存せず候補payload上限へ収まることを確認した。

観測はversion・principal・非秘密credential ID・request ID・公開audit hash/sequence・受入時刻を含む。Producerは実際の完全検証済みpublish履歴から派生し、indexのimmutable release identityに結合する。主張はregistryが観測した認証付き公開と表現する。人間の作者・外部source repository・build platform・再現可能buildを認証しない。Issuerの署名は、独立rootが認可した主張だけを認証する。

### Freshness, replay and consumption

新規受理には利用可能な現在root、正しいregistry/origin/name、認可署名threshold、厳密index/descriptor/archive binding、issued <= now < expires、保持host-time/metadata floor以上を要求する。Release lifetime候補は最大7日でroot期限を超えない。低いsequenceは拒否し、同sequenceは同一署名payload byteを要求する。Root置換でもpackage floorと観測済み権限変更を保持する。永続受理/source解放前の再検査にcaller指定timestampを使わない。

Payload I/O前に認証済みmetadata/root進捗を永続化する。Payload download失敗でfloorだけ上がることはあってもsource消費を許可しない。Cache byteだけでは不十分である。厳密digestの指定だけでもyank releaseのreplayを許可せず、以前の認証済み受理と一致する永続recordを要求する。新しく観測したquarantineはreplayを拒否し、新規選択はyank/quarantineを除外する。失効はconsumerが知った情報にだけ作用し、expiryは更新を隠された際の利用可能期間を制限する。古い署名/offline証拠を現在rootでどう扱うかは採択前に決める。

Trust stateは書換え可能なbyte cacheと区別する。Commit済みtrust recordの改竄/欠落は安全側に拒否し、再openで破損上にbootstrapを作り直したりfloorをresetしたりしない。検証成功を`ConsumedArchive`唯一の本番constructorとする。既存の隔離された未署名constructorはtest専用のまま。Package manifest・lock・receipt・download metadataはtransport/filesystem権限を与えない。

## Execution modes

Source semanticsとVM/native/bytecode/component ABIは変えない。F03/F04は実署名依存を既存graph経由で消費し、macOS/WindowsのVM/nativeで実行する。厳密offline replayと敵対trust拒否を含む。検証でguest codeやguest providerを実行しない。

## ABI and compatibility

Index・archive・schema-3 lock・toolchain-rootの意味を暗黙変更せず、独立versionのtrust formatを追加する。既存`Binding` policy IDはregistry・origin・root digestを含み、後継rootでも初期policy digestを維持する。未知schemaは安全側に拒否する。Cache/trust-store formatの具体化は実装前に採択し、migration/復旧への影響を明示する。自動origin変更/trust reset migrationは提案しない。

## Diagnostics and tooling

構造化した有界trust診断を使い、鍵や任意の署名record dumpを含めない。証拠不在・認証失敗・期限切れ/失効trust・rollback/conflict・容量・永続公開結果不明を区別する。本番command/API設定で独立trust・state/cache・network入力を明示する。具体的CLI統合は未決定。既存local-only commandの動作は保持する。

## Performance and cost model

作業前にroot-chain step・鍵・署名・metadata byte・version・record・保持物理storageを制限する。Index/archive payloadより先に小さい認証metadataを検証する。厳密保持byteと既存graph集約上限を再利用する。Serviceでは最大envelope 1,024個を予約なしに保持せず、署名metadata map/request bufferを既存budgetへ計上する。Verifierは不正decodeとroot-chainの重なりも計上する。Encoding実験はrecord sizeの実現可能性のみを示し、割当て・遅延・永続storage費用は実装証拠を必要とする。

## Security and trust boundaries

候補controlをRFC 0045 R07–R09・R12・R16と比較する。Release threshold侵害時、consumerが失効を知るかtrustが期限切れになるまで悪意contentへ署名できる。Root threshold侵害はそのtrust domainを破る。Local trusted-state writer、不正な初期trust/clockは署名の保護外だが、未信頼cacheは保護対象である。保持checkpointは選んだanchorとの矛盾を示せるが、署名metadataと同様に全体完全性/非分岐を証明しない。正直なprovenanceでも悪意sourceを記述でき、署名は実行権限を与えない。

## Alternatives

- **変更なし / HTTPSとhash:** 独立issuer trust・認証付き更新・署名rollback/freshnessが未解決のまま。
- **Toolchain verifierをそのまま再利用:** 検証済みprimitiveを再利用できるが、immutable-root policyはregistry更新要件を満たさない。適するprimitiveだけを共有し、installer policyをregistry policyと扱わない。
- **完全なTUF:** root・targets・snapshot・timestamp roleを分け、metadata version/hashを結合し、認証root連続性とrollback/freeze防御を規定する。最も強い成熟した代替案である。狭いpackage snapshot profileはroleと実装面積を減らすがrole分離を失い、TUFの安全性/互換性主張は引き継がない。採択時には等価性の主張ではなく、要件を明示比較する。
- **公開transparency / keyless build attestation:** 別の脅威仮定には有用だが、witness/issuer/builder trustと正確に限定したprovenanceが必要。明示package identity・現在失効・永続consumer floorの代わりにはならない。

## Implementation plan

F01で本草案の未決定事項を解決し、独立vectorを記録して契約を採択する。F02は明示producerと検証可能な公開主張を実装する。F03は有界検証・永続trust/証拠保持・sealed本番source adapterを実装する。F04は更新/失効/改竄と署名付き両host journeyを検査する。各結果を選択する前に再監査し、1–3 ptを超える作業は現在見積りに隠さず公開する。本準備中、実装deliveryのdoingはE08だけとし、ここでgate/deliveryを完了しない。

## Test plan

公開RFC 8032 vectorとfixture専用鍵、異なる/重複/未知/弱いsigner、旧/新threshold組合せ、domain置換・非canonical署名を検査する。Root連続性、期限切れ中間root、失効旧鍵、異なるorigin/name、同sequence置換、clock regression、expiry境界、署名index不一致、descriptor/archive改竄、公開観測偽造、cache破損、既知quarantine、yank replayの以前受理要件を試す。全floor/receipt commit境界で実storage/process中断を注入し、証明前のpayload/source実行を拒否する。実装には全baselineと最新headの両host CIを要求し、model成功だけで実装要件を満たさない。

## Open questions

- 完全TUFと狭い候補の選択を、脅威要件比較と明示した侵害復旧限界で決める。
- Root-chain step上限・期限切れroot更新・rootのみの進捗APIを決め、長期間offlineからの復旧でfloorを暗黙resetせず作業を無制限にしない。
- 署名artifact配置・公開/提供atomicity・mutable indexとの同期を、新しく観測したquarantineも含めて決める。
- 永続trust-stateとcache証拠の所有、以前受理の現在root検証、lock順序、上限、crash semanticsを決める。
- 本番source/CLI統合と厳密初期root-policy digestを定め、未署名互換escape hatchを許可しない。

## Sources

- [TUF specification 1.0.37](https://theupdateframework.github.io/specification/latest/)、最終更新2026-10-08、2026-10-31 JST確認。Role、認証付き旧/新root threshold、metadata hash/version結合、rollback/freeze要件。
- [SLSA v1.2 build provenance](https://slsa.dev/spec/v1.2/build-provenance)、approved specification、page公開/更新日不明、2026-10-31 JST確認。Builder trustとprovenance主張の範囲。本稿はSLSA levelを主張しない。
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032)、2017-01公開、2026-10-31 JST確認。Ed25519と公開vector。
- [ed25519-dalek 3.0.0](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/struct.VerifyingKey.html)、2026-09-21 release、個別page更新日不明、2026-10-31 JST確認。既に固定済み、strict verificationと弱鍵拒否。
- [RFC 0042](0042-signed-toolchain-installation.md)、最終更新2026-10-30。[RFC 0045](0045-registry-threat-model.md)、[RFC 0049](0049-registry-package-integration.md)、[RFC 0050](0050-registry-cache-and-offline-replay.md)、[RFC 0056](0056-registry-retained-checkpoints.md)、最終更新2026-10-31。既存project契約であり、外部security認証ではない。
