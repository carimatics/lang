<!-- translation-of: ../../rfcs/0052-registry-owner-and-credential-control.md -->

# RFC 0052: Registry所有権とローカルcredential失効

[English](../../rfcs/0052-registry-owner-and-credential-control.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E03
- Supersedes: なし。RFC 0046–0048と0051を具体化
- Superseded by: なし

公開checkpoint（2026-10-31 19:15 JST確認）: `18b2128`でfmt、厳密workspace Clippy、2,667 test/182 suite、rustdoc、locked fuzz Clippyが成功した。CI 41190281059は対象18 job、native-debug 41190281058は3 hostすべて成功し、Windowsと両macOS logでowner/失効・index-capacity fixtureを確認した。PR #40は`ee0d985`としてmergeし、mainはclean・同期済み・検証済みtreeと同一である。以下の以前の検証待ち記録は履歴とする。Local host契約を実装済みとし、HTTPS統合はRFC 0053/E06の作業memory支援を使うRFC 0051/E05に残す。

## 概要

認証owner追加・削除、非公開owner読取り、安全な2段階所有権移管、明示的なローカルcredential失効を定義する。これらは現在のregistry lock、完全履歴検証、最新時刻検査、単一HEAD commitを共有する。削除されたownerや失効credentialは過去の成功をreplayして失った権限を回復できない。

## 動機

RFC 0047は現在のowner/credential policyと実mutationを分離した。E02は監査付きrelease flagを変更できるが、package移管や失効の永続化はできない。残るE03 fixtureは実装前に厳密なstate/audit遷移を必要とし、authority recordの直接編集はRFC 0048の履歴契約に反する。

## 目標

- active ownerを少なくとも1人維持して登録済みownerを追加・削除する。
- 認可されたmemberまたはscope付きadministratorだけに現在のowner集合を返す。
- 明示的な中間共同所有と復旧可能なrequest identityで所有権を移管する。
- 別のローカル管理capabilityでcredential失効を永続化し、古い通常requestを最新状態で拒否する。

## 非目標

- atomic transfer action、公開principal登録、credential発行、network credential管理endpoint。
- principal無効化、後続provisioning、管理者によるowner回復、quarantine workflow。これらはE04の具体化に残す。
- 署名provenance、本番消費、source言語変更、電源断時availability、悪意あるoperatorの認証。

## ガイドレベル設計

Owner Aがpackage revision Rでactiveな登録済みprincipal Bを追加する。R+1で両者がownerになる。次にAまたはBがR+1でAを削除し、R+2でBだけになる。各actionは異なるrequest IDを持つ。第2段階に失敗すれば両者が残り、transferはatomic置換ではない。既存のname-bound grantはBの操作を引き続き制約し、所有権はcredentialやgrantを発行しない。

成功した自己除名はcommit済みreceiptを返す。Aによる後のretryは、そのIDが以前成功していても現在ownerではないため拒否する。自己除名の応答を失った場合、残る認可済みownerまたはscope付きadministratorが現在のmemberを読み、結果を確認する。cached receiptを返すために削除済み権限を残してはならない。認可を維持するactorは以前成功したactionをretryし、再適用せず元の結果を取得できる。

ローカルoperatorはservice/store writerを明示的に解放し、管理host APIでprivate directoryを開き、期待state revisionと理由を指定してcredentialを失効する。新commit状態からserviceを再openすると、古いbearerはretryも含め直ちに拒否される。保持中writerがあれば管理openはbusyになる。この有界initial-profile workflowはambient credential、隠れたHTTP route、自動lock待機を使わない。E05のlive service統合ではservice停止・失効・再openを検証する。

## 参照semantics

### Owner actionと読取り

Owner action bodyはRFC 0051のcanonical JSONと2,048-byte上限を使い、field順は`schema:1`、`request_id`、`expected_revision`、`action`、`reason`、`principal`とする。Actionは`add_owner`または`remove_owner`、principalは既存の32文字lowercase-hex IDである。Request ID、正のpackage revision、reason（UTF-8で1–256 byte、制御文字なし）、厳密name、閉じたfield/encoding規則は既存の意味を維持する。適用外の`version`、未知/重複field、null、未対応schemaを拒否する。Debug/errorに任意reasonやcredentialを再掲しない。

上限付きhost actionは権限ではない。`Operation::ChangeOwners(name)`はrequest照合前とcommit時に、現在lock中のviewで実bearerを認証する。Principalは厳密nameの`owners` grantを持つactiveな現在ownerでなければならない。`admin` grantだけでは通常owner mutation scopeにならない。古い条件revisionや対象memberの検査より先に、global principal/request-ID mapを照合する。現在の認可は常に必須である。

新actionは現在のpackage revisionを要求する。追加対象はactiveな登録済みprincipalで、owner集合に未所属でなければならない。不存在/inactive対象はnot found、既存memberはconflict、128 ownerを超える増加は挿入前にlimitとする。削除対象は既存memberであり、少なくとも1人のactive ownerを残す。非memberまたは最後のactive owner削除はconflictとする。Active ownerが残るならinactiveな過去memberを削除できる。IDはsort済み・重複なしを保つ。これらは遷移の前提条件でありcaller認証の代替ではない。

Candidateはstateと対象packageのrevisionを各1増やし、そのpackageのauthority owner集合だけを変更し、時刻floor/audit linkを進める。全release、source byte、他package、principal、token record、grantを保持する。Revisionだけが変わる場合も導出index上限を適用する。E02と同じ事前quota予約、stage/sync済みaudit/state、最新event時刻、最終の最新認可、atomic HEAD置換を使う。構造化not-published/unknown結果と完全な過去履歴を維持する。

Owner読取りは`Operation::ReadOwners(name)`を使う。Activeな現在memberにmutation grantは不要であり、非memberには厳密nameの`admin` grantを要求する。完全な1 viewをcaptureし、最新時刻で認証してencodeし、返却前に最新時刻/認可を再検査する。Canonical応答は順に`{schema:1, registry, name, revision, owners}`を持ち、IDをsortし、8 KiBを上限とする。Revision/floorを変更せず、受取側に権限を与えない。公開package indexはownerを公開しない。E05が既定の認証GET routeを提供する。

### Owner auditとreplay

Schema-1 operationとして`{kind:"add_owner", name, principal}`と`{kind:"remove_owner", name, principal}`を追加する。Operation内の`principal`は対象memberで、共通audit principalは認証済みactorである。Resultのfield順は`{state_revision, package_revision, name, principal, member}`で、memberはそれぞれtrue/falseとする。共通credential/request/prior-hash fieldはnon-null、reasonは受理したaction reasonである。Host receiptは`{schema:1, audit_sha256, result}`を維持する。

記録request ID、直前package revision、operation、reason、対象principalからcanonical actionを再構成する。Request digestは`POST`、`/v1/packages/NAME/actions`、直前revision、完全canonical body hash/lengthを使う。Event時刻の直前authorityで記録actorを検証し、完全な期待state/resultを再構成して両方を比較する。Owner変更にcredential、release、無関係な所有権、quarantine変更を紛れ込ませられない。再構成したno-op、最後のowner削除、重複request identityは全hashを再計算しても拒否する。

### 独立したローカル管理capability

`Administrator` host wrapperを公開し、構築には明示的なparent directory capability、上限付きprivate child名、独立設定service identity、設定operator IDを要求する。既存のno-follow/通常file/履歴境界で同じregistryを排他的にopenする。Registryとoperator bindingをprivateに所有し、bearer、owner list、receipt、network body、booleanからのconstructor、通常`Registry`からのpublic変換、内部を公開するdereferenceを提供しない。通常の認証mutation APIはこのcapabilityを取得できない。

Operator IDは信頼済みhost設定が選ぶaudit帰属であり、credentialやcaller指定権限ではない。受理時と最終commit時に正しい形式で、現在登録済みactive operatorであることを要求する。付与されたprivate directoryがローカル管理accessを制御する。Name-scoped bearerの`admin` grantはglobal credential失効capabilityではない。Bootstrap同様にhost/operatorを信頼する。この仕組みはローカルcapabilityを使った人間を暗号学的に特定しない。

Wrapperは検証済み現在state revisionを読み、既存credentialを1個失効できる。失効はprincipal登録、token作成、grant変更、所有権回復を行わない。期限切れcredentialやinactive principalのcredentialも失効できる。Operator自身のbearer失効は、別に保持するローカルdirectory capabilityを失効しない。ただしそのbearerを使う通常requestは新状態で直ちに認証失敗する。

### ローカル失効requestと遷移

上限付きcanonical local requestのfield順は`schema:1`、`request_id`、`expected_state_revision`、`action:"revoke_credential"`、`reason`、`credential_id`である。同じ2,048-byte、閉じたJSON、ID、reason規則を使う。期待state revisionは正でありpackage revisionとは異なる。Canonical request-digest fieldはRFC 0046の`{schema, method, path, conditional_revision, body_sha256, body_bytes}`を維持するが、methodは厳密に`LOCAL`、pathは`/v1/admin/credentials/ID/revoke`、条件は期待state revisionとする。これは内部domain separatorであり、HTTP methodや対応network routeではない。Request bodyはローカルcapabilityを付与できない。

現在のoperator/時刻検証後、同じglobal principal/request-ID mapを照合する。Digestが同じなら、条件が古くても現在operator/時刻を再検査した後に元receiptを返し、異なればconflictとする。新IDでは厳密な現在state revisionと既存の未失効credentialを要求する。不存在対象はnot found、既失効対象や古い条件はconflictとする。Recordやidentityを削除・再利用しない。

対象tokenのrevoked flagをfalseからtrueにし、state revision、state/authority時刻floor、audit linkだけを変更する。Package revision、owner、release/index byte、登録principal、他の全token fieldは不変とする。書込み前に全peak quotaを予約し、event時刻を再取得し、audit/stateをstage・sync・installした後、既存HEAD置換直前に設定active operatorと最新時刻を再検査する。最終時刻はevent時刻より後退できない。管理read/retryはfloorを解除しない。Unknown結果には既存mutationと同じ明示recovery/照合契約を要求する。

Operation `{kind:"revoke_credential", credential_id}`、result `{state_revision, credential_id}`を追加する。共通audit principalは設定active operatorとし、共通`credential_id`はbearer認証ではなくlocal capability管理なのでnullとする。Request ID/digestとprior hashはnon-null、reasonは受理理由である。履歴は直前active operator帰属、対象の存在/未失効、再構成LOCAL request digest、厳密な全遷移/resultを検証する。通常の非初期operationではnull credential fieldを引き続き禁止する。Result state revisionはevent sequenceと一致する。Host receiptやhash chainはpackage署名ではない。

## 実行mode

Owner/credential状態はregistryの操作可能者を変え、source実行は変えない。E03 fixtureはpackageをBへ移管し、Aの保持requestを拒否し、Bで公開して同じ保持sourceをVM/nativeで実行する。別途実local失効でBの古いbearerを拒否し、package revisionと厳密archiveを保持する。MIR、VM、native、JIT、component、embedding semanticsを変更しない。

## ABIと互換性

影響domainはpackageとtoolingである。Authority/state field、source/archive/lock/ABI formatは不変とする。Private schema-1 audit/receiptに閉じたvariantを追加し、旧readerはこれらを含む履歴を拒否しなければならない。最初の操作前にhost readerをupgradeする。Event削除、HEAD reset、失効credential復活、owner捏造によるdowngradeを行わない。未対応variantは拒否を維持する。HTTP owner actionはRFC 0046を維持し、local失効はnetwork endpointを追加しない。

## 診断とtooling

既存のREG0001 malformed、REG0002 limit、REG0003 authentication、REG0004 authorization、REG0005 conflict、REG0006 integrity、REG0007 state、REG0008 I/O/busy/not-found分類と、適用箇所では既存HTTP mappingを使う。形式不正operator設定を拒否し、不存在/inactiveな設定operatorに管理権限はない。診断は有界・staticとする。Source formatter、language server、parser、debugger変更は不要である。

## 性能・cost model

各mutationは完全stateとauditを1つずつ保持するがarchiveは追加しない。Owner集合は128、principal/tokenは4,096、record/entry/履歴は既存上限を維持する。Retryはeventを追加しない。完全履歴検証とencodingはinitial profileのcostを維持し、throughput/latencyを主張しない。明示的な管理停止/再open workflowにより新たな共有lock、queue、RPC権限systemを避ける。Live-service availabilityと作業memory計上はE05に残す。

## Securityと信頼境界

現在bearerの所持、active member、厳密grantが通常actionとprivate readを制御する。Active local operator帰属と明示的なdirectory capability付与が失効を制御する。古いactorも成功receiptも現在の検査を迂回できない。停止/再openはlocal管理前に保持writerを解放し、競合は有界busyとする。最後のactive owner検査はownerのlive credentialを保証しない。緊急失効は意図的に最後のnetwork credentialを除去でき、後のprovisioningは明示的な監査済み管理操作でなければならない。永続reasonにsecret、source text、host pathを含めない。Host侵害/rollbackは受容済み残余riskである。

## 代替案

- **変更なし:** 監査外authority編集なしの実transfer/失効fixtureを実現できない。
- **外部identity/registry service:** accountは管理できても、このregistryのatomic所有権、保持request identity、local履歴semanticsを定義しない。
- **Atomic transfer variant:** 中間共同所有を避けるが、RFC 0046にないaction/互換契約を追加する。明示add/removeはaccepted operationを再利用し、部分完了を明らかにする。
- **公開/global admin bearer endpoint:** 遠隔管理は可能だがcredential audienceとtransport surfaceを広げる。明示local capabilityはinitial profileに適合し、package grantから推測できない。
- **自己除名/失効後のcached成功:** retry応答を簡単にするが権限を復活させる。現在権限を要求し、残る認可actorにcommit結果を確認させる。
- **未記録token file書換え:** 小さいが単一HEADのaudit/owner/index一致とrecovery検証を破る。

## 実装計画

1. E02のsource/headを固定し、E02を唯一のdoingとする間に本契約を準備/acceptする。E03は選択時の残作業監査まで既存3 ptでtodoを維持する。
2. Owner action/read受理と完全state/audit/replay遷移を実装する。無関係な管理操作より先に実transferと両mode source fixtureへ到達する。
3. 別途構築するlocal管理と1件の監査済み失効を実装し、実current-bearer拒否とpackage/source不変を証明する。広い管理workflowはE04、HTTPS dispatchはE05に残す。
4. 関連boundary/偽造/process/競合fixture、全baseline、最新headの両host CIを実行する。証拠を公開して残るG5作業を監査する。E03単独でG5を完了にしない。

Branch準備: `codex/m5-registry-owner-control`は`carimatics/lang`の`codex/m5-registry-mutations`・`f40d643`から作成する。PR #39が依存先であり、記録したorigin上の初期PR baseである。Work unit開始branchは`main`を維持し、新checkoutに既存変更はなかった。#39 merge後にのみretargetする。

## Test計画

- 実add/remove/transfer、private owner read、no-op/古い条件、未知/inactive対象、sort済み128-owner境界、inactiveな過去memberと最後のactive owner。
- 認可維持中の厳密な元結果retry、自己除名/credential失効後の拒否、operation間request-ID衝突、reason/対象/body変更。
- 移管後の実B公開とVM/native source実行、credentialのみの失効によるarchive/package revision不変。
- Local capability構築、保持writerのbusy、誤service/operator、通常admin bearerとlocal権限の違い、最終最新時刻、unknown結果の明示recovery。
- Canonical 2 KiB action/8 KiB owner応答上限、独立digest vector、閉じたfield、悪意入力、peak quota、導出indexのrevision桁数増加。
- 全hashを再計算した遷移偽造。隠れたrelease/grant変更、無権限operator主張、owner/失効no-op、重複request keyを含む。
- 両hostの実process中断と競合writer。完全な旧/新状態、最後のactive owner、全commit済みauditを保持する。

## 未解決事項

実装checkpoint（2026-10-31 JST確認）: Store 42 testが成功した。実owner移管とB公開、未変更archiveのVM/native実行、local credential失効、元owner/失効bearerのretry拒否、128-owner上限、実process中断72境界（旧68・新4）、owner/admin競合の両順序、hash再計算済み偽造18 case、最終時刻、結果不明の明示照合を含む。Owner/失効受理は独立vector 2件と一致し、上限付き入力変異1,024件を処理した。合成した妥当stateのpreflightで、package revision 99 → 100だけで厳密2 MiB indexを1 byte超えることを独立に検証した。追加68 commitを実行した証拠とはしない。以下の純粋model採択記録は履歴とする。全baseline・最新headの両host CI・公開は引き続き必須である。

2026-10-31 JST、RFC 0001の実験期委任でacceptedとする。純粋な`/private/tmp/lang-m5-owner-policy-probe.js`は2段階transfer、現在権限retry、最後のactive owner、128-owner容量、archive/package revision不変、独立modelのlocal失効を含む24 caseに成功した。独立JavaScript/Python encodingは一致した。Owner bodyは205 byte、SHA-256 `682954cddbf3597314b8c2f3dfee606ca6a9aec1d7aa9ea9732872fb68f6a5bf`、request digest `5ab32ddd58e4c0b92843e59999d01f63f1736b5b75a3805542f3a8014034168b`。失効bodyは225 byte、SHA-256 `e026380b51d06ee10382aefed48690b5ad6059963248b7cd5d463daec12bce07`、request digest `0b043b277a723f8e2edea2b4cdf598b11a66baee9243b46791632f3a532e23c1`。

理由: 明示的な現在権限検査、独立local capability、厳密な完全遷移は、2つ目のauthority fileやatomic-transfer wire variantなしにaccepted registry modelへ適合する。Modelはpolicy/encoding証拠に限り、実認証、directory capability、process中断、Windows挙動、service availabilityは証明しない。これらは実装の責務として残る。観測可能な結果が増えた場合、E03の既存見積りを実装前に再監査する。本acceptanceでgateは完了しない。

## 出典

- [RFC 0046](0046-registry-protocol-and-recovery.md)、更新2026-10-31: owner action、local管理、request identity、quota。
- [RFC 0047](0047-registry-authority-records.md)、更新2026-10-31: 現在owner/read/admin policy、active owner、失効。
- [RFC 0048](0048-registry-state-and-publication.md)、更新2026-10-31: 単一HEAD世代、完全遷移、保持audit/replay。
- [RFC 0051](0051-registry-yank-transactions.md)、更新2026-10-31: 閉じたaction/receipt variantと最新host transaction pattern。
- 上記local 24-case policy/独立encoding実験、実行2026-10-31 JST。Project判断の証拠でありsecurity/性能測定ではない。
