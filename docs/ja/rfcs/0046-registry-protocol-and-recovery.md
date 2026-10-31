<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0046-registry-protocol-and-recovery.md -->

# RFC 0046: Registry protocol・受入・復旧

[English](../../rfcs/0046-registry-protocol-and-recovery.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-C02, D01–D05, E01–E04
- Supersedes: none
- Superseded by: none

## Summary

Portable source archive・不変公開・厳密origin download・scope付き変更authority・process crash復旧のための有限registry profileを定める。RFC 0045の脅威モデルを、upload前にwire／state規則と観測可能security caseへ対応付ける。Package署名／root継承は別途F01契約を必要とし、未署名開発fixtureを認証済みproduction consumer profileとはしない。

## Motivation

現source snapshotはtarget／compiler固有のlocal graph全体を含む。Registry releaseには単一portable package・宣言済み厳密依存identity・敵対入力reader・中断後も公開／所有／auditが一致するtransactionが必要。任意TAR展開・可変index file・checksumのみのdownloadではこの境界を満たさない。

## Goals

- Filesystem accessやguest実行なしに有限canonical source archiveを受入れ、別directory capabilityでmaterializeする。
- Requestからreplayまでorigin・package・version・digestを明示する。
- 不変release identity・現在の認可・auditを単一の可視state遷移としてcommitする。
- Request・parse・storage・復旧を制限し、stateを暗黙resetせず失敗を保持する。

## Non-goals

圧縮・任意TAR互換・package script・version range・暗黙credential探索・任意download URL・HTTP redirect・分散writer・public account signup・production key準備・電源断時availability保証。本RFCはTUF／OAuth／SLSA適合を主張しない。認証package消費の前にF01で署名／provenance／freshness format、hosting前にI01で文書／検索出力規則を採択する。Manifest／lock統合には下記の明示的な実装前具体化境界を適用する。

## Guide-level design

最初の観測可能journeyは`answer` version `0.1.0`を公開し、設定registryから厳密digestを明示downloadし、その依存を使うsourceをVM・nativeで同じ値へ実行する。同package／versionに別bytesを公開すると競合。Local packageからの公開は派生portable descriptorと厳密source inventoryをpreviewし、private path依存を暗黙にpublic依存へ変換しない。保存previewは公開authorityではない。

Client操作はregistry設定とcache directory capabilityを明示する。Publish・owner／yank操作ではさらに上限付きstdin等の保護された明示入力からtokenを受け、command argumentやpackage指定credential providerは使わない。Serverは設定originのみを受入れ、test／local directory transportは明示注入に限り、危険なpublic client fallbackを有効にできない。

## Reference semantics

### 共通encodingとidentity

全metadataはclosed object・重複fieldなし・有限nesting・指定unsigned整数範囲のUTF-8 JSON。Hashは小文字64文字SHA-256 hex、request／principal IDは小文字32文字hex。未知schema／format、未知field、整数fieldのfloat、末尾非whitespaceを拒否する。生成canonical JSONは本書のfield順・2 space indent・末尾LF 1つを使い、順序付きarrayは重複なし。Hash対象stored descriptor／stateのcanonical byte要件は個別に明示し、通常request JSONの空白まではcanonicalでなくてよい。

Registry設定は`schema: 1`・`registry`・`origin`・`trust_root_sha256`を結合する。Registry名は`[a-z][a-z0-9_]{0,63}`。Originはpath・userinfo・query・fragmentなしのcanonical `https://HOST[:PORT]`。HOSTは小文字ASCII DNS名またはcanonical IPv4 addressで、Unicode／IDNA表記・末尾dot・曖昧な数値形式・IPv6をprofile 1では拒否する。PORTはleading zeroなし10進1–65535、443は省略。Redirectやdownload由来の別originなし。同registry名へ2 originを結び付けたり、rootを暗黙置換したりできない。RootはF01に従い独立取得し、設定digest自体は信頼を確立しない。

Package／alias名・厳密version・edition・許可する2 target tripleはRFC 0034を維持する。依存edgeはfield順`{alias, registry, name, version, archive_sha256, targets}`。Targetsは`aarch64-apple-darwin`・`x86_64-pc-windows-msvc`のsort済み非empty subsetで、全対応targetなら両方を含む。Edgeはalias順。Aliasは直接依存のみをbindする。同じ`(registry, name, version)`に異なる2 archive digestがあるとdiamond経由でも必ず競合。Consumerは参照registryをすべて明示設定し、packageからnetwork authorityを追加できない。

### Portable source archive profile 1

非圧縮POSIX USTARを使い、`.lang-package.json` 1つを先頭とし、descriptorのsource fileをASCII path昇順で続ける。Descriptor field順は`format: "lang-registry-source"`・`schema: 1`・`package`・`tests`・`dependencies`・`files`。Packageは`name`・`version`・`edition`・`entry`。Testsは宣言順、dependenciesは上記edge形状。File recordは`{path, bytes, sha256}`。Entry／testは一覧の`.lang` fileを指す。入れ子manifest・workspace・lock・binary・doc asset・暗黙依存archiveは含めない。Source bytesはUTF-8必須で、comment・Unicode・CRLFを含む元bytesを保持する。Compileせずbyte受入中に検証する。Uploadの副作用としてcompileしない。

Publisherは明示的に固定した入力snapshotと、明示registry依存bindingからdescriptorを派生する。未解決local path依存を拒否する。Previewは生成metadata全fieldと全source file／hashを列挙する。元local `Lang.json`・`Lang.lock`をreleaseへコピーもrewriteもせず、descriptorが元manifestのbyte identityであるとも主張しない。RFC 0035 snapshot exportは不変で、このformatとして受入れない。

File pathは1–100 byteのASCII slash pathで、非empty componentは英字・数字・dot・underscore・hyphenのみ。絶対／backslash／colon path、`.`／`..`、末尾dot、Windows予約device stem（最初のdotより前をcase-insensitive比較）、case-fold重複、file／directory prefix衝突を拒否する。`.lang-package.json`と子孫を予約し、先頭componentの`.git`・`.codex`・`target`をcase-insensitiveに拒否。通常fileはすべて`.lang`で終わる。100 byte制限でlong-name extensionを不要にするため、local packageで使えるsource pathも公開時には正確なpath診断付きで拒否する場合がある。

D01明確化（2026-10-31 JST）: Case衝突検査は暗黙directory prefixにも適用する。`src/a.lang`と`Src/b.lang`がcase-sensitive／insensitive hostで別source treeを生まないようにする。同一表記directoryの再使用は許すが、fileをdirectory prefixにもできない。既存portable alias禁止要件を明確化したもので、公開済みformatは変えない。Profile 1は未実装である。

Headerは100 byte name fieldに名前を入れ、prefix／linkname／owner／group fieldはzero、type `0`、mode 0644、uid／gid／mtime zero、magic `ustar\0`、version `00`、device major／minor zero、未使用byte zeroとする。数値fieldはzero padding付きASCII octalと末尾NUL。Checksumは6 octal桁・NUL・spaceで、checksum fieldを8 spaceとして計算する。Payload paddingはzero。末尾は正確に2 zero blockとし、追加bytes・連結archive・base-256数値・directory／link／device entry・PAX／GNU extension・sparse file・圧縮を拒否する。受入archiveの再encodeは元bytesと一致必須。Descriptor JSONはcanonical、file size／hashは厳密で、全列挙fileが正確に1回存在する。Hashは展開fileだけでなくencode済みarchive全体を識別する。

最大18 MiBと超過検知1 byteまで読む。Descriptor 128 KiB、source 256 file、file単位1 MiB、source計16 MiB、依存128、test128、JSON深さ16を上限とする。Allocation前にheader・payload／padding・decode recordを課金する。Byteのみの受入APIは非公開の検証済み不変inventoryを返し、別materializerが明示directory authorityを受け、新規destinationを排他的作成し、検証regular bytesだけを書き、失敗時は認識したprivate stagingをcleanupする。未信頼入力を汎用extractorへ渡さない。両hostの実open／公開境界でsource／destination aliasとnonregular fileを拒否する。

### HTTP profileとrecord

証明書検証HTTPS・HTTP/1.1を使い、1 connectionに1 request、`Connection: close`とする。Redirect・content encoding・自動展開・chunked transfer・`Transfer-Encoding`なし。Bodyには有効なContent-Length 1つを必須とし、上限内で正確にそのbyte数を読む。Serverは応答前にclient EOFを待たず、第2／pipeline requestを処理しない。Consumerは切れたresponseと余分なbody byteを拒否する。Framing／authorization／conditional／request-ID headerの重複、異なるHost、未対応method、未知routeを拒否。Request line 2 KiB、header計8 KiB／32 field、handshake／connect 10秒、request／response全体120秒を上限とする。明示parse上限を持つ保守済みHTTP／TLS実装を使い、独自の寛容なHTTP parserを作らない。Serverはhandshakeを含め最大8 connection、application stateに無制限accept queueを持たず、受入request memory総量256 MiBとする。超過workはbody受入前に拒否／closeする。

D03実装明確化（2026-10-31 JST採択）: 読取専用serviceの共有取得済みindex/mapも256 MiB budgetへ計上する。TLS受理前に上限付き接続/TLS/header/worker記憶域、archive open/read前に完全な送信bufferを予約し、所有dataを退役してから予約を解放する。宣言された受信GET body（0 byte）だけを計上してはならない。TLS buffer・workerを制限し、永続TLS session cacheやapplication接続queueを持たない。Handshake接続はsocket/workerの実終了まで数え、cancelはactive socketを閉じてworkerをjoinする。RFC 0048のread snapshot詳細化に従い、接続受理前に起動時の全履歴検証を行う。これは保守的な論理割当て計上であり、process全体RSSや可用性の保証ではない。Allocator/runtime管理情報と信頼済み実行file/config記憶域は別である。既存のbyte・並行・deadline上限は増やさない。実budget境界と敵対peerはD03の受入testとして残る。

全pathはorigin root基準。Name／version／digestは制限済みで、percent decodeで別identityへ変換しない。任意filesystem pathやremote URLをendpoint入出力にしない。

| Methodとpath | 契約 |
| --- | --- |
| `GET /v1/packages/NAME` | Yank／quarantine flagを含むpublicな上限付きpackage index。欠落packageは安定した拒否を返す。 |
| `GET /v1/objects/SHA256` | Commit済み非quarantine releaseが参照するpublic不変archive。厳密yank releaseも含む。Bytes／長さ／digestを厳密検査し、staging・未commit objectを公開しない。 |
| `GET /v1/packages/NAME/owners` | 認証済み現memberまたはscope付きadmin。`{schema:1, registry, name, revision, owners}`を返しprincipal IDはsortする。 |
| `PUT /v1/packages/NAME/VERSION` | 認証archive body、`Content-Type: application/x-tar`、32 hexの`Idempotency-Key`。既存名は引用符付き10進revisionと等しい`If-Match`、初回claimは`If-None-Match: *`とclaim／publish両scopeを要する。置換操作なし。 |
| `POST /v1/packages/NAME/actions` | Schema・request ID・期待package revision・action固有field・reason必須の認証JSON action。 |

Index fieldは`format: "lang-registry-index"`・`schema: 1`・`registry`・`name`・`revision`・`releases`。Releaseは数値version tuple順で、`version`・`archive_sha256`・`archive_bytes`・`descriptor_sha256`・`edition`・`dependencies`・`yanked`・`quarantined`を含む。単一commit stateから派生し2 MiB以内。Production consumerは別途認証するF01 release／selection metadataとindex identity／digestが一致するまで信頼しない。HTTPSだけではこの認証にならない。未知署名profile・証拠欠落は消費拒否。D段階の分離した未署名fixtureをproduction CLIが暗黙選択できない。

**派生index受入（2026-10-31 JST採択）:** D03がindex/downloadを公開する時点で、2 MiB上限を公開不変条件にもする。新candidate bytesを配置する前に、次のpackage revisionと現在flagを含む候補stateから対象packageの完全indexをencodeする。その厳密canonical encodingが2,097,152 byteを超えれば`REG0002` / HTTP 413で拒否し、等しい場合は許可する。派生indexを変える後続package変更にも同じ検査を適用する。Lock内transactionで検査し、release/依存の切詰め・暗黙pagination・不変archive bytesの変更はしない。Readerも独立して同じ上限を検査し、部分indexを返さない。

実行実験`/private/tmp/lang-m5-index-bound-probe.rs`は各128依存を持つ実archiveを受理する。Canonical indexは29 versionで2,033,722 byte、30 versionで2,103,846 byteとなり、各descriptorは128 KiB未満である。したがってarchive/state/count受入だけではindex容量を証明できない。これはローカル受理/encodeの証拠で、server公開ではない。D03で厳密閾値、拒否時HEAD不変、両host transport testを追加する。

互換性とrollout: field・format versionは維持するが、公開受入を厳しくする。D02は実験的private storage checkpointであり、D03はservice・consumer公開前にこの規則を実装する。上限超過の既存実験stateは明示operator検査/復元用に保持し、そのindexと当該packageへの新規変更を拒否する。切詰め・version削除・履歴reset・自動migrationはしない。RFC 0001委任に基づき、提供不能indexの公開を防ぐcommit前容量拒否を採択する。上限引上げは合意上限を弱め、paginationには別の互換性・trust契約が必要。Server統合と厳密境界fixtureはD03まで未検証。

Action object共通fieldは`schema: 1`・`request_id`・`expected_revision`・`action`・`reason`（制御文字なしUTF-8 1–256 byte）。`yank`／`unyank`は`version`、`add_owner`／`remove_owner`は`principal`、`quarantine`／`unquarantine`は`version`とadmin scopeをさらに必須とする。Variantに無関係なfieldは拒否。Owner操作は最後のactive ownerを消せず、登録済みstable principalを対象とする。管理者owner復旧は別scope・reason・audit付きのlocal管理transactionを必要とし、release bytes置換や履歴resetはできない。Public endpointでprincipal登録・credential発行を行わない。

変更成功はcanonical `{schema:1, registry, request_id, revision, audit_sequence, outcome}`を返す。Outcomeは`published`・`yanked`・`unyanked`・`owner_added`・`owner_removed`・`quarantined`・`unquarantined`。照合済みretryはrevision／eventを追加せず元commit結果を返す。初回publishは201、replayは200、actionは200。Public GET成功は200。拒否は400不正、401一般的認証失敗、403認可、404欠落／非提供resource、409競合／復旧必須、413上限、429busy、500I/O失敗。Error bodyは16 KiB以内の`{schema:1, code, message, request_id}`で、安全に受入れた場合以外request_idはnull。Code分類は下記診断に従い、credentialや任意入力bodyをechoしない。

### 認証と可変authority

外部で準備した一様乱数32 byteのopaque tokenを小文字64 hexへencodeし、単一`Authorization: Bearer TOKEN` headerで渡す。変更・owner endpointだけが消費し、public GETはAuthorizationを省略／拒否して転送しない。Serverはbearerそのものではなく`SHA256("LANGREGTOKEN1\0" || token_bytes)`だけを永続化する。Authenticator比較で秘密依存の早期一致判定を使わない。Client明示入力は正確に64 hexと任意の末尾LF 1つに限定しdecode前に制限する。Test tokenはfixture専用でproduction credentialではない。

Token recordはstable principal・registry audience・issued／expires時刻・revoked flag・明示`(action, package-name)` grantを含む。Actionはclaim／publish／yank／owners／admin。寿命は最大7日、grantは最大128。Tokenだけではownershipを与えない。既存package変更は現在owner membershipと該当grantを要し、管理quarantine／復旧は別admin scopeを使う。新規名公開はnameに限定したclaim／publish両grantを要し、公開principalを初期ownerとしてatomic登録する。Server設定は明示local管理capabilityでprincipal登録・token準備／失効を行い、これらも直列化・監査する。信頼管理を未認証HTTPの抜け道としない。

時刻は0..=253402300799の整数Unix秒。受入時とcommit直前にhost timeを読み、issued <= now < expires、now >= 永続time floorを要求する。時計後退・失効token・membership変更を拒否。Body parse／stagingは古い認可判断を保持しない。失効・owner変更・publishは最終受入で同じ非blocking排他registry lockを使い、競合は無制限待機ではなくbusy。盗難された有効scope付きtokenは失効まで残余riskとなり、TLS・短寿命・狭いgrantで露出を減らすが解消したとは主張しない。

### 不変transaction・audit・復旧

Storage rootは明示許可private directoryでありarchive pathではない。Initは新rootと監査bootstrap stateを排他的作成し、既存rootをresetしない。Regularな非blocking排他lockを持つwriterは1つ。上限はpackage名1,024、version／name128、release計8,192、principal／token各4,096、owner／name128、commit audit event65,536、現state16 MiB、staging／履歴を含む管理storage計4 GiB。管理entryと各復旧列挙は262,144を上限とし、未知entryは削除や無限走査でなく拒否する。Consumer cacheもgraph上限とは別に4 GiB／262,144 entryを上限とする。全算術をcheckedにする。上限到達時は変更前に拒否しreadは継続。Profile 1のquotaでありinternet規模capacityの主張ではない。Audit暗黙削除・identity自動再利用・quota起因trust resetはない。

Content-addressed object・不変canonical state generation・audit recordをprivate固定namespaceへ永続化する。小さなcanonical HEADは`{schema:1, state_sha256}`だけを含む。Stateはregistry identity・単調revision／time floor・package／version／owner／quarantine record・登録principal／token hash／scope・末尾audit sequence／hashを持つ。Stored valueからhost pathを作らない。[RFC 0047](0047-registry-authority-records.md)でE01前にclosed authority recordを具体化し、[RFC 0048](0048-registry-state-and-publication.md)でD02前に完全なgeneration/state・audit・receipt encodingを採択した。2026-10-31 JSTに採択したこれらの具体化は、独立して書けるauthority fileを追加しない。必要なatomicity・上限・authorityはここで固定したまま。Private versioned formatでも厳密受入を免除しない。

Audit eventはsequence・前commit event hash・前state hash・受入時刻・principal・秘密でないcredential identifier・操作・request ID／digest・対象identity・reason・commit結果／revisionを記録する。最大2 KiBでcredential secret・source bytes・filesystem pathは含めない。Token準備／失効・管理復旧を含む全state変更にeventがある。Hash chainは保持headに対する破損を検知し、悪意operatorの消去を防ぐものではない。Export可能な検証receipt／checkpointはF01／E04の作業として残る。

Lock保持中にcommit済みHEAD／state／auditを再load／検証し、再認可、request ID照合、期待revision／quota検査、全archive検証を行い、全新規不変object・event／state fileを書いてsyncする。参照bytesがすべて完成した後に同filesystemでHEADをatomic置換する。この置換をindex・所有・auditの単一可視化点とする。Readerは単一の取得済みcommit stateを観測し、独立した可変indexを見ない。HEAD公開後に成功を返し、その後の応答失敗でrollbackしない。Version identityは削除／置換せず、yank／quarantine／ownerはstateだけを変える。

Request IDは保持audit履歴中principal単位で一意。Request digestはfield順のcanonical JSON `{schema:1, method, path, conditional_revision, body_sha256, body_bytes}`のSHA-256。Pathはcanonicalなorigin root基準request path、conditional_revisionは期待revision整数または初回claimのnull、body identityはcanonical action JSONまたは厳密archive bytesを対象とする。最初に現在authorityを確認する。既知IDと同digestなら古いconditional revisionがstaleでも元結果を返し、異digestなら競合。未知IDとstale revisionも競合。失効／削除済みcallerは旧成功replayで現在認可を迂回できない。接続失敗から未commitと推測せず、clientは同request identityでretry／照合する。

HEAD置換前のprocess中断は旧commit state、置換後は完成した新stateを残す。復旧はlockしHEADと全到達可能な有限record／objectを検証する。RFC 0048の2026-10-31 JST具体化により、cleanupは認識可能で上限内のregular staging file（部分書込みを含む）と、完全でdigest検証済みかつ未参照の不変object/audit/state candidateだけを許可する。不変名へ配置する前にbytesをstagingしてsyncし、process中断で不変candidateが部分書込みにならないようにする。全commit履歴世代は到達可能として保持する。未知・破損した不変entry削除、最大の未commit generation採用、欠落HEAD再生成、旧state rollback、floor消去は行わない。Commit済みdataの破損／欠落には明示的な検証済みoperator復元が必要で、通常操作はfail closed。File syncとatomic置換は必須だが、cross-platformの電源断durability／availabilityは保証せず、電源断で破損／欠落したstateを検知して拒否する。D02でWindowsを含むplatform置換挙動を検証してから依存する。

### Consumer・offline・統合境界

同じframing／timeout上限で設定originのみからdownloadする。要求した不変identityへresponse長／hashを結び付け、F01信頼／freshness検証後にgenerationを消費可能にする。Cache identityはregistry／root方針とarchive digestを含み、再使用時もbytesを検証。全体を有限受入した後にだけ新規destinationを公開する。Callerはnetwork・cache write authorityを明示許可し、`--offline`はDNS／network／credential探索をしない。

新規解決はyank／quarantine versionを除く。厳密lock再現は、F01 receipt／期限／失効方針の下で、後からyankされても以前認証した非quarantine releaseを使える。Quarantineは新serviceと新たに観測したquarantine replayを拒否するがoffline copy消去を主張しない。新規受入に必要な信頼が未知／期限切れならofflineで迂回できない。Local bytes欠落は別package／version／originを選ばず失敗。RFC 0034のgraph上限（package／module256、source16 MiB、edge4,096）を維持し、再訪も課金して依存cycleを拒否する。

D04は実装前に本RFCまたは後続RFCでmanifest schemaとlock表現を具体化する。Local schema-1／2 lock挙動を維持し、明示versioned registry edge形式を追加し、registry／root／target／toolchain／source identityを固定、厳密canonical replayを保つ。上記archive／edge protocolは固定済みで、任意cache pathからregistry identityを再構築してはならない。これはD04の予定統合作業であり、実装済みpackage featureや暗黙schema拡張ではない。

## Execution modes

Upload・download・復旧・index構築でsource・native tool・package hookを実行しない。固定受入sourceと明示graphを既存frontend／typed MIRへ渡し、VM／native適合を要する。Registry credential／設定はguest capabilityにならない。Wasm component・native linkは別の採択ABI／信頼規則を保つ。

## ABI and compatibility

独立versionのregistry wire／archive formatを追加する。RFC 0034 manifest／lockとRFC 0035 snapshotの意味を維持し、registry formatと推測してはならない。初期public archiveはpath長・入力種別を意図的に制限し、明示拒否してsourceを暗黙renameしない。Source edition・bytecode・native ABI・component C v1は不変。後続encoding／上限変更には明示profileレビューと移行規則が必要で、F01署名・D04統合の具体化は実装に先行する。

## Diagnostics and tooling

`REG0001`を不正／未対応profile、`REG0002`を資源上限、`REG0003`を認証、`REG0004`を認可、`REG0005`をidentity／version／revision競合、`REG0006`を完全性／信頼／freshness失敗、`REG0007`を永続state／復旧失敗、`REG0008`をbusy／I/O／transport失敗へ予約する。不正command optionは既存CLI0001、guest sourceは既存source診断を使う。構造化失敗はtokenなしの有限stage／identity文脈を含み、確実な未commitと公開後結果不明を区別する。Human出力は結果不明時に新request IDの盲目的生成ではなく照合へ導く。保存JSONはcapabilityでも実行可能planでもない。

## Performance and cost model

厳密archive上限により有限memoryで先に検証し、検証bytesをmaterialize時にstagingへ1回copyできる。全state検証／履歴lookupは有限でも線形・高コストになり得るため、throughputを主張しない。受入前に全active body・staging・旧generation・auditを課金する。復旧で任意ambient directoryをscanしない。測定により後続index付きstate形式を採用してもtransaction／audit契約を弱めない。

## Security and trust boundaries

| Case | RFC 0045脅威 | 実行可能受入条件 |
| --- | --- | --- |
| P01 | R01,R07,R14 | 設定した2 originで同名／versionを公開し、diamond／offlineを含め厳密依存がorigin／digestを越えない。Private path依存はpublic encodeを拒否。 |
| P02 | R04,R05 | 未対応header／extension、traversal／link／device／case／prefix alias、size／hash不一致、不正descriptor、過剰padding／末尾bytes、上限境界をdestination書込み前に拒否。 |
| P03 | R02,R03 | 異audience／action／package、期限切れ／失効token、upload中owner削除、stale owner revision、最後のowner削除はcommit不可。未知token IDの認証errorは識別不能。 |
| P04 | R06,R11,R12 | 1 versionで異bodyを競合させcommit／eventは1件。Staging／HEAD／応答の各境界で中断し、retry後も不変identity／eventは1件。 |
| P05 | R07,R08,R09 | Object／index／state改竄、署名rollback／期限切れ、root置換、時計後退を拒否。F01で署名消費実装前に具体key／root fixtureを追加。 |
| P06 | R05,R13 | Redirect・別origin／metadata URL・不正framing・過長／遅延body・並行数超過を拒否。Public readにpublish credentialを送信しない。 |
| P07 | R10,R14,R15 | Upload／復旧でguest／provider実行やambient file readを起こさない。Hosting前にI01で安全render／query caseを追加。 |
| P08 | R11,R12,R16 | Disk-full・HEAD／audit破損・quota満杯・不正admin操作はcommit済みidentity／floorを保つか復旧を拒否。自動rollback／reset／消去なし。 |

信頼local administratorはserver stateを読んだりserviceを破壊でき、認可publisherは悪意ある新sourceを公開できる。TLS credentialはpackage署名の信頼を確立しない。これらはRFC 0045の明示残余riskでありchecksum／audit chainで解決したとしない。

## Alternatives

- 変更なし／local snapshotではregistry downloadと認証変更を満たさない。
- 汎用object storeと外部認証はbytesをhostできるが、package／owner／audit commitには単一のレビュー済みauthorityが必要で、deployment adapterで迂回できない。
- Cargo互換の圧縮crate／index慣習はecosystemを再利用できるが、異なるmanifest／解決／archive前提を取り込む。初期profileは厳密宣言edgeと小さい厳密source archiveを選び、実測した使いやすさが後続拡張の理由になり得る。
- 分散database transactionはsingle writerを越えてscaleできるが、最初の依存実行fixtureより前に配置／復旧依存を増やす。有限single writer profileはscaleを主張せず、レビュー可能な単一commit点を与える。
- 可変fileとbest-effort auditは単純だが、version置換とcrash後の成功／履歴分離を許すため不採用。

## Implementation plan

順序明確化（2026-10-31 JST）: 既存E01の認証・scope付き認可deliveryをD02公開より先に実装する。D02は新たにlockしたstateを使いcommit前にその方針を検査し、byte受入や古い認証結果だけで後の変更を認可しない。既存依存を明示したものでdelivery・見積りを追加せず、E02–E04の操作／audit統合を維持する。全baseline・CIの終了まではD01を実装中とする。

Upload前にC02をレビューする。D01はbyte-only archive受入と拒否corpus、D02は具体化した不変state transaction、D03はindex／download identity検証、D04はpackage統合を具体化して両hostで実依存を実行、D05はoffline／競合／crash journeyを加える。E01–E04は同じtransactionでtoken／owner／audit操作を実装。署名production消費にF01、hosting／検索にI01が先行する。新実行fixtureなしに前提deliveryを2件終えたら既存board規則で順序を再監査する。本書で新実装deliveryは追加しない。

## Test plan

P01–P08の入力・state／副作用観測と厳密test commitを保持する。Archive testは実encode bytesと再encode一致を使い、不正corpus／fuzzingでheader／JSON整数・nesting・総量課金境界を検証する。Fault injectionは実公開境界を対象とし、process crash復旧を別確認する。TLS／framing testはmacOS／Windowsでfixture証明書の実loopback接続を使う。単なる認可boolean mockでなく厳密token／audience／期限／失効と並行owner／version変更を検証する。実依存全体がVM／nativeとlocked offlineで同じ結果を返す必要がある。全compiler baselineをこれらのdomain testの代用とせず、code変更には通常baseline／CIも適用する。

## Open questions

2026-10-31 JST、RFC 0001の実験段階委任により採択。根拠: byte-onlyの厳密受入、明示identityと現在authority、単一のstate／audit commit可視化点、有限復旧、対応する拒否caseにより、compiler実行authorityを広げず初期registryを実装可能にする。レビューでHTTPのclient EOF待ちによるdeadlock riskを修正し、quarantine object拒否と復旧列挙／cache storageの上限を明示した。日英ともR01–R16すべてがP01–P08へ対応する。

[Canonical archive実験](../../evidence/2026-10-31-m5-registry-archive-profile.json)は指定headerとCRLF scalar sourceから3,072 byteを構築した。導入済みの独立TAR readerで2 recordを列挙し、元source bytesを厳密復元した（両exit 0）。これはlocal形式実現性だけを示し、敵対入力decoder実装や両host適合の証拠ではない。全受入recordの保守的上界17,172,223 byteはencode上限18,874,368 byte内に収まる。性能主張は行わない。

C02を妨げる未決設計事項はない。RFC 0047でE01 authority record、RFC 0048でD02 state/audit encodingを採択した。Manifest／lock encodingはD04、署名／root／freshnessはF01、hostingはI01で明示具体化し、いずれも実装前に採択する。上限はproduction capacity／使いやすさの測定を伴わない保守的project判断。TLS parser統合・両hostのatomic置換・実際の敵対入力／cancel挙動は該当deliveryまで未検証。Public service・production rootは確立せず、採択は実装／安定化を意味しない。

## Sources

- [RFC 0045](0045-registry-threat-model.md)、2026-10-31更新。[RFC 0034](0034-package-workspaces-and-cli-diagnostics.md)・[RFC 0035](0035-source-package-snapshots.md)、2026-10-29更新。[RFC 0042](0042-signed-toolchain-installation.md)、2026-10-30更新。2026-10-31 JST確認。脅威義務・既存capabilityは採択済みproject契約に由来し、本protocol／数値上限は外部適合主張ではなくproject判断。RFC 0045にTUF／SLSA／OAuth／Cargo一次出典と公開／更新日を記録する。
