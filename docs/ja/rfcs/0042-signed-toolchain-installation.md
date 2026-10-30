<!-- translation-of: ../../rfcs/0042-signed-toolchain-installation.md -->

# RFC 0042: 署名付きtoolchain installation

[English](../../rfcs/0042-signed-toolchain-installation.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M4-F01–F05
- Supersedes: none
- Superseded by: none

## Summary

明示rootのtoolchain managerを追加する。payloadを読む前にrelease metadataを認証し、実compiler／SDK fileをinstallし、channel／versionを選択し、失敗やprocess中断でもactive installationを保持する。M4のtoolchain配布であり、M5のpackage registryは実装しない。

## Motivation

現在のnative SDKはsource checkoutなしでassemble／検証できるが、署名配布、永続channel選択、復旧可能なupdate commandがない。新規developerにはsource buildと同じ検証済みcompiler／SDK、明確な初期trust判断、暗黙のshell／global設定変更をしない操作が必要である。

## Goals

- 公開前に厳密な署名、repository／channel／target binding、expiry、rollback floor、正確なpayload digestを検証する。
- macOS AArch64／Windows x86-64 MSVCで実compilerと対応SDKを分離installする。
- 有限storageと整合したactivationを持つinstall／update／select／list／run／which／uninstall／recoverを提供する。
- local署名配布と上限付きHTTPS transportを、実両host fixtureで検証する。

## Non-goals

trust rootの自動探索、公式production signing identity、既存root内でのtrust-root rotation、package registry signing、install script、任意archive展開、OS code signing／notarization、実行中manager自体の置換、暗黙PATH／shell編集、敵対的local管理者、電源断durability保証は初期profile外とする。test keyをproduction trust rootにしない。配布metadataの期限切れだけで既存toolを利用不能にしない。

## Guide-level design

bootstrap managerとroot fileは独立に信頼できる経路で取得する。同じ非信頼downloadで取得した鍵による署名検証だけでは初期trustを確立しない。明示root fileはlocal trust設定であり、自己認証されたdownload証明書ではない。

```text
lang toolchain identity
lang toolchain init --root DIR --trust-root FILE
lang toolchain verify --trust-root FILE --metadata FILE --channel stable
lang toolchain install --root DIR --source DIRECTORY_OR_HTTPS_BASE --channel stable [--version 0.1.0]
lang toolchain update --root DIR --source DIRECTORY_OR_HTTPS_BASE --channel stable
lang toolchain list --root DIR
lang toolchain select --root DIR --channel stable --version 0.1.0
lang toolchain which --root DIR
lang toolchain run --root DIR -- --version
lang toolchain uninstall --root DIR --channel stable --version 0.1.0
lang toolchain recover --root DIR
```

各commandは通常のhuman／JSON診断選択を持つ。installation操作はrootを明示し、ancestor／environment／global既定探索をしない。`identity`は現compilerの閉じたschema-1 record `schema`、`version`、`target`、`compiler_build_id`だけを返し、installation rootを必要としない。`init`は新rootを排他的に作成し、既存destinationを拒否する。installは指定／current署名versionを選び、完全検証後だけactivateする。updateは署名current versionを選ぶ。selectは既存install済みversionを使いchannel metadata floorを下げない。uninstallはactive versionを拒否する。`which`はactive compiler path、`run`は検証済みexecutableへ明示argumentを渡してstatusを返す。guest programへcapabilityを付与しない。

channelは`stable`、`beta`、`nightly`の3種。version labelは既存packageのcanonicalなunsigned 3成分version構文を使い、実験releaseの区別はchannelとbuild identityで行い、別version文法を増やさない。version選択は完全一致でありrange解決ではない。

## Reference semantics

### Trust and signed metadata

schema-1 rootは`schema`、`repository`、`expires`、`threshold`、`keys`を持つ閉じたJSON object。repositoryはASCII英数字／dot／underscore／hyphenの1–128文字。keyは32 public bytesのlowercase SHA-256である`id`と、64文字lowercase hexの`public_key`を持つ。異なるnonweak Ed25519 keyを1–8件、thresholdを1..=key_countとし、root入力は64 KiB以下。正確なbytesとSHA-256を一度だけ保存し、全後続stateをそのidentityへbindする。root更新／置換は新しい分離rootへの明示bootstrapで行い、既存installationのrollback履歴を暗黙resetしない。自動root successionは将来のformat変更とする。

schema-1 envelopeは`schema`、`payload`、`signatures`だけを持つ。payloadはUTF-8 JSON文字列。signatureは`key_id`と128文字lowercase hexの`signature`を持つ。`b"lang-toolchain-channel-v1\0"`と正確なpayload UTF-8 bytesの連結に対し、異なる許可signer IDによるEd25519 strict検証を行う。検証前にJSONを再serializeしない。未知／重複signer ID、不正encoding、弱い鍵、不正署名、threshold不足を拒否する。署名は8件、envelopeは1 MiB以下。これはproject formatでありTUF wire実装ではない。

閉じたpayloadは`schema`、`repository`、`channel`、`target`、`sequence`、`issued`、`expires`、`current_version`、`releases`を持つ。整数はunsigned、sequenceは正。時刻は最大253402300799の整数Unix秒で、issued <= 現system時刻 < expires、metadata lifetimeは最大90日とする。rootも有効期限内でなければならない。production commandはadmission用に一度取得したhost時刻を使い、activation前に新しい時刻で再確認する。test clockはlibrary testへ注入できるがCLI bypassは提供しない。

targetは現在の対応host triple、repositoryはrootのrepository、channelは明示requestと一致しなければならない。catalogueは異なるversionのreleaseを1–64件持ち、current_versionを含む。releaseは`version`、`compiler_build_id`、`files`を持つ。build IDは64文字lowercase hex。fileは`path`、`bytes`、`sha256`、`executable`を持つ。catalogue全体のfile recordは4,096件以下、各releaseは1–4,096 file、合計1 GiB以下、単一fileも1 GiB以下。合計はchecked演算とし、file SHA-256は64文字lowercase hexとする。

pathは最大240 bytesの相対UTF-8 ASCII slash pathで、各componentは英数字／dot／underscore／hyphenを使う。空／dot／dot-dot component、絶対path、backslash、colon、末尾dot、Windows予約device stem、case-fold重複、file／directory prefix衝突を拒否する。compilerはhostに応じ`bin/lang`または`bin/lang.exe`、対応SDKは`sdk/`配下とする。このcompilerとSDK内の対応`bin/lang[.exe]` copyだけをexecutable=trueとし、両digestは一致させる。SDK互換性は元のexact-build契約を維持し、SDK target／build ID、installed compiler identityを署名releaseと照合する。candidateの全file bytesを確認してから上限付き`toolchain identity` subprocessを起動し、activation前にpermission、platform format、必須SDK fileを検証する。未検証payloadを実行しない。

### Admission, transport and rollback

metadataは`channels/<channel>/<target>.json`、payloadはcontent-addressed `objects/<sha256>` fileとする。metadataが任意network URLを与えることはない。呼出側は明示local配布directory capability、またはcredential／query／fragmentを含まないHTTPS baseを与える。証明書検証HTTPS、redirectなし、透過content decodeなし、有限connect／overall timeout（requestごと10／120秒）を使う。admitしたbyte上限+1まで読み、余剰／不足を拒否する。local入力は上限付きregular-file検査を使い、symlink逸脱、FIFO／device、directoryを拒否する。有効な署名を確認するまでpayloadをopen／downloadしない。

stateはchannelごとの最大commit済みsequenceと正確なpayload digestを保持する。小さいsequenceを拒否し、同値sequenceは同じpayload digestを要求する。新しい認証済みmetadataを保持し、payload転送前にfloorをatomic commitする。そのためpayload install失敗でも認証済みfloorは進み得るが、active versionは変えない。明示versionは新しくadmitしたcatalogueに存在しなければならない。読取専用`verify`は新規trust contextでmetadataを検査し、installed-state rollback floor、state公開、payload読取を伴わない。出力にそのscopeを明示し、既存installation更新の許可にはしない。install／updateは必ず永続floorを使う。F02 verifier APIは明示的な過去admission recordを受け取るため、rollback／同sequence検証は後続filesystem transaction実装へ依存しない。

stateはadmission成功時刻のfloorを保持し、新metadata admissionではそれより過去のclockを拒否する。これで新machineの正確なwall clockを確立したり、local installation rootを書ける攻撃者へ対抗したりはしない。期限切れmetadata／rootは新install／update admissionを拒否する。過去にadmitしたinstalled releaseは署名とbytesを再検証すればselect／runできる。同sequenceのretryで転送失敗から復旧できる。metadata保持はinstalled receiptと現3 channel recordに制限し、無制限履歴を保持しない。

### Publication, concurrency and recovery

installation rootは呼出userが所有する信頼済みlocal stateであり、一時／最終fileを同じfilesystemへ置く。専用regular lock fileとstable標準libraryのnonblocking exclusive file lockを使い、競合は無制限待機せず明示拒否する。state変更操作とchild executableの選択／検証中にlockを保持し、`run`ではchildをspawnしてから終了を待つ前に解放する。上限付きcandidate identity検証はinstall transactionの一部としてlock下で行う。実行中executableによりWindowsの後続削除が失敗した場合、失敗を報告して整合したinstallation recordを保つ。

排他的private staging directoryを作り、payloadをstream／hashし、完全manifestと対応SDKを検査してから公開する。型付きreleaseのcanonical compact JSON表現のSHA-256で不変generationを命名する。既存generationは全bytes／permissionを比較し、名前だけをcache真正性検査にしない。署名metadata／receipt証拠をgenerationと共に保存する。local stateは1 MiB、installed releaseは64件以下。容量到達時はactive／installed versionを暗黙evictせず拒否する。

閉じたschema-1 stateはrepository／root digest／host target、3 channel floor、installed release identity、任意active identityをbindする。完全な一時stateをwrite／syncして同一filesystem上で既存state fileをatomic置換する。置換失敗は旧stateを保持し、generation公開をactivationより先に行う。activation前のprocess中断では旧active stateと、場合により未参照candidateが残る。recoveryはlock取得後に認識済み未参照manager staging／generationだけを削除し、未署名の残骸を採用しない。不正stateはrollback floorをresetせず拒否する。state／receiptの正確なfieldはF03で実装前に、この上限／不変条件内で詳細化する。

uninstallはinactive generationをbytesを変えずprivate trashへ移動し、stateからの削除をcommitしてからtrashを削除する。state commit失敗では元directoryを復元し、recoveryは上限付きtransaction markerに従って参照済みtrashを復元、未参照trashを破棄できる。Windows rename失敗はstateを変えない。markerは元generationと意図operationをbindし、markerの任意pathへ追従しない。正確なmarker／失敗checkpointはF03／F04で実装前に定める。契約は注入失敗とprocess中断を対象とし、成功したsync／atomic操作以上のfilesystem／hardware電源断挙動を保証しない。

### Installed state and receipts

F03詳細化、2026-10-30 JSTに同じ実験委任で採択: 閉じたschema-1 stateのfieldは`schema`、`root_sha256`、`repository`、`target`、`time_floor`、`channels`、`installed`、`active`とする。初期arrayは両方空、time_floorは0、activeはnull。channelsは異なる`{channel, receipt}`を最大3件持つ。installedは異なるchannel／versionの`{channel, version, generation, receipt}`を最大64件持つ。activeはnullまたはinstalledに正確に存在する一つの`{channel, version, generation}` identity。閉じたreceiptは`{schema: 1, metadata_sha256, admitted_at}`。時刻とhashはmetadataの上限を維持し、任意filesystem pathを指定するstate fieldは設けない。

正確なenvelopeを`metadata/<metadata_sha256>.json`、固定rootを`root.json`へ保持する。state読込時に全参照content hashを検査し、receiptのadmitted_at時刻でenvelopeを再認証する。未検証rollback値をdeserializeせず、検証済みenvelopeからchannelの過去Admissionを復元する。全receipt時刻はtime_floor以下とし、各installed releaseは認証済みgenerationと一致し、そのsequenceはchannelの現floor以下、同sequenceならpayload hashも同じでなければならない。新admissionでは全体time floorも検査する。記録時刻での過去検証により既存installed toolは期限後も使えるが、production userがadmission時刻を上書きすることはできない。

manager所有layoutは`lock`、`state.json`、`root.json`、`metadata/`、`generations/`、`staging/`、`trash/`とする。state／root入力はregular fileを要求し、所有stateのsymlinkを拒否する。lock下で排他的な一つの`staging/install` directoryと`state.next` fileを確保する。state.nextをwrite／sync／closeしてからatomic置換する。中断した一時fileやstaging directoryがある場合は、復旧前に次の変更操作を開始しない。metadataをwrite／syncしてからstateで参照し、既存metadataは正確なdigest一致を要求する。metadata／generation directoryはそれぞれ最大128 entryとし、容量制限で拒否した場合は復旧後に再試行する。未知entryは任意名へ追従／削除せず拒否する。installed receiptと現channel floorで保持metadataを決定し、無制限履歴は設けない。

generationは署名済みfileと必要な親directoryだけを含む。元の上限付きSDK manifestのtarget／build IDと全file表を署名済みSDK fileへ照合し、compilerのplatform formatとpermissionを検証してからidentity subprocessを呼ぶ。subprocessは10秒、出力はstreamごとに64 KiBまで。閉じたschema-1 identityはcanonicalなcompiler versionと、署名済みtarget／build IDの正確な一致を要求する。compiler buildを跨いで更新できるよう、manager自身のbuild IDとcandidateの一致は要求しない。distribution version labelは署名bundleを識別し、compiler bytesが同じでもSDK文書やpackagingの変更で変わり得る。identity.versionはdistribution labelではなくcompiler crate versionを報告する。異なるlabelだけで異なるcompiler codeとは主張しない。

既存channel／versionが別generationへbindされている場合は置換を拒否し、同一retryは全検証後に許可する。generationはchannel間で共有できるため、uninstallは最後のinstalled参照が消えた場合だけfileを削除し、active identityは削除しない。library失敗checkpointはfloor commit前後、payload検証後、generation公開後、activation commit前を対象とする。失敗やprocess終了の注入だけを行い、通常CLI optionに検証bypassは設けない。macOSのtransaction実験では置換失敗、中断一時file拒否、commit済みfloor保持、activation前のprocess終了、lock解放を確認した。Windowsと実installerの検証は引き続き必要。

install／initおよび後続state操作の成功は、`schema`、`kind`、`operation`、`root_sha256`、`repository`、`target`、`active`、`installed`、`floor_advanced`を持つ閉じたschema-1 `toolchain_operation` recordとする。activeはnullまたは`{channel, version, compiler_build_id, generation}`概要、installedはその概要array。操作失敗のJSON modeは閉じた`{schema: 1, kind: "toolchain_failure", diagnostic, floor_advanced}` recordで、diagnosticは通常の構造化diagnostic。booleanは失敗前に今回操作が新sequenceまたはadmission時刻floorをcommitしたかを示す。human modeも同じ事実を示す。CLI misuseと読取専用verifyは通常diagnostic recordを維持する。出力は1 MiB以下とし署名秘密情報を含めない。

## Execution modes

信頼済みhost CLIの変更である。installed VM／native実行は既存の上限、grant、source意味を維持する。package install hook、guest network capability、JIT／component／embedding実行変更はない。native SDKは既存の厳密compiler／rustc／profile検査の下でだけ利用できる。

## ABI and compatibility

root、channel envelope／payload、state、receiptは独立にversionを持つ新host recordとする。未知version／field、重複を拒否する。source構文、edition、MIR、bytecode、package lock、SDK、公開C ABIは変更しない。既存source checkoutの移行は不要。任意tool directoryや別trust rootから自動importせず、別rootを初期化して認証済みmetadataから再installする。

## Diagnostics and tooling

F02の検証結果はschema-1 JSON recordとし、`schema`、`kind: "toolchain_metadata"`、`scope: "fresh_trust_context"`、`root_sha256`、`repository`、`channel`、`target`、`sequence`、`payload_sha256`、`current_version`、`releases`を持つ。release summaryは`version`、`compiler_build_id`、`generation`、`files`、`bytes`を持ち、generationは署名された型付きreleaseのidentityであり検証済みinstalled directoryを意味しない。結果は1 MiB以下とし、payload bytesやlocal絶対pathを含めない。`TLC0001`はadmission／入力失敗、`TLC0002`はCLI誤用を表す。失敗時にpartialな成功結果を出力しない。

構造化installer診断を使い、誤用は通常のexit 2、admission／transaction失敗はexit 1とする。signing secretを出力しない。JSONは正確なrepository／channel／version／target／generation identity、active state、転送失敗時に認証済みfloorが進んだかを報告する。LSP／DAP／compiler診断は変更しない。`run`はchildのargument／output／statusを保持し、spawn失敗を別に報告する。外部指定signing materialを使うrelease-fixture producerを提供し、private keyをversion管理や公開fixture rootへ入れない。

## Performance and cost model

署名検証は8 key／signatureと1 MiB metadataで制限する。転送はstreamingで、署名sizeとtimeoutで制限する。candidate disk使用量は1 releaseと保持installed version、上限付き復旧trashを含む。installed hash再検査はfile I/Oを要し、処理不要のcache再利用とは主張しない。file-set transportは非信頼archive parserを避けるがfileごとのrequestを必要とし得る。初期署名probeで性能比較を成立させない。

## Security and trust boundaries

攻撃者は配布metadata／payloadを制御し、replay、組合せ、切断、保留を行える。trustは明示bootstrap rootと単調なcommit済みstateから得る。activation前に真正性／expiry／target／path／digest errorを拒否し、可用性回復のため検査を緩和しない。signing thresholdが侵害されると悪意toolを許可できる。敵対的local管理者や不正な初期trust／clockは保証外である。recoveryはtrust resetではない。producerとprivate keyは信頼済みhost操作で、通常scriptの権限を増やさない。

## Alternatives

- managerなしでは手動の信頼済みSDK assembleを維持できるが、要求されたupdate／channel／recovery journeyを提供しない。
- 外部package managerはbootstrap trustを提供できるが、このcompilerの厳密SDK／receipt identityを単独では定義できない。
- 完全なTUFは複数roleの委任とroot rotationを提供する。長期のupdate設計として強力だが、全wire／workflowの採用は初期の単一repository catalogueを超えるroleを増やす。このprofileはTUF互換や同等の侵害復旧を主張しない。
- 未署名checksum一覧は単純だが悪意ある配布を認証できず、active directoryの無条件置換も中断復旧を満たさない。

## Implementation plan

F01で設計を採択し、署名／file-lock実験と照合する。F02で転送前に厳密root／envelope／payload検証、expiry／floor／target／digestと不正入力testを実装する。F03で有限state／receiptを詳細化し、実分離install一件と中断時のactive保持を実装する。F04で復旧markerを詳細化し、update／select／list／run／which／uninstall／recoverを完了する。F05で実tool／SDKの一時鍵署名配布を生成し、両対応hostでinstall／update／失敗journeyを実行する。既存5件の3-point見積りを維持し、実装が成果を超える場合は前提を公開して再見積りする。実journey成功までG9は未完了。

## Test plan

RFC 8032公開検証vector、一時signing key、threshold／重複／弱い鍵／改変／version／expiry／rollback／clock／target／件数／overflow／path拒否、上限付きreader fuzzingを使う。署名admission前にpayload I/Oがないことを確認する。実installed compiler／SDKを実行し、別署名versionへupdate／selectする。metadata-floor commit、payload検証、generation公開、state置換、uninstall rename／commitの前後へ失敗を注入し、旧active executableの利用可能性と決定的復旧を示す。nonregular入力、symlink／case／prefix衝突、manager並行競合、Windows実行file lockを検証する。local transportとHTTPS policy／上限をtestし、production signing credentialは不要とする。実装ではrepository baselineと両host CIを実行する。

## Open questions

2026-10-30 JST、RFC 0001の実験委任で採択する。分離したRust 1.99／ed25519-dalek 3.0.0実験はRFC 8032 test 1を受理し、message／signature改変、不正scalar、弱い鍵を拒否した。macOSのfile-lock競合／解放も成功した。これらはlibrary実現可能性でありinstaller完了ではない。Windows atomic置換／lock、HTTPS境界、実release／SDK照合、全中断checkpointはF02–F05の明示的な未検証仮定に残る。root置換は意図的に新しい明示trust installationを使い、自動rotationには将来の互換設計判断を必要とする。採択で実装gateを閉じない。

## Sources

- [TUF specification 1.0.37](https://theupdateframework.github.io/specification/latest/)、更新2026-10-08、2026-10-30 JST確認。署名threshold、metadata上限、expiry、rollback、root successionを脅威modelの参考とするが、このformatはTUFではない。
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032)、公開2017-01、2026-10-30 JST確認。Ed25519検証と公開test vector。
- [ed25519-dalek 3.0.0 VerifyingKey](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/struct.VerifyingKey.html)、release 2026-09-21、2026-10-30 JST確認。厳密検証と弱い鍵の拒否。
- [ureq 3.5.0 configuration](https://docs.rs/ureq/3.5.0/ureq/config/struct.ConfigBuilder.html)、release 2026-10-21、2026-10-30 JST確認。上限付き同期HTTPS transport候補であり実行検証は未完了。
- [Rust 1.99 File locks](https://doc.rust-lang.org/1.99.0/std/fs/struct.File.html#method.try_lock)、[rustup channels](https://rust-lang.github.io/rustup/concepts/channels.html): page更新日は不明、2026-10-30 JST確認。
- 既存native SDK snapshot／publicationとpackage admission APIを2026-10-30 JSTに確認。sourceに埋込更新日はない。上記wire field、上限、CLI／recovery policyはproject判断である。
