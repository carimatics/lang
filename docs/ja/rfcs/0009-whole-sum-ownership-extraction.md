<!-- translation-of: ../../rfcs/0009-whole-sum-ownership-extraction.md; canonical-language: en -->

# RFC 0009：Option／Result全体からの所有権取り出し

[English](../../rfcs/0009-whole-sum-ownership-extraction.md)

- ステータス: accepted
- 著者: Project contributors
- 作成日: 2026-10-18
- 最終更新日: 2026-10-18
- Tracking issue: [実装Todo](../milestone-2.md) のM2-S01〜S03
- Supersedes:
- Superseded by:

## 概要

RFC 0007の有限core catalogへ、Option／Result全体をvalueで受け取る明示的な3操作を追加します。期待するcaseならpayload全体を返し、別caseならRFC 0008のcleanupを伴うguest panicにします。既存borrowed matchとpartial move禁止は維持します。Source／MIR解析と初期Bytes／Copy VM profileは実装済みで、広いpayload、REPL、native実行は未完了です。Acceptanceと実装完了／stabilizationは別に扱います。

| 操作 | Signature | 期待するcase | 別caseのpanic code |
| --- | --- | --- | ---: |
| `core::option_unwrap::<T>` | `(core::Option<T>) -> T` | Some | -1 |
| `core::result_unwrap::<T,E>` | `(core::Result<T,E>) -> T` | Ok | -2 |
| `core::result_unwrap_err::<T,E>` | `(core::Result<T,E>) -> E` | Err | -3 |

i64 codeは各操作で固定し、userの明示的 `core::panic` も同じ数値を使えます。User codeの予約namespaceではありません。操作のsource spanとguest-panic categoryをcodeに付随させます。

## 動機

最初のsource Bytes実行fixture（`ece8690`）は `Result<Bytes, AllocError>` を構築し、borrowed matchで長さを読みます。Ok bindingからmutableなowned Bytes localを初期化すると、`SRC0014`（expected `core::Bytes`, found `&core::Bytes`）になります。2026-10-18 JSTに再現したRFC 0007の仕様どおりの動作であり、lowering bugではありません。

このためallocation resultからBytes変更、所有権を消費するUTF-8変換、collection／shared constructorへownerを渡せません。Borrowed matchではVec pop／Map removeのownedな結果も取り出せません。User library内のmatchにも同じread-loan規則が適用されるため、libraryだけでは移譲を実装できません。異なる動作を実装する前に、設計を明示的に修正します。

## 目標

- 小さく明示的なsource surfaceで、失敗可能なconstructorから所有権を使う変更／変換へ進める。
- Partial moveなしで、入れ子のowning payloadを含むResult両caseとOption Someを扱う。
- 既存loan／region規則、allocator identity、exactly-once破棄、source span、backend非依存typed MIRを維持する。
- 回復可能なerrorは明示match／returnのままにし、unchecked extractionにpanicの可能性を明示する。

## 対象外

- 消費するpattern構文、任意record field／user enum caseの取り出し、mutable match binding、`?`、catch／throw、user generics、type argument推論。
- 新allocator、通常loanの結果、lexical loan短縮、arena escape、concurrency権限、公開表現、wire encoding、VM／native ownership profile全体の完成。

## 利用者向け設計

期待するcaseを必須とする場合、またはborrowed matchで別caseを処理した後にunwrapを使います。次の受理済みexampleはallocation failureを先に処理し、matchのloanが終わってからResult全体をmoveします。

```text
fn main() -> i64 {
    let mut made: core::Result<core::Bytes, core::AllocError> =
        core::bytes_new(core::allocator());
    match made {
        core::Result::Ok(_) => {},
        core::Result::Err(_) => { return -1; },
    }
    let mut bytes: core::Bytes =
        core::result_unwrap::<core::Bytes, core::AllocError>(made);
    let mut pushed: core::Result<core::Unit, core::BufferError> =
        core::bytes_push(bytes, 65);
    match pushed {
        core::Result::Ok(_) => {},
        core::Result::Err(_) => { return -2; },
    }
    return core::bytes_len(bytes);
}
```

