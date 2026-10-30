<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0044-checked-source-migration.md -->

# RFC 0044: 検証付きsource修正と旧仕様からの移行

[English](../../rfcs/0044-checked-source-migration.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-30
- Last updated: 2026-10-31
- Tracking issue: M5 delivery boardのM5-B01–B05
- Supersedes: none
- Superseded by: none

## Summary

previewを既定とする検証付きsource修正と、この言語の旧scalar構文からedition `2026`への明示移行を追加する。通常compileの現grammarと有限edition集合は維持する。現editionのno-opやpackage manifestの追加だけでは移行完了を証明しない。

## Motivation

実在するcompiler `69b1c4dc66d04a4a103d4eb843232df22e52eac0`はlocal名`shared`・`weak`を使うsourceで`42`を返すが、現compilerは`SRC0024`を4件出してこれらの識別子を拒否する。その旧315 byte artifactは現compilerでも`42`を返す。[保持したprobe](../../evidence/2026-10-30-m5-legacy-source-probe.json)にsource・producer・lock・artifactのhashを記録した。これによりsource移行とbytecode互換性を区別する。別のprobeでは既存editorの`LNT0001` rename修正がinitializerと実行結果を保持した。batch toolではこの証明を宣言testと検証付き書込みに拡張する必要がある。

## Goals

- 決定的でレビュー可能なpreview、明示書込み、構造化された拒否を提供する。
- 有限recipe集合について解決済みの動作と副作用を保持する。
- 実在する旧sourceを移行し、現VM・nativeで両host上の実行を確認する。

## Non-goals

別editionの創作、通常compileでの旧構文受入、汎用旧compiler、任意のユーザーpatch、公開・foreign symbolの変更、module file移動、無関係なbyteのformat、lock・依存変更、並行writerに対する全file rollback。

## Guide-level design

```text
lang fix --manifest-path PATH [--package NAME] [--target TRIPLE] [--apply] [--message-format human|json]
lang migrate --from-profile rfc0006-scalar --to-edition 2026 --source-root DIR [--apply] [--message-format human|json] FILE
```

どちらも既定はpreview。`--apply`は書込み前に新しいprivate planを計算・検証し、過去のJSON reportを実行入力にしない。重複・未知option、未対応profile・edition、両立しない引数は書込み前に拒否する。`fix`は選択したedition-2026 packageと宣言test entryを対象とする。`migrate`には旧profileとsource rootを明示し、parse失敗やambient manifestから推測しない。

例えば旧local `shared`・`weak`の宣言と全参照をfreshな`_migrated_shared_1`・`_migrated_weak_1`へ変更し、initializer・加算・returnを維持する。実際の名前は既存識別子から決定的に選ぶ。旧名を含むcommentも保持する。

## Reference semantics

### 現sourceの修正

最初のrecipeは`LNT0001`で、RFC 0039の解決identity renameと意味等価証明を使い、未読local・parameterへ`_`を付ける。宣言・initializer・代入・評価順・制御flow・cleanupを保持する。未使用に見える式を削除しない。名前衝突、曖昧なidentity、foreign・未対応宣言、証明失敗はplanを拒否する。parser suggestionの一般的な自動適用ではない。

選択targetに従い、選択package entryと全宣言test entryをcaptureする。証明のため到達可能依存も解析するが、編集は選択package内のsourceに限る。対象root・fileを報告する。全編集後に影響する各rootを再検査し、元のeditor snapshotがentryしかcaptureしないことを理由にtest専用sourceを対象から落とさない。同一fixの重複観測をまとめ、競合editは拒否する。未選択packageを暗黙変更しない。

### 旧scalar profileの受入

`rfc0006-scalar`は固定した旧commitのsourceのうち移行専用subsetであり、通常compileの対応editionではない。単一UTF-8 file内のprivate scalar関数、`i64`・`bool`のparameter・result・local、可変local宣言と代入、`if`・`else`・`while`・`return`、scalar literal・括弧・名前・call・加算・小なり比較を受け入れる。`main() -> i64`を必須とする。RFC 0006の使用前宣言、関数全体のlocal名一意性、型、RFC 0002のwrapping加算、左から右の評価、制御flow規則を維持する。旧operator・keyword集合を使い、現parserが受け入れるだけの新構文を通してはならない。

module・import・qualified path、公開・foreign宣言、aggregate・pattern・ownership・borrow・task・host capability、およびsubset外の構文を無書込みで拒否する。特に`break`・`continue`・文字列・減算を旧grammarへ導入しない。ただし旧grammarが許す位置ではその綴りを通常識別子として扱える。旧識別子はASCIIで、単独`_`を除く。commentとCRLFのbyteを保持する。rewrite前に旧UTF-8・NUL・token・入れ子comment検証を行う。

上限付きの旧token・AST viewを読む。現構文で新たに予約された識別子と予約root名`core`ごとに、入力の全識別子に存在しない相異なる有効名を選ぶ。private関数宣言・参照を含め、その綴りの全identifier tokenを一貫して変更する。comment・literal・部分文字列は変更しない。対応は単射かつ全体でfreshであり、他の名前は固定されるため、scopeへのcaptureを生じない。変換前に旧scalar構造と名前解決を検証し、変換後は対応表の下で現parse・resolve後のscalar構造を比較する。型・call・値・operator・文順を含める。parse成功だけでは意味証明にならない。diagnosticと証明比較のためsourceからoutputへのoffset対応を保持する。

このprofileにallocation・destruction・cancel・unwind・unsafe・FFI・並行性の遷移はない。これは入力制限であり、それらの移行対応を意味しない。recipeは実行時評価とerrorを保持し、compile-time executionを追加しない。衝突のない有効入力にはunchanged reportを返すが、それだけでsource遷移の実証にはならない。

[固定producerの受入probe](../../evidence/2026-10-31-m5-scalar-admission.json)（2026-10-31 JST確認）で旧規則を具体化した。localには明示型が必要で、parameterは不変、block localは外へ漏れず、分離したbranchを含む関数全体でlocal名を一意とする。local bindingは同名callableを隠し、callできない。到達不能な名前も解決し、必要な全return経路を検査する。前方・再帰private callは受け入れる。B05はrewrite前にこれらを維持し、現grammarでのparse成功で代用しない。

同probeは`else`の省略とparameter・argument末尾のcommaを受け入れ、local宣言の`mut`は必須とする。これらは既存の旧scalar grammarの一部であり、移行readerの拡張や省略可能な簡略化ではない。

### Planと公開

未変更の依存・testも含め、各解析が使うmanifest・source bytes・filesystem identityをfreezeする。planのconstructorをprivateとし、元byte span順に整列する。JSONは情報を公開し、書込み権限は渡さない。staging前に意味証明と符号化output上限を検証する。

RFC 0036の公開境界を再利用する。明示write authority、regularでcanonicalな入力・directory、symlink・hardlink alias拒否、全staging後の最終freeze入力再検証、fileごとのatomic replacement、後続置換失敗時の正確なcommit済みprefix報告を維持する。未変更入力やmanifestのstale検出も未公開plan全体を無効にする。協調writerは文書化された排他書込み区間を守る必要があり、悪意あるwriterのpath race阻止やrollbackを保証しない。復旧時に並行editを上書きしない。

## Execution modes

fixは既存の共通typed MIR契約を通じてVM・native AOTのsource意味を保持する。移行受入fixtureは固定した旧producerで移行前を実行し、現VM・native binaryで移行後を実行する。現compilerでの旧artifact実行は追加のformat証拠となる。optional JIT・embedded callback・Wasmに新意味や自動移行対応を付与しない。

## ABI and compatibility

source editionは`2026`のままで、旧profileは別に識別する。bytecode・component C v1・private native ABI・package schemaは変更しない。private scalar関数のrenameは報告どおり表示・debug名を変更する。公開・foreign symbolは対象外。現source fixは既存rename toolのABI制限を保持する。package source変更はexact source checksumを無効にするため、通常の明示package workflowでlockを再生成する。fixを通すだけの目的でversion・manifest edition・依存制約・`Lang.lock`を書き換えない。

## Diagnostics and tooling

reportのpath文字列はpackage診断と同じくOS標準の区切り文字を保持し、native path APIで解釈する。両host fixtureはnative pathを使い、rootのidentityと順序を厳密に比較する。

schema-1 JSON reportにoperation、profile・edition、対象root・file、順序付きedit（元byte span・replacement・diagnostic/recipe identity）、適用可能性、変更・適用件数、outcomeを含める。authority相対canonical path、元byte offsetの順で並べる。pathは既存package diagnostic規則で符号化する。human出力も同じcoverageと拒否を伝える。使用法errorは`CLI0001`・exit 2、操作拒否は構造化したmigration/fix diagnostic・exit 1、no-opを含む成功preview/applyはexit 0。実装時に固有diagnostic IDを割り当て、無関係なcodeを再利用しない。

通常parser recoveryと既存LSP動作は維持する。editor quickfixとbatch fixは証明logicを共有し、batch coverageと公開を明示する。format・debugger data・文書は編集後sourceから通常どおり再生成し、cache済みIRやdebug artifactをpatchしない。

## Performance and cost model

operation全体のsource bytes・root・token・AST/証明work・candidate・replacement・report bytesに総量上限を設ける。renameごとに上限をリセットせず、再解析も共通work budgetへ課金する。安全性を証明できるfreeze snapshotは再利用する。previewは元bytes・replacement・証明dataを確保し、applyは完全replacementを追加stagingする。runtime overheadは増やさない。実装は具体的な既定上限と境界testを公開する。未測定のlatency・memory性能を主張しない。

B02 previewの既定値（2026-10-31）: root 64件、fix 256件、token edit 65,536件、累積解析source 256 MiB、総量work 1,000,000,000単位、累積replacement 64 MiB、符号化report 16 MiB。既存editor・compiler上限も独立に守る。各index・proof passは実行前にeditor work許容量全体を予約し、検証付きrenameはindexとedit/proofの両passを予約する。source bytesには毎回の解析と適用予定renameのunderscore増分を課金する。保守的な予約のため、個別上限をすべて使う前に拒否する場合がある。reportは上限付きcounting writerで符号化量を測り、巨大JSON bufferを先に確保しない。これは入力受入上限であり、CPU時間や最大常駐memoryの測定値ではない。

## Security and trust boundaries

source・manifestは未信頼data。解析はsource・native code・host importを実行せず、network・process capabilityを取得しない。source-root/package authorityでreadを制限し、別の明示authorityでwriteを制限する。nonregular・alias・stale入力、不正UTF-8、未対応旧構文、曖昧edit、上限超過は未検証planを公開せず拒否する。任意JSON edit programやユーザー指定applicabilityを信用しない。file単位atomicityと協調writer制限をCLI文書に明示する。

## Alternatives

- 変更しない場合、予約名による破壊の発見と安全renameの再実装を利用者に委ねる。
- editor/libraryだけでも有用な証明はあるが、宣言testやbatch filesystem transactionを扱わない。各command内で再利用する。
- Cargoの`fix --edition`は有力なsource先行移行モデルだが、文書はfeature・target選択による解析coverage制限を述べる。このprojectのexact-input公開保証の根拠ではない。
- 単純token置換とcompile成功は実装が小さいが、旧入力受入・名前解決を独立に証明せず、全入力保護や外部名保持を保証しない。
- 完全な旧frontendや新editionはこの成果より広い。未対応の旧構文を明示拒否し、別のrecipeレビュー後にのみ拡張する。

## Implementation plan

1. B01で契約を採択し、実装前に旧profileの前提作業を監査する。
2. B02でentry・test coverageと既存rename証明を備えたpackage fix previewを追加する。
3. B03で検証付き公開を再利用し、stale・競合・後続失敗を検証する。
4. B05で上限付き旧scalar入力受入、fresh-name証明、実在する`shared`・`weak` preview fixtureを追加する。
5. B04で明示migration CLIを公開し、macOS・Windowsの移行前後VM・native journeyを検証する。検証付きfixと実遷移の両方が成功してからG2を完了する。

frontend/editor証明adapter、package/format公開、CLI・test・文書を変更する。runtime・ABI変更は不要。旧reader・証明sliceは新規露出した3 ptの前提作業。B04の3 ptは統合と両host証拠用に維持し、新作業を隠して含めない。

## Test plan

未読local・parameter、initializer・代入の副作用保持、test専用module、依存・未選択package、UTF-8・CRLF span、衝突拒否、上限境界、決定的な再実行no-opを検証する。freeze manifest・未変更入力、symlink・hardlink・directory alias、staging失敗、正確なcommit済みprefixを検査する。human・JSON errorとlockへの影響も検証する。

移行では旧source・artifact hashを保持する。全新予約名、scope・call解決、fresh名衝突、comment、未対応構文・ABI・module境界、不正入力を扱う。正規化した移行前後の意味構造を比較し、小さなscalar program生成による差分検証も行う。保持した実fixtureを現VM・native backendで両host上に実行する。実装delivery完了前に全repository baselineと最新head CIを成功させる。手動edit・package化・同edition no-opを実装済み移行と数えない。

## Open questions

2026-10-30 JST、RFC 0001の実験段階委任に基づき採択。根拠: 明示的な旧入力受入、全体でfreshな名前対応、解決済み構造の比較、既存の検証付きrename・公開境界の再利用により、通常compileを変えずに限定recipeをレビューできる。保持した旧producerで実在する遷移を確認した。上限付きreaderと証明は未実装・未測定であり、採択はG2完了ではない。具体的な総量上限とdiagnostic割当は境界testを伴う実装判断であり、grammarを広げる許可ではない。追加旧profileと将来editionには別のrecipeレビューが必要。選択sliceの設計を妨げる未決事項はない。

## Sources

- [RFC 0002](0002-prototype-control-flow.md)、更新2026-10-27: 2の補数のwrapping加算。2026-10-31訂正: 当初案は加算を検査付きと誤記していたが、実行overflow probeと正本の算術契約で誤りを確認した。compiler意味・互換性は変えず、移行仕様の記述を訂正する。

- [RFC 0006](0006-source-frontend-and-portable-programs.md)、現更新2026-10-30、固定commitの旧版更新2026-10-17: source字句・scalar・module規則。
- [RFC 0007](0007-source-ownership-and-core-values.md)、更新2026-10-21: ownership予約名と移行diagnostic。
- [RFC 0036](0036-checked-package-formatting.md)、更新2026-10-31: 検証付き公開とcommit済みprefixの制限。
- [RFC 0039](0039-package-editor-snapshots.md)、更新2026-10-30: rename・quickfix証明。
- [RFC 0043](0043-editions-and-compatibility.md)、更新2026-10-30: editionと独立した互換性領域。
- [Cargo Book: cargo fix](https://doc.rust-lang.org/cargo/commands/cargo-fix.html)、公開・更新日なし、2026-10-30 JST確認。source先行移行と解析coverageは出典事実、本RFCのgrammar・証明・write保証はproject判断。
- [旧probe](../../evidence/2026-10-30-m5-legacy-source-probe.json)、2026-10-30 JST作成。同日に手動rename sourceのVM実行とnative生成も観測した。これは実現可能性の観測であり、移行command実装やnative実行結果ではない。
