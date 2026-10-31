<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0049-registry-package-integration.md -->

# RFC 0049: 固定package graphへのregistry依存統合

[English](../../rfcs/0049-registry-package-integration.md)

- Status: accepted
- Authors: Codex, under the owner's experimental design delegation
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-D04 and M5-D05
- Supersedes: None; extends RFCs 0034, 0035 and 0046
- Superseded by: None

## Summary

Manifest schema 2のregistry edgeと、registry sourceを含むpackage graph向けのlock schema 3を追加する。既存の固定graph、source provider、compiler pipelineを再利用する。Registry identityは独立設定のserviceとtrust-root policyであり、cache pathnameではない。取得sourceは不変で、filesystem編集authorityを得ない。

既存manifest schema 1、local lock schema 1/2、source edition 2026、bytecode/native形式の意味は維持する。D04ではまず隔離した未署名test fixtureで依存実行の全経路を証明する。本番消費には引き続きF01–F03が必要で、未署名CLI switchやfallbackは設けない。

## Motivation

RFC 0046はD04前に明示manifest/lock詳細化を要求する。現在のgraphは全packageにfilesystem metadataとdirectory handleを要求するが、受理済みarchiveは既に所有された不変inventoryである。展開してlocal manifestを合成するとprovenance、source identity、編集authorityを混同する。

2026-10-31のlocal実験では実archiveを公開し、検証付きHTTPSでindex/archiveを読み、serviceを削除してから固定依存を既存VM/native JITで実行し、両方42を返した。2つの明示source namespaceは元のCRLF/UTF-8を保って84を返す。未宣言依存はgraphを作らず`SRC0013`となる。URL形の診断pathは既存MIRの`InvalidPath`検証で拒否される。これは実行可能性の結果であり、manifest/resolver/offlineやWindows実装の証拠ではない。

## Goals

- 正確に宣言されたlocal/registry依存を1つの上限付き不変graphへ解決する。
- 元bytes、package namespace、span、target条件、VM/native等価性を保つ。
- 独立service/root policy、source、compiler identityをcanonical lockへ固定する。
- 既存local挙動を維持し、registry入力を読取専用にする。

## Non-goals

Version range、依存solver、build script、origin自動発見、registryからlocalへのedge、archive展開、新言語/backend、署名policy、本番trust root、公開未署名consumerは対象外。D05がcache公開/offline replayを実装し、F01–F03が認証済み本番消費を別途確立する。

## Guide-level design

Schema 2はschema 1のpackage/workspace/tests fieldと上限を維持する。各依存は2つのclosed source形式から明示選択する。

```json
{
  "schema": 2,
  "package": {"name": "app", "version": "1.0.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {
    "helper": {"source": "local", "path": "helper", "version": "1.0.0"},
    "math": {"source": "registry", "registry": "example", "name": "math", "version": "1.0.0"}
  }
}
```

どちらも既存の任意`targets`配列を持てる。Registry aliasはpackage名と異なってよい。Local aliasは名前一致規則を維持する。Versionは完全一致のまま。Root-levelの`mod math;`はその宣言済みactive edgeだけを選ぶ。ManifestのURL、credential、trust root、cache directoryはnetwork/filesystem authorityを与えない。信頼するcallerが明示service/root-policy bindingとsource adapterを渡し、binding欠落はnetwork lookup前に拒否する。

既存local callerは継続利用できる。認証済みregistry受理を持たないcallerはpackage診断でregistry graphを拒否する。Checksum、HTTPS成功、lock、archive descriptorだけでは受理条件を満たさない。

## Reference semantics

### Manifest admission and resolution

Schema 1の厳密文法は維持する。Schema 2は全依存の`source`を必須とし、`local`は`source,path,version,targets`だけ、`registry`は`source,registry,name,version,targets`だけを許す。必須fieldは推測しない。任意fieldの省略とnullの区別も維持する。既存64 KiB/128項目上限でduplicate/unknown field、非対応schema/edition/target、不正name/version/pathを拒否する。Registry識別子はRFC 0046の文法に従う。Schema 2はschema-1 local packageを参照できるが、registry archiveはmanifest schema 2ではなくRFC 0046 schema 1のままとする。

Host bindingはregistry識別子を厳密に1つの`ServiceIdentity`と32-byte root-policy SHA-256へ対応付ける。Keyは`ServiceIdentity::registry()`と一致しなければならない。Digestは独立trust設定を識別し、その厳密な認証済み計算方法はF01で定める。任意bytesの受理はtrustの証明ではない。競合bindingは拒否する。Descriptorもlockもbindingを追加・置換できない。

