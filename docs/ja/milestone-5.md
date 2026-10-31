<!-- canonical-language: en; japanese-translation: ja/milestone-5.md -->

# Milestone 5 delivery board

[English](../milestone-5.md)

2026-10-30 JST、`3b676c5`から開始。[ロードマップ](roadmap.md#milestone-5ecosystemとcompatibility)のM5全範囲（P1の2ゲートを含む）を追跡する。M4のinstallerとlocal packageは前提条件であり、registry完了を意味しない。

**16 done · 1 doing · 24 todo · 70 remaining pt**

**次: M5-E03 — owner移管・local失効の検証と公開（1 pt）。**

E03実行checkpoint・残scope監査（2026-10-31 18:49 JST）: 実owner移管 → B公開と独立local失効で厳密source実行（VM/nativeで42）を維持した。Store 42/42が成功し、元owner/失効bearerの最新拒否、private read、128-owner上限、process中断72境界（旧68・新4）、owner/admin競合の両順序、全hash再計算済み偽造18 case、最終時刻、結果不明の明示照合、期限切れ/inactive credential対象、operation間identity衝突を含む。独立vectorと受理入力変異1,024件が成功し、妥当stateのpreflightでrevision 99 → 100が厳密index上限を+1 byte超えることを確認した。Fmt、厳密workspace Clippy、locked fuzz Clippyが成功。残受入条件は固定全baseline、最新headの両host CI、公開であり、E03を3 → 1 ptへ再見積り（純−2）、累積純−6、退役36を維持して112 − 6 − 36 = 残70 ptとする。新規完了・追加deliveryなし。E03だけをdoingとし、E04/E05は別途維持する。本実装を固定して全検証へ進む。

E03選択（2026-10-31 18:39 JST）: [RFC 0052](rfcs/0052-registry-owner-and-credential-control.md)を`31240ab`でacceptedとした。最終実行checkpointは24-case policy/独立encoding modelであり、永続owner mutationではない。Owner action/read受理、完全遷移/retry、実移管 → B公開 → VM/native実行、独立local失効、敵対履歴/process/競合境界、両host公開の残作業を既存3 ptと監査する。E03だけをdoingとし、新規delivery・再見積りなし。[PR #40](https://github.com/carimatics/lang/pull/40)は#39 merge後にmainへretargetした。以下の旧E02/E03準備件数は履歴とする。

E02完了・公開（2026-10-31 18:39 JST確認）: 固定`f40d643`でfmt、厳密Clippy、全2,655 test / 182 suite、rustdocが成功し、source/head不変を確認した。CI `41188137283`の対象18 job（22個の実装/未分類pathにより文書専用は意図的skip）とnative-debug `41188137315`の3 hostが成功。Windows core-1 `124499182307`、macOS core `124499182172`/`124499182204`でyank全10 fixtureと厳密index上限拒否、locked fuzz Clippyを確認した。[PR #39](https://github.com/carimatics/lang/pull/39)は`f74428c`としてmergeし、元mainはclean・origin/mainと0/0同期・検証済みtreeと同一である。E02の最終1 ptを退役し純−4を維持、112 − 4 − 36 = 残72 pt。E03/E04/E05に対してG5はopen、G1–G4は完了を維持する。

E02実行checkpoint・残scope監査（2026-10-31 18:16 JST）: canonical host yank/unyankが厳密archiveを保持し、完全な監査済みflag遷移をcommitする。Publish → yank → reopen → 別version公開 → unyankでVM/nativeが42を実行し、元publish/actionのretryは状態を再適用せず元receiptを返す。Store 30 caseが成功し、実action process中断36境界（旧34・新2）、publish/yank writerの両順序、最終時刻拒否、明示保持HEADによる照合、nonowner/失効/誤scope credential、履歴偽造9 caseを検証した。完全にhashを再計算したno-op履歴も拒否し、厳密2 MiB index上限で+1 byteとなるunyankは書込み前に拒否する。独立action/request hash、512入力変異、fmt、厳密workspace Clippy、locked fuzz Clippyが成功。E05のHTTP統合は別deliveryに維持する。E02の残受入条件は固定全検証、最新headの両host CI、公開であり、3 → 1 ptへ再見積り（純−2）、累積純−4、退役35を維持して112 − 4 − 35 = 残73 pt。新規delivery追加・完了なし。E02だけをdoingとし、以前の3 pt checkpointは履歴とする。この実装を固定して全検証へ進む。

E03設計準備（2026-10-31 18:28 JST確認）: [RFC 0052](rfcs/0052-registry-owner-and-credential-control.md)で認証owner追加・削除・読取り、明示2段階移管、独立構築するlocal credential失効をacceptedとした。純粋modelの24 caseが成功し、JavaScript/Pythonの独立owner/LOCAL失効digestが一致した。Policy/encoding証拠であり実装ではない。`codex/m5-registry-owner-control`は固定E02 `f40d643`から作成し、#39 mergeまでorigin/codex/m5-registry-mutationsをPR baseとして記録する。E02だけをdoing・1 ptに維持し、全検証中のsource/headを固定する。E03は選択時監査までtodo・3 ptとし、delivery・見積り・gateの変更なし。

D05完了・G4監査（2026-10-31 18:03 JST確認）: 固定`43dea74`でfmt、厳密Clippy、全2,645 test / 182 suite、rustdocが成功し、source/head不変を確認した。CI `41185690356`の対象18 job（文書専用はfull-change分類により意図的skip）とnative-debug `41185690326`の3 hostが成功。Windows core-1 `124492064305`の実ログで対象cache全15 case、macOS core `124492064188`/`124492064255`でUnix専用を含む16 case、locked fuzz Clippyの成功を確認した。[PR #38](https://github.com/carimatics/lang/pull/38)は`b0d4d76`としてmergeし、元mainはclean・origin/mainと0/0同期・検証済みheadと同一treeである。D05の最終1 ptを退役し、112 − 2 − 35 = 残75 pt。D01–D05でarchive受理、不変store/index/download、両host registry graph実行、中断/競合offline replayの証拠が揃い、G4を完了とする。G5–G10と両P1はopenを維持し、署名付き本番消費はF01–F04に残る。

E02選択（2026-10-31 18:03 JST）: [RFC 0051](rfcs/0051-registry-yank-transactions.md)を`0140d1e`でacceptedとし、`f318749`でE05のscopeを分離した。最終実行checkpointは17:48 JSTの純粋22-case遷移/独立encoding probeであり、実yank transactionではない。残るhost action/受理、可変flagと不変archiveのbinding分離、完全audit/履歴遷移、最新bearer/replay、VM/native/process/競合fixture、全検証・両host公開を3 ptと監査する。E02だけをdoingとし、PR #39はD05 merge後にmainへretargetした。E05は別途3 ptで追跡する。Service統合前にhost yank/unyankを通じた未変更archiveの実行へ到達する。

E02/service scope監査（2026-10-31 17:57 JST）: 現HTTP serviceは1個の不変`CapturedReads`世代を所有し、認証mutation dispatch・更新世代公開・要求時store作業memory計上を持たない。E02はcanonical action、閉じたaudit/履歴遷移、現在owner/競合制約、厳密retry、process中断recovery、未変更archiveのVM/native実行という具体host結果に3 ptを維持する。別の観測可能結果である認証HTTPS publish/action/owner journey、処理中要求/store作業memory上限、read世代更新をM5-E05・3 ptとして明示する。E05はE02/E03に依存し、E04の完全な権限外操作/recovery journeyの前提となる。実装前に1 delivery・+3 ptを追加し、累積純追加・再見積りを−5 → −2、退役見積りを34に維持する。41 deliveryで112 − 2 − 34 = 残76 pt。新規完了はなく、D05だけをdoing・1 ptに維持する。以下17:39/17:48の件数は履歴とし、gateや受入条件を削除しない。

E02設計準備（2026-10-31 17:48 JST確認）: [RFC 0051](rfcs/0051-registry-yank-transactions.md)で閉じたyank/unyank action、audit/result variant、未変更archive identity、最新認可による元結果replayをacceptedとした。純粋な遷移modelの22 caseが成功し、JavaScript/Pythonの独立encodingで173-byte actionとrequest digestが一致した。Policy/format証拠であり、実transactionやHTTP mutationの証明ではない。`codex/m5-registry-mutations`はD05固定`43dea74`から作成し、PR #38 mergeまで`codex/m5-registry-offline-cache`をorigin上のPR baseとして記録する。D05だけをdoing・1 ptに維持し、E02は選択時監査までtodo・3 ptとする。Delivery・point・gate完了の変更なし。D05固定全baselineと最新head CIは実行中で、source/headは不変である。

D05実行checkpoint・残scope監査（2026-10-31 17:39 JST）: private cacheが完全な有界inventoryを受理し、厳密archiveとschema-3世代のinstall後にroot lockを置換し、trust constructorなしで不変offline byteをcaptureする。実HTTPS → 公開 → service停止 → reopenでVM/nativeが42/84を実行した。初期化12・公開20のprocess中断で拒否または完全な旧/新stateを保持し、結果不明は同じidentityで照合、2競合processは両variant順で1個の不変versionだけを保持する。敵対的inventory、read-onlyとcleanup権限の分離、alias、元byte、root policy、source/target変更、物理/派生index/raw-byteの厳密上限を検証した。集中実行でpackage unit 37 caseとcommand/format/lock/workspace回帰53 testが成功。残受入条件は固定全baseline、最新headの両host CI、公開であり、D05を3 → 1 ptへ再見積り（純−2）する。累積純追加・再見積りは−5 pt、完了時退役は34を維持し、112 − 5 − 34 = 残73 pt。新規done・delivery追加なし。D05を唯一のdoing、G4をopenに維持し、以下の旧3 pt noteは履歴とする。次の実装前にcodeを固定して検証する。

D04完了・公開（2026-10-31 17:17 JST確認）: 固定`a289641`はsource不変で全baseline 4 command（2,629 test / 182 suite、fmt、厳密Clippy、rustdoc）、CI `41182449763`（対象18 job。文書専用はfull routingにより意図的skip）、native-debug `41182449769`（3 host）に成功した。Windows core-1 `124482741908`とmacOS core `124482741860`の実ログで、HTTPS graph実行、推移diamond、独立canonical lockを含むregistry/lock全12 fixtureの成功を確認した。macOS locked fuzz Clippyも成功。WindowsのLF fixture不具合は期待hashや受理条件を弱めず修正した。[PR #37](https://github.com/carimatics/lang/pull/37)は`9e81a8f`としてmerge済みで、元mainはclean・origin/mainと0/0、検証済みheadと同一treeを確認した。D04の最終1 ptを退役し、112 − 3 − 34 = 残75 pt。G4はopenを維持し、以下のD04未完・D05準備noteは履歴とする。

D05選択（2026-10-31 17:17 JST）: [RFC 0050](rfcs/0050-registry-cache-and-offline-replay.md)を`ab2fed9`でacceptedとした。最終実行checkpointは17:12 JSTの独立marker/世代計算と現schema-3 reader probeであり、cache実装ではない。残受入条件を監査し、有界private inventoryと完全世代/root公開、実service停止/reopen/offline VM/native fixture、実process終了・並行不変version・破損/alias・quota case、固定baseline・両host CI・公開を3 ptに維持する。D05を唯一のdoingとし、delivery追加・再見積りなし。[PR #38](https://github.com/carimatics/lang/pull/38)はD04公開後にmainへretargetし、work branchへorigin/mainをmergeした。次は受理済みcache経路を実装し、無関係な仕組みを加える前にoffline source実行へ到達する。

D05設計準備（2026-10-31 17:12 JST確認）: [RFC 0050](rfcs/0050-registry-cache-and-offline-replay.md)で明示private cache、厳密archive object、完全schema-3世代、install完了後のroot-lock公開をacceptedとした。JavaScript/Pythonの独立計算で53-byte markerと3,664-byte合成世代のdigestが一致し、現lock readerを使う実行probeがcache I/Oなしで世代を受理して未知schemaを拒否した。Format準備の証拠でありcache/offline実装ではない。`codex/m5-registry-offline-cache`は`codex/m5-registry-dependencies`の`a289641`から作成し、同branchをorigin上のPR baseとして記録する。PR #37 merge後、公開前にmainへretargetする。D04を唯一のdoing、D05をtodo・3 ptに維持し、件数/見積り変更なし。D04の修正済み固定baselineは2,629 test / 182 suiteとfmt・厳密Clippy・rustdocがすべて成功しているが、次の実装にはCI/公開をなお必要とする。

D04 host検証checkpoint（2026-10-31 16:54 JST確認）: 固定`e6b15f0`でfmt、厳密Clippy、182 suiteの全2,629 test、rustdocが成功し、4 commandとも終了0・source不変を確認した。一方、Windows core-1 job `124479059969`は新しい入れ子の入力fileがGitによりCRLF変換されたためregistry fixture 8件を拒否した。最上位のcanonical lockにはLF属性があったが、元入力にはなかった。`core.autocrlf=true`の再現でcanonical descriptorを含む4入力すべてが変化した。既存LF属性をこの入力directoryにも適用し、同じ変換後に独立計算のdescriptor・lockを含む全元byte/hashが保持されることを確認した。期待dataの変更やarchive受理の緩和は行わない。Native-debug `41181189973`は3 hostすべて成功。修正headは全baseline、最新headの両host CI、公開をなお必要とするため、D04だけをdoing・1 ptに維持し、残76 ptの照合は変更しない。検証修正でありdelivery完了や新規前提ではない。

D04残scope監査（2026-10-31 16:32 JST）: 推移fixture全3 testが成功した。実5-package diamondが1個のpin済みmath archiveを共有し、CRLF/UTF-8を保持してVM/native JITで84を実行する。Package=5、edge=5、source-file=6、保持source byteの厳密境界は成功し、各上限−1は拒否する。新規/キャッシュ済み推移hash不一致、非active/active host binding規則、異なるorigin/root policyで同一archive byteを保持するidentityを確認した。最初のfixtureは空descriptor targetを正しく拒否され、archive受理を変えず両対応targetを明示した。実装checklistは実行可能checkpointまで完了し、残る受入条件は固定全baseline、最新headの両host CI、公開である。D04を3から1 ptへ再見積り（純−2）し、累積純追加・再見積りは−3 pt、完了時退役は33 ptを維持する。112 − 3 − 33 = 残76 pt。新規done・delivery追加なし。D04を唯一のdoing、G4をopenに維持し、以前の3 pt noteは履歴とする。D05実装を始める前にこのcheckpointを固定して検証する。

D04 lock/local-edit checkpoint（2026-10-31 16:28 JST確認）: schema-3 captureがcompiler-ID置換後の独立fixtureとbyte完全一致する。閉じたcanonical受理でorigin付きID、checksum、source/edge/binding参照、graph到達性/循環、必須workspace、深さ16、1 MiB上限を検証する。Golden fixtureがlocal root keyの余分な末尾区切りを検出し修正した。Workspace captureは元の未選択metadataを保持する。Formatと検証済み置換は保持したlocal物理bindingを使い、論理keyがlocal物理pathと一致してもregistry編集を拒否し、固定proofにはregistry入力を保持する。Package unit全18 testとarchive/editor/fix/format/lock/source/workspace回帰71 test、workspace/all-target/all-featureの厳密Clippyが成功。本番認証済みreplay/cacheはD05/F01–F03に残し、adapterのないhybrid editor操作とsnapshot profile 1は明示拒否する。残るD04受入条件は推移graph/拒否実行と境界fixture、固定全baseline、最新headの両host CI、公開である。この監査後もD04だけをdoing・3 ptに維持し、delivery追加や見積り変更は行わない。次は推移source実行fixtureを追加する。

D04 graph実行checkpoint（2026-10-31 16:19 JST確認）: 既存Graph/inventory/providerが型付きlocal入力または封じたregistry入力をcaptureし、元descriptor/source byteと独立したlocal物理bindingを保持する。Unit全4 testが成功し、実HTTPS取得後にservice/sourceを除去してVM/native JITで42を実行、registry形local pathと改名/共有registry aliasを持つ実hybrid graphでidentityを区別して84を実行、registry overlayの拒否、明示host bindingと独立policy-key fixtureを確認した。未署名constructorは`cfg(test)`内だけに置き、本番constructor・CLI fallback・trustの主張は追加していない。Localのmanifest/graph/lock/workspace/archive回帰は全54 test成功。残作業はlock schema 3、型付きlocal専用format/editor対応、推移/固定version/root-policy/拒否case、両host証拠、固定全baseline、最終公開である。現在lock/edit consumerはhybrid graphを明示拒否し、snapshot profile 1は契約どおりlocal専用を維持する。この残作業監査後もD04をdoing・3 ptに維持し、前提追加や見積り変更は行わない。次のcheckpointは同じ実行経路のlock captureとlocal toolingを実装する。

D04 manifest checkpoint（2026-10-31 16:11 JST確認）: 厳密schema-2受理がschema 1の文法、重複/未知/null拒否、共通上限を維持する。型付きlocal/registry宣言に権限は含めず、active registry edgeは認証済みcontextを明示要求し、local fallbackを行わない。Manifest/graph/lock/workspace/archiveの対象全54 testが成功し、実CLIと、既存lock schema 1/2を維持するschema-2 local workspaceの42実行を含む。Repository consistencyは8/8、fmtとworkspace/all-target/all-featureの厳密Clippyも成功。これは実行可能な中間checkpointであり、D04完了でも全baseline証拠でもない。残る受入条件は、型付きregistry graph/source captureと読取専用tooling guardの統合、厳密lock-3 capture、両hostの推移/identity/拒否VM/native fixture、全baselineと最新head公開検証。この残作業監査後もD04を唯一のdoing・3 ptに維持し、追加・退役なし。次は受理した取得sourceを実行へ接続する。

D04 scope監査（2026-10-31 16:03 JST）: D03 merge後にD04を唯一のdoingとして選択する。RFC 0049は`4054b0c`でaccepted。最後の実行checkpointは、実HTTPS取得後のVM/native JITによる42、独立再現したschema-3 fixture、local/registry namespace衝突での84である。これはlocal実行可能性の証拠で、統合resolverではない。残る受入条件は、local互換性を保つ厳密manifest-2受理、型付き認証済みregistry graph/source captureと読取専用tooling guard、厳密lock-3 capture、両hostの実推移/identity/拒否VM/native fixture、全baselineと最新head公開検証。この残作業監査後も3 ptを維持し、新たな前提や見積り追加は行わない。[PR #37](https://github.com/carimatics/lang/pull/37)はmainベースとなり、branchへD03のremote mergeを取り込んだ。最初の実装checkpointで宣言を受理し、registry contextなしの消費を明示拒否する。次のcheckpointでは取得sourceを実行へ接続する。

D03完了・公開（2026-10-31 16:03 JST確認）: code `f58659c`はsource不変で固定全baselineに成功（182 suite・2,611 test、fmt・厳密Clippy・rustdoc、全4 exit 0）し、CI `41177539886`（対象18 job、full routingにより文書専用だけ意図的省略）、native-debug `41177539882`（全3 host）も成功した。Windows job `124468380664`の実logでtransport全18、store対象19 testを確認し、8 handshakeのstop、shutdownなしのread/write cancel、証明書検証付きlocalhost読取り、厳密16 MiB sourceを含む。macOSでもlocked fuzz Clippyを確認した。[PR #36](https://github.com/carimatics/lang/pull/36)は`723e2eb`としてmerge済み。元mainはorigin/mainと一致し、clean・ahead/behind 0/0で、treeは`f58659c`と一致する。D03の最後の3 ptを退役し、112 − 1 − 33 = 残78 pt。G4と署名はopenに維持する。下のD03未完了noteは履歴である。

D04設計準備（2026-10-31 JST）: `codex/m5-registry-dependencies`を`codex/m5-registry-reads`の`f58659c`から`/private/tmp/lang-m5-registry-dependencies`に作成した。初期PR baseは`git@github.com:carimatics/lang.git`の前提branchである。実装前に[RFC 0049](rfcs/0049-registry-package-integration.md)をRFC 0001委任でacceptedとした。明示manifest-2 edge、型付き不変origin、相互分離した論理source key、lock schema 3でlocal挙動を維持し、独立した本番trustを要求する。保持した3,619-byte fixtureをJavaScript/Pythonで独立計算し、Rust archive再構築と現行VM/native JITは42、local/registry path衝突caseは84を返した。これは設計/fixture証拠でありresolver実装ではない。D04はtodo・3 ptに維持し、修正後baseline/CI実行中のD03だけをimplementation doingとする。Delivery/point追加・退役なし。D03をmerge/同期してからD04実装を選択する。

D03 CI修正checkpoint（2026-10-31 15:38 JST確認）: 固定`5317901`はsource不変で全baseline 4 commandに成功（182 suite・2,609 test）した。Windows job `124464445376`ではclient TLS読取り修正とtransport 15 test成功を確認したが、8 handshakeのstop testが変更していない2秒上限を超えた。Cancelは各socket timeoutを最大100 msに制限して共有stop flagを確認し、絶対deadlineと実worker joinまでの予約を維持する。Shutdownも追加の起床手段として残す。Shutdownなしのread/write cancelを含むlocal transport全18 testが成功。別件のmacOS CI失敗も再現し、`fuzz/Cargo.lock`で新しい直接production依存2件の記載が欠けていた。Cargoでその2行を再生成し、locked offline fuzz Clippyは成功した。残る受入条件は修正後の固定全baseline、最新headのmacOS/Windows CI、PR公開。D03だけをdoing・3 ptに維持し、見積りの追加・退役なし。

D04実行可能性確認（2026-10-31 15:38 JST）: `/private/tmp/lang-m5-registry-source-probe.rs`で隔離した未署名archiveを実際に公開し、証明書検証付きHTTPSでindex/archiveを取得した。Service停止・削除後に固定依存をVM/native JITで実行し、両方42を返した。別の2 namespace fixtureは84を返し、元のCRLF/UTF-8 sourceを保持する。未宣言edgeは`SRC0013`となりgraphを作らない。URL形の診断pathは既存MIRの`InvalidPath`で拒否されるため、そのpath形式にorigin identityを埋め込めない。これはlocalの統合/設計実験でありD04完了ではない。Manifest/lock encodingの受理、本番trust、実resolver/cache統合、両host証拠は残る。D04はD03直後のtodo・3 ptを維持する。

D03 server checkpoint（2026-10-31 15:17 JST確認）: 読取専用HTTPS serviceが排他lock中に完全検証済み世代を保持し、上限付きindexを共有、HEADを再確認して参照済みarchiveの厳密bytesだけを配信する。共有data、上限付きTLS/worker/header記憶域、送信archive bufferを受理前に256 MiBへ予約する。最大8接続には未完了handshakeを含み、超過peerをapplication queueなしで閉じ、stopは全workerをjoinする。Transport全16 testが成功し、実local socket fixtureでは16 MiBの元source、line/header/field数と集約budgetの厳密境界、未公開/link/改変object、pipeline拒否、低速peer、cancelを含む。取得済み旧超過indexの拒否を追加したstore全20 testも成功。RFC 0046/0048にはread snapshotと論理memory計上の契約を実装前に明記した。残る受入条件は固定全baseline、最新headのmacOS/Windows CI、実platform結果の確認、PR公開である。D03だけを3 ptに維持し、追加・退役なし。次はD04のsource依存実行。

D03 Windows追跡（2026-10-31 15:17 JST）: client head `cb849f2`のnative-debug `41174823189`は成功したが、CI `41174823192`のjob `124460498312`は、変更していないfixtureの2秒connect deadlineで正当なTLS読取りに失敗した。逐次connectorは最初のDNS addressだけで全deadlineを消費でき、後続候補（例: IPv4 listenerに対し先に返るIPv6 localhost）を試せなかった。絶対deadlineを維持しつつ残予算を残address数へ配分し、正当caseのassertionへhost情報を追加した。変更後のlocal transport全16 testは成功したが、Windowsの原因/修正確認には新headのCI証拠が必要。Timeout・assertion・CI範囲は弱めず、旧失敗runを成功とは扱わない。

D03 HTTPS client checkpoint（2026-10-31 14:58 JST確認）: `registry::transport::Client`が明示TLS rootと単一設定origin、rustls/httparseによる上限付きraw HTTP/1.1 framing、絶対connect/request deadline、未完了DNS worker上限を実装した。実TLSのlocal 7 testで、厳密index/archive照合、同versionの内容差替え、別origin/証明書、欠損/余剰byte、header上限、非対応framing、遅いheader/body/handshake、response EOFなしを検証。公開読取りはcredentialを送らず、checksum/TLS受理もpackage署名・鮮度・実行authorityを付与しない。Root packageの厳密Clippyも成功。Handshake受理から最大8接続、集約memory計上、cancelを含むserverは未実装で、両host/全検証と公開も残る。この残作業監査でもD03だけを3 ptに維持し、追加・退役なし、D03 → D04実行順を保つ。先行するstoreの20 testは別の証拠である。

D02公開（2026-10-31 14:45 JST確認）: 最終head `9361092`の固定全baseline（182 suite・2,590 test、fmt・厳密Clippy・rustdoc、source不変）、CI `41172448771`（対象18 jobすべて成功、文書専用のみ意図的省略）、native-debug `41172448726`（全3 host）が成功した。最終Windows logでもregistry対象16 testを確認。[PR #35](https://github.com/carimatics/lang/pull/35)は`50eeeb3`としてmerge済みで、元`main`を`origin/main`へfast-forwardし、未commit変更なし・0 ahead / 0 behindを確認した。Merge treeは`9361092`と完全一致する。[D03 PR #36](https://github.com/carimatics/lang/pull/36)はbaseを`main`へ変更し、最初のread checkpointは`3a89172`。以下の古い公開待ち記録は履歴である。

D03実装checkpoint（2026-10-31 14:41 JST確認）: Canonicalなcommit済みindex/archive読取り、型付き外部index受理、厳密archive結合、commit前index容量guardを実装した。重点20 testが成功し、2 MiB−1/等値の実publishと+1 byteの全file不変拒否、旧超過stateの保持とindex/新規公開拒否を含む。Archive/authority回帰19件、repository consistency 8件、fmt・厳密Clippyも成功。署名・鮮度の証明は付与せず、HTTPS統合・両host・全baseline/公開は残る。D03だけをdoing・3 ptに維持する。

D03 branch記録（2026-10-31 JST）: `codex/m5-registry-reads`を`codex/m5-registry-store`の`9361092`からclean状態で作成した。Checkoutは`/private/tmp/lang-m5-registry-reads`、初期PR baseは`git@github.com:carimatics/lang.git`の同前提branchで、PR #35の最終検証待ち。元`main`は`5cd0815`でcleanを維持する。

D02完了（2026-10-31 14:23 JST確認）: code `e76dd7b`の固定全baseline（182 suite・2,589 test、fmt・厳密Clippy・分離targetのrustdoc、全exit 0・source不変）、CI `41170930627`（対象18 job、full routingにより文書専用のみ意図的省略）、native-debug `41170930592`（全3 host）が成功した。Windows core-1 job `124448866705`の実logでregistry対象16 testを確認し、process終了46切断点・writer競合・reopen後の厳密source実行も成功。17件目はUnix symlink fixtureである。両platform不具合はassertionを弱めず修正した。最後にcommitした残1 ptを退役し、112 − 1 − 30 = 残81 pt。G4/G5はopen。[PR #35](https://github.com/carimatics/lang/pull/35)の最終head検証・mergeは残る。後続の符号付き見積りvalidator修正は重点8 testが成功し、最終baseline/CI検証が必要である。

D03範囲監査（2026-10-31 14:23 JST）: D03だけをdoing・3 ptに選択する。最後の実行checkpointは実受理archiveによる厳密2 MiB index境界と、証明書検証付きlocal TLSのframing/deadline実験であり、registry serviceではない。残る受入条件はcanonicalな上限付きindex受理/encodeとcommit前容量guard、commit済みだけを対象とする厳密identity archive読取り、HTTPS service/clientのframing・deadline・接続/body budget、両hostの実index/download/改変/拒否fixture、全baselineと公開check。本番消費前にはF01–F04の独立署名検証を維持する。前提・見積り追加なし。E01/D02後は直接D03、次にD04のsource依存実行へ進む。

D02 baseline・再見積り（2026-10-31 14:20 JST確認）: 固定`e76dd7b`の全4 commandがexit 0・source不変で完了。fmt・厳密Clippy・182 suiteの2,589 test・分離targetのrustdocが成功した。結果とlogは`/private/tmp/lang-m5-fixes-target/m5-d02-lock-host-validation/`に保持。既存CIの最適化profileでもdebug assertion・overflow check・debug情報は有効。Native-debug `41170930592`は全3 hostで成功。残る受入条件はCI `41170930627`の残2 jobと集約結果、実Windows registry結果の検査、その後の最終文書head CI・PR公開である。条件を削除せずD02の残作業を3 → 1 ptへ下げ、純再見積り+1 → −1、退役見積り29のままとし、112 − 1 − 29 = 残82 pt。D02だけをdoing、G4/G5をopenに維持する。

D02 Windows lock修正（2026-10-31 14:01 JST確認）: CI `41169801575`のcore-1 job `124445401025`で9件失敗。排他lock中の空`LOCK`を別handleからEOFまで読もうとしてWindows error 33になった。Inventoryはmetadataでno-follow・通常file・link数の不変条件と厳密な0 byte長を再検査し、test snapshotも0 byte長から厳密な空内容を得る。Lock・破損・並行・crashのassertionは削除しない。ローカルfmt・厳密Clippy・重点17 testは成功。固定`9f238c4` baselineはこのsource修正に伴い明示終了した（実session exit 143、process group残存なしを確認）。全test・rustdocは未完了。修正headで新しい全baselineと、実Windows registry testを含む最新head CIすべてを必要とする。D02だけをdoing・3 ptに維持し、追加・退役なし。

D02 CI修正（2026-10-31 13:44 JST確認）: CI `41169200725`のWindows workspace job `124443312245`が、Unixだけでmodeを変更するdirectory builderの不要mutを拒否した。警告を抑制せず、mutable bindingをUnix block内に限定する。修正後のローカルfmt、厳密Clippy、重点17 testは成功。旧`f9b2069`の全baselineは、このsource修正に伴って明示終了した（session exit 143・process group残存なしを確認）。完了や一時timeoutとは扱わない。修正commitを固定して全baselineと最新head CIをやり直し、D02はdoing・3 ptを維持する。


D02実行checkpoint（2026-10-31 13:32 JST確認）: `registry::store`が明示capabilityで選んだ新規private rootを初期化し、非待機lock内でcanonical state/audit全履歴を検証、staging・sync済みfileと単一HEAD置換で不変archiveを公開する。Replay前とstaging後・置換直前に実credentialを再認証する。ローカル17 test（subprocess driver 3件を含む）が成功: bootstrap 22切断点、公開24切断点、両version順序で競合するwriter process、厳密replay/競合、独立計算したrequest digest、最終guardの失効/時刻逆行、明示unknown outcome照合、cleanup前の破損/alias拒否、byte/count/peak quota境界。Reopenした厳密source bytesは42へ実行される。Archive/authority回帰19/19と対象strict Clippyも成功。残る受入条件は固定全baseline、両hostの最新head CI、PR公開。この残作業監査後もD02だけをdoing・既存3 ptで維持し、追加・退役なし。G4/G5はopenで、このcheckpointはdelivery完了ではない。

D03事前検証（2026-10-31 JST）: `/private/tmp/lang-m5-index-bound-probe.rs`で実archiveを独立構築・受理検査し、規定のcanonical indexをencodeした。Releaseごとに厳密依存128件を含む場合、29 releaseは2,033,722 byte、30 releaseは2,103,846 byteとなり、各descriptorが128 KiB未満でも既存index上限2,097,152 byteを超えた。これは受理/encodeの証拠で、storage・network公開ではない。D03はread API公開前に、公開時の派生index容量を解決する必要がある。暗黙paginationや読めない成功公開を導入せず、影響する採択済み契約を実装前に改訂する。既存D03のindex境界成果・3 pt内として扱い、新delivery・見積り追加なし。D02だけがdoingで、`f9b2069`を固定して全baselineと[PR #35](https://github.com/carimatics/lang/pull/35) CIを実行中。

D03判断（2026-10-31 JST）: RFC 0046で派生indexのcommit前上限をRFC 0001委任により採択し、旧データの明示拒否と暗黙migrationなしを規定した。上記事前検証の契約対応は実装前に解決した。Roadmapをmerge済みE01、D02の実行checkpoint、不変のD02 → D03 → D04実行順に整合させた。D03はtodo・3 ptを維持し、選択にはD02全検証の完了が必要。

D03厳密境界fixture（2026-10-31 JST）: `/private/tmp/lang-m5-index-exact-bound-probe.rs`で、依存128件を持つ実archiveを各caseで30個受理した。最後のarchiveにある正当なregistry識別子だけを短くし、完全なcanonical indexを厳密に2,097,151・2,097,152・2,097,153 byteへ調整できた。対応するSHA-256は順に`982e693f4b13246d27cd7f0f39adbabd0fa2975663fd2fa872ee4b26c9a2d934`、`d2697d8e0de1c56783ca1e45becd2ecfdc55d4327f0c6f84ed05a272b2b7e78d`、`aa8b6297244e99eaaabdf18f750a260d1c60eadb31e783e38b04fbe748b179fd`。D03の閾値testに使える実受理入力の証拠であり、transaction拒否やHEAD不変は未検証。Delivery・見積り変更なし。

D03 deadline実験（2026-10-31 JST）: `/private/tmp/lang-m5-https-deadline-probe.rs`で実TLS fixtureへ単調時刻の絶対I/O deadlineを追加した。元の7 caseと遅いheader・遅いbody・TLS EOFなしの計10 caseが成功。実験用200 ms deadlineに対し、70 msごとにbyteが届いても後者3 caseを200–201 msで拒否し、個々のreadが全体budgetを延長しないことを確認した。これはlocal機構の証拠であり、本番10/120秒上限・全parser case・集約/並行budget・Windows動作の検証ではない。D03はtodo、D02だけをdoingに維持する。

D03 transport実験（2026-10-31 JST）: `/private/tmp/lang-m5-http-framing-probe.rs`でlock済みureq 3.5.0のメモリ内transportを使用した（network/TLSではない）。1 byte宣言の厳密入力は1 byteを返し残り0 byte。4 byte余分な入力も1 byteの正常EOFを返し、wireの残り4 byteは呼び出し側から隠れる。5 byte宣言で1 byteしかない場合は正しく失敗した。したがって既存の高水準body reader単独ではRFC 0046の余分なresponse拒否を実現できない。D03は保守されるHTTP/TLS部品を使いつつ上限付きraw framingを検査可能にし、両hostで厳密/不足/余分な実transport fixtureを加える。これは範囲を限定した統合制約で、library脆弱性の主張やM4動作変更ではない。D03はtodo・3 ptを維持する。

D03 HTTPS実現性（2026-10-31 JST）: `/private/tmp/lang-m5-https-framing-probe.rs`でlock済みrustls/httparseを実local TLS socketに接続し、fixture証明書を明示的に信頼した。厳密1 byte response、余分/不足本文、Content-Length重複、Transfer-Encoding、Content-Encoding、未信頼証明書の7 caseが成功。Clientの書込側を開いたままserverが応答し、ローカル各caseは1–3 msで完了した（性能保証ではなく観測）。高水準body readerと異なり、上限付きraw framingで受理前に余分なbyteを検出できた。Sandboxがlocal bindを拒否したため、作成済み実験を一回限りの権限拡張で実行した。これはlocal実現性の証拠のみ。8接続/集約budget、完全なdeadline/framing matrix、registry対応、Windows transport実行はD03の残作業で、D02だけをdoingに維持する。

D02 branch記録（2026-10-31 JST）: `codex/m5-registry-store`を`codex/m5-registry-authority`の`26fe3df`からclean状態で作成し、checkoutは`/private/tmp/lang-m5-registry-store`。初期PR baseは`git@github.com:carimatics/lang.git`の同前提branchで、PR #34は最終文書headを検証中。元`main`はmerge済み`f5084f1`でcleanを維持する。不変storage/公開という一貫した成果を新branchで分離し、doing行や見積りは追加しない。

E01公開（2026-10-31 13:34 JST確認）: 最終head `26fe3df`はCI `41167648530`の対象18 jobすべて成功（full routingにより文書専用のみ意図的省略）、native-debug `41167648534`も全3 hostで成功。[PR #34](https://github.com/carimatics/lang/pull/34)は`5cd0815`としてmerge済みを確認。元`main`を`origin/main`へfast-forwardし、0 ahead / 0 behind・未commit変更なし。D02 branchも実装変更を保持して同前提mergeへfast-forwardし、PR baseは`main`となった。以下の古い公開待ち記録は履歴である。

E01完了（2026-10-31 13:10 JST確認）: code `56f9f84`の固定全baselineは182 suite・2,572 test、fmt、厳密Clippy、分離targetのrustdocが成功。全4 child exitは0でsource fingerprintは不変。実結果・logを`/private/tmp/lang-m5-fixes-target/m5-e01-validation/`へ保持する。CI `41165285468`は対象18 jobすべて成功（full routingにより文書専用のみ意図的省略）、native-debug `41165285479`も全3 hostで成功。Authority 10 testとcompile済みfuzz targetはE01の純粋host境界を検証し、本番server・commit時storage統合・継続的fuzz campaignは主張しない。最終3 ptを追加なしで退役し、112 + 1 − 29 = 84 pt。[PR #34](https://github.com/carimatics/lang/pull/34)の最終文書head CIとmergeは未完了。

D02範囲監査（2026-10-31 13:10 JST）: D02だけをdoingに選択する。[RFC 0048](rfcs/0048-registry-state-and-publication.md)は`cd30c02`で採択済み。最後の実行checkpointはmacOS process/staging prototypeの13切断点で、registry実装ではない。残る成果を監査し3 ptを維持する: private canonical generationと全履歴受入、最新E01認可を伴う上限付きstaged公開、厳密replay/競合/quota拒否、両hostの実reopen/source inventory/並行/process終了fixture、全baselineと公開check。G4/G5はopen。依存実行の順序はD02 → D03 index/download → D04 source依存実行を維持し、E01/D02後に水平前提を新設する前に再確認する。

D01公開（2026-10-31 12:49 JST確認）: 最終head `2704805`はCI `41164333928`（対象18 job、full routingにより文書専用のみ意図的省略）とnative-debug `41164333850`（全3 host）が成功。[PR #33](https://github.com/carimatics/lang/pull/33)の`f5084f1`としてのmergeを確認した。元`main`を`origin/main`へfast-forwardし、ahead 0 / behind 0・未commit変更なし。E01の[PR #34](https://github.com/carimatics/lang/pull/34)はbaseを`main`へ変更し、実装`56f9f84`を固定したまま全baselineとCIを実行中。以下に残るD01公開待ちの記録は過去のcheckpointである。

E01実行checkpoint（2026-10-31 12:36 JST確認）: policy modelの23 scenarioと失効・owner削除後のretry拒否を確認し、[RFC 0047](rfcs/0047-registry-authority-records.md)を実験段階委任で採択した。`registry::authority`は独立した設定identityに対して上限付きcanonical viewを受理し、秘匿・消去する明示bearer入力と、現在の認証・操作／所有権scopeを評価する。独立計算authenticator、決定的入力変異512件、byte／depth／collection境界、4,096 recordの末尾credential、失効・時刻・ownerの再評価拒否を含む統合10 testが成功。archive回帰9/9、repository consistency 7/7、対象厳密Clippy、fuzz targetのcompileも成功。残る受入条件は固定全baseline・最新head両host CI・PR公開。E01のみをdoing、3 ptに維持し、見積り追加・退役なし。D02では完全なstate/audit形式と実lock内の再認可統合が必要。準備した`codex/m5-registry-authority`は`codex/m5-registry-archive`の`b3c8d1b`から開始し、`2704805`へfast-forwardした。記録した初期PR baseは`git@github.com:carimatics/lang.git`のarchive branchで、PR #33のmerge待ち。元の`main`は変更せずcleanを維持する。

D01完了（2026-10-31 12:24 JST確認）: code `b3c8d1b`の固定全baselineは181 suite・2,562 test、fmt、厳密Clippy、分離targetのrustdocが成功。全4 child exitは0で全source fingerprintは不変。実結果・logを`/private/tmp/lang-m5-fixes-target/m5-d01-validation/`に保持する。CI `41161839406`は対象18 jobすべて成功（full routingにより文書専用のみ意図的省略）、native-debug `41161839444`も3 host jobすべて成功。archive 9 testは元byteのsource実行と決定的変異512件を含む。fuzz targetはcompile済みだが継続的fuzz campaignは主張しない。最終3 ptを退役し追加なし、112 + 1 − 26 = 87 pt。E01のみをdoingとし、3 ptは権限レコード仕様の採択、上限付き秘密入力・ポリシー実装、検証を含む。G4/G5はopen。[PR #33](https://github.com/carimatics/lang/pull/33)の最終文書head CIとmerge、公開完了と最終main同期は未完了。

C02・G3完了（2026-10-31 11:27 JST確認）: `53376e5`の[RFC 0046](rfcs/0046-registry-protocol-and-recovery.md)で厳密source archive・厳密origin wire profile・scope付き変更check・不変state／audit commit・有限復旧を採択。P01–P08がRFC 0045の16脅威すべてに対応する。[Local archive実験](../evidence/2026-10-31-m5-registry-archive-profile.json)は独立readerで列挙・厳密source復元に成功したが、形式実現性のみで敵対decoderや両host実装の証拠ではない。新しいrepository consistencyは7/7成功。追加なしでC02の最終3 ptを退役し、G3は設計ゲートとして完了、G4–G10はopen。最終head `c49a9a5`のCI `41160242518`は対象18 job成功（full routingで文書専用のみ意図的省略）。Native-debugは指定source／SDK pathの変更がなく意図的に未起動。[PR #32](https://github.com/carimatics/lang/pull/32)を11:49 JSTに`9694ec3`としてmergeし、元`main`はcleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。

D01実行可能checkpoint（2026-10-31 11:49 JST確認）: `registry::archive::AdmittedArchive::admit`は有限canonical USTAR／JSONを、元UTF-8 bytesと厳密archive／descriptor／source digestを持つ非公開の所有inventoryへ検証し、I/O・実行authorityを持たない。独立tar-rs fixtureは保持archive hashに一致し、受入sourceは42を返す。呼出し元bytesを変更しても固定inventoryは不変。9 testで敵対header・切断／padding・closed／canonical metadata・path／device／case／prefix alias・digest／UTF-8失敗・厳密byte／record／深さ／総量上限・依存identity・決定的変異512件を検証。対象厳密Clippyとrepository consistency 7/7が成功し、libFuzzer targetもcompile成功。cargo-fuzz／nightlyは未導入のため継続fuzz実行を主張しない。残る受入条件は固定全baseline・最新head両host CI・PR公開。これらの確認まで3 ptを維持し、D01だけをdoingとする。

D01順序監査（2026-10-31 JST）: PR #32の証跡JSONは既存full CI経路を選択するため、採択済みRFCに従う実装を分離branch `codex/m5-registry-archive`で進める。`codex/m5-registry-contract`（`c49a9a5`）からclean状態で作成し、checkoutは`/private/tmp/lang-m5-registry-archive`。初期PR baseは`git@github.com:carimatics/lang.git`の同契約branch。PR #32のmerge後に作業branchを`9694ec3`へfast-forwardし、PR baseを`main`へ変更した。契約公開完了まで実装を待つ旧計画を更新するが、設計採択前にuploadを受け入れるものではない。範囲はbyte-only decoderと検証済み不変inventoryのままで、network・storage公開・materializeは後続deliveryに残す。新規delivery・見積り変更なし。元の`main`はcleanを維持する。

C01完了（2026-10-31 JST）: `94a5200`の[RFC 0045](rfcs/0045-registry-threat-model.md)を実験段階委任により採択。16の脅威でorigin／namespace・scope付きcredential・敵対archive・不変transaction・replay／key侵害・audit・実行・hostingを観測可能な受入義務へ結び付けた。新しいrepository consistencyは7/7成功、日英の脅威ID・出典URLも一致。追加なしで最終3 ptを退役する。C02だけをdoing（3 pt）とし、protocol／復旧判断の採択までG3はopen。Upload実装は未着手。

B04・G2完了（2026-10-31 10:59 JST確認）: code `62e742c`と[保持した証拠](../evidence/2026-10-31-m5-source-migration.json)で、実旧producer・現VM・link済みnativeの両host journeyを検証した。固定`2f905c7`の全baselineは180 suite・2,553 test、fmt・厳密Clippy・rustdocが成功し、4つのchild exitすべてが0、source fingerprintも不変。logは`/private/tmp/lang-m5-fixes-target/m5-b04-validation/`に保持する。最終head `ab5cda8`のCI `41156030921`（対象18 job、full routingにより文書専用のみ意図的省略）とnative-debug `41156030861`（3 host job）が成功。[PR #31](https://github.com/carimatics/lang/pull/31)を`46cb6a1`としてmergeし、元の`main`はcleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。以下の日付付き未完了checkpointは履歴であり、現在の阻害条件ではない。

C01範囲監査（2026-10-31 JST）: 採択脅威モデルと検証可能な信頼境界に3 ptを維持する。C02はupload前のwire format・上限・復旧・security受入ケースとして別に3 ptを維持する。実装deliveryのdoingはない。branch `codex/m5-registry-contract`は`main`（`46cb6a1`）からclean状態で作成し、checkoutは`/private/tmp/lang-m5-registry`、PR baseは`git@github.com:carimatics/lang.git`の`main`。delivery追加・見積り変更なし。P1の両成果を含む8つのopenゲートをすべて維持する。

B04受入監査（2026-10-31 10:34 JST確認）: [保持した証拠](../evidence/2026-10-31-m5-source-migration.json)はmacOS 15/26・Windows 2025の各6 fixtureについて、実旧source遷移5件、現VM・link済みnative実行、厳密な編集適用、再実行no-opを示す。CLI・公開・拒否要件は実装済みで、残作業はlocal全baseline・最新head CI・PR公開。これらのcheckを省かず2 → 1 ptへ再見積りする。B04はdoing、G2はopenを維持し、固定`2f905c7`のtest processは稼働中、fmt・厳密Clippyは成功している。

B04再開checkpoint（2026-10-31 10:26 JST確認）: `62e742c`のCI `41154299004`はWindowsを含む3 hostのSDK producer移行journeyと隔離SDK install jobに成功し、残りのCI jobは実行中。local `5480e14` baselineに終了結果はなく、session 60960とdriver・Cargo・test processは消失し、logは10:17 JSTで停止している。中断試行は`/private/tmp/lang-m5-fixes-target/m5-b04-validation-5480e14-interrupted/`に保持し、途中の645 test成功を全baseline成功に数えない。作業branchを`2f2e2b9`までfast-forwardし、新たに固定するcheckpointで全baselineを実行する。local全検証と最新head CIが成功するまでB04はdoing・2 ptを維持する。

B04 CI修正（2026-10-31 JST）: run `41109397767`はmacOSの移行journeyに成功したが、WindowsではGitの自動CRLF変換が固定旧lockのbytesを変え、実行前に拒否した。一時checkoutで`core.autocrlf=true`の不一致と`false`の保持hash一致を再現した。旧worktree作成時だけ変換を無効にし、厳密なlock検査は維持する。このCIだけの修正は`5480e14`から作った`/private/tmp/lang-m5-migration-ci`に隔離し、元worktreeでRust baselineの入力を不変に保って継続する。同branch・PRに公開し、baseline終了後に元worktreeをfast-forwardする。失敗CIをgate完了に数えず、deliveryの範囲・見積りも変更しない。

B04 host checkpoint（2026-10-31 03:42 JST確認）: `tools/check-source-migration.py`はlock不変で固定旧compilerをbuildし、元fixture 6件を実行、実際のmigration CLIを適用し、現VMと対応SDKでlinkしたnative結果を検査した。実遷移5件は保持した`shared`・`weak`、wrapping parameter、再帰、末尾comma、loop・名前衝突・UTF-8 comment・CRLF保持を検証し、変更不要の6件目でno-opを別途検査する。macOS localですべて成功し、source・compiler・lock・SDK・実行fileのhashと59 commandの観測を`/private/tmp/lang-m5-b04-journey/report.json`に保持する。既存SDK producer matrixにも同journeyを追加し、macOS 15/26・Windows 2025のreportを保持する。Windows・最終全baseline・最新head CIは未完了で、B04はdoing・2 ptを維持する。

B04実行checkpoint（2026-10-31 03:32 JST確認）: `lang migrate`はprofile・edition・rootを明示必須とし、既定でpreview、新しく証明した単一source置換を共通公開境界で適用する。CLI fixtureは元bytesを変更せずpreviewし、適用後42を実行して、再実行がno-opになる。移行test 15件は旧受入・証明に加え、適用・stale bytes/identity・異なるauthority・readonly/alias・usage拒否・cwd基準/絶対source pathを検証し、既存fix 11/11・format 7/7も成功。この実行可能な統合checkpointを受け、残見積りを3から2 ptへ下げる。残る受入条件は固定旧producer・現VM・対応SDK nativeの両host journey、最終全baseline、最新head CI。このcheckpointはdelivery完了ではなく、同じ一貫したbranchでB04だけをdoingとする。

B05完了（2026-10-31 03:17 JST確認）: code `7f41dd0`は実在する旧`shared`・`weak` sourceを受入れ、元token offsetと型付き解決構造を証明し、rewrite後sourceは42を返す一方、通常compileは元sourceを拒否する。integration test 8件・proof test 2件で、固定producerの受入11ケース、生成scalar実行24件、wrapping加算、燃料切れ保持、衝突・trivia・CRLF、再実行no-op、意味の反例、引上げ不可の再帰上限を含む厳密な資源境界を検証。source固定全baselineの179 suite・2,546 test、fmt・厳密Clippy・rustdocが成功し、全child exitは0、全入力fingerprintは不変。logは`/private/tmp/lang-m5-fixes-target/m5-b05-validation/`に保持する。同headのCI `41101729179`は対象18 jobすべて成功（full routingにより文書専用のみ意図的省略）、native-debug `41101727954`も3 host jobすべて成功。[PR #31](https://github.com/carimatics/lang/pull/31)は関連するB04継続用にdraftを維持し、後続headもその公開check成功を必要とする。

branch `codex/m5-historical-source-migration`は`codex/m5-checked-source-fixes`の`791237e`からclean状態で作成し、checkoutは`/private/tmp/lang-m5-migration`、初期PR baseは`git@github.com:carimatics/lang.git`の同前提branch。PR #30のmerge確認後、`6ea2cd1`へfast-forwardし、PR baseを`main`へ変更した。元worktreeは`main`上でcleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。

公開checkpoint（2026-10-31 JST）: code `01b425e`のCI `41085682122`（18 job成功、full routingにより文書専用のみ意図的省略）とnative-debug `41085682104`が成功。全local baselineは178 suite・2,536 test、fmt、厳密Clippy、rustdocが成功し、全child exitは0、source fingerprintは不変。logは`/private/tmp/lang-m5-fixes-target/m5-b03-validation-authorized/`に保持する。最初のsandbox実行はHTTPS fixture 2件のloopback待受を拒否したため、同一の固定sourceで承認済み実行を行った。失敗・取消runを成功とは数えない。最終head `791237e`もCI `41094820142`（対象18 job）とnative-debug `41094820280`が成功し、[PR #30](https://github.com/carimatics/lang/pull/30)を`6ea2cd1`としてmerge、local `main`を同じremote tipへfast-forwardした。

B03完了（2026-10-31 JST確認）: `lang fix`はpreviewが既定で、`--apply`はformatterの固定入力検証・全件staging・file単位置換を再利用し、非公開の検証済みplanを再計算する。entry・testの適用前後VM実行、古いsource・依存・manifestと異なるauthority拒否、readonly・alias拒否、staging失敗cleanup、正確な適用済みprefix、再実行no-op、CLI診断、適用成功reportのsize境界を検証。取得inventory外のhardlinkが既存alias禁止要件の漏れを露出したため、両live検証でopen済みfileのlink数を確認し、RFC 0036を日英で明確化した。実装と両host CIは完了し、最終文書公開は上記の別条件として追跡する。B04は旧source移行統合を維持する。

B02 local完了（2026-10-31 JST確認）: 実装`2a3aca1`は元spanを合成し、各rename後にentryと全宣言testを証明する。source固定全baselineの178 suite・2,529 test、fmt、厳密Clippy、rustdocが成功し、実終了codeを`/private/tmp/lang-m5-fixes-target/m5-validation/`に保持した。修正`5e2b593`はPOSIX専用test期待値を厳密native pathへ置換し、対象preview・整合性・fmt・Clippyが成功。公開には最新head CIが必要で、検証付き適用と旧source移行は別のB03・B05・B04成果とする。

B01 checkpoint（2026-10-30 JST）: [RFC 0044](rfcs/0044-checked-source-migration.md)でpreview既定のfix、検証付き公開、明示的な旧scalar profileを採択。保持した旧`shared`・`weak` sourceは42を返し、手動alpha-rename後も現VMで42を返してnative出力を生成する。移行commandとnative実行journeyは未実装。B01検証は日英整合性7/7成功、`1ab1201`としてcommit済み。作業branch `codex/m5-checked-source-fixes`は`main`（`3b676c5`）から作成し、PR baseを`git@github.com:carimatics/lang.git`の`main`として記録。その後PR #29の前提commit `8a43f76`へfast-forwardした。checkoutは`/private/tmp/lang-m5-fixes`。元worktreeはcleanを保ち、同PRのCIを待つ。

A02公開checkpoint（2026-10-31 JST確認）: 最新head `8a43f76`のCI `41072332781`とnative-debug `41072332783`（attempt 2）が成功した後、[PR #29](https://github.com/carimatics/lang/pull/29)を`1cdad09`としてmergeした。full routingにより文書専用jobだけを意図的に省略。元worktreeは`main`へ復帰し、cleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。remoteは`git@github.com:carimatics/lang.git`。A01・A02のlocal baselineは2,522 test、fmt、Clippy、分離targetのrustdocが成功。[Draft PR #30](https://github.com/carimatics/lang/pull/30)で別branchの検証付きfix previewを追跡し、その全local baselineは成功、修正headのCIは未確定。M5の9ゲートはopenを維持する。

## Exit gates

B05 budget監査（2026-10-31 JST）: 再利用するscalar検査内部の独立task構文分類3回をすべて計上する。各scanは共有budgetからphase許容量全体を予約し、新しい小source回帰testで不十分な総量budgetを拒否する。最初の`a55cda0`全体検証はこの修正のため意図的に中断し、`/private/tmp/lang-m5-fixes-target/m5-b05-validation-a55cda0-interrupted/`に保持した。成功baselineとは数えず、修正後の固定commitで全体を再実行する。既存の課金要件を満たす修正で、新規delivery・見積り変更なし。

| Gate | 必須成果 | State |
| --- | --- | --- |
| G1 | edition・非推奨方針・互換性表 | done |
| G2 | edition移行と機械適用可能な修正 | done |
| G3 | upload前のregistry脅威モデル採択 | done |
| G4 | storage・index・download・immutable version | done |
| G5 | 認証publish・yank・owner操作とaudit | open |
| G6 | 署名release manifestとpackage provenance | open |
| G7 | portable WebAssembly component plugin target | open |
| G8 | Wasm sandboxとcapability適合 | open |
| G9 | 文書hostingとmetadata検索（P1） | open |
| G10 | 公開LLVM release backend評価（P1） | open |

ゲート完了: **3/10; 21/65 historical gate weight**。以下は残作業見積りであり、実時間やロードマップのゲート重みではない。

## Deliveries

| ID | State | Remaining | Needs | Gates | 観測可能な受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M5-A01 | done | 0 | — | G1 | edition・非推奨方針と領域別互換性表を採択 | `b654ae7` |
| M5-A02 | done | 0 | M5-A01 | G1 | CLIで互換性を表示し未知editionと旧consumer境界を検証 | `a473cc0` |
| M5-B01 | done | 0 | M5-A01 | G2 | 移行レシピ・適用可能性・書込みトランザクション契約を採択 | `1ab1201` |
| M5-B02 | done | 0 | M5-B01 | G2 | 元spanを保持した実ソース修正previewと意味再検査 | `2a3aca1` |
| M5-B03 | done | 0 | M5-B02 | G2 | CLI修正適用とstale・競合・部分書込み拒否fixture | `01b425e` |
| M5-B05 | done | 0 | M5-B01 | G2 | 旧scalar構文を受入れfresh renameを証明し実在する旧sourceをpreview | `7f41dd0` |
| M5-B04 | done | 0 | M5-B03,M5-B05 | G2 | edition移行CLIと両hostの移行前後実行journey | `62e742c` |
| M5-C01 | done | 0 | M5-A01 | G3 | identity・namespace・authority・濫用・攻撃者の脅威モデル | `94a5200` |
| M5-C02 | done | 0 | M5-C01 | G3 | upload前にprotocol・上限・復旧・security受入ケースを採択 | `53376e5` |
| M5-D01 | done | 0 | M5-C02 | G4 | 上限付き未信頼archiveの受入とtraversal・重複・bomb拒否 | `b3c8d1b` |
| M5-D02 | done | 0 | M5-D01,M5-E01 | G4 | 内容address方式のstorageとimmutable version公開transaction | `e76dd7b` |
| M5-D03 | done | 0 | M5-D02 | G4 | versioned indexとidentity・checksumを検証したdownload | `f58659c` |
| M5-D04 | done | 0 | M5-D03 | G4 | registry依存を解決して両hostで実行 | `a289641` |
| M5-D05 | done | 0 | M5-D04 | G4 | lock・offline replay・公開中断・並行immutable検証 | `43dea74` |
| M5-E01 | done | 0 | M5-C02 | G5 | 認証identity・credential取扱い・scope付き認可 | `56f9f84` |
| M5-E02 | done | 0 | M5-E01,M5-D02 | G5 | owner・競合制約付きhost publish・yank・unyank transaction | `f40d643` |
| M5-E03 | doing | 1 | M5-E02 | G5 | owner追加・削除・移管と最後のowner・credential失効fixture | — |
| M5-E04 | todo | 3 | M5-E03,M5-E05 | G5 | 永続audit trail・復旧・権限外操作のend-to-end検証 | — |
| M5-E05 | todo | 3 | M5-E02,M5-E03 | G5 | 認証HTTPS mutation・要求作業memory上限・read世代更新 | — |
| M5-F01 | todo | 3 | M5-C02 | G6 | package信頼root・provenance・署名・freshness契約を採択 | — |
| M5-F02 | todo | 3 | M5-F01,M5-D02 | G6 | 署名package release manifestと検証可能provenanceを生成 | — |
| M5-F03 | todo | 3 | M5-F02,M5-D03 | G6 | 消費前に信頼・artifact結合・期限・replayを検証 | — |
| M5-F04 | todo | 3 | M5-F03 | G6 | 鍵更新・失効・改竄と署名packageの両host journey | — |
| M5-G01 | todo | 3 | M5-A01 | G7 | Wasm component lowering・interface対応・runtime profileを採択 | — |
| M5-G02 | todo | 3 | M5-G01 | G7 | scalarソースpluginを検証可能componentにcompileし実行 | — |
| M5-G03 | todo | 3 | M5-G02 | G7 | 型付きhost import・result/error・resource adapterの往復 | — |
| M5-G04 | todo | 3 | M5-G03 | G7 | package・CLIでportable component生成と両host plugin load | — |
| M5-G05 | todo | 3 | M5-G04 | G7 | 採択plugin profileのVM・native・Wasm適合 | — |
| M5-H01 | todo | 3 | M5-G01,M5-C02 | G8 | sandbox capability方針とresource上限失敗契約を採択 | — |
| M5-H02 | todo | 3 | M5-G03,M5-H01 | G8 | ambient filesystem・network・environment・process・FFIを拒否 | — |
| M5-H03 | todo | 3 | M5-H02 | G8 | memory・fuel・stack・host call上限とcancel・cleanup | — |
| M5-H04 | todo | 3 | M5-H03,M5-G05 | G8 | 不正component・escape・DoS corpusと両host適合 | — |
| M5-I01 | todo | 2 | M5-C02 | G9 | 文書hosting・検索metadata・未信頼content方針を採択 | — |
| M5-I02 | todo | 3 | M5-I01,M5-D03 | G9 | sanitizedなversion結合package文書を公開しfixture実行 | — |
| M5-I03 | todo | 3 | M5-I02 | G9 | 上限付きmetadata検索・version/yank可視性・攻撃query検証 | — |
| M5-J01 | todo | 2 | M5-A01 | G10 | LLVM評価corpus・debug基準・再現環境を定義 | — |
| M5-J02 | todo | 3 | M5-J01 | G10 | corpus用に比較可能LLVM release backend実験を構築 | — |
| M5-J03 | todo | 3 | M5-J02 | G10 | compile・runtime・startup・size測定と実debugger動作確認 | — |
| M5-J04 | todo | 2 | M5-J03 | G10 | benchmark artifactと限界付き採用判断を公開 | — |
| M5-Z01 | todo | 3 | M5-A02,M5-B04,M5-D05,M5-E04,M5-F04,M5-H04,M5-I03,M5-J04 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | 署名ecosystem・移行・pluginの連続journeyを両hostで実行 | — |
| M5-Z02 | todo | 3 | M5-Z01 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | 全exit条件を監査し日英文のcapability guideを公開 | — |

## 見積り照合

**112 − 6 − 36 = 70 pt**

| 基準残作業 | 純追加・再見積り | 完了時に除去 | 残作業 |
| ---: | ---: | ---: | ---: |
| 112 pt | −6 pt | −36 pt | 70 pt |

## Delivery noteとロードマップ監査

- 見積りvalidator保守（2026-10-31 JST）: 負の純再見積りでunsignedのみを想定したparserの制約が判明した。既存validatorで基準・完了・残作業の非負pointと符号付き純追加を分け、算術・表の符号・行合計・翻訳の検査を維持する。誤った合計、矛盾する表の符号、負の残作業を拒否し、repository consistencyは8/8成功。日常的な追跡修正でありdelivery・工数pointは追加しない。

- D02準備（2026-10-31 JST）: [RFC 0048](rfcs/0048-registry-state-and-publication.md)で厳密generation/audit/receipt record、全commit履歴保持、orphan cleanupを実験段階委任で採択し、storage実装は未開始。レビューで不変名への直接書込みをsync済みstagingへ変更し、完全な遷移検証と明示的な公開結果不明を追加した。拡張したlocal macOS probeはobject/audit/state/HEADの部分書込みを含むprocess終了13切断点に成功し、初期の全書込み後6切断点を置き換えた。配置済み不変candidateは全件完全でdigest一致し、可視stateは完全な旧状態か新状態だった。Windows・電源断・実認証・lock・完全な敵対的recoveryは未検証。固定baseline/CI実行中のE01だけをdoingとし、D02は既存3 ptのtodoを維持。Delivery・見積りの追加・退役・削減なし。準備作業は固定checkoutから分離している。

- Registry順序監査（2026-10-31 JST）: 選択済みD01だけをdoingに維持。既存計画のE01認証／credential／scope境界をD02より先に実装し、不変公開がlock内の最新stateで再認可できるようにする。D02はD01・E01へ依存し、E01の依存は採択済みC02のまま。Memory内transactionの探索ではcrash／retryの6切断点と異payloadの両順序を確認し、古い事前認可で失効後のpublishを許可できない理由を示した。これは設計証拠のみでfilesystem・認証実装ではない。既存installerのfile／lock処理は参考になるが、そのstateにはregistry owner／release／auditのatomicityがない。新規delivery・見積り変更なしで残90 ptの照合を維持する。予定するE01／D02の2前提後に次のsource実行fixtureを再監査する。

- B04完了（2026-10-31 JST）: local全baseline・最終head CI・PR merge確認後に最終1 ptを退役。退役見積りは17 pt、純追加は+1 ptを維持する。C01だけをdoing（3 pt）とする。新規作業・範囲削減なし。G2完了、8ゲートはopen。

- B04受入監査・再見積り（2026-10-31 10:34 JST）: 実旧source・現VM・link済みnativeのjourneyが両hostで成功し、report・source hashも独立に照合した。残る最終検証・公開を2から1 ptへ下げ、純追加・再見積りは+3 − 1 − 1 = +1 ptとする。B04は未完了のため退役見積り16 ptは維持する。delivery・gate・受入条件の削除なし。

- B04 checkpoint・再見積り（2026-10-31 JST）: CLIと非公開の単一source公開adapterで移行fixtureを実行できた。残作業は両hostの旧producer・現VM・native journeyと最終検証のため、受入条件を削除せず3 → 2 ptへ下げる。純追加・再見積りは+3 − 1 = +2 ptとし、B04はdoingなので退役見積り16 ptは変えない。新規delivery追加なし。

- B05完了（2026-10-31 JST）: `7f41dd0`の全local・同head両host検証後、最後の3 ptを退役。追加・再見積りなし。次はB04で、上記統合監査後も3 ptを維持する。明示CLI・検証付き適用・実旧producerと現VM・nativeのjourneyが両hostで成功するまでG2はopen。他の8 openゲートとP1の両成果も必須を維持する。

- B03完了（2026-10-31 JST）: code `01b425e`、全local baseline 2,536/2,536とfmt・Clippy・rustdoc、両host CIが成功。最終3 ptを退役し、追加・再見積りなし。B05・B04の旧source受入、検証付き移行CLI、両hostでの実旧producer・現VM・native journeyが成功するまでG2をopenとする。次はB05。他の全ゲートとP1成果も必須を維持する。

- 旧source受入準備（2026-10-31 JST、delivery退役なし）: 11件の[固定producer probe](../evidence/2026-10-31-m5-scalar-admission.json)でblock scopeと関数全体の名前一意性、不変parameter、明示local型・`mut`、localによるcallable隠蔽、到達不能名の検証、必要なreturn経路、前方再帰、任意の`else`、parameter・argument末尾commaを確定した。RFC 0044の日英版に既存要件を記録。B05で予定する解決名・型の証明が必要であることを確認し、追加前提・3 ptの見積り変更なし。B03は全local検証の終了まで唯一のdoing行を維持する。

- B02 local完了（2026-10-31 JST）: 実装`2a3aca1`のsource固定全baseline（2,529 test、fmt、厳密Clippy、rustdoc）が成功。修正`5e2b593`はnative pathの厳密比較を維持し、対象検証が成功。最後の3 ptを退役し、追加・再見積りなし。G2はopen。最新head CI・PR mergeは公開条件として未完了。B03だけをdoingとし、新規planのCLI・共通writerを`codex/m5-checked-source-fixes`上の一貫した続きとして進める。

- B01完了（2026-10-30 JST）: `1ab1201`でRFC 0044を実験段階委任により採択し、未検証の実装前提を明示した。日英repository consistencyは7/7成功。validatorがNext markerの変更を検出したため既存markerへ戻した。B01の最終2 ptを除去し、別に監査した前提+3 ptは維持する。G2は未完了、doingはB02のみ。

- B01範囲監査（2026-10-30 JST）: 実在する旧仕様からの遷移には、現editionのLNT0001 fixに加え、上限付きscalar readerと解決名の等価証明が必要。実装前にB05を3 ptで明示する（純追加+3、計40 delivery）。受入条件は実在する旧`shared`・`weak`のpreview。B04はB03・B05後のCLI統合、検証付き適用、両host VM・native実行用に3 ptを維持する。B02の3 ptは宣言test captureと総量proof課金を含み、B03の3 ptは既存公開guardを再利用する。この範囲監査でゲートを削除・完了しない。

- 開始監査（2026-10-30 JST）: 元の10ゲートすべてを維持し、P1の先送りなし。積上げ39件、112 pt。upload受入より先にsecurityを設計する。registry署名信頼はRFC 0042のtoolchain配布と別契約。Wasmはcontainer生成だけでなくソースpluginを実行する。LLVMゲートは根拠に基づく評価であり、採用は必須ではない。
- 順序: 互換性方針 → 実行互換性fixture → 移行・修正 → 認証・署名registry消費 → portable plugin実行・sandbox → hosting・検索とbackend評価 → 両host連続journey・exit監査。独立ゲートも維持する。実装doingは最大1行、前提を追加する前に再監査する。
- 移行を実演する目的だけで新editionや破壊的構文を作らない。採択済みレシピと意味のある移行前後実行証拠を使い、未対応の遷移は拒否する。新format・protocolは実装前に独立したRFC採択を要する。
- 完了には日英`milestone-5-capabilities.md`、正確なCI head証拠、PR merge、開始branchへの復帰・同期を要する。この開始記録はexitゲート完了ではない。

出典: repositoryのroadmap・architecture・採択済みRFC。2026-10-30 JST確認。更新日を持たないソースファイルの公開日は推測しない。


A01完了（2026-10-30 JST）: 日英RFC 0043を委任採択し、既存editionのみを受入れる方針、非推奨手順、領域別互換性を定義。repository consistency 7/7成功。最終見積り2 ptを除去し、追加・再見積りなし。ロードマップの全10ゲートは引き続き必要。

B01準備（2026-10-30 JST、delivery完了への変更なし）: 2 moduleのstandalone sourceと、同じbytesにedition-2026 manifestを付けたpackageはどちらも42を返した。これはpackage化であり、edition間移行の証拠ではなく、これだけでG2を完了できない。実stdio LSPは`unused`だけを`_unused`へ変更するversion付き`LNT0001` quickfixを返し、適用後もinitializerを保持して42を返し、lintが解消した。次のfix CLIでは既存の意味rename証明を再利用する。現editor snapshotはroot entryを扱うため、B02では宣言testのcoverageを明示的に扱う必要がある。契約案はB01レビューまで採択RFC集合の外に置く。見積り・exitゲートの変更なし。

A02完了（2026-10-30 JST）: 実装`a473cc0`は上限付きhuman・JSON互換性reportを公開し、manifestと有限edition受入を共有する。保持した直接終了codeでfmt・Clippy、全workspace test 2,522件（177 suite、失敗0）を確認した。最初のrustdocは古い依存metadataを参照して`NativeContext`・`Method`のAPI不一致になったが、同じsourceが分離targetの`cargo doc --workspace --no-deps`で成功した。detached再試行では観測用一時directoryの消失も確認し、永続する`/private/tmp`で解消した。logと実終了codeはignoreされた`target/m5-validation/`に保持し、観測timeoutは成功と数えない。実装headのCI `41064851495`とnative-debug `41064851345`は旧consumer境界を含め成功。3 ptを除去し、追加なし。G1を完了（gate重み5/65）とし、他9ゲートと全P1成果は維持する。文書公開もそのPR最新headのcheck成功後にmergeする。

B01の歴史的移行証拠: [保持したprobe](../evidence/2026-10-30-m5-legacy-source-probe.json)は架空の後継editionではなく、実在する`69b1c4d`（2026-10-17）のRFC 0006 sourceを使う。`shared`・`weak`識別子はlockを固定した旧compilerで42を返すが、現compilerでは`SRC0024`が4件出る。その旧libraryから生成した315 byteのartifactは現compilerでも42を返す。これを旧profileから2026への実移行fixtureとする。B01は明示profile受入を定義し、capture・renameの前提作業を実装前に明示・再見積りする。通常compilerの現予約語拒否は維持する。この実験はG2完了ではない。