意図する成功時の結果は `1` です。取り出しとBytes push dispatchを実装して初めて実行できます。Guardなしのunwrapも合法で、runtimeにcaseを確認します。Compilerに先行matchの証明は要求しません。Owned Errを回復可能な形で取り出すには、borrowed matchでOkを処理してからResult全体へ `result_unwrap_err` を呼びます。取り出さずResult全体をreturnすることもできます。自動伝播／変換は挿入しません。

## 厳密な意味論

### 構文・型・評価

予約済み `core` namespaceに3つのcompiler専用nameを追加し、既存 `::<...>` call grammarを使います。列挙したtype argument全部とvalue argument 1個を必須にします。実引数はspecializationしたOption／Result型と厳密に一致する必要があります。Owning parameterの暗黙borrowや、通常loanからownerへの自動dereferenceは行いません。結果は選択したtype argumentです。新parser keywordや文脈依存match形式は導入しません。

各type argumentに既存Storable規則を適用し、transitive contentsに通常loan、allocator authority、字句的arena-owner authorityを含めません。特にcore read操作のprovisional `Result<&T,E>`／`Option<&T>` はunwrapできず、引き続き即時matchが必要です。Storableなarena付きvalueはRFC 0007のregion制約内でのみ許します。

単一argumentを既存の式の評価順序で1回だけ評価します。Non-Copy argumentはcase確認前に全体をmoveし、returnする全pathで使用不能になります。Copy Option／Resultはcopyを渡して元のsourceを使用可能に保ちます。Copy判定はactive payloadだけでなく、Result両type argumentを含むsum全体で決まります。Argument評価完了前のfailureでは、初期化済みtemporaryをRFC 0008に従ってcleanupします。

### 所有権・失敗・region

期待するcaseでは唯一のpayload全体をresultへ移譲し、消費したsumの外側の初期化obligationを取り除きます。Clone、retain、release、payload cleanupを行わず、sourceに使用可能なaliasを残しません。入れ子の構築順metadata、allocator identity、logical identity、transitive arena provenanceを維持します。PayloadがCopyならresultへcopyできますが、外側のsumがnon-Copyならそのsumは消費します。生き残るaggregateのfieldを部分moveする操作ではなく、value全体の消費です。

Case確認と成功時のobligation移譲は1つのownership transitionとし、fuel、interruption、callbackなどのfailure boundaryがownerのないpayloadや二重obligationを観測できないようにします。検証と上限付きdestination／retirement storageのadmissionをその前に済ませます。未公開resultはcallerへ渡るまで通常のfailure／return cleanup規則に従います。

別caseでは表のguest panicをcall spanのprimary execution failureとして先に記録し、その後activeなinput payload全体とその他の初期化済みobligationを生成failure pathでcleanupします。`result_unwrap` のowning Err、`result_unwrap_err` のowning Okもexactly onceでcleanupします。Cleanup failureはsecondaryとして記録し、残りcleanupを続け、poisonはRFC 0008に従います。このguest panic単独でcleanupが成功した場合、instanceをpoisonしません。Source catcher、隠れたtyped Err、payload formatting、panic message用allocationは導入しません。Input評価のfailureは元のprimary reasonを維持し、unwrap panicへ置き換えません。

Live loanはnon-Copy inputのmoveを禁止します。以後使わなくても字句的にliveな名前付きloanを含みます。Borrowされたnon-Copy fieldをvalueで渡せません。有効なread placeにあるCopyなsum全体はcopyできます。取り出したpayloadのarena制約をlocal、aggregate、container、call、return経由でも維持し、shared storageやscope外へregionを洗い流せません。新しいloan、authority、共有、atomic、scheduling、unsafe exemptionは作りません。他のowning core操作と同様、constant／compile-time実行は未対応です。

## 実行モード

