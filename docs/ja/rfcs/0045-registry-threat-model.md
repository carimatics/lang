<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0045-registry-threat-model.md -->

# RFC 0045: Public registryの脅威モデル

[English](../../rfcs/0045-registry-threat-model.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-C01–C02 and G3 in the M5 delivery board
- Supersedes: none
- Superseded by: none

## Summary

Registryがuploadを受け入れる前にidentity・authority・攻撃者・security受入義務を定義する。Package releaseのidentityは、明示registry・package名・厳密version・不変content digestで定まる。Download・認証・所有のいずれも実行やhost capability利用の許可を与えない。

これはC01の脅威モデル判断である。C02の[RFC 0046](0046-registry-protocol-and-recovery.md)はupload実装前に具体的protocol format・資源上限・復旧規則・対応する拒否caseを別途採択した（2026-10-31 JST）。両判断でG3設計ゲートは完了。G4–G6・G9は動作するstorage・認証操作・署名付き消費・hosting／検索を引き続き要する。

## Motivation

RFC 0034は明示的に許可したlocal pathを解決する。RFC 0035は信頼済みのtarget・compiler固有graph snapshotのproducerであり、未信頼archive importerやportable registry releaseではない。RFC 0042は別の信頼領域でtoolchain配布を認証する。いずれかを暗黙のregistryとして使うと、公開namespaceのauthority・悪意あるpublisher・不変versionの競合・credential侵害後の復旧が欠落する。Libraryのchecksumだけでは解決しない。

## Goals

- 公開・依存解決・cache・lock再現・実行を通して厳密なpackage／source identityを維持する。
- 敵対的な未認証requestと認証uploadを、allocation・展開・永続変更より前に制限する。
- Namespace所有・操作認可・byte完全性・provenance主張・実行authorityを分ける。
- Publish・yank・owner操作と復旧を、両tier-1 hostで観測・帰属・検証可能にする。

## Non-goals

このRFCはwire encodingや暗号envelopeを選定せず、server実装・公式production registry／keyの確立・世界的availability保証・publisher codeの安全認定も行わない。Version range・暗黙registry探索・build script・package install hook・native plugin sandboxは追加しない。Private packageの機密性と透過的なregistry間mirrorは初期public profile外。Malware moderation・名前紛争・account審査は、下記の制限付き管理操作を伴うoperator責務として残る。

## Guide-level design

Consumerはregistry authorityを明示する。Registry Aの`example_math` version `1.2.3`とregistry Bの同名・同versionは、bytesが同じでも別依存である。Local aliasはsource言語のbindingにすぎずidentityを置換できない。Lockは解決済みorigin・名前・厳密version・artifact digestを固定する。Package欠落・拒否・timeout・署名失敗時に他registryへfallbackしない。

Publisherは送信対象fileと依存metadataを正確にpreviewし、1 releaseを明示公開する。同じtransactionの成功retryは同じidentityを持ち、別bodyでそのversionを置換できない。Yankは新規選択から除外するが、不変identity・provenance・既存厳密lockの意味を消さない。Owner移管は将来のauthorityを変え、旧releaseの著者帰属を変えない。Security quarantineは別途監査する将来serviceの拒否であり、暗黙rewriteや別bytesへの誘導ではない。

Errorは不正入力・上限超過・認証・認可・version競合・stale state・署名／freshness失敗・復旧必須状態を区別する。Credential・private filesystem内容・bearer tokenを開示しない。認証失敗からtoken identifierの存在を識別できてはならない。

## Reference semantics

### Identityとnamespace

- RFC 0034のpackage名・alias文法`[a-z][a-z0-9_]{0,63}`、3成分の厳密な10進u32 version、edition `2026`を維持する。異なるversionは別の不変release。Case-foldやUnicode正規化による別名identityは作らない。
- Registry論理identityを、明示設定transport originと独立に与えた信頼方針へ結び付ける。Downloadしたpackage内のURL・key・issuer・論理名で置換できない。C02で正規encoding、F01で認証root継承を定める。Origin移転でconsumerのrollback履歴をresetできない。
- Package名は1 registry内の1 namespace recordに属する。初回claimは認証・認可付きatomic操作で、通常uploadは既存名を奪えない。Owner削除・yank・quarantine後も名前を再利用しない。Flat名ではsquatting・紛らわしい名前が残余riskとなるため、UIはregistryと厳密名を示す。
- Principalは安定したserver identityを持つ。Package単位のowner membershipは明示的で、表示名はauthorityではない。各変更transactionの直列化区間で、現在membership・token scope・audience・期限・失効を再検査する。最後のactive ownerを削除できない。移管は旧ownerを退役させる前に後継authorityを確立する。Owner credentialに暗黙admin権限はない。

### Authorityとdata flow

| 境界 | 必須authorityと検証 |
| --- | --- |
| Local workspace → publish client | 明示directory capabilityと固定選択inventory。Previewはnetwork／write許可を与えない。Ambient credential・output・未宣言fileを除く。 |
| Client → registry | 明示origin・操作、証明書検証HTTPS、redirect・content由来URLなし。Public readはpublish credentialを送らない。Local test transportは明示的な分離fixtureに限る。 |
| Credential → 操作 | 1 registry audience・principal・有限寿命・指定action／package scopeに限定。Read／publishからowner／adminへのwildcard昇格なし。秘密は明示的な保護入力経路を使い、argv・log・archive・lockへ入れない。 |
| Upload → storage／index | Sourceを実行しない厳密な上限付きmetadata／archive受入。Private staging、不変identityとaudit recordのatomic commit。 |
| Index／object → consumer | Parse前にbytesを制限し、公開・使用前にorigin・要求identity・digest・署名・freshness方針を検証。Cache objectも再検査しauthorityとしない。 |
| Package → compiler／runtime | Sourceを未信頼のまま扱い、span・既存frontend／IR検証・資源上限を維持。Importはfilesystem／network／process／FFI権限を与えない。 |
| Documentation／search → browser／user | 描画source・名前・説明・URL・query textは敵対入力。出力をencodeしactive contentを拒否、文書service authorityを公開credentialから分離。 |

認証・membership・scope付き認可は別のcheckである。暗号digestはbytesのidentityでありpublisherを認証しない。有効なrelease署名が確立するのは信頼issuerに許可した主張だけで、安全性や監査済みbuildを証明しない。Source package provenanceは実際に観測した事項を示し、publisherの自己申告と独立検証builder証拠を区別する。この判断でSLSA levelを主張しない。

Registry・consumerの永続stateは明示directory capabilityを使い、metadata内host pathを再openしない。Path alias・symlink・hardlink・device・case衝突・file／directory prefixのcheckを該当open／公開境界に適用する。C02は安全性を汎用TAR展開へ委ねず、portable archive profile・全体課金・crash復旧を定める。

Registry操作は変更境界で直列化する。並行owner削除より前の古い認可checkで後続commitを許可しない。成功応答にはversion／owner／yank変更とaudit eventの同時commitが必要。復旧は不正・不整合stateを拒否し、rollback floor・不変identityを維持し、staging残骸から成功を捏造しない。C02でprocess crashと電源断の保証、retry照合を明示する。

Sourceの評価順・型・ownership・destruction・atomics・cancellation・定数実行は変更しない。Transfer cancelは有限request resourceを解放するが、commit済みversionを取り消せず、応答欠落が未commitを意味することもない。

## Execution modes

受入時にguest codeやbuild hookを実行しない。受入sourceは既存の共通frontend／typed MIR経由でVM・native実行する。実依存fixtureは両modeで一致する必要がある。Native実行は信頼host実行のままで、署名でsandboxにはならない。Wasm plugin実行は別途G7／G8の検証component・ambient authority拒否・runtime上限を要する。Registry由来guestという理由でtokenをguest host-call contextへ渡さない。

## ABI and compatibility

影響領域は`package`・`tooling`。Source構文・native／component ABI・MIR・bytecodeは変更しない。既存manifest／lock／archive schemaは採択契約どおり読取り可能に保ち、local pathやRFC 0035 snapshotをregistry identityへ再解釈しない。C02で新metadataをversion化し移行・厳密lock／offline挙動を定める。未知schema・algorithm・identity形式は拒否する。RFC 0042のroot・key・署名・rollback stateはpackage releaseを暗黙認可しない。

## Diagnostics and tooling

Client reportは選択registry・package・version・digest・操作結果と、該当時は上限付き診断位置を示す。Materialize後も元source path／spanを保ち、合成source rewriteではなく別origin labelで区別する。保存preview／reportにwrite authorityはない。公開時にeditor callback・package metadata由来credential-provider command・formatter・任意subprocessを実行しない。C02で安定診断identityとresponse形状、I01で安全なhosting／search出力方針を定める。Parser・formatter・LSP・debuggerのsource意味は不変。

## Performance and cost model

Archive bytes・decode metadata・entry・path・graph深さ／edge・cache／staging size・request並行数・token検証・署名work・owner／audit record・search workに有限上限が必要。宣言長と実観測長をchecked算術で検査し、展開後file数だけを課金してはならない。成功と拒否の両方を制限する。監査storage保持と満杯時の挙動はC02の責務で、無制限logはDoS対策ではない。測定なしにlatency・throughput・footprint・使いやすさの改善を主張しない。

## Security and trust boundaries

信頼基盤はconsumer binary／OS、明示root／設定、local capability directory、およびowner操作に関するregistry認可／transaction実装。Registry credential管理とpackage署名の信頼は別role。Network peer・mirror／cache・任意未認証client・認証済み悪意publisher・依存content・文書・検索入力は未信頼。Credential盗難／失効、online署名key侵害、悪意storage応答、process crash、並行する正常requestを想定する。

| ID | 攻撃と資産 | 必須制御と観測可能な拒否 | Delivery |
| --- | --- | --- | --- |
| R01 | Dependency confusion・package origin置換 | 全edge／lock／cache identityにoriginを明示。他origin同名で要求を満たさない | C02,D03–D05 |
| R02 | Namespace乗取・owner競合・最後のowner喪失 | Atomic name claim、現在のscope付きmembership検査、条件付きowner変更、所有者を空にしない | E01–E04 |
| R03 | 盗難・期限切れ・失効・異audience token | 全変更でfail closed。有限secret取扱いとreport秘匿。準備済みrequestでも失効を迂回できない | E01–E04 |
| R04 | 不正archive・traversal・link・device・platform alias | Materialize前に上限付き論理inventoryを検証。Capability root外へ出ず既存fileを上書きしない | D01,D05 |
| R05 | 巨大・終わらない・深い入力、署名／query flood | 副作用前にbyte／work／depth／並行数／storage上限を強制し、cancel／deadline時cleanup | C02,D01,E01,I03 |
| R06 | 同version競合・publish／audit不整合 | 不変commit点は1つ、winnerは1件。Retryは同じidentityを照合し変更を重複させない | D02,D05,E04 |
| R07 | Cache／storage改竄・indexとpayload不一致 | 要求identity・認証manifest・bytesを結合。部分／未検証generationは消費不可 | D03,F02–F04 |
| R08 | Rollback・freeze・mix-and-match・期限切れ・時計後退 | Version／time floor保持、freshness検査、record間厳密結合。Offline方針で信頼stateを暗黙消去しない | C02,F01–F04 |
| R09 | Online key侵害・不正root置換 | 独立初期信頼、domain分離、上限付き認証継承と失効。自己申告new rootを拒否 | F01–F04 |
| R10 | 悪意source／build／docに対する正当署名 | 受入中に実行せず、暗黙host grantなし。有限compiler／runtimeと安全な文書service | D01,G7/G8,I01–I03 |
| R11 | 公開中crash・disk-full・応答欠落 | Commit state／auditを一緒に復旧。認識済みstagingのみcleanup、旧version保持、結果不明操作を照合 | C02,D05,E04 |
| R12 | Audit偽造・secret流出・履歴消去 | Secretなしの帰属可能な有限永続record。検知した破損は拒否。Operator証拠には外部保持checkpoint | E04,F01 |
| R13 | SSRF・credential転送・敵対origin redirect | 設定HTTPS originのみ、redirect・metadata URLなし。Public readにcredentialなし、network capability明示 | C02,D03,E01 |
| R14 | Workspace漏洩・path依存公開 | 固定受入fileのみpreview。Registry identityへの明示変換なしにprivate local依存pathを公開できない | C02,D01,E02 |
| R15 | 保存／反射script・query injection | 無害化したversion結合文書、text encode、任意code評価なし、有限構造化search | I01–I03 |
| R16 | Admin侵害・悪意operator | 明示的な別admin復旧／quarantine authorityとaudit。暗黙byte置換なし、consumerは独立identity／信頼state保持 | C02,E04,F01–F04 |

残余riskを明示する。正当に認可されたpublisherも悪意ある新codeを公開できる。失効は実行済みcodeを取り消せずregistry外のcopyも無効化できない。Yankは消去や普遍的malware停止ではない。完全な署名thresholdまたは信頼consumer hostの侵害は、そのauthority内の保証を破る。悪意registry operatorはserviceを停止し、可変ownershipを偽り、外部未保存audit履歴を消せる。Local hash chainだけでは防げない。独立保持receipt／checkpointは一部の矛盾を露出できるが、完全性・全世界での非equivocationを証明しない。Traffic枯渇・operator容量はavailabilityへ影響し、request単位の有限workはpublic serviceのSLAではない。

## Alternatives

- **変更なし:** Local path依存はM4境界を保つが、M5 public registryゲートを満たさない。
- **外部library／tool:** 適切なら検証されたHTTP・hash・署名・archive primitiveを再利用する。それ自体はpackage identity・owner authority・atomic release／audit・実行capability方針を決めない。暗黙に設計を取り込まない。
- **Cargo型registry ecosystem:** Cargoは名前付きalternate registryとcredential providerを持ち、単純file serverより強い実用比較対象。Protocol全採用は異なるpackage／manifest／解決ecosystemも持ち込む。明示origin identityと保護credentialを維持しつつ本言語のversioned profileを定める。Cargoが本RFCの脅威保証を提供すると主張しない。
- **完全なTUF repository model:** Role分離・metadata結合・threshold・更新手順は成熟した強力な配布設計。F01で全modelと限定package署名profileを比較する。本脅威モデルは攻撃への対応義務を採用し、未実装TUF適合を主張しない。
- **単純なHTTPSとchecksum:** 偶発的破損検知とtransport認証はできるが、独立publisher authority・侵害storage耐性・署名metadata replay対策・version／audit transactionを満たさず、G5／G6には不十分。

## Implementation plan

1. C01で本脅威モデルを採択。C02でR01–R16を具体的protocol／上限／復旧と実行可能受入caseへ対応付け、採択までupload実装を待つ。
2. D01–D05で有限archive受入・不変storage／index／download・実依存実行・厳密offline再現を構築。E01–E04で同じstate遷移規則に沿ったscope認証・公開／所有権・永続auditを実装。
3. F01でpackage root／provenance／freshness formatを採択した後、F02–F04で署名消費とkey復旧を実装。分離した未署名開発fixtureはproduction security profileでもG6完了でもない。
4. I01–I03で安全な文書hosting／検索を実装。G7／G8は独立plugin sandboxゲートを維持。Z01／Z02で両host連続journeyと最終監査／guideを要する。

## Test plan

各R IDについて敵対入力・拒否境界・state／副作用観測を保持する。意味がある上限は単なる拒否例だけでなくlimit − 1／limit／limit + 1を検証。同名2 origin、owner競合中のtoken scope／期限／失効、1 versionへの異なる2 payload、各commit境界での中断／retry、storage改竄と署名rollback／期限／root変更を実行する。拒否時のguest非実行・credential非漏洩・許可staging外への非書込みを確認する。macOS・Windowsのpath／case／link挙動、厳密offline identity再現、VM・nativeで同じ値を返す実際の推移依存を含める。Archive／metadata parserは不正入力corpus／fuzzing、資源上限保証は観測可能counterを必要とする。後続の署名・hosting・sandbox判断は義務を削らずfixtureを具体化する。

## Open questions

2026-10-31 JST、RFC 0001の実験段階委任により採択。根拠: 明示identityとcapability境界、不変release、現在stateでの認可、test対応攻撃者により既存compiler契約を守り、新public入力境界をレビュー可能にする。C01を妨げる未決事項はない。C02のencoding・有限上限・永続transaction profile・復旧case、F01のkey／freshness／provenance方式は未実装判断であり暗黙挙動ではない。配置quota・moderator体制・実key保管・incident responseは未検証の運用前提として残る。採択はsecurity・実装・安定化の証明ではない。

## Sources

- [RFC 0001](0001-project-charter.md)、2026-10-28更新。[RFC 0034](0034-package-workspaces-and-cli-diagnostics.md)・[RFC 0035](0035-source-package-snapshots.md)、2026-10-29更新。[RFC 0042](0042-signed-toolchain-installation.md)・[RFC 0043](0043-editions-and-compatibility.md)、2026-10-30更新。2026-10-31 JST確認。Project契約であり外部security認証ではない。
- [The Update Framework specification 1.0.37](https://theupdateframework.github.io/specification/latest/)、最終更新2026-10-08、2026-10-31 JST確認。Rollback／freeze／mix-and-match、有限更新、独立信頼root、認証rotation脅威の出典。
- [SLSA 1.2 threats overview](https://slsa.dev/spec/v1.2/threats-overview)・[provenance](https://slsa.dev/spec/v1.2/provenance)、公開／更新日記載なし。2026-10-31 JST確認。Provenanceは出所の証拠を記述し、脅威モデルは悪意producerを直接防ぐとはしていない。
- [RFC 9700, OAuth 2.0 Security Best Current Practice](https://www.rfc-editor.org/rfc/rfc9700.html)、2025-01公開、2026-10-31 JST確認。2.3・4.10.2節は最小権限／audience検査の根拠。本projectでOAuthを選定する意味ではない。
- [Cargo registries](https://doc.rust-lang.org/cargo/reference/registries.html)・[registry authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html)、公開／更新日記載なし。2026-10-31 JST確認。名前付きalternate registryと設定可能credential providerは出典の事実で、authority／transaction方針は本projectの判断。
