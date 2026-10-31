<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0050-registry-cache-and-offline-replay.md -->

# RFC 0050: Registry cache世代とoffline replay

[English](../../rfcs/0050-registry-cache-and-offline-replay.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: [M5-D05](../milestone-5.md)
- Supersedes: なし。RFC 0046とRFC 0049を詳細化する
- Superseded by: なし

## Summary

独立して許可したprivate cacheに、registry archiveの厳密なbyteと完全なlock-schema-3世代を永続化する。Cache世代全体の完成後にのみroot lockを公開する。Networkなしで指定世代を再openし、再利用する全byteを検証して、明示したoffline trust adapter経由で既存の固定graphへ渡す。Cache byteやlock checksumはpackageを認証しない。

## Motivation

D04は不変registry入力を実行してcanonical lockをcaptureするが、archive byteはmemory内だけにある。再起動後、serviceが消えた状態ではreplayできない。個別にdownloadしたfileのdirectoryだけでは、1個のroot lockに必要な全archiveが完全に公開されたことも示せない。D05には上限付きcache transactionと、download → publish → service停止 → reopen → VM/nativeの観測可能なjourneyが必要である。

## Goals

- 再起動、中断、競合writerをまたいで厳密なorigin/root-policy/name/version/archive identityを保持する。
- Lock schema 3、archive受理、既存graph/source/compiler pipelineを再利用する。
- Cache変更、root-lock出力、独立trust権限を明示して分離する。

## Non-goals

依存解決solver、展開、保持世代のGC、第2compiler pipeline、自動cache探索、本番trust root、公開の未署名/offline CLIは対象外。F01–F03が、本番消費を有効にする前に署名receipt、その永続保持、鮮度/失効検証を定義する。

## Guide-level design

信頼するhostがlocal source・root-lock出力directoryとは別にprivate cache directoryを許可する。Unlocked操作は明示registry bindingと封じた消費証拠で完全graphを解決し、そのcache世代とroot lockを公開する。Locked offline操作は既存root lockとcache read権限を指定し、選択した不変archive byteをcaptureして同じlocal入力を解決し、新たなcapture lockを元byteと照合する。Data欠落、local入力・target/compiler・origin/root policy・archive identityの変更は、出力を書かず別releaseを選ばずに拒否する。

Cache APIはDNS、network、環境credential探索、credential provider呼出しを行わない。Offline trust adapterは明示許可されたlocal証拠だけを独立に検証し、既存の封じた消費proofを返す。D05の隔離された未署名adapterは`cfg(test)`に限定し、保存recordから選択できない。本番proof constructorやfallbackを追加しない。

## Reference semantics

### Private cache profile 1

初期化は明示directory capability配下のhost指定childを新規・排他的に作成する。Openは既存rootを初期化・resetしない。Root entryは通常file `LOCK`、通常file `FORMAT`、実directory `objects`、`generations`、`staging`だけとする。実open境界でsymlink/reparse alias、非通常file、複数hard link、未知entry、未対応profileを拒否する。非blocking排他lockで変更と復旧を直列化する。Read-only captureは共有lockを取得してよいが、cleanupや書込みはしない。Windowsでlock中のlock file長はmetadataで確認し、別handleから読まない。

`FORMAT`は次のUTF-8 byteと完全一致し、2 space indentと最終LFを含む。

```json
{
  "format": "lang-registry-cache",
  "schema": 1
}
```

独立markerは53 byte、SHA-256は`60488014b120ab4f7b157fb97abe6af19c7efc12b80976d43d8740083f5c368f`。未知・欠落markerは拒否し、移行を推測しない。初期化中断で不完全private rootが残った場合は、operatorの明示削除/復元を必要とし、通常openは推測修復しない。

`objects/HASH`は厳密なRFC 0046 archiveを1個保持し、HASHはその小文字64文字SHA-256とする。`generations/HASH`は最終LFを含む完全canonical RFC 0049 lock-schema-3 recordを1個保持し、HASHはそのbyteのSHA-256とする。第2世代manifestや可変cache HEADは置かない。保存pathは固定namespaceと検証済みdigestだけで構成し、source path・package名・originをhost cache pathにしない。Rootの`Lang.lock`が自身の厳密digestで完全世代を選択する。

既存の独立schema-3 fixtureの`<compiler_build_id>`だけを64個のASCII zeroに置換すると3,664 byteで、世代digestは`03bf5a6f4df5180f7915ea02bab6e44693bdae4fbdf787c44ad3c97d69781360`となる。Zeroはformat fixture用の合成入力であり、現在のcompiler identityではない。Package checksumと元archive byteはRFC 0049のままとし、実行可能replayでは実compiler identityを使う。

### Admission and identity

変更・cleanup前に上限付きinventory全体を受理する。保持する全世代のcanonical schema・digest・registry archive参照を検証し、参照archiveの長さ/hash、descriptor identity、元source inventoryを照合する。未参照candidateを含む全不変objectを検証してからcleanupを検討する。全保持世代を通してpolicy/name/version locatorは1個のarchiveに結び付け、競合digestを拒否する。Object byteを共有しても異なるorigin/root policyは別identityを維持する。Local専用lock schema 1/2をcache世代にはしない。

RFC 0046の独立した物理storage 4 GiB・entry 262,144上限を維持し、metadata・directory・既存file・staging・install時のpeak spaceを変更前にchecked arithmeticで計上する。Archive/descriptor上限とlock 1 MiB上限を維持する。選択世代には追加で保持raw archive合計64 MiB上限を設け、割当て/read前に計上する。既存graphはsource 16 MiBとpackage/file/edge数をそれぞれ制限する。Raw byte追加上限はsource上限を増やさずdescriptor/TAR overheadを収容する。拒否entryと復旧stagingを含め、列挙を有界にする。

### Publication, interruption and recovery

Cache lock下で、現inventory検証、完全graph/lock capture、不変locator競合とpeak quota確認を行い、必要な全archiveを排他的作成・完全書込み・sync済みstaging file経由でinstallする。既存不変名は検証済み厳密byteと一致する必要があり、競合objectは置換しない。全参照の完成後にcanonical世代をinstallする。その後でのみ、独立output capabilityを通じてroot lockをstage・sync・atomic replaceする。Root置換が操作の可視化点であり、cache installだけではroot公開成功を返さない。

Staging名はobject/generation用の固定kindとdigest componentで構成し、packageが選ぶpathを含めない。Root置換前の中断では旧root lockが残り、置換後は完全不変世代を参照する。Root置換が起きた可能性のある失敗は結果不明を報告し、versionを変えたり別identityを盲目的に開始せず、同じ予定lock byteをrootと完全世代で照合する。厳密retryは不変byteを再利用する。競合writerはlock取得またはbusyを返し、取得後に再検証する。

復旧はstaged/最大世代の昇格、欠落root lock再構築、trust state reset、保持完全世代の刈込みを行わない。全体受理後、変更を伴う復旧が削除してよいのは、既知の有界な通常staging fileと、全保持世代から未参照の完全digest検証済みobjectだけとする。外部root lockを列挙しないため、root commit前に残った完全世代も保持する。未知・破損不変fileはcleanup前に拒否する。Root directoryのstage fileは既存root-lock出力契約に従い、cache復旧にsource directory掃除権限を与えない。File syncと同一filesystem上のatomic replaceを必須とするが、両platformでの電源断耐久性は約束しない。電源断後の欠落・破断dataは検出して拒否する。

### Frozen offline continuation

明示root lockが厳密に1個の受理済み世代を選択する。Cache lock下で所有archive byteをcaptureし、再利用のたびに長さ/hashと完全identityを検証する。以降のgraph実行は可変cache pathを再openしない。呼出側の独立registry bindingはlockの型付きorigin/root-policy recordと一致させる。Cache adapterは選択世代の厳密releaseだけを公開し、任意objectや新しい世代を公開しない。既存Graphで解決後、実行/出力公開前にcanonicalな`Lock::verify`完全一致を要求する。構造受理だけでは不十分である。

本番trust証拠は引き続きF01–F03が別途規定する。そのprofileで世代を消費可能にする前に、verifierが必要証拠を独立認証して永続保持し、offline replayも現receipt/期限/失効policyを適用しなければならない。Cache profile 1はsource byteとlockを保持し、仮の署名/trust-receipt形式を作らない。全cache hashが一致しても証拠欠落は拒否する。厳密な認証済みyank継続と新たに観測したquarantineはRFC 0046に従い、D05独自の鮮度例外を設けない。

## Execution modes

同じcapture source provider・typed MIR・選択targetをVM/native実行に渡す。Cache identityはguestのfilesystem・network・process・native-plugin権限を与えない。Wasm lowering、ABI、実行意味論は変更しない。

## ABI and compatibility

実験的private cache profileの追加であり、manifest/lock/archive schemaの変更ではない。既存local lock schema 1/2とsnapshot profile 1の動作を維持する。旧readerは新markerを拒否し、自動移行・ambient cache採用・trust resetを行わない。将来のtrust-receipt形式は本番有効化前に独自の受理済み契約を必要とする。Native/component ABI、edition、bytecode、serialized MIRは変更しない。

## Diagnostics and tooling

不正/未対応cache recordはREG0001、quotaはREG0002、不変identity競合はREG0005、byte/trust不一致はREG0006、破損/不完全永続stateはREG0007、busy/I/OはREG0008を使う。確実な未commitと公開結果不明を型付きで区別する。Package境界を通るsource/lock診断は既存codeを維持する。Messageは有界としcredentialを表示しない。意味toolingは固定入力を使い続け、cache pathをeditor書込み権限にしない。

## Performance and cost model

有界inventory全体の検証は保持storageに線形となり得、throughputは約束しない。1回のinventory capture内ではobject検証を重複排除するが、後の再利用時には実byteを再検証する。選択archive bufferと固定source inventoryは所有・有界とし、cache全体の展開やmemory複製を避ける。測定前のbenchmark主張は行わない。

## Security and trust boundaries

Cache byteを敵対的入力として扱う。Digest名と閉じたcanonical recordは整合性/identity一致を示すだけで、作者や現権限を証明しない。Cache/lock fieldからfilesystem/network権限を得ない。元source byte、型付きlocal binding、封じたregistry証拠を分離する。既存のcapability相対・no-follow・staged-write・lock手法を使い、unsafeやnative plugin sandboxの仮定を追加しない。

## Alternatives

- Cacheなしでは再起動/offline実行を提供できない。
- 一般download directoryや外部cache libraryはbyteを保持できるが、固定graph全体・現trust policy・root公開を結び付けられない。
- 可変cache index/HEADが最も強い競合layoutだが、内容アドレス付き完全lock世代は第2の可変可視化点を避け、schema 3を再利用する。
- 展開したlocal packageはfilesystem toolingには単純だが、型付き不変originを失いcache編集を招く。所有受理済みarchiveはD04の境界を維持する。

## Implementation plan

既存D05内でprofile受理/capability storageと厳密世代公開を実装し、隔離HTTPS-to-cache-to-offline VM/native journey、実process終了・競合writer caseへ進む。D04公開条件を満たすまでD04を唯一のdoing実装に維持する。この準備でdeliveryやpointを追加しない。後続F01–F03が、未署名fallbackなしで認証済み永続証拠と本番adapterを提供する。

## Test plan

独立marker/世代digest、不正/未知/欠落entry、canonical byte、source/hash/identity不一致、別policyの同一byte、不変version競合、厳密quota境界を検証する。実serviceを停止してからcacheを再openしVM/nativeで42/84を実行し、network/credential経路が呼ばれないことを示す。Cache byte欠落/破損やlocal入力・root/target/compiler変更は旧出力を変えず拒否する。Create/partial-write/sync/install/root-replaceの各process境界、厳密retry、結果不明、競合processを両hostで実行する。Cleanup前の破損検証、alias拒否、cache除去後の固定実行を確認する。全compiler baselineと最新head CIを行い、未署名fixtureでG6 trust完了を主張しない。

## Open questions

2026-10-31 JST、RFC 0001の実験期委任でaccepted。内容アドレス付き完全lock世代、明示capability、厳密再検証、単一root公開境界により、別source modelや可変indexを追加せず既存D05成果を満たせると判断した。Markerと合成世代digestは独立計算であり、cache実装・両host中断/競合動作・性能は未検証。F01の署名証拠/鮮度policyは別gateに残し、暗黙defaultとはしない。

## Sources

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md)、最終更新2026-10-29: root-lock出力とlocal graph上限。
- [RFC 0046](0046-registry-protocol-and-recovery.md)、[RFC 0048](0048-registry-state-and-publication.md)、[RFC 0049](0049-registry-package-integration.md)、最終更新2026-10-31: archive/cache quota、不変公開/復旧、固定package identity。2026-10-31 JST確認。本RFCのlayoutとraw世代上限はproject判断であり、新たな外部仕様や耐久性を主張しない。