Typed MIRで各操作の具体的signature、by-value operand、result type／provenance、確認対象case、panic／failure edgeをbackend非依存で表します。有限HIR／MIR core-call contractを拡張し、arity、storage制約、input／result型、loan／region behaviorを独立検証します。Source checkerの結果だけで安全と判断しません。Backendはcase testとwhole-payload移譲へ展開できますが、source matchは引き続きborrowへlowerします。不一致caseへのunchecked `UnpackVariant` は、規定のguest panicの実装にはなりません。

VM、native AOT、optional JIT、component adapterでresult、move後source、panic code／span、drop順序、failure precedenceを一致させます。段階実装中の未対応操作／payload kindは、既存structured feature gateでguest／provider effectやcode publication前に拒否します。最初のBytes実行で入れ子aggregate、Vec／Map／shared／arena、REPL、nativeの完成は主張しません。Frame／cell上限とfuelは通常どおり課金し、移譲とfailure reportingで新guest storageを取得しません。

Embedded hostには既存boundaryで同じstructured failureを渡します。RFC 0008のC profileは引き続きowning／loan-bearing signatureを拒否し、このhelperはC exportやforeign unwind機構にはしません。RFC 0003のcomponent resourceもadapter contractを維持します。このRFCのacceptanceで別途JIT、component、LSP、DAP実装は追加しません。

## ABIと互換性

影響domainはsource、内部MIR／VM意味論、native実装、tooling、bootstrapです。RFC 0007の有限core catalogを明示的に拡張し、RFC 0007全体やRFC 0008をsupersedeしません。`_` やnon-Copy binding loanを含め既存source matchの意味を維持します。予約namespaceのため、新nameと従来合法だったuser declarationの衝突はありません。Edition migrationは不要で、利用者は明示callを選び、match bindingのmoveに失敗した箇所をmatch後unwrapへ書き換えられます。

Schema-1 artifactとgolden byteは変更しません。別のownership-format RFCのacceptanceとdecoder／verifier実装前にはserialized tagを付けません。Native表現はtoolchain-privateのまま、新C／component layoutやpackage byteは追加しません。将来の公開metadataは受理済みversioned formatに従います。Panic、取り出し、match意味論の後続変更には、実装前に別の明示RFC改訂／follow-upを要します。

## 診断とtooling

既存structured identityを使います。Unsupported specialization／arityは `SRC0024`、不正storageは `SRC0023`、argument型不一致は `SRC0014`、move済み／未初期化inputは `SRC0020`、live-loan conflictは `SRC0021`、region escapeは `SRC0022` とし、既存の詳細なdiagnostic precedenceを適用します。Call／type argument／move／borrow／region spanと関連位置を維持します。別caseはruntime guest panicであり、型不一致やinvariant破壊にはしません。Signatureとともに固定panic codeを文書化します。

構文を変えず既存formatter generic-call規則を使い、入れ子specializationの往復fixtureを追加します。LSP hoverはspecialization後のby-value signatureとresultを表示でき、debuggerはmove後sourceをunavailableにします。Borrowed bindingからunwrapへの自動fixはpanicの可能性を導入するため行いません。既存revision-safe diagnosticと、延期済みlinter／DAPの実装責任は維持します。

## 性能とコストモデル

Case確認とownership移譲で、新しいguest payload、transfer metadata、drop stack、panic reportをallocationしません。必要storageはeffect前に通常の上限付きframe／allocation planでadmissionし、そのadmission自体は失敗し得ます。Allocating expression全体を失敗不能にしたり通常frame課金を省いたりはしません。物理的なzero-byte copyは要求せず、inline payload／構築metadataの移動にはlayoutに比例するworkがかかり得ます。Heap payloadを辿るcloneやreference count増加はしません。成功時にuser dispatch／callbackはありません。

Match後unwrapではtagを2回確認する場合があります。重複確認の除去は任意で、failure behaviorとprovenanceを維持する必要があります。Compile-time specialization／型検査は既存のwork／depth／instantiation上限に課金します。計測済み速度、ergonomics、code size、constant-timeを保証しません。該当pathの実行時にsource fixtureと入れ子移譲costを測定します。

## Securityとtrust boundary

