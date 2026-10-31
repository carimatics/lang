<!-- translation-of: ../../rfcs/0055-registry-local-administration.md; canonical-language: en -->

# RFC 0055: Registryのローカルlifecycleとscope付きowner復旧

[English](../../rfcs/0055-registry-local-administration.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E08, M5-E04
- Supersedes: none; RFC 0046–0048と0052を具体化
- Superseded by: none

## 概要

明示的なlocal registry administratorに、監査付きprincipal登録・credential hash追加・principal無効化を追加する。別途構築し、厳密なpackage scopeを持つrecovery administratorは、そのpackageのownerをactiveな登録済みprincipal 1人へ原子的に置き換えられる。HTTPやbearer credentialからは利用できない。既存の不変archiveとsource実行を維持する。

2026-10-31 JSTにRFC 0001の実験期委任でacceptedとする。RFC 0046が要求するlocal lifecycleを満たし、既存のlocal directory capabilityと再構築可能なtransaction履歴を保ち、owner全128枠が埋まっていても緊急復旧を可能にする設計である。外部policy/encoding実験の23 checkで提案する権限/replay規則とrecord sizeを確認したが、本番適用・crash安全性・platform動作の証拠ではない。これらは実装受入条件として残す。E07公開完了までE08はtodoを維持する。

## 動機

RFC 0052はowner membership変更とlocal credential失効を可能にするが、後からprincipalを登録してcredentialを追加できない。通常の移管にも現在認可されたownerが必要である。全owner credentialを失ったoperatorには、authority file編集や履歴書換えによらない、scopeと監査のある復旧経路が必要である。追加するgrantは2 KiB audit-record上限を超え得る。実験の128-grant credentialはcanonical形式で15,694 byteであり、digestで結合したaudit operationは179 byteであるため、完全recordを有界authority stateに保持できる。

## 目標

- 明示local capability・現在operator権限・単一の永続transaction protocolで登録・追加・無効化・復旧する。
- 追記専用identity・共通request identity・厳密replay・全履歴検証を維持する。
- Local復旧後の通常認可による公開と、VM/nativeで同一sourceを実行するjourneyを示す。

## 非目標

- 公開登録・token発行・秘密保存・HTTP管理endpoint・principal再有効化・ID再利用。
- 通常の2段階owner移管、quarantine、release byte、package trustや署名の変更。
- 汎用identity provider、online local/service writer多重化、新たな言語意味論や公開native ABI。

## 利用者向け設計

Registry serviceを停止してwriterを解放した後、信頼されたdirectory capability・service identity・設定済みactive operatorを渡してlocal administratorを開く。Bを登録してから、別途生成したcredentialのSHA-256 authenticatorと明示grantを追加する。登録だけではpackage所有権もcredentialも与えない。追加操作は生bearerを受け取らず、生成もしない。

Package所有権の喪失・侵害時は、同じ明示local入力に厳密package名を加えて`RecoveryAdministrator`を構築する。現在のstate/package revision、active B、新request ID、理由を持つ`OwnerRecovery`を送る。単一transactionでowner一覧を`[B]`にし、以前の全ownerを除く。128-owner上限でも動作する。既にBだけならconflictであり、複数owner中のBへ一本化する復旧はできる。Bがpublishするには、別途追加したname-bound grantが引き続き必要である。

Principal無効化は、影響を受ける全packageに別のactive ownerを残した後に行う。設定された実行operatorは自分自身を無効化できない。既存owner entryとcredential recordは履歴に残し、inactive principalの認証を拒否する。再登録・再有効化も拒否する。行き詰まったowner一覧は先に復旧で置き換えられる。

Commit結果が不明なら明示capabilityで再openし、同一requestを照合する。保存済み追加/復旧receiptの成功は、後から失効したcredentialや古いowner一覧を復活させない。Administratorのwriterを解放してからHTTPS serviceを再開する。

## 参照意味論

### Capabilityと閉じたaction

`Administrator`に登録・追加・無効化methodを追加するが、`Registry`・bearer token・保存JSON・booleanからの変換は提供しない。`RecoveryAdministrator::open`はlocal親`Dir`・child・`ServiceIdentity`・operator・canonical package名1つを別途必須とする。内部所有するregistryは公開せず、通常registry accessやより広いadministratorへ変換できない。Name scopeは新transactionだけでなくreplay前にも確認する。HTTP route・bearerの`admin` grant・action bodyからこれらのcapabilityを構築できない。

Actionは受理済みの不透明なRust host値とする。Canonical JSONのfield順は厳密で、未知/重複fieldを拒否し、schemaは1とする。全actionで既存の32桁小文字hex request/principal/credential ID、検査付きrevision、空でなく制御文字を含まない最大256 byteのUTF-8 reasonを使う。Field順は次の通り。

| 型 | 順序付きfield | Canonical byte上限 |
| --- | --- | ---: |
| `Enrollment` | `schema,request_id,expected_state_revision,action,reason,principal` | 2,048 |
| `CredentialProvision` | `schema,request_id,expected_state_revision,action,reason,credential` | 65,536 |
| `Deactivation` | `schema,request_id,expected_state_revision,action,reason,principal` | 2,048 |
| `OwnerRecovery` | `schema,request_id,expected_state_revision,expected_package_revision,action,reason,name,principal` | 2,048 |

対応action文字列は`enroll_principal`・`provision_credential`・`deactivate_principal`・`recover_owner`である。`credential`はRFC 0047の既存token record全体で、canonical順`id,principal,registry,origin,authenticator_sha256,issued,expires,revoked,grants`を変えない。受理時に構造・深さ・grant・scalarを制限し、state依存検査は新requestだけに適用する。このlocal 64 KiB上限はHTTP actionやaudit-record上限を拡大しない。

既存request-digest tupleのmethodは`LOCAL`、conditional revisionは`expected_state_revision`、bodyはcanonical digest/lengthとし、次のpathを使う。

| Action | Path |
| --- | --- |
| 登録 | `/v1/admin/principals/ID/enroll` |
| 追加 | `/v1/admin/credentials/ID/provision` |
| 無効化 | `/v1/admin/principals/ID/deactivate` |
| 復旧 | `/v1/admin/packages/NAME/recover-owner` |

これらはdigest-domain識別子であり、HTTP endpointではない。復旧のexpected package revisionはbody内にも結合する。

### 現在権限・replay・遷移

全actionは既存の排他writer下で現在の全履歴を再open・検証し、設定operatorが現在activeであることを確認し、永続floor以上の新しい妥当clockを取得して、厳密復旧scopeがあれば確認する。その後、共通`(operator, request_id)` mapを調べる。同じdigestなら元receiptを返し、異なればconflictとする。この照合は新targetのstateとcredential有効期間の検査より先に行う。このため無効化・期限切れ・失効・別owner変更後の認可済みreplayは、遷移を再適用せず旧証拠を返す。Inactive operatorが旧証拠で権限を取り戻すことはできない。

新IDではexpected state revisionと現在revisionの一致を必須とする。Revisionとtime floorは既存の検査付き規則で進め、対象の条件は次の通り。

- 登録: principal IDが一度も存在していないこと。Active principal 1つをcanonical順に挿入する。Credential/packageは変えない。
- 追加: 登録済みtargetが現在activeで、新IDとauthenticator digestが失効済みも含め一度も現れていないこと。設定registry/originの厳密一致、`revoked == false`、canonicalな1–128 grant、妥当範囲、発行から期限まで最大7日、`issued <= now < expires`を必須とする。既存のprincipal 4,096件・token 4,096件上限を維持する。Credentialを変更せずcanonical順に挿入し、packageは変えない。
- 無効化: targetが存在してactiveであり、実行operatorと異なること。そのactive bitだけを変えても全packageにactive ownerが1人以上残ること。全credential field・owner・package revisionを保持し、token revocation bitを暗黙に書き換えない。
- 復旧: body名とcapabilityの厳密scopeが等しく、packageが存在し、両revisionが一致し、targetが登録済みactiveであること。対象owner一覧だけを`[target]`に置換してpackage revisionを進め、Bのみのno-opを拒否する。別owner・release descriptor/flag・archive・credentialを維持する。Revision桁増加を含め、派生indexの厳密size preflightを適用する。

全candidateで完全authority/state上限、保持object計上、audit容量、storage quotaをstaging前に満たすこと。登録・追加・無効化はpackage revisionと派生index/archive byteを変えない。既存stage/sync/install/final-guard/HEAD手順を共用する。最終guardで現在operator権限とclockを再取得し、新追加transactionではそのclockでもcredentialが有効であることを必須とする。Cancellation・conflict・recovery-required・unknown-result動作は変更しない。暗黙retry、orphan採用、最大generation選択、rollback-floor resetは追加しない。

### Auditと履歴再構築

これらlocal operationの共通audit credential IDは初期化/local失効と同様nullとし、bearer operationは引き続き実credential IDを必須とする。Request ID/hashとreasonは必須。閉じたoperationと固有形状のresultは次の通り。

| Operation field | Result field |
| --- | --- |
| `kind:enroll_principal,principal` | `state_revision,principal,active:true` |
| `kind:provision_credential,credential_id,record_sha256` | `state_revision,credential_id,principal` |
| `kind:deactivate_principal,principal` | `state_revision,principal,active:false` |
| `kind:recover_owner,name,principal` | `state_revision,package_revision,name,recovered_principal` |

`record_sha256`はaction全体でなく厳密canonical token recordをhashする。2 KiB audit recordに完全credentialを埋め込まない。履歴検証ではcandidate stateの一意な追加tokenを取得し、canonical record hashを検証し、完全actionとdigestを再構築した後、許可遷移に照らして次state全体とresultを比較する。他operationも以前のstate・operation field・reasonから同様にbodyを再構築する。以前のactive local operatorと記録時刻、全target規則、共通request mapを検証する。Resultやhash chainの検査だけでは不十分である。全state比較で無関係なprincipal・grant・release・owner・floor変更を拒否する。

新local result形状は既存untagged result variantと区別できなければならない。特に復旧で`OwnerResult`を流用せず、全local operationでHTTP `package_result` projectionを提供しない。このauditは信頼されたlocal-host model内の整合性証拠であり、人間のdirectory capability保有を証明する署名ではない。

## 実行mode

Host操作はparse・型・effect・MIR・source span・bytecode・native実行・callback・定数評価を変えない。認可利用に復旧したregistry archiveはVM/native AOTで同じsourceを実行する。JIT・Wasm・component-host capabilityは追加しない。非信頼scriptはdirectoryやlocal管理handleを取得できない。

## ABIとcompatibility

既存credential・authority・audit schema番号と旧canonical encodingを維持する。閉じたprivate operation/result variantを追加し、古いreaderはそれを含む履歴を拒否する。初回新operation前にserviceとlocal administratorを一緒に更新する。Downgrade/履歴書換え/自動migrationは提供しない。新Rust host APIはexperimentalで、安定C/component ABIや言語edition変更はない。通常owner移管request/receiptの意味は維持する。

## 診断とtooling

既存の有界構造化registry admission・unauthorized・conflict・capacity・busy・recovery-required・unknown-result failureを使う。Debug出力/errorからcredential record・authenticator・無制限bodyを除く。Serviceや別administratorがwriterを保持していればqueueせずbusyを返す。新parser・formatter・LSP・DAP・秘密を含むCLI optionは不要で、host lifecycleと明示復旧scopeを文書化する。

## 性能とcost model

有界な全履歴検証と単一writer transactionを再利用する。追加action受理はlocal 64 KiB上限まで割当てでき、candidate authorityは既存aggregate上限を維持する。Token 1件のhashと有界principal/credential/package走査は無制限collectionを追加しない。このprofileでは失効tokenとinactive principalも恒久的に上限へ計上する。Standalone local APIはRFC 0053の明示local capability意味論を保ち、service requestにならず、network serviceの共有memory計上を迂回しない。Policy実験からlatency/throughputを主張しない。

## Securityとtrust境界

信頼hostがlocal directory権限を明示付与し、operator/scopeを設定する。Storageを書き換えられる悪意あるlocal operatorは暗号学的trust modelの対象外である。Network bearer・cached receipt・body由来identity・通常package ownerからlocal管理権限を作れない。Bearer secretは記録/永続化せず、外部生成hashだけをauthority stateに入れる。失効後もhash再利用を拒否する。不変binding、quarantine/yank独立性、全index/storage上限、replay前と最終commit時の現在権限、中断後の厳密generation復旧を維持する。

## 代替案

- 変更しなければ、必須の登録とowner喪失復旧が未監査file編集に依存する。
- Library/tool wrapperだけでは全履歴検証が拒否する遷移を安全に追加できない。後からUIで明示host APIを呼べる。
- 汎用identity providerやHTTP管理APIはonline管理を提供するが、remote権限と秘密lifecycleを本milestoneの範囲外へ広げる。
- 通常owner membership再利用や追加専用復旧は小さいが、既存認可ownerを要するか、128枠占有時に失敗して侵害ownerも残す。厳密scopeの原子的置換で必要な緊急結果を満たす。
- Audit operationへtoken全体を埋め込むとreplayは単純になるが、妥当grantでも既存audit上限に違反する。Tokenのcanonical hashと一意な次state挿入を結合すれば、全audit recordを拡大せず完全再構築を維持できる。

## 実装計画

1. 閉じたaction codec・local capability・transition/result variant・全履歴検証を追加し、登録 → credential追加 → 厳密scope復旧 → 通常公開 → VM/native 42を実行する。
2. 現在権限/replay・敵対履歴・process中断・writer競合・厳密上限fixtureを追加し、停止/local管理/HTTPS再開実行を含める。
3. 受理fuzz・重点/全baseline・最新headの両host CIを実行しE08を公開する。統合G5結果はE04で別途監査し、設計採択だけでG5完了や最終M5 capability guideとしない。

## Test計画

- 独立canonical body/request/record digest vector、round trip、不正/重複/未知/過深/過大field、2 KiB/64 KiB境界、128-grant制約。
- 存在しない/inactive/重複/自身target、未来/期限切れcredential追加、7日境界、異なるidentity/origin、失効済み入力、保持ID/hash重複、4,096件上限、全128-owner復旧。
- 最後のactive owner無効化、異なる復旧scope/revision、no-opの拒否。無関係package・credential・両release flag・厳密source byteの保持。
- 後続無効化/失効/owner変更後もrollbackせず旧登録/追加/復旧を照合し、replay前の異なるdigest/inactive operator、最終時刻期限切れ/逆行、operation間identity再利用を拒否する。
- 全新遷移のhash再計算済み偽造履歴で、全stateの厳密再構築・receipt識別・無関係変更の隠蔽拒否を確認する。
- 子processで全永続境界を中断し、local/service writer競合の両順序、単一ack済みgeneration保持、結果不明の明示照合を確認する。厳密index/storage上限と書込前preflight拒否を試す。
- 対応macOS/Windowsで実local登録/復旧後の通常HTTPS公開/download、再open後VM/nativeの42を実行し、fmt・厳密workspace/fuzz Clippy・全test・rustdoc・repository整合性を確認する。

## 未解決事項

採択を妨げる事項はない。本番境界/process test、実platform動作、resource計上は未検証の実装仮定として残る。再有効化・identity圧縮・online管理serviceは将来別途判断する。

## 出典

- [RFC 0001](0001-project-charter.md)、実験期委任2026-10-17 JST、2026-10-31 JST参照。
- [RFC 0046](0046-registry-protocol-and-recovery.md)・[RFC 0047](0047-registry-authority-records.md)・[RFC 0048](0048-registry-state-and-publication.md)、repository契約、最終更新2026-10-31、2026-10-31 JST参照。
- [RFC 0052](0052-registry-owner-and-credential-control.md)・[RFC 0053](0053-registry-request-workspace.md)・[RFC 0054](0054-registry-quarantine-transactions.md)、最終更新2026-10-31、2026-10-31 JST参照。
- 外部local管理policy/encoding実験23 check、2026-10-31 JST実行、設計証拠のみ。本RFCはproject判断を記録し、外部標準や本番実測性能を主張しない。
