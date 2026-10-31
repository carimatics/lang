<!-- translation-of: ../../rfcs/0051-registry-yank-transactions.md -->

# RFC 0051: 認証されたyank・unyank transaction

[English](../../rfcs/0051-registry-yank-transactions.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E02, M5-E05
- Supersedes: none; RFC 0046–0048を具体化
- Superseded by: none

## 概要

E05 adapter準備（2026-10-31 JST確認）: owner変更はyank/unyankと同じ認証action routeを使い、private owner readはowner GET routeを使う。HTTP receiptはhost audit receiptと別形式で、元のpackage revisionとglobal audit sequenceを持つ。公開は初回commit（201）と照合（200）を区別し、actionは200を返す。Request byteを送信した可能性があればtransport失敗は結果不明となり、検証済みrecovery-required拒否もrollbackを証明しない。認証拒否は今回の試行を説明し、以前の同一ID試行の結果を確定しない。外部`/private/tmp/lang-m5-http-contract-probe.py`の54 caseは条件header policy、receipt対応、送信段階分類だけを検査し、実TLS・最新権限・framing・deadline/cancellation・両host証拠はE05要件に残る。

既存の不変registryに、閉じたyank/unyank actionとaudit遷移を追加する。現在の認可されたownerは、archive・所有者・identityを変えずにreleaseの選択flagを変更する。単一HEAD置換でflag・package revision・audit eventを同時公開し、改めて認可された再試行には元の結果を返す。

## 動機

RFC 0046は公開action routeを定めるが、RFC 0048は意図的に初期化・公開のaudit variantだけを受理する。完全な遷移を定めずflagを変更すると、recoveryが失敗するか、監査されない状態変更を許す。本提案はE02実装前に不足している互換性契約を定義する。

## 目標

- Indexの可視性metadata変更中も厳密archiveを保持する。
- 受理時とcommit時に実bearerと現在ownerを認証する。
- Canonical action identity・競合・履歴検証・再試行結果を定める。
- 上限付き認証serviceを通じて公開とyank/unyankを検証する。

## 非目標

- Owner変更、principal登録、token発行/失効、quarantine操作（E03–E04）。
- Package署名、信頼された本番消費、新source構文、compiler semantics。
- Release削除、履歴pruning、電源断時の可用性、悪意あるoperatorのrollback対策。

## 利用者向け設計

Ownerは`math` version `1.0.0`をpackage revision 1で公開し、expected revision 1のyank actionを送る。成功するとpackage revision 2になる。Indexはその厳密releaseをyankedと示すが、quarantineされていない厳密archiveは引き続きdownloadできる。Expected revision 2のunyankはrevision 3を作りflagを解除する。Yankedでも不変versionの置換公開は拒否する。

応答を失った場合、同じcanonical actionとrequest IDで再試行する。成功済みactionには、その後のunyankで現在flagが変わっていても元のreceiptを返す。古いactionを再適用しない。毎回、現在の認証/所有者検査が必要である。別request IDで既に現在と同じflagを要求すると競合になり、eventを作らない。

途中の公開が、`false`から`true`へのencodingで空いた1 byteを使う場合がある。その後のunyankで次の厳密canonical indexが2 MiBを超えるならREG0002で拒否し、現在flag・archive・履歴を保持する。Index受理規則は公開だけでなくactionにも適用し、切詰めや暗黙の上限拡張は行わない。

## 参照semantics

### Canonical action

`POST /v1/packages/NAME/actions`はRFC 0046の明示HTTPS originとbearer処理を維持する。E02のhost actionとE05のHTTP adapterが受理するJSON field順は厳密に`schema:1`、`request_id`、`expected_revision`、`action`、`reason`、`version`とする。`action`は`yank`または`unyank`。Bodyは2-space canonical JSONと末尾LF、最大2,048 byteで、未知/重複/variantに不適切なfield、非canonical encoding、未対応schemaを拒否する。Request IDは32文字の小文字hex、expected revisionは正の`u64`、versionは既存の厳密数値version文法を使う。NAMEは独立検証したrouteから取得し、任意body pathから取得しない。

ReasonはUnicode制御文字を含まない1–256 UTF-8 byteとする。呼出元は永続化される説明に秘密・source本文・host pathを含めてはならない。Error/debug出力は任意bodyやbearerを反映しない。Host actionのconstructor/admitterにも同じ上限を適用し、厳密canonical action identityを保持する。

既存RFC 0046のrequest digestを、method `POST`、path `/v1/packages/NAME/actions`、actionのexpected revisionと等しいconditional revision、request IDとreasonを含む完全canonical bodyのSHA-256/lengthで再構成する。したがってreason・flag・version・revisionを変えた要求は、成功済みprincipal/request-ID組を再利用できない。別のreplay namespaceは追加しない。

### 受理と遷移

既存private非blocking排他registry lockの下で以下を行う。

1. 完全なcommit済みviewをrecover/検証し、最新host時刻とRFC 0047の`Operation::Yank(name)`権限で実bearerを認証する。Credentialには厳密nameのyank grantと現在のactive owner所属が必要であり、publishまたはadmin scopeだけでは足りない。
2. 現在の認可後にのみprincipal/request-ID組を照合する。Request digestが等しければもう一度最新認可を確認して元receiptを返し、異なれば競合とする。古いexpected revisionは、認可済みの厳密再試行を妨げない。
3. 新規要求では現在package revision、存在する厳密version、現在と異なる要求flagを必要とする。古いrevisionまたは既に同じflagは競合。認可済み要求でversionがなければnot found。拒否時はHEAD・archive byte・commit済み履歴を変更しない。
4. Stateと対象package revisionを各1回増分し、対象releaseの`yanked` booleanだけを変え、state/authority時刻floorとaudit linkを進めるcandidateを作る。他のrelease field・owner・principal・credential・scopeは保持する。Candidate配置前に既存canonical派生index・state・audit・event・物理/staging quotaを検査する。
5. 永続event用の信頼時刻を再取得して再認可し、auditとstateをstage/sync/installした後、既存の最新時刻/bearer最終guardをHEAD置換直前に実行する。Archive candidateは不要。RFC 0048の構造化されたnot-published/unknown-outcome照合を維持する。

Quarantine済みreleaseはyank flagにかかわらず利用不可であり、このactionでquarantineを解除できない。Guest filesystem/network capabilityは追加しない。Writer競合は既存busy/conflict動作を使い、lock待機queueや保存actor権限を追加しない。

### 閉じたaudit・receipt variant

Schema 1に2つのoperation variantを追加する。Field順は`{kind:"yank", name, version, archive_sha256}`と`{kind:"unyank", name, version, archive_sha256}`。Resultのfield順は`{state_revision, package_revision, name, version, archive_sha256, yanked}`とする。`yanked`はそれぞれtrue、falseで、operationとresultの両方が以前の厳密releaseの未変更archive digestに結び付く。共通audit reasonには受理済みaction reasonを使う。Principal・非秘密credential ID・request ID/digest・以前のhash・event時刻はRFC 0048の意味を維持する。非初期化で必要なfieldは引き続きnon-nullとする。Host receiptは`{schema:1, audit_sha256, result}`のままとする。

履歴検証は記録request ID、以前のpackage revision、operation、reason、versionからcanonical actionを再構成する。Request digestとevent時点の記録actor権限を検証し、完全な期待次stateを再構成してstateと完全resultを比較する。Flag変更に所有者・credential・archive・quarantine・別packageの変更を紛れ込ませられない。全commit済みgeneration/event/objectを保持し、全mutation種別をまたぐrequest key重複を拒否する。

HTTP応答はRFC 0046のcanonical `{schema:1, registry, request_id, revision, audit_sequence, outcome}`を維持する。Revisionは対象package revision、audit sequenceはcommit済みstate revisionとする。Yank/unyank outcomeは`yanked`/`unyanked`で、replayにも元の結果を使う。内部の型付きcommit/replay区別で初回publishの201とreplayの200を判別し、古い呼出元indexから推測しない。Actionは200を返す。

### 上限付きservice統合

Private registry capabilityと独立設定したservice identity/時刻sourceを所有する、明示host service設定でのみmutationを有効にする。既存read-only入口は保持する。Bearerは明示client secret入力と単一認証headerからだけ取得し、環境変数・redirect・fallback origin・保存actor認可を追加しない。公開GETは引き続きcredentialを拒否する。PUT archive identityは公開前に検証済みrouteと厳密一致し、action POSTは厳密canonical bodyとJSON content typeを必要とする。

RFC 0046/0048のframing・deadline・最大8接続・admitted request合計256 MiB memory budget・非blocking mutation直列化・最新locked authority・captured read-generation規則を維持する。受理body・parse済みpayload・応答copy・保持/新read dataを割当て前に計上し、古いdataはdropまで課金を保持する。Commitしたmutationと一致する検証済みread generationを、次の応答がservice stateを利用する前に公開する。Commit後の再構築/検証に失敗した場合、commit済み結果と同一要求照合を保持し、明示recoveryまで以後のservice readを拒否する。古いcaptured HEADでserviceを継続したり、commit後にrollbackしたと主張したりしない。実peak計上と並行read/mutationは実装受入条件として残る。

## 実行mode

Yankが変えるのはregistry選択metadataであり、source実行ではない。Quarantineされていない厳密locked archiveは、別途必要な消費証拠の下で引き続き利用できる。VMとnative AOTは同じ保持sourceを実行し、JIT・component・embedding ABIを変更しない。E02はsourceを置換せずhost publish → yank → 厳密archive read → unyankを示し、E05は認証HTTPS経由で同じjourneyを検証する。

## ABIと互換性

影響domainはpackageとtooling。既存source edition・MIR・bytecode・component/native ABI・archive/lock formatは変わらない。State/audit/receipt schema番号は1のまま、閉じたvariantを明示追加し、旧readerはそれを含む履歴を拒否する。最初のyank前にprivate serviceを更新する。Downgradeには新しい互換readerの保持または明示復元が必要で、event破棄・flag解除・HEAD resetでは代用しない。稼働中の公開registry移行は想定しない。

## 診断とtooling

REG0001 malformed、REG0002 limit、REG0003 authentication、REG0004 authorization、REG0005 conflictと、既存not-found・busy・integrity/state・transport mappingを維持する。上限付き固定診断にbearer byteを含めない。新しいsource parser・formatter・lint・LSP・debugger動作は不要。HTTP request IDは安全に受理した場合だけ返し、応答喪失時は厳密な同一要求照合を必要とする。

## 性能とcost model

受理actionは完全な不変stateとaudit recordを各1つ、staged HEAD byteを追加し、source archiveは追加しない。全履歴検証・canonical encoding・generation保持はRFC 0048の上限付き初期profile costを維持し、throughputは主張しない。Read-only captured generationを可変authority cacheに転用しない。対応deliveryの完了前に実host fixtureとresource上限を測定する。純粋な遷移実験ではHTTP memory・latency・crash動作は証明できない。

## Securityとtrust境界

Actionやreceiptはcapabilityではない。実bearer所持、現在の登録/所有者/scope、信頼時刻で全commit/retryを制御する。保持archive hashはflag変更による内容置換を防ぐが、package著作者を証明しない。Hash chain履歴はHEAD基準の破損を検出し、悪意あるoperatorのrollbackは検出保証しない。敵対的reason・body framing・request衝突・失効/除外actor・不正履歴遷移は、未知dataをcleanupせずfail closedにする。

## 代替案

- **変更しない:** yank mutationとaudit/履歴契約を実装できないまま残す。
- **外部registry/tool:** 類似actionを提供できても、このprofileのreceipt・不変local state・最新replay権限を定めない。
- **別の可変flag/index file:** 書込みは小さいがpackage revisionとauditのatomicな一致を失う。既存single-HEAD generationがより強い設計である。
- **新IDによる冪等no-op:** CLI意図の繰返しを簡略化できるが、新しいaudit/revision結果の仕様が必要。同じflagを拒否すれば受理eventと実状態遷移が1対1になり、厳密IDで安全な再試行は既にできる。
- **Yanked release削除またはarchive書換え:** 厳密lock replayと不変identityを壊し、RFC 0046に反する。

## 実装計画

2026-10-31のscope監査: E05の認証HTTPS統合前に、E06でguard付きstore作業memoryと有界read世代captureを実装する。[RFC 0053](0053-registry-request-workspace.md)は監査した保守的予約modelをacceptし、本番適用をE06/E05完了条件に残す。本RFCのaccepted budgetや受入要件を緩和しない。

E03具体化: 2026-10-31 JST acceptedの[RFC 0052](0052-registry-owner-and-credential-control.md)がowner追加・削除・読取り、2段階移管、独立local credential失効を定義する。本transaction/replay境界を維持し、E05が認証HTTPS統合を提供する。

1. D05を唯一のdoing deliveryに維持し、検証headを固定したまま本具体化を受理・準備する。2026-10-31 17:57 JSTのscope監査で、host transactionにE02・3 pointを維持し、認証HTTP統合を実装前にE05・3 pointとして明示する。受入条件は削除しない。
2. 上限付きaction・audit/result variant・完全履歴遷移検証・最新認証yank/unyank transactionを追加する。不変公開と既存API動作を保持する。
3. E03のowner transaction後、E05で明示認証publish/action/ownerを上限付きservice/clientと最新read generationに統合し、実HTTPSでresource計上・競合・再試行結果を検証する。E05はE04の完全な権限外操作/recovery journeyの前提とする。
4. 各実装deliveryで局所fixture・全baseline・最新headの両host CIを実行し、証拠を公開してG5の残作業を再監査する。E02だけでE05やG5を完了とはしない。

Branch準備: `codex/m5-registry-mutations`は`carimatics/lang`の`codex/m5-registry-offline-cache`、`43dea74`から作成する。PR #38が依存先かつ記録した初期PR baseである。Work unit開始branchは引き続き`main`で、新checkoutに既存変更はなかった。依存先merge後にのみ`main`へretargetする。

## Test計画

- 実bearerと未変更archiveによるpublish・yank・unyank、古い/存在しないversion、同一flag、誤scope/name/owner、不変置換拒否。
- 逆action後の厳密replay、reason/body変更・ID衝突、最終HEAD guardを含む最新expiry/time/ownership検査。
- 独立canonical action/request-digest vector、reason byte/control境界、不正/重複/未知field、入力上限。
- Flag・authority・archive・quarantine・別release・audit result・replay digestを変える完全遷移偽造test。
- 両hostで実初期化/公開/action process終了、reopen後の完全old/new state、結果不明の照合、並行request順序。
- E05: HTTPS publish/action/read journey、mutation無効service、不正framing/auth/conditional header、遅い/超過body、admission budget境界、worker取消し。
- Flag journeyを通じた厳密download archiveのVM/native実行。本番未署名消費constructorは追加しない。

## 未解決事項

Host E02公開checkpoint（2026-10-31 JST）: `f40d643`は全2,655 test / 182 suite、fmt・厳密Clippy・rustdoc、CI `41188137283`の対象18 job、native-debug `41188137315`の3 hostに成功した。PR #39を`f74428c`でmergeし、元mainのclean・0/0同期・検証tree一致を確認した。本RFCはE05の認証HTTPS統合を含むためacceptedを維持し、host完了をservice完了と扱わない。

Local E02実装checkpoint（2026-10-31 JST）: 上限付き`YankAction`と`Registry::yank`が厳密archiveを保持しながら完全な監査済みflag遷移を公開する。実host publish → yank → reopen → 別version公開 → unyankでVM/nativeが42を実行し、状態をまたぐretryにも元receiptを返す。Store 30 caseが成功し、実yank/unyank process中断36境界（旧state 34・完全新state 2）、publish/yank writerの両順序、最新最終時刻guard、明示保持したHEAD byteによる結果不明照合、実nonowner/失効/誤scope credential、再計算hash付き履歴偽造9 caseを検証した。Indexの厳密上限公開後に+1 byteとなるunyankは書込み前に拒否する。独立canonical vector、512入力変異、厳密workspace Clippyが成功し、既存authority fuzz入口にaction readerを追加した。Host実装の証拠であり、固定全検証・最新headの両host CI・公開はE02受入条件に残る。E05の認証HTTP/作業memory budget/read世代統合は未実装である。

2026-10-31 JSTにRFC 0001の実験段階委任でaccepted。純粋な`/private/tmp/lang-m5-yank-policy-probe.js`は、flag/revision変更、逆action後の元結果retry、ownership/time拒否、不変digest保持、reason上限の22 caseに成功した。Canonical fixtureは173 byteで、action SHA-256は`d944ece5a45d67c089f6a0b81bfd9415c4fe5a23e11355e1b3ed90562d1c5dc1`、request SHA-256は`b61f815bee0a02abd24cd9e29e3ac22510c4b506e42b241692a0c3ec57ce6e47`。独立Python encodingも一致した。これはpolicy/encoding証拠だけであり、実認証・filesystem・transport・crashの証明ではない。

単一flag遷移、元結果retry、未変更archiveは既存accepted契約に適合する。受理時にはservice memory計上・可変read-generation公開・最終時刻動作・process中断・Windows統合が未検証だった。上記checkpointが後続のlocal host検証を記録し、固定全検証・両host公開とE05のservice統合は残る。1つの観測可能deliveryを超える場合は実装前に再見積りまたは前提deliveryを明示する。このacceptanceで性能やgate完了は主張しない。

## 出典

- [RFC 0046](0046-registry-protocol-and-recovery.md)、更新2026-10-31: HTTP action・request identity・authority・recovery profile。
- [RFC 0047](0047-registry-authority-records.md)、更新2026-10-31: 現在のyank grantとowner policy。
- [RFC 0048](0048-registry-state-and-publication.md)、更新2026-10-31: generation・audit・最新commit/replay契約。
- [RFC 0049](0049-registry-package-integration.md)、更新2026-10-31: 厳密registry依存実行境界。
- [RFC 0050](0050-registry-cache-and-offline-replay.md)、更新2026-10-31: 不変cache generationとoffline replay。
- 上記local遷移/encoding実験、実行2026-10-31 JST。Project判断の証拠であり、外部性能/securityの主張ではない。
