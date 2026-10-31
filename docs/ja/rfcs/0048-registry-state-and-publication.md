<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0048-registry-state-and-publication.md -->

# RFC 0048: レジストリ世代と不変公開

[English](../../rfcs/0048-registry-state-and-publication.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-D02, M5-E02–E04
- Supersedes: none; elaborates RFCs 0046–0047
- Superseded by: none

## Summary

不変公開の前提となる内部stateとauditの形式を定義する。1個の`HEAD`置換で完全なrelease・現在owner・auditを同時に可視化する。公開者は実bearerと厳密request identityを渡し、storeは非blocking排他lock内でcommit状態を再読込・検証してから認可やreplayを行う。

## Motivation

D01は不変source bytesを受理し、E01は上限付きauthority recordを検査するが、どちらもregistry releaseをcommitしない。Indexとowner/audit fileを独立に置換すると、中断後に不完全transactionを公開し得る。キャッシュした主体は失効後も残り得る。本提案は別のcommit点を追加せず、採択済み境界を結合する。

## Goals

- 新しい厳密`(registry, name, version, archive digest)`を一度だけ公開する。
- 1個のlock内で認可・expected revision・冪等性・公開を直列化する。
- 有限のcommit履歴を検証し、古いheadを選ばず破損を拒否する。
- 両対応hostの実process終了境界で、完全な旧状態か新状態だけが見えることを示す。

## Non-goals

- HTTP endpoint、本番署名、依存解決、source実行。
- 電源断後の可用性、悪意ある運用者のrollback対策、internet規模のthroughput。
- Owner/yank/revocation commandのworkflow。E02–E04統合として残る。

## Guide-level design

明示的に許可したprivate registry directoryを、独立したservice identityと管理者が登録したauthority recordで一度だけ初期化する。既存destinationへの初期化は失敗する。`parser` version `1.0.0`の新規名取得には、受理済みarchive・bearer・request ID・名前不在条件を渡す。後続versionには現在のpackage revisionが必要。同じdigestで成功request IDを再利用すると、現在の認可後にのみ元receiptを返す。異なるbytesへの再利用は競合する。公開済みversionを置換する操作はない。

## Reference semantics

### Private layout and encoding

Private rootにはregular `LOCK`、regular `HEAD`、固定の`objects`・`states`・`audits`・`staging` directoryのみを置く。不変recordのfile名は接尾辞なし64文字小文字hex digest。Staging名は厳密に`object-HASH`、`audit-HASH`、`state-HASH`、`head-HASH`のいずれかで、HASHはcandidate objectのdigest（HEADでは参照stateのdigest）。Stagingは各上限内の不完全bytesを含めるが、commit stateからは参照しない。Stored stringから任意pathを作らない。全JSONはRFC 0046のclosed・順序付き・整数のみ・canonical形式。Symlink、非regular file、hardlink alias、不明名、予期しないsubdirectoryを拒否する。総managed bytesは4 GiB、entryは262,144、recovery passごとの列挙は上限付き1回を維持し、割当・書込み前に算術を検査する。

`HEAD`は最大128 byte、フィールド順は`{schema:1, state_sha256}`。参照stateは最大16 MiB:

| Field | 意味 |
| --- | --- |
| `format`, `schema` | `"lang-registry-state"`、整数`1`。 |
| `registry`, `origin` | 独立した設定service identityと一致。 |
| `revision`, `time_floor` | 正のstate revisionと最後にcommitした信頼時刻。 |
| `authority` | 厳密なRFC 0047 object。identityとtime floorは外側stateと一致。 |
| `packages` | 重複しない名前順record、最大1,024。名前集合は`authority.packages`と完全一致。 |
| `last_audit_sha256` | sequenceがこのstate revisionに一致するeventのdigest。 |

Package recordは`{name, revision, releases}`。Package revisionは1から開始し、commitしたpackage変更ごとに1回増える。Releaseは数値exact-version tuple順とし、RFC 0046のindex releaseフィールドを規定順で持つ: `version`、`archive_sha256`、`archive_bytes`、`descriptor_sha256`、`edition`、`dependencies`、`yanked`、`quarantined`。最大128 release/name、総8,192。新規公開の両flagはfalse。Metadataは対応する受理済みarchiveと厳密一致し、派生indexを独立した書込み可能authorityとしない。

Audit recordは最大2 KiBで、順序付きフィールドは`format: "lang-registry-audit"`、`schema:1`、`sequence`、`previous_audit_sha256`、`prior_state_sha256`、`time`、`principal`、`credential_id`、`operation`、`request_id`、`request_sha256`、`reason`、`result`。ID/hashはRFC 0046–0047の形式。最大65,536 event、sequenceは正。Reasonは最大256 UTF-8 byteで制御文字を禁止し、secret/source/pathを含めない。D02は以下のclosed tagged operation/result組を使用する:

| Operation | Result |
| --- | --- |
| `{kind:"initialize"}` | `{state_revision:1}` |
| `{kind:"publish", name, version, archive_sha256}` | `{state_revision, package_revision, name, version, archive_sha256}` |

初期化のsequenceは1、previous/prior hashとcredential/request fieldはnull、principalは管理者が選ぶ有効な登録ID。初期authorityに取得済みpackageはない。公開ではprevious/prior hash・credential ID・request ID・request digestをnon-nullとし、principal/credentialは最新E01認証から得る。Resultのstate revisionはsequenceと一致。D02のreasonは固定`initialize`または`publish`で、任意caller textを受け取らない。後続E02–E04 operation variantは実装前の明示RFC具体化を要し、旧readerは未知variantを推測せず拒否する。D02のreceiptは`{schema:1, audit_sha256, result}`で、package署名を主張せず、生成state hashへの循環参照を含まない。

### History validation and reachability

State → last audit → prior stateで有限の後方chainを構成する。各state/auditを厳密digestとcanonical形式で読み、identity・revision・package/authority record・非減少時刻の整合性を確認する。State time floorはそのevent時刻と一致。初期化以外のeventは前stateのlast auditを参照し、後方走査のsequenceは厳密に1ずつ減る。Revision 1が一意の初期化終端。欠落・重複・不整合linkを拒否する。Commit storeを信頼する前に、到達可能な全releaseを厳密source archiveと照合する。

Revision field一致だけでなく、完全な遷移を検証する。Publish eventで変更できるのは、宣言release 1件の追加、そのpackage revisionの加算（または1で開始）、新規名の場合だけpublisherを初期ownerへ設定すること、state/authority time floorとaudit linkの更新である。他のpackage・owner・principal・token・scopeはcanonical dataとしてbyte等価を維持する。前package revision（初回はnull）と厳密archive identityからRFC 0046のrequest digestを再構成する。過去principal/credentialはevent時刻の前stateで有効かつ必要scopeを持っていなければならない。これは保存した主張の検査であり、旧secretの所持や悪意あるoperatorに対する真正性の証明ではない。現在requestには引き続き実bearerを要求する。未記録の権限変更がpublish eventに紛れ込むことを防ぐ。

**採択した履歴方針:** このchainから到達できる全commit state generation・audit event・source objectを保持する。Prior-state hashは単なる未監査の観測値ではなく、実際の到達辺である。過去state分の容量を使い、event件数上限より前に4 GiB quotaへ達する可能性があるが、初期profileでのpruningや暗黙trust resetを避ける。Commit済みversion/auditを自動削除しない。

Cleanup前に完全な上限付きinventoryとcommit chainを検証する。Recoveryが削除できるのは、HEAD公開前に残った、認識可能で上限内のregular staging file（部分書込みを含む）と、完全でdigest検証済みかつ未参照の不変candidateだけである。不変candidateの破損、alias、未知entryではcleanupを拒否する。Candidate head採用、欠落HEAD再生成、旧generationへのrollbackはしない。RFC 0046のorphan cleanupをstateだけでなく未公開object/auditへ明示具体化し、削除実装の前に英日RFCの改訂へ採択を記録する。

### Transaction and replay

1. 許可rootをno-follow/regular-file検査付きで開き、`LOCK`を非blocking取得する。子processへ保持lockを継承せず、既存installer同様にrelease時に明示unlockする。
2. `HEAD`を再読込し、完全な上限付きcommit viewを検証して新しい信頼host時刻を選ぶ。Floorより前への巻き戻りを拒否する。
3. 実bearerを再認証し、現在authorityに対して名前操作を認可する。名前不在ならclaim+publish、存在ならpublishを選ぶ。保持した監査actorを入力権限にしない。
4. RFC 0046のcanonical request digestを使い、保持audit履歴中のprincipal/request-ID組を照合する。同digestなら元receiptを返し、異なるdigestなら競合。既知成功IDでも現在の権限が必要。
5. 新requestではexpected package revision・不変version identity・archive/request/service対応・peak count/byte quotaをすべて確認する。固定`AdmittedArchive`が検証済みbytesを供給し、そのproofと別に呼び出し側の置換bufferを受け取らない。
6. 新candidateをprivate stagingへcreateし、全bytesを書込みfileをsyncしてから同filesystemの不変namespaceへ移動する。最終不変名へ直接書かない。そうするとprocess終了で破損と区別できない不完全candidateが残るためである。既存不変digest fileは厳密bytes一致を要求し、置換せず再利用する。協調writer間の検査と移動はlockで直列化する。参照する全不変file配置後にstagingへHEAD candidateを書込みsyncする。
7. HEADを原子的に置換し、唯一の可視化点とする。その後にreceiptを返す。応答喪失でも成功し得るため同request identityでretryする。置換付近のI/O errorではHEADを再読込し、完全な新headが証明できればcommit receipt、旧headなら未公開、どちらも確定できなければ明示的な公開結果不明を返す。不明を確実なrollbackとして報告しない。これは構造化host errorの区別であり、RFC 0046のwire error形状は変えず、照合が必要な場合を静的messageで説明する。

公開readは1個の検証済みcommit stateを取得し、参照済みで非quarantineのobjectだけを公開する。Staging・未公開contentはhashだけでは取得できない。Yank済みexact versionはRFC 0046どおりaddress可能。D03がindex/download APIを提供し、D02は一般filesystem interfaceを公開しなくてよい。

実装の明確化（2026-10-31 JST）: 受入時に信頼するhost時刻を読む。Archive staging後の再認証でevent時刻を選び、event/state floorへ保存する。Event・state・HEAD candidateをsync後、最終guardは時刻を再取得し、event時刻未満への逆行を拒否してHEAD置換直前に再認証する。既に受理したevent timestampは書き換えない。これによりmetadata staging中の失効を拒否し、時刻取得とfilesystem置換が同一瞬間という主張はしない。3回の検査すべてで実bearerの所持と同じlock下のcommit済みauthorityを使う。

初期化は新規private destinationを使い、bootstrap event/state完成後にだけ最初のHEADを公開する。中断した不完全destinationを黙って再初期化しない。明示的なoperator cleanup/restorationは通常recoveryと区別する。暗黙credential・registry・root・cache fallbackはない。

## Execution modes

このhost storage層はguestを実行せず、VM/native意味を変更しない。対応macOS/Windowsで実child-process中断・file lock・置換・再open状態を検証する。Local macOS prototypeはWindows証拠ではない。

## ABI and compatibility

影響領域は`package`・`tooling`。新しい内部版付きstate/audit/receipt形式はsource edition・MIR・bytecode・native ABI・component ABIを変更しない。本提案が移行する配備済みregistry stateはない。未知schema/operation variantを拒否し、将来の移行では復旧・downgrade影響を明示する。

## Diagnostics and tooling

RFC 0046のREG0001–REG0008を使用する。認証と認可の失敗を区別し、古いrevision・異なるrequest identity・version置換は競合、commit履歴破損はstate/trust failure、lock競合はbusyとする。診断は静的操作labelと安全なIDのみで、任意payloadやsecretを含めない。Reportは観測であり、後続変更への権限ではない。

## Performance and cost model

初期検証は総保持bytes/recordに対するO(n)で、物理storage quotaにより有限となる。履歴、archive hash、canonical decodeには実費用がある。新generationは上限付きstateをcopyし、全source objectはcopyしない。Peak quotaはHEAD公開前の旧履歴と新candidateを含む。Throughput・大規模registry容量の主張はしない。全履歴保持は単純さを優先する判断であり、測定で不適切と判明した場合は採択済みformat/migration変更を通じて見直す。

## Security and trust boundaries

Directory capabilityは信頼hostのstorage accessであり、guest filesystem accessではない。協調lockはregistry操作を直列化するが、directoryを直接操作できる悪意あるoperatorをsandbox化しない。Digest chainは保持HEADに対する破損を検出し、全stateを運用者がrollbackする攻撃は防がない。Package来歴/freshnessはF01。Archive解析が先行した場合や過去に成功したrequestでも、lock内の最新認可を要求する。

## Alternatives

- **変更なし:** byte受入とpolicyだけでは完全なregistry stateを公開できない。
- **トランザクションDB:** transactionを提供できるが、engine・migration/backup境界・配備前提が増える。初期profileの実測費用が必要性を示した場合に再検討する。
- **Index/owner/auditを別々にatomic file化:** 各fileがatomicでも、中断後に全体の可視性が不整合になり得る。
- **現在stateとhash-only履歴のみ:** 容量は減るが、audit/recovery到達性とcheckpoint意味の明示が必要。最初のprofileは固定quota内の全commit世代保持を提案する。

## Implementation plan

1. 厳密record・全履歴費用・orphan cleanupをレビュー/採択し、D02コード前にRFC 0046英日を更新する。
2. E01の全検証まではE01だけをdoingに保つ。D02は既存3 ptを維持し、新しい前提を追加しない。
3. D01/E01を囲むprivate bounded storage・初期化・不変publish/replayを実装する。実reopen/process終了と競合writerの両host testを追加する。
4. D03でindex/downloadを公開し、D04/D05で実registry依存を実行・replayする。E01/D02後は水平前提を増やす前にこの実行順序を再監査する。

## Test plan

- 初回claim、次version、同一retry、変更retry、古いrevision、置換試行。元receiptと不変bytesを検証する。
- 全write/sync/HEAD切断点でparentが再openし、完全な旧/新release・owner・auditを検証する。
- 独立writer handle/process 2つ、busy拒否、競合公開の両順序。
- 異なるservice、偽造/古いactor、不正credential、scope/owner失敗、時刻巻戻り、実装済み変更が存在する場合の介在state変更後の最新policy。
- 全physical/record/byte/count上限。HEAD・link state/audit/object破損、不正canonical field、未知entry、symlink/hardlink/nonregularをcleanupせず拒否する。
- 有効commit-chain証明後だけのorphan cleanup。欠落HEAD reset、最大candidate採用、履歴pruningなし。

## Open questions

実装checkpoint（2026-10-31 JST確認）: `e76dd7b`の`registry::store`は全baselineとmacOS/Windows CIに成功した。Bootstrap 22・公開24のprocess終了切断点、writer process競合、reopen後の厳密source実行、replay/最終認可、公開結果不明の照合、敵対的recovery/quotaを含む。Windowsで排他lockを保持している間のlock file検証は、別handleからのEOF読取りではなくmetadataを使う。[M5 board](../milestone-5.md)に厳密なrunと残るPR公開を記録する。電源断可用性は非目標であり、HTTP/署名・本番消費ではない。以下の初期採択記録は履歴である。

2026-10-31 JST、RFC 0001の実験段階委任により採択。レビューで不変名への直接書込みではprocess終了後に部分contentが残り得ると分かり、採択した手順では先にstagingとsyncを行う。拡張した`/private/tmp/lang-m5-store-crash-probe.py`はlocal macOS 27.2で実process終了13切断点に成功した。Object/audit/state/HEADの部分書込みを含み、cut 0–11では完全なrevision 0を維持、cut 12では完全なrevision 1を公開し、配置済み不変digestは全件一致した。可視stateの完全性を証明した後に、認識可能なorphan staging/candidateを削除できた。以前の全書込み後6切断点probeを、このより強い実験で置き換える。

全履歴保持は保守的で上限付きの初期profile費用として採択し、容量保証とはしなかった。完全な遷移検証と明示的な不明結果により、state/audit/errorの残る曖昧さを解消した。採択時にはWindows置換、並行reader/writer、完全な敵対的recovery、実認証統合、電源断挙動が未検証だった。Prototype単独ではD02を完了せず、E01はその全検証まで唯一のdoingだった。上記実装checkpointが後続の両host process/concurrency証拠を示し、電源断可用性は非目標を維持する。

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md)と[RFC 0047](0047-registry-authority-records.md)、最終更新2026-10-31: 採択済みproject要件。
- [POSIX.1-2024 rename](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html)、2024版、個別page更新日不明、2026-10-31 JST確認。Atomic renameの意味を定めるが、cross-platform電源断可用性の保証ではない。
- [Microsoft MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)、最終更新2023-06-01、2026-10-31 JST確認。Platform置換flagは実adapter検証を要し、このpageだけでは本storage実装の証拠にならない。
- [Rust File](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)、公開・更新日不明、2026-10-31 JST確認。Lock相互作用とclone handleの振る舞いはplatform依存。

上記record形式・全履歴保持・cleanup規則はproject判断である。Prototypeの観測と未検証のplatform/durability前提を分けて示した。
