<!-- translation-of: ../../rfcs/0032-installed-component-packages.md -->

# RFC 0032: 明示host bindingによるinstalled component package

[English](../../rfcs/0032-installed-component-packages.md)

- Status: accepted
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-27
- Tracking issue: M3-I15; M3-I08 retains both-host startup
- Supersedes: None; extends RFCs 0020, 0030 and 0031
- Superseded by: None

## Summary

canonical schema-5 componentと明示的にbind済みのhost Programに対して、native program manifest **version 3 / profile `component`**とSDK manifest **version 3**を受理します。対応installed SDKが元LANGNAT-2 Component objectと必須のtrusted Rust hostをリンクします。hostが埋込みcomponentを独立decodeし、実際のcontract・grant・providerを用意して、コード生成せず元objectをbindします。

2026-10-27 JST、RFC 0001の実験期委任で受理しました。`042fc38`の厳密SDKだけを使うprototypeは保存componentをリンクし、全producer入力から実行形式を移した後、Cargoなしで42・panic −42・正常再利用1を実行してresource/allocation inventoryを0にしました。これは選択経路のlibrary利用可能性を示します。package受理・resource callback網羅・最終sourceの隔離install・Windows起動は未検証の受入条件です。

## Motivation

RFC 0031はtaskのみを受理します。component objectは元Programの検査済みexport/host権限を保ちますが、package reader/linkerにはcomponent profileがありません。taskと暗黙扱いすれば必須bindingを失います。現SDKは厳密なroot `lang` libraryと依存closureを含むため、別compilerのinstallやproviderの捏造は不要です。

## Goals

- 明示host権限でcomponent packageを公開・検査・リンクする。
- 保存scalar/resource/callback fixtureを元allocator・Control・task identity・preparation・必須退役で実行する。
- 旧package/SDK組合せと有界なtransactional入力検査を維持する。

## Non-goals

- 暗黙provider/grant、自動dynamic loading、default component main、source/C ABI変更、新engine。
- 一般component source CLI推論、WebAssembly、別hostへのobject出力、simulationによるWindows証拠。

## Guide-level design

trusted Rust producerはcanonical component artifact bytes、その独立decode済みUnitから用意した実Program、選択済みexport FunctionId、host targetをnative component package APIへ渡します。実provider ownerは出力完了まで保持します。packageはsemantic・native_record・object・bindings roleを持ち、mainとc_headerは持ちません。

`lang link-native --sdk SDK --package PACKAGE --host HOST.rs --output APP`は明示hostを必須にします。生成bindingは厳密target/build identityとsymbol addressを保持し、linkerは既存runtime libraryに対応`lang` libraryを加えます。hostはpackage bytesを埋め、独自の明示bindingを復元してnative component binding facadeを呼び、既存LinkedProgramのcomponent実行methodを使います。facadeはcallerの代わりにresource table・invocation・allocator・Control・provider・taskを作りません。

## Reference semantics

source構文・推論・静的所有権/effect・評価・取消し・cleanup・failure選択はRFC 0027を維持します。canonical component artifactはscript entryなしのlibraryです。manifestの非null entryは明示選択した既存exportで、LANGNAT Component entryと一致させます。component引数とrich resultを許し、通常の引数なしi64 script entryとは扱いません。

producer/consumerは、object出力またはsymbol解決前に、decode artifactの完全な検査済みdocument・code・binding mapと、渡されたProgramの元Unitを比較します。immutable graph走査前に有限preparation上限へ比較work/storageをadmitし、部分一致やinterface名だけの一致は認めません。provider inventoryの変更で保存objectを書き換えず、実際に明示準備したProgramを復元/実行の権限とします。

公開は実host targetに限って元Component objectを出力します。外部tool前のsnapshot検査は厳密manifest/version/role/path/size/hash、target/build identity、完全な独立component decode、選択export、record kind/entry、別objectとのbytes一致を確認します。完全なnative node/metadata preparationには実bound consumer Programが必要なため、binding facadeでsymbol解決/guest entry前に実施します。このphase差を明示し、package検査に権限を与えず仮providerも作りません。旧task/copy/owned/Cのpreparation phaseは維持します。

unsafe binding契約は厳密な未変更検査済みobject・対応compiler/SDK・正しいaddress解決・LinkedProgramの生存中codeを保持するownerを要求します。hashは任意native codeを安全にしません。返すLinkedProgramは渡されたProgramをborrowし、code ownerを保持します。既存の型付きruntime failureとowner/poison寿命を維持します。

## Execution modes