Untrusted MIR／artifactがintrinsic名を使って、期待tagの断定、payload型の偽装、owner複製、arena brand消去、事前admission／cleanupの回避をできないようにします。既存trust boundaryでtype／tag／layout／authorityを検証します。不正な内部valueは既存規則によるvalidated runtime fault、有効な別caseは規定の通常guest panicです。Failure後のcleanupは上限付きでallocation不要に保ちます。Filesystem、network、process、environment、native address、host capabilityは付与しません。

## 代替案

| 代替案 | 判断とtradeoff |
| --- | --- |
| 変更しない | Constructorから変更へ進めない受理済み設計が残り、実測した `SRC0014` が次のsource fixtureを妨げる。 |
| User helper／library／tooling | 既存move／borrow規則を越えられない。Compiler専用helperが必要なので、選択した有限core操作で例外を明示する。 |
| 消費する `match move`／pattern | 最も強い汎用案。全user-enum caseを所有権付きで回復可能に扱い、二重tag確認を避けられる。新しい構文／型／branch-join契約、全caseのobligation移譲、Copy／wildcard／入れ子patternの判断が必要。広い設計は延期し、将来helperと共存させられる。 |
| Mutable payload loan | Bytesのその場での変更は助けるが、UTF-8変換が消費するowned Bytesやcollectionからの取り出しには足りず、新loan形式も必要。 |
| Ok／Some unwrapだけ | Intrinsicが少し減るがowning Errを取り出せない。3つ目の操作で、guard match後にResultのどちらのbranchも回復可能な形で扱える。 |
| 失敗不能constructor／unchecked extraction | Failure pathは簡単になるが、受理済みの回復可能allocation／checked cleanup保証を捨てるため不採用。 |

## 実装計画

1. **M2-S01：** RFC 0001の委任でこの明示follow-upを受理し、RFC 0007と両翻訳を更新する。実装statusは別に扱う。
2. **M2-S02（3 pt）：** 有限HIR／MIR signatureとsource loweringを、compile-pass／fail、move／loan／region／provenance、不正MIR、formatterの裏付け付きで拡張する。既存call構文とby-value検査を再利用し、このsliceではwire encodingやruntime dispatchを追加しない。
3. **M2-S03（3 pt）：** 対応Bytes／Copy profileのadmission済みwhole-payload移譲とcase failure実行を追加する。Source owner取り出しfixture、panic cleanup、全fuel prefix、OOM／admission境界、transfer用追加allocationがないことを検証する。未対応payloadはeffect前にgateする。
4. **M2-V04／V06と既存collection／shared／arena／native task：** 変更操作、入れ子構築順序の移譲、Result両branch、Optionへ実操作／payload対応を広げ、その後cross-mode／tier-1 conformanceを満たす。従来exit gateを削除しない。S02／S03は暫定であり、実装前提が上限付きoutcomeを超える場合はcode前に分割／再見積もりする。

## 検証計画

- Compile pass：3操作の明示specialization、Copy source再利用、Bytes owner取り出し後のmutable local利用、guard付きErr取り出し、同一regionのarena-bearing payload、従来borrowed match。
- Compile fail：不正／不足type argument、不正sum／型／loan argument、禁止storage型、provisional readのunwrap、liveな名前付きloan、move後source再利用、branch／loop初期化、新call経由のtransitive arena escape。
- MIR／trust：不正signature／result型、捏造tag／guard、偽provenance、owner複製、不正transfer先を現在の検証boundary内で独立拒否する。既存の広範なarena-verifier taskも引き続き必須。
- 実行：期待case、各固定wrong-case panic、成功／失敗両方のowning payload、入れ子の逆構築順exactly-once cleanup、shared／allocator／region identity保持、全fuel prefix、host interruption、cleanup failure precedence／poison、effect前preflight／OOM拒否。Provider request数でtransfer用追加allocationがないことを確認する。
- 互換性／tooling：既存Copy source／borrowed match fixture、schema-1 golden artifact、generic-call format／recovery、正確なsource診断。実装時に既存fuzz targetへunwrap入力を追加し、encoding受理前にwire fuzz targetを作らない。
- Cross-mode：実装後、VM／native differentialと両tier-1実行でvalue、panic code／span、drop trace、limitを一致させる。性能主張前にinline／入れ子移譲と二重tag確認のcostを測る。設計acceptance自体はこれらの実行証拠ではない。