Active edgeをalias順に解決する。Local edgeはcapability-relative captureと厳密name/version確認を維持する。Registry edgeは設定serviceから要求された厳密releaseを選び、新規解決ではyanked/quarantined versionを除外する。Descriptor edgeはその厳密`archive_sha256`も要求し、registry packageはlocal path edgeを導入できない。Graph受理前にindex/archiveの全重複fieldを照合する。1つのservice/root-policy/name/version locatorは1つのarchiveへ解決し、競合digestは別identityを2つ作らず拒否する。共有diamondは同じ受理済みpackageを再利用する。全source bytesが一致しても別origin/root policyは別identityとなる。Cycle拒否、上限付き反復走査、256 package/module、4,096 edge、16 MiB source計上を維持する。繰返しvisitもedge/走査上限へ、保持する全archive source fileもgraphのsource inventory上限へ計上する。

解決、hash、parse、型検査中に依存codeを実行しない。Compile後もcallerの元の実行controlとhost grantが必要である。

### Frozen inputs and logical source paths

Local captureと受理済みregistry archiveを別の入力kindで表す。Registry packageは合成`Lang.json`ではなく元のdescriptor/source bytesを保持する。元filesystem metadata/handleはlocal入力だけに属する。Registry source identityには完全な受理済みrelease identityと元pathを含め、架空device/inode値は使わない。Captureの成功/失敗は既存graph epochで固定する。後続analysisはcache pathを開き直さず、registryをrefreshしない。

Local-only graphは既存source key/displayを維持する。Registry入力があるgraphでは、全compiler source pathに相互分離した論理namespaceを使う。

- Local package key: authority-relative root `.`なら`local`、それ以外は`local/<root>`。Source keyは`local/<original-authority-relative-path>`。
- Registry package key: `registry/<policy-id>/<name>/<version>/<archive-sha256>`。Source/descriptor keyは元の受理済みarchive pathを追加する。

`policy-id`は`LANGREGISTRYPOLICY1`とNULに続け、registry識別子、canonical origin、小文字root-policy digestを、それぞれUTF-8 byte長のlittle-endian u64とともに連結した値の小文字SHA-256とする。この表記は論理source namespaceであり、cache配置やauthorityではない。型付きorigin recordと元local入力bindingで全実file操作を制御する。文字列prefixを取り除いてfilesystem authorityを復元してはならない。

Source scopeは引き続きmodule探索を受理済みentry/test directoryへ制限し、各packageの`root::`はlocalに留まる。選択entryからloadしない場合も保持する全fileを上限管理し固定する。元UTF-8 bytes、CRLF、spanはloweringを通じて維持する。論理pathはURLやbytecode schema変更を導入せず既存portable MIR path契約を満たす。

### Lock schema 3

Registry入力があるgraphはschema 3を使う。Local-only graphはlocal manifestがschema 2でも引き続きschema 1/2を使う。Lock/manifest versionは独立する。Schema 3は既存1 MiB上限、nesting上限16のclosed UTF-8 JSONで、2-space indentと最終LFを1つ持つ。Field順は次のとおり。

`schema`, `target`, `compiler_version`, `compiler_build_id`, `root`, `workspace`, `packages`。

`root`は選択local package key。`workspace`は必須で、standaloneならnull、それ以外は元authority-relative pathを使う既存schema-2 workspace record/checksum。Packageを論理package keyのUTF-8 byte順に並べる。各packageのfieldは次のとおり。

`id`, `origin`, `name`, `version`, `edition`, `metadata`, `sources`, `dependencies`, `bindings`, `checksum`。

Closed origin recordは2種類。

- Local: `kind`（`local`）、`root`（元authority-relative directory）。
- Registry: `kind`（`registry`）、`registry`、`origin`（設定済みcanonical HTTPS origin）、`root_sha256`、`archive_sha256`、`archive_bytes`、`descriptor_sha256`。

`metadata`は`kind`（`manifest`または`descriptor`）、`path`（論理key）、元bytesの`sha256`を持つ。Sourceは`path,sha256`でpath順、active dependencyは`alias,id`でalias順。Bindingはschema 1の`kind,scope,path,target`を維持し、論理keyを使い`(scope,kind,path,target)`順にする。参照する全package/source/scopeは存在し型付きoriginと一致しなければならない。Registry metadata/sourceは受理済みarchiveと厳密一致し、local bindingは元の取得済みalias関係を保つ。

`checksum`は`LANGPACKAGE3`とNULに続け、そのpackage recordから`checksum` fieldを省いたcanonical JSON（最終LFを含む）を連結した値の小文字SHA-256とする。Metadata/source digestが元bytesを固定し、origin、edge、bindingも対象になる。これはrecordを認証しない。Schema-3 writer/replay実装前に、独立計算したcanonical fixtureを記録する。旧schema-1/2 fixtureとchecksum algorithmは変えない。