VM/JITを差分oracleとして維持します。nativeは保存済み生成計算と元component/task serviceを使い、source parse・ISA lookup・object出力・JIT・VM fallbackを使いません。明示的な実bindingでresource importとVM/JIT/AOT callbackを既存same-task規則へadmitします。公開C embedding surfaceは拡張しません。

## ABI and compatibility

program version 1/2は厳密profile集合とbytesを維持し、version 3はcomponentだけを受理します。SDK version 1/2の挙動を保ち、SDK 3は同じ厳密buildのversion-1 copy/owned/C・version-2 task・version-3 componentを受理します。taskはSDK >=2、componentはSDK 3を必要とし、不対応時はtool前に拒否します。旧readerはrevision 3を拒否します。LANGBC 5・LANGNAT 2・task packet・component C ABI versionは変えません。program/SDKを一緒に再buildし、既存有限schema・panic=unwind・固定rustc検査を維持します。

## Diagnostics and tooling

code解決前に構造化component decode/対応関係/export/plan/profileエラーを返し、既存host不足・toolchain・snapshot・rollback診断を使います。package APIは明示Rust toolingで、通常source native compileは既存profileを維持します。SDK guideと日英文書を更新します。source spanと実module originをcanonical artifact/native recordに残します。

## Performance and cost model

既存厳密SDK closureを再利用します。binding scopeごとにmetadataを独立decodeしてnative nodeを復元し、有界対応比較は追加preparationとします。runtimeコード生成や新schedulerを加えません。実行compile/link/startupとbinary sizeを記録し、性能gateの完了とはしません。prototypeは機能のみを証明します。

## Security and trust boundaries

package/artifact/record/manifestは有限byte/work/storage/depth上限付きの未信頼dataです。trusted host codeと厳密machine codeはsandbox外です。公開・検査・linkでcapabilityを付与したり実providerを置換したりしません。副作用前検査、元権限、owner寿命、必須cleanup、transactional公開/出力を保ちます。tool前phaseでprovider依存nodeを復元できなくても、不正metadataをguest実行前に拒否します。

## Alternatives

- 変更なし: 拒否を保つがinstalled componentを使えない。
- loose objectをリンクする単独script: prototypeは動くが厳密package・transactional公開・共通診断がない。
- 別component SDK/runtime: 既存closureを重複させ権限/cleanupの乖離を招く。
- task version 2流用やprovider合成: 配線は簡単でもprofile互換性を隠すか偽権限を与えるため拒否する。

## Implementation plan

1. 実行SDK prototypeを記録し、実装前にI15を明示する。
2. 厳密program/SDK revision 3・明示component公開/binding・必須host link・拒否testを追加する。
3. installed scalar/resource/callback・cleanup/control失敗・旧profileを実行し、入力固定全baselineと最終clean-producer隔離probeを実施する。
4. 実両host task/component startupをI08に残す。Windowsは利用できずpushもしない。

## Test plan

全version/profile組合せ、host不足、未export/誤entry、完全Unit不一致、誤record kind/target/hash/object、不正metadata、有界preparation拒否を適切なtool前/解決前phaseで検証し、rollbackも確認します。コピー済みSDK consumerでscalar成功/panic/再利用と実resource/callback/取消し/destructor失敗を元権限で実行し、inventoryが残らないことを確かめます。旧profile testと独立record/fuzz網羅を保ち、workspace/fuzz fmt・warnings-denied Clippy・全target/feature test・rustdocを実施します。commit済みclean producer provenanceとcheckoutなしconsumer実行を最終受入条件とし、実hostを別記します。

## Open questions

Windows task/component startupは未検証です。さらなるCLI改善とSDK縮小は最初のinstalled Rust-host profileに必須ではありません。厳密APIとresource fixture costは未検証で、新前提が判明したら実装前に明示します。

## Sources

- [RFC 0020](0020-native-sdk-and-startup.md)、2026-10-27更新。厳密SDK/toolchainと有界link。
- [RFC 0027](0027-production-component-interface.md)、2026-10-27更新。実component/grant/resource権限。
- [RFC 0030](0030-native-task-component-records.md)と[RFC 0031](0031-installed-native-task-packages.md)、2026-10-27更新。保存codeとinstalled task互換性。
- `042fc38`、SDK build `fc1cbc129a1a4581f7bc674e165abd60a121891c4fa61d45db1efdf4754a8511`。prototypeは2026-10-27 03:54 JST、macOS AArch64で検証。codeに公開更新日記載はありません。外部designの主張は使いません。