## 未解決事項

Decision：**2026-10-18 JSTに実験段階の委任に基づきaccepted**。再現済みの具体的blocker、既存構文／ownership規則を使う追加的で有限なAPI、明示panic／cleanup意味論、RFC 0007／0008に整合する上限付きcompiler／runtime sliceがあるとreviewで判断しました。このscopeのblocking design questionはなく、個別owner approval待ちにはしません。

2026-10-18 JST確認の部分実装：S02でsource signature 3種類と独立検証済みMIR callを追加し、move／loan検査と選択payloadのregion伝播も確認しました。S03ではBytes／Copy payloadの3操作と有限Option／Result構築を実行し、panic code／spanとfailure cleanupを維持します。[Owned Bytes fixture](../../../examples/scripts/owned_bytes.lang) はtransfer allocationなしで `0` を表示します。Guide全体のfixtureはBytes push dispatch（V04）が未対応です。入れ子non-Copy source aggregate、他owning payload、REPL、native conformanceは実装gateを維持します。

未検証の仮説：guard後unwrapのergonomicsが初期source workloadに十分なこと、今後対応する全payload adapterをadmission済みmetadataで移譲できること。現testは実際のBytes、Copy value、managedな入れ子の順序／span移譲とfailure cleanupを扱い、全source payload／modeを証明しません。新作業はslice拡大前に明示し、未対応payload／modeのgateを維持します。将来の汎用的な消費matchは別の設計判断であり、borrowed matchを暗黙変更する許可ではありません。

## 情報源

- [RFC 0001](0001-project-charter.md#実験段階のrfc判断)：判断／改訂の委任方針。2026-10-17 JST最終更新（`b08f24b`）、2026-10-18 JST確認。
- [RFC 0007](0007-source-ownership-and-core-values.md)：whole-owner move、borrowed match、core catalog、region規則。元の受理本文は2026-10-17 JST最終更新（`31ab7ac`）。Follow-up参照を2026-10-18 JST追加、同日確認。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)：panic、cleanup precedence、allocator／C boundary。2026-10-17 JST最終更新（`b094a58`）、2026-10-18 JST確認。
- [MIR core validation](../../../crates/mir/src/unit/ownership/core/validation.rs)：既存transitive Storable検査、2026-10-17 JST最終変更（`4619937`）。[Ownership instruction](../../../crates/mir/src/unit/ownership/bodies.rs)：既存whole `UnpackVariant` contract、2026-10-18 JST最終変更（`a984e5e`）。両方2026-10-18 JST確認。実装の接続点を示し、取り出しの実装完了ではない。
- [Source Bytes fixture](../../../examples/scripts/bytes.lang)：2026-10-18 JST最初の実装（`ece8690`）。Read-only source実行は `0`、match payloadをowned Bytesへbindingする試みは `SRC0014` を再現。2026-10-18 JST確認。実装前の基準 `bde2902` ではguide exampleは `SRC0024` となり、S02（`b562c0f`）で解析が成功しました。[Owned取り出しfixture](../../../examples/scripts/owned_bytes.lang)、[実行test](../../../tests/owned_execution.rs)、[managed移譲test](../../../crates/runtime/tests/managed_values.rs)、[source／MIR test](../../../crates/semantics/tests/sum_extraction.rs) ではBytes／Copy実行、allocation数、panic code／span、failure cleanupを確認しています。2026-10-18 JST更新／確認。広いsource payload／native coverageは未証明です。
- [実装Todo](../milestone-2.md)：scope／依存／残見積もり。2026-10-18 JST更新・確認。本RFCはproject判断であり、外部言語の新しい事実や計測済み性能を主張しない。