[独立fixture](../../../tests/fixtures/package-lock/schema3-registry.json)はliteralの`<compiler_build_id>` placeholderを含み3,619 byte、SHA-256は`82d2766f6a3593ca68448c9a20da438f601d89af3ebd4ff326f41eeab8d5c91b`である。Replay testではそのplaceholderだけを実compiler identityへ置換する。[元入力](../../../tests/fixtures/package-lock/schema3-registry-inputs/)はlocal manifest/sourceとregistry descriptorを保持する。`math/source-bytes.json`は厳密source文字列をescaped CRLFとともに保存するtest入力用記憶域であり、新archive形式ではない。Rust archive受理で4,608-byte archiveの厳密hashを再現した。独立JavaScript/Python計算でpolicy ID `21ccc6bc6516ce62c75c6f34d3990e619db61fc8000ce53d452940845371d0c9`とpackage checksum `3cec99cdc1c8eadb7fd8badaf0a627d371821de53aba25943f77c134d2de2538` / `ac38271807a61aad98e890479af04eaf9e180f66aa5fe2c52e6e2896d189e6bd`が一致した。Root digestは合成fixture入力であり本番trust rootではない。この論理keyのlocal compileはVM/native JITで42を返し、registry形path suffix全体を持つlocal packageも別identityを保ち、組合せfixtureは84を返した。Schema-3本番encoding/replayは未実装である。

Lock構造検証はreplay/trust authorityを確立しない。Captureはanalysisと同じ固定graphから完全recordを導出する。Locked replayは厳密canonical bytesを比較し、現在必要なtrust証拠も独立に検証する。Source、binding、origin/root、target/compiler、選択workspaceの変更は旧outputを書き換えず拒否する。これらをcache pathから復元しない。

### Publication and offline continuation

D04は暗黙cacheを追加せずschema 3をcapture/受理検査する。D05はcache read/writeを個別にgrantし、再利用ごとに厳密bytesを検査し、root lock commit点より前に完全な不変世代を公開する。OfflineではDNS/network/credential lookupを行わず、bytes/証拠の欠落を拒否し、version/originを変えない。以前認証済みの厳密replayがyanked releaseを保持できるのはRFC 0046とF01の鮮度/失効規則の下だけであり、新たに観測したquarantineは拒否する。本番replayを有効にする前に、F01–F03が署名済み証拠とその永続保持を定める。

## Execution modes

既存package module provider、binding、型analysis、共有typed MIRを使う。VM/JIT/linked nativeは同じ固定programと元の実行grantを消費する。Registry metadataはnative import、allocator、task authority、callback、component bindingを供給できない。D04では実際にdownloadした依存をmacOS AArch64とWindows x86-64のVM/nativeで実行する。Native SDK/compiler一致条件は維持する。

## ABI and compatibility

Manifest schema 2とlock schema 3は明示的かつ実験段階のpackage形式追加である。旧readerは拒否し、既存schema-1 manifestとlocal lock schema 1/2は厳密replayを保つ。推測/自動migrationは行わない。Manifestをschema 2へ変える際は明示source tagを追加し、認可済みunlocked解決を行う。拒否時は以前のfileを保持する。Source edition、language/native ABI、component C v1、MIR、bytecode、SDK形式は変更しない。

RFC 0035のsource-snapshot archive profile 1はlocal-graph形式のままとし、そのprofileを暗黙に広げずhybrid graphを拒否する。RFC 0046のportable registry公開は別archive契約である。Snapshot profileを後で拡張する場合は別のcompatibility判断を要する。

## Diagnostics and tooling

元compiler診断code/spanと、schema/field/limit/authority/graph/lock失敗の既存package errorを維持する。Binding欠落、未認証消費、厳密identity競合を説明するが、remote応答textやcredentialをechoしない。論理registry pathは固定sourceを識別し、それだけで操作可能なphysical pathにはならない。

読取専用semantic toolingは固定graphを使える。Formatter/checkはlocal編集可能sourceを選び、registry依存はsemantic proofへ保持する。Registry編集の明示要求は拒否する。Local編集は元local path/metadataと既存再検証/公開境界を使う。Registry overlayやphysical file bindingが必要な操作は、明示immutable-source adapterがなければ拒否し、論理keyをfilesystem lookupへ変換しない。既存local editor/LSP/DAP挙動は維持する。本番registry toolingはbuild/runと同じ認証済みsource contextを運び、document URIから発見しない。

## Performance and cost model

完全identityごとに受理済みpackageを1つ保持する。共有diamondは所有archive記憶域を共有できる。別policy/originは別semantic identityでありgraph budgetへ計上する。無制限の作業を保持する前にsource bytes、metadata、path、edge、走査を計上する。論理namespace/hash metadataは有限の追加作業であり、高速化の主張ではない。Cache再利用もarchive/hash/trust検証を省略できない。展開やmutable fileからの2回目のsource parseは不要である。

## Security and trust boundaries

TLS/index/archive整合性、独立package trust、network/cache grant、guest実行authorityを分離する。本番のconsumed-archive型は構築を非公開にし、F01–F03 verifierに限定する。任意の受理済みarchiveをcallerが認証済み消費へ昇格できない。D04/D05 unit journeyでは`cfg(test)` fixture constructorが隔離未署名archiveを供給できるが、本番buildには存在せず、CLI/config/lock metadataから選べない。未知/欠落trust証拠は消費を拒否する。

Capture、lock、toolingを通じてsource originを型付きで維持する。Path表記や合成registry識別子はfilesystem編集capabilityにならない。実行/公開前にcycle、競合不変version、未宣言import、別origin、source scope外alias、敵対metadata、集約上限超過を拒否する。新しいunsafe codeやnative sandbox仮定は不要である。

## Alternatives

- 変更なし/手動downloadでは、宣言されreplay可能なregistry依存を提供できない。
- 別library/compiler pipelineはpackage identityとtooling proofを重複するため、既存graph/providerを再利用する。
- Archiveをlocal graphへ展開する案は有力な簡易統合だが、架空local manifestとmutable cache aliasの慎重な保護が必要になる。所有archive入力ならprovenanceを直接保てる。
- Schema 1の暗黙拡張やvirtual pathからのidentity導出は小規模だが、旧来の意味を変えpresentationとauthorityを混同する。明示versionと型付きoriginで独立境界を維持する。

## Implementation plan

1. D04: 厳密schema-2 edge、型付きlocal/registry graph capture、明示認証済みsource adapter、論理namespace、独立固定したschema-3 fixture/captureを実装する。実HTTPS依存と推移/共有graphを既存VM/native経路で両host実行し、local回帰と読取専用source境界を維持する。
2. D05: 個別grantによる不変cache/lock公開と厳密offline replayを実装し、中断、同時再利用、競合identity拒否を含める。
3. F01–F03: 本番registry消費CLI経路を有効にする前に、独立検証済みproduction trust/root/receipt構築を提供する。既存D-stage fixtureを未署名fallbackへ変えない。

既存deliveryを詳細化するもので、前提rowやpointは追加しない。D03の公開gateを通るまでimplementationのdoingはD03だけとする。

## Test plan

Closed schema dispatchと旧fixture、厳密推移/target条件依存、共有diamondと同名別origin/root identity、cycle/競合digest/未宣言import、source/path namespace衝突、元CRLF/UTF-8診断、registry/cache削除後のsource不変性、集約上限、local-only編集authority、canonical schema-3 fixtureと全固定入力の不一致を網羅する。実index/archive serviceからconsumerまでのVM/native fixtureを両対応hostで実行し、全baselineも実行する。D05はnetworkなしoffline証明とfilesystem crash/race caseを追加し、F01–F04は認証済み本番journeyを追加する。未署名fixture成功は後続trust gateを証明しない。

## Open questions

2026-10-31 JST、RFC 0001の実験的委任に基づきacceptedとした。元bytesのarchive/namespace実行と独立計算fixtureが統合設計を支える。型付き入力origin、明示physical binding、非公開consumed-archive構築、version付きmetadataが採択済みauthority/compatibility境界を保つ。D04のblocking設計事項は残らない。実装、両hostのgraph/拒否test、全検証は引き続き必要で、採択はそれらを証明しない。F01のroot-policy計算と署名済み証拠は別gateであり、性能/広いeditor統合は未測定である。Local実験から本番容量を主張しない。

## Sources

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md)、最終更新2026-10-29: local manifest、graph上限、lock schema、output公開。
- [RFC 0035](0035-source-package-snapshots.md)、最終更新2026-10-29: 独立したlocal source-snapshot archive契約。
- [RFC 0046](0046-registry-protocol-and-recovery.md)、最終更新2026-10-31: 不変archive/edge identity、network上限、D04詳細化、trust/offline gate。
- Repository実装`f58659c`、2026-10-31 JST確認: `src/package/{manifest,graph,source,lock,format}.rs`、source inventory、`crates/semantics/src/modules.rs`。Version固有の実装証拠であり外部標準ではない。
- Local実行実験`/private/tmp/lang-m5-registry-source-probe.rs`、SHA-256 `ca92d3c29cde8c2ef511a046221f045a597c1182fe3065ae8c4356c9befaddb3`、2026-10-31 JST確認。Local実行可能性だけの証拠であり、delivery完了にはrepositoryへ保持したtestで置き換える必要がある。
