<!-- translation-of: ../../rfcs/0007-source-ownership-and-core-values.md; canonical-language: en -->

# RFC 0007：Sourceのownershipとcore value

[English](../../rfcs/0007-source-ownership-and-core-values.md)

- ステータス: implemented
- 著者: Project contributors
- 作成日: 2026-10-17
- 最終更新日: 2026-10-21
- Implemented: 2026-10-20
- Tracking issue:
- Supersedes:
- Superseded by:

## 現在の実装状態

2026-10-20 JSTに[M2完了監査](../milestone-2-capabilities.md)と照合しました。有限source/core profileは型検査、独立MIR検証、VM/native実行、両hostのconformanceまで実装済みです（M2 G03–G10、G14/G16）。必要なcleanup、抽出、artifact契約はRFC 0008/0009/0011で具体化済みです。 implementedであり、stabilizedではありません。

以下の動機、当初の段階的計画、日付付き部分実装checkpointは、判断とrolloutの履歴です。当時の未完了ゲートは上記根拠で完了しており、規範的契約と明示的非目標は引き続き有効です。この状態照合でsource、ABI、形式の意味論を変更しません。

## 概要

RFC 0004の受理済みownership policyを再検討することなく、Milestone 2の最初のdecision gateを具体化します。RFC 0006のCopy programを維持し、non-copyなcore valueと字句的loanを追加し、CLI toolと将来のbootstrap compilerに必要な最小collection／型表面を定義します。以下の受理済み判断は、上記の有限profileで実装済みです。

Parameter typeはRust風の `&T`／`&mut T`、明示的ownership categoryは `shared T`／`weak T`、字句的arenaに推論上のbrandで結び付く参照は `arena T` とします。通常のrecord／enumは全field／payloadがCopyの場合に限りCopyです。Coreの `Text`、`Bytes`、`Vec<T>`、`Map<K, V>` はstorageを所有しmoveします。ここに列挙する組み込みgeneric type／functionのみ導入し、user-defined generic declaration、method、trait、lifetime parameter、暗黙のcloneは延期します。

Cleanup／allocator／unsafe／FFIの独立したdecision gateは引き続き必須です。このRFCはsource typeとoperationの表面を定め、allocator callback、foreign memory validity、destructor failureの順序、ownership bytecode encodingは定めません。Acceptanceだけではimplementation taskは完了しません。

## 動機

RFC 0006は意図的にcopyableで非循環のrecord／enumだけを扱います。Backend prototypeの `Own(ResourceTypeId)` ではowned text、字句的loan、container、arena brand付きsource valueを表現できません。RFC 0004はownershipの結果を受理しましたが、constructor／parameterの表記は暫定のままです。Parser、HIR、VM、native backendが独自に決めれば契約が分裂します。

代表的toolにはUTF-8 text、生のsource／input byte、可変長sequence、symbol table、明示的error、予測可能なlifetime境界が必要です。Libraryだけではmoveやarena escapeを強制できません。この提案はその基盤を用意し、milestoneをscalarだけの実演へ縮小せず、完全な標準libraryも主張しません。

## 目標

- 受理済みRFC 0006のsource／diagnostic fixtureをすべて保持する。
- Compile-pass／fail fixtureを記述できる精度でCopy、move、borrow、field mutation、字句的arenaを定義する。
- Allocation、shared clone、weak upgrade、checked shared mutationを明示的な名前付きoperationにする。
- Text／byte、collection、fallible operationの有限なcore surfaceを決め、user genericなしで具体的functionによるbootstrapを可能にする。
- Typed MIRをVM、native layout、allocator実装から独立させる。

## 非目標

- User generic、trait、method、closure、local annotationの推論、暗黙変換、partial move、reference field、通常loanのescape、自己参照／pinning、suspensionをまたぐborrow。
- User-defined destructor構文、effect／exception構文、`?` propagation、allocator callback ABI、C export構文、raw pointer、`unsafe` block。関連する公開interface実装前に第2decision gateで契約を決める。
- 新しいcomponent ABI、package protocol、stable native表現、完全なUnicode library、計測を追加しないnative sandbox。
- 任意のkey hash／equality callback、並行container mutation、performance保証。

## 利用者向け設計

以下は明示的 `core::` pathを使い、暗黙のpreludeは追加しません。通常のbindingはRFC 0006の必須 `let mut name: Type = expression;`、function全体の名前一意性、明示return、parameter bindingの不変性を維持します。

```text
struct Message { body: core::Text }

fn length(message: &Message) -> i64 {
    return core::text_len(&message.body);
}

fn forward(message: Message) -> Message {
    return message;
}
```

適合するplaceへの `length(value)` はread loanを推論し、`length(&value)` は明示形です。`forward(value)` は `value` をmoveし、後の利用はmove／use span付き `ownership.use-after-move` となります。Call用に推論したloanはcall終了時に終わり、localへ格納したloanはそのlocalを囲む字句的blockの終了まで続きます。

```text
fn update(value: &mut Message, replacement: core::Text) -> core::Unit {
    (*value).body = replacement;
    return core::Unit {};
}
```

Exclusive loanが終わるまで同じownerへの別のaccessを禁止します。Borrowはowner全体を単位として保守的に扱い、fieldが互いに異なってもこの規則を回避しません。初期化済みfieldの置換では、別途受理するcleanup規則で古いfieldを破棄し、囲むrecordのpartial moveは行いません。

```text
struct Node { next: core::Option<arena Node>, value: i64 }

fn example(allocator: &core::Allocator) -> core::Unit {
    arena graph(allocator) {
        // Allocationとreplacementはfallible。Resultを明示的にmatchする。
        // Compilerがallocationやunwrapを挿入することはない。
    }
    return core::Unit {};
}
```

`arena graph(allocator) { ... }` は RFC 0004 の暫定構築構文を確定します。T16 の明確化（2026-10-21 JST、実装前に実験段階の委任で採択）: entry は invocation の allocator 権限を compiler-created arena owner に捕捉する間だけ allocator 引数を借用し、引数 temporary を entry 直後に終えます。指定された host allocator は全 task frame と arena retirement より長く生存します。これにより guest reference を suspension に持ち越さず allocator identity を保持します。別に binding した通常 allocator loan は lexical lifetime を維持し、通常 payload/read loan と同様に suspension 越しを拒否します。既存 region escape/task transfer 制限は必須です。entry temporary が終わった arena scope の中断を認めます。以前の source は temporary を領域全体に保持していたため拒否していました。arena owner は compiler-created で move/return/copy できず、ユーザーの parameter 型として記述できません。同じ region 内の参照は cycle を作れます。`arena T` は reference token であって dereference 可能な raw pointer ではなく、アクセスには下記 core arena operation を使います。

## 参照セマンティクス

### Grammarと名前

`type` に `& type`、`& mut type`、`shared type`、`weak type`、`arena type`、列挙した組み込みgeneric pathを追加します。Prefix place borrowの `&place`／`&mut place`、括弧付きdereferenceの `(*expression)`、field assignment、`arena IDENT "(" expression ")" block` を追加します。Placeはlocal、borrowed parameter／dereference、record field chainです。任意の一時expressionを明示borrowしてlocalに保持することは禁止します。通常loanを1段だけ自動field dereferenceし、`(*loan)` でも同じ意味になります。Shared／weak／arena tokenには自動dereferenceしません。

新しいpre-alpha profileで `shared`、`weak`、`arena` を予約語とし、従来のidentifier利用にはmigration diagnosticを出します。`core` を予約root namespaceとし、item／import／localによるshadowingを禁止します。他のRFC 0006字句規則、precedence、module privacy、record-head制限、match末尾commaは維持します。Lexerは `&`、`*`、genericの `<`／`>`、`::` を識別し、符号付き整数tokenizationと比較の意味を変えません。

`core::Option<T>`、`core::Result<T, E>`、`core::Vec<T>`、`core::Map<K, V>` のtype argumentは必須です。Generic core callは `core::vec_len::<i64>(&values)` のように `::<...>` を明記します。`::<` はcall specializationであり、通常の `<` は比較です。入れ子generic末尾は個別の `>` tokenです。User itemのtype parameter宣言や列挙外のgeneric constructorは禁止します。

Option／Result caseは `core::Option::None`、`core::Option::Some(value)`、`core::Result::Ok(value)`、`core::Result::Err(error)` と書きます。Type argumentはannotation付きlocal、function parameter／result、aggregate field、match scrutineeの期待型から決まり、context不足／曖昧さはerrorです。Patternは既存のfull-shape／exhaustiveness規則に従います。Unitはinhabitedな空record `core::Unit {}` です。

### Copyとowning type

| 型 | Copy規則 |
| --- | --- |
| `i64`、`bool`、`core::Unit` | Copy |
| User record／enum | 全field／全case payloadがCopy |
| `core::Option<T>`／`core::Result<T, E>` | 全type argumentがCopy |
| `arena T` | Copy tokenだがbrand／region制限を保持 |
| `core::Text`、`core::Bytes`、`core::Vec<T>`、`core::Map<K, V>` | Storageを所有するnon-copy型 |
| `shared T`、`weak T` | Non-copy tokenで、複製は明示cloneのみ |
| 通常の `&T`、`&mut T` | 格納可能なowning valueではなくloan。転送はreborrow |
| `core::Allocator` | Hostが作るopaque authorityで、loan経由のみ利用可能 |

Copy valueはRFC 0006のassignment／call／returnを維持します。Non-copy placeのmoveは初期化済みvalue全体を移譲します。Non-copy aggregateのfieldをvalueで取り出すpartial moveは禁止し、match bindingを1つだけmoveすることも禁止します。そのaggregateのmatchはscrutineeをmatch全体で1回borrowし、non-copy bindingはfieldへのread loan、Copy bindingはcopyになります。Loanがliveの間scrutineeのmutation／moveを禁止します。By-value wildcardはownerを暗黙consumeしません。Match終了でloanが終わればscrutinee全体をmoveできます。

2026-10-18 JST受理のfollow-up：[RFC 0009](0009-whole-sum-ownership-extraction.md) が `option_unwrap`、`result_unwrap`、`result_unwrap_err` によるOption／Result全体の明示取り出しを追加します。Sum全体をvalueで消費し、期待payloadを返すか、RFC 0008のcleanupを伴うpanicにします。以下の有限core catalogを拡張しますが、borrowed matchやpartial move禁止は変えません。Acceptanceは実装完了を意味しません。

Record／enumが自身を再帰的に参照できるのは、owning indirection（Vec、Map、shared、weak）またはarena reference経由だけです。Compilerは有限inline layoutと有限個の具体的組み込みinstantiationを確認し、type argumentが無限展開する再帰を課金付きwork／depth／instantiation budgetで拒否します。`Option<Node>` だけではinline cycleを切れません。通常loanをfield、container、function result、globalへ格納してはいけません。Allocatorもguest valueへ格納できません。

### Loanとregionの確認

Owning parameter向けargumentは暗黙borrowしません。Borrowed parameterは明示的に適合するloanか、適合placeからの推論loanを受け取ります。Shared reborrowはreadを許し、exclusive reborrowはその字句的期間に親loanをfreezeします。Mutable loanにはmutable local／placeまたは既存exclusive loanが必要です。Owning parameterのbindingは不変のままです。Shared loanは別shared loanを許可しmutation／move／destructionを禁止し、exclusive loanはそのloan経由以外のaccessを禁止します。

名前付きlocal loanは未使用でも囲むblockまでliveです。Call専用の推論／明示loanと一時reborrowはerror edgeを含めcall後に作成の逆順で終了します。Branch joinは初期化／move／loan stateを保守的に合流し、owning useには到達する全pathでの初期化が必要です。全体をmove済みのmutable localは再初期化できますが、未初期化localのborrowは禁止します。Loop back-edgeはheaderのownership stateを再確立しなければなりません。Partial initialization／moveは禁止します。Assignmentはdestination place、RHSの順に評価し、置換の検証、publication、旧valueのcleanupを行い、中間の未初期化ownerを公開しません。

User functionは通常loanをreturn／storeできません。Core read／projection operationはcompilerが認識する例外で、call siteのownerへ結び付くloanを生成し、任意のlibraryからreferenceを返す機構にはなりません。全valueに含まれるarena brandをlocal、field、container、call、patternを通して追跡します。Region付きvalueはregionより長く生存したり、shared／weak storageへ入ったり、別arenaをまたいだり、function resultとしてescapeしたりできません。同じregion内の通常owning localはarena tokenを保持できます。Functionへ渡すarena referenceはcaller brand付きのread-only tokenで、calleeのallocation／mutationにはcallerのarena ownerが必要ですが、これは公開parameter typeにはできません。Task、suspension、global storage、closure escapeは追加しません。

### Core typeとoperationの表面

以下のgeneric parameterはすべてcall siteで具体化します。`A` は `&core::Allocator`、`U` は `core::Unit`、`R<T, E>` は `core::Result<T, E>` の説明用略記で、sourceの別名ではありません。Count／indexは非負の `i64` で、負値／範囲外はstructured errorです。変換でindex／sizeを切り詰めません。

| 分類 | 必須の名前付きoperationとsignature |
| --- | --- |
| Allocator authority | `allocator() -> &Allocator`。Compilerが認識する実行instanceのloan。Hostはentry前にallocatorを供給／認可し、不足時はguest effectなしでentry preflightが失敗する |
| Text | `text_from_utf8(Bytes) -> R<Text, Utf8Error>`、`text_clone(A, &Text) -> R<Text, AllocError>`、`text_len(&Text) -> i64`、`text_bytes(A, &Text) -> R<Bytes, AllocError>`、`text_equal(&Text, &Text) -> bool`、`text_concat(A, &Text, &Text) -> R<Text, AllocError>` |
| Bytes | `bytes_new(A) -> R<Bytes, AllocError>`、`bytes_clone(A, &Bytes) -> R<Bytes, AllocError>`、`bytes_len(&Bytes) -> i64`、`bytes_get(&Bytes, i64) -> R<i64, BoundsError>`、`bytes_push(&mut Bytes, i64) -> R<U, BufferError>`、`bytes_set(&mut Bytes, i64, i64) -> R<U, BufferError>` |
| Vec | `vec_new::<T>(A) -> R<Vec<T>, AllocError>`、`vec_len::<T>(&Vec<T>) -> i64`、`vec_push::<T>(&mut Vec<T>, T) -> R<U, AllocError>`、`vec_pop::<T>(&mut Vec<T>) -> Option<T>`、`vec_get::<T>(&Vec<T>, i64) -> R<&T, BoundsError>`、`vec_get_mut::<T>(&mut Vec<T>, i64) -> R<&mut T, BoundsError>` |
| Map | `map_new::<K,V>(A) -> R<Map<K,V>, AllocError>`、`map_len::<K,V>(&Map<K,V>) -> i64`、`map_insert::<K,V>(&mut Map<K,V>, K, V) -> R<Option<V>, AllocError>`、`map_get::<K,V>(&Map<K,V>, &K) -> Option<&V>`、`map_remove::<K,V>(&mut Map<K,V>, &K) -> Option<V>` |
| Shared／weak | `shared_new::<T>(A, T) -> R<shared T, OwnershipError>`、`shared_clone::<T>(&shared T) -> R<shared T, OwnershipError>`、`shared_read::<T>(&shared T) -> R<&T, OwnershipError>`、`shared_replace::<T>(&mut shared T, T) -> R<U, OwnershipError>`、`weak_new::<T>(&shared T) -> R<weak T, OwnershipError>`、`weak_clone::<T>(&weak T) -> R<weak T, OwnershipError>`、`weak_upgrade::<T>(&weak T) -> R<shared T, OwnershipError>` |
| Arena | `arena_new::<T>(&mut arena-owner, T) -> R<arena T, ArenaError>`、`arena_read::<T>(&arena-owner, arena T) -> R<&T, ArenaError>`、`arena_replace::<T>(&mut arena-owner, arena T, T) -> R<U, ArenaError>`。Owner表記は内部用で、sourceでは字句的arena名を渡す |

ここでText／Bytes／error名は `core::` の型です。表中のloanを含むOption／Resultはcompiler専用loan resultで、直ちにmatchでき、格納／return／他のowning valueへの入れ子は禁止します。成功bindingは字句的loanとなり、未使用／失敗armはprovisional loanを終了します。一般のuser typeとして `Option<&T>`／`Result<&T,E>` は導入しません。

Textは妥当なUTF-8で、lengthはbyte数、equalityはbyte比較です。Normalizationやscalar／grapheme indexingを意味しません。`text_from_utf8` は成功／失敗どちらでもBytesをconsumeし、最初の不正byte offsetを報告し、不正入力を黙って置換しません。Bytes要素は `0..=255` のchecked integerで、それ以外は `BufferError::InvalidByte` です。このprofileはtext／byte literal escapeを導入せず、明示的typed host inputとbyte構築でdataを供給します。String literalのergonomicsは別途提案でき、compiler sourceの取り込みを妨げません。

Map keyは `i64` または `core::Text` だけで、数値／厳密UTF-8 byte equalityを使います。Iteration APIや観測可能hash順は導入せず、順序が必要なbootstrap algorithmは別Vecを保持します。Key置換では既存keyを保持し、供給された等しいkeyをconsume／dropし、旧valueを返します。Push／insert失敗はcollectionを変更しませんが、owning argumentは既にmove済みでoperation内でcleanupします。Allocating constructorも失敗時にowning argumentをconsumeします。失敗でmove元の変数を暗黙復元しません。Failure atomicityはpreflight／publicationを指し、後続の評価済みincoming ownerのcleanupではreferenceをreleaseし得ます。Allocationする各collectionは将来のgrowth／destruction用allocator identityを保持し、allocator／host contextは全allocationより長く生存しなければなりません。`core::allocator()` により既存の引数なし `main` からowning programを実行でき、実行instanceへ明示供給されたauthorityのloanだけを公開し、ambient native allocatorやcallback addressは公開しません。

Core errorは有限のCopy enumです：`AllocError::{Limit, OutOfMemory, Overflow}`、`BoundsError::OutOfBounds`、`BufferError::{Bounds, InvalidByte, Allocation(AllocError)}`、`Utf8Error::Invalid(i64)`、`OwnershipError::{Allocation(AllocError), StrongCycle, ExpiredWeak, BorrowConflict, Unavailable, CountOverflow}`、`ArenaError::{Allocation(AllocError), CrossRegion, Unavailable, BorrowConflict}`。`text_from_utf8` は検証後にowned byteとallocator identityを移譲するため追加allocationを必要としません。Clone／new／concat／growthのerrorは表のresult error型に従います。空storageはallocationを省略できてもallocator identityと設定済みaccountingの一貫性を保持します。Allocation／publication前にlengthが非負i64へ収まることを確認し、表現できない要求はoperationのallocation-overflow errorを返します。

Shared payloadはchecked read loanか `shared_replace` によるatomicな全体置換だけを許し、無制限mutable dereferenceは提供しません。どのaliasでもliveなreadがあればdynamic boundaryで置換を拒否します。置換は入れ子storageの全strong edgeをpreflightしてから新payload全体をpublishし、拒否時は旧payloadを保持し、atomic publication境界を出てからcleanupします。Incoming valueの明示的ownershipを含めRFC 0004のcycle／count rollback policyに従います。Expired weak upgradeは受理済みlogical identity付き `OwnershipError::ExpiredWeak` を返します。Arena readはarenaをborrowし、そのloan終了まで当該ownerでの置換／allocationを禁止します。同一regionのreferenceはnodeをretainせずcopyできます。Arena exitはaccessを取り消してからallocationの逆順でteardownし、いずれのcategoryもraw address authorityを作りません。

## 実行mode

HIRはownershipと推論region／loanを保持し、MIRはmove、borrow begin／end、replacement、shared／weak operation、arena enter／exit、cleanup edgeを明示します。VM／JIT／AOTは同じ検証済みsemantic transitionとruntime結果を使います。Checked operationは元spanを保持し、拒否は守るべきguest／host effectより先に起こります。Resource exhaustionで部分stateをpublishしません。既存copy-only bytecode schema 1は維持し、新operationのserialize前に別途受理されたversioned ownership encodingが必要です。In-memory typeのacceptanceはdecoder tagの独自追加を認可しません。

`926fb3b` のnative backend専用cell規約はcopy valueの実装evidenceであり、owning heap ABIではありません。未実装ownership operationは実行／code publication前にstructured unsupported-feature errorとし、その中間制限でMilestone 2 exitを満たしたことにはしません。Constant evaluationはまだ利用できません。Embeddingはtyped valueとallocator authorityを明示供給し、component resourceは引き続きRFC 0003 adapterを使います。これらのtypeからambient filesystem／network／process／environment permissionは生じません。

## ABIと互換性

Compatibility domainはsource、bytecode、native-abi、tooling、bootstrapです。新しい予約語／root namespaceを除き全RFC 0006 Copy programの意味を維持します。Migrationは該当nameを個別diagnoseし、conflict確認付きrenameを提案して暗黙cloneは行いません。Generic delimiter／loan構文は既存parse fixtureを保持します。Schema-1 golden artifactはbyte同一で引き続きsupportし、将来のownership artifactは別schemaで非対応versionを拒否します。

Native ownership／allocator表現はtoolchain専用でfrontend APIへ漏らしません。Shared／weak／arena tokenとsource containerはC／component layoutではありません。公開module signatureは正規化したownership／type argumentを記録し、通常loan／arena brandの漏洩を拒否します。それを記録するpackage metadataは独自の受理済みversioned formatに従い、この提案はversionなしpackage byteやstable ABIを導入しません。Bootstrapはuser genericなしで具体的helper functionと組み込みcollectionの具体的specializationを利用できます。

## 診断とtooling

RFC 0004のlogical identityと既存 `SRC0001`–`SRC0019` を維持します。`SRC0020` はuse-after-move／uninitialized owner、`SRC0021` はloan conflict／illegal borrowing、`SRC0022` はloan／arena brandのescape、`SRC0023` は不正ownership／type storage、`SRC0024` は未対応built-in specialization／reserved-name migrationに予約します。詳細categoryと関連move／borrow／region spanはstructured fieldで、後段backend failureだけからdiagnosticを選びません。Compile-limit失敗は `SRC0011`、runtime cycle／cross-region／stale／expired-weakはRFC 0004のidentityとoperation spanを保持します。

Parser recoveryは不正loan／type argumentと後続itemを保持します。Formatter往復は入れ子 `>>`、`::<...>`、`&mut`、prefix category、dereference-field assignment、arenaを含めます。LSP hover／referenceはCopyかmove／loanか、source owner、推論arena regionを示し、revision-safe publicationを維持します。Native debug viewはaddressを露出せずsource value／token／unavailableを区別します。完全なlinter／DAP実装は予定済みmilestoneのままで、このRFCで完了扱いにはしません。

## Performanceとcost model

以下は仮説と実装要件で、計測済みperformanceではありません。Copy aggregate判定は構造的で、move／loanにはallocation／retainは必須ではありません。Text／Bytes／Vec／Mapの構築／growthは明示的fallible allocation pathとallocator identityを持ちます。Text clone／concat、byte変換は入力長に比例したcopyを行い得ます。Generic built-inの具体的型specializationはcode sizeを増やし得るため、compilerのinstantiation／work／allocation budgetで展開を制限します。Shared／weak metadataとcycle traversalはRFC 0004の明示的costを維持し、arenaはbulk lifetimeを保証してもbump allocationは保証しません。

代表的performanceを主張する前に、source parse／check、container growth、UTF-8検証、symbol table、retain／releaseとcycle publication、peak live／requested byte、executable size、VM／native差を両tier-1 platformで計測します。同じinput、最適化mode、allocator／accounting範囲、sample方針で等価なRust／C workloadと比較します。現在のnative copy testはbehaviorを証明し、ownership costやbootstrap速度を証明しません。

## Securityとtrust boundary

MIR／bytecode／host境界でowning type／region identity、loan、handle、generation、初期化、operation順を検証します。Source path／diagnostic nameはI/Oを認可しません。Storage access／reserve前にsize／index arithmeticを確認し、count／generation枯渇はfail closedとします。明示compile／decode／runtime work／allocation limitでgeneric instantiation、container request、UTF-8 scan、graph traversal、cleanup bookkeepingを制限します。Foreign mutationでtracked shared publicationを回避できません。Native callbackと計測用制限のないnative実行は信頼する境界で、ownershipはprocess sandboxではありません。

## 代替案

- **変更なし：** RFC 0006のCopy-only sourceを継続する。Ownership／core-library outcomeは未完了のままで、要求するproduction profileを実装できない。
- **Library／toolingだけ：** Host containerで機能を供給できても、全modeのmove／loan validityとarena escapeを確立できない。
- **最も強力な競合surface：** 無制限user generic、trait、method、nonlexical lifetime analysis、reference-valued API。抽象化は広がるが、最初のowning source program前により大きなinference／coherence／monomorphization／lifetime契約が必要。将来RFCで導入でき、ここではergonomics／performance上の優位を推測しない。
- **より単純なownership-only surface：** Textとmoveだけ追加しsharing／arena／collectionを延期する。有用な最初の実装sliceだが、既存Milestone 2を置き換えられない。本提案は全outcomeを維持して段階実装を可能にする。

## 実装計画

1. OwnerまたはRFC 0001の実験段階の委任による判断を記録する。Acceptanceまで `proposed` を維持し、各契約が受理されるまで両roadmap decision gateは未完了とする。
2. Lossless grammar／format／recovery fixture、正規化source type identityを追加し、Copy／move／lexical-loan／region analysisをcompile-pass／fail診断と課金付きdataflow／instantiation上限で実装する。
3. 公開／wire interface実装前にcleanup、allocator、unsafe／FFI、versioned ownership bytecode契約を定義／受理する。上記surfaceに対してcore-library／native integrationを再見積もりし、元の全outcomeを維持する。
4. 明示MIR transition／verifierとruntime allocation／shared／arena serviceを追加し、同じsource operationをVM／JIT／AOTで実装する。開発中の未対応pathはeffect／publication前に拒否する。
5. Text／bytes、有限collection specialization、allocator-aware growth、代表的source CLI／bootstrap形状workloadを追加する。Typed host I/Oには別途受理するcapability／host-value interfaceが必要。
6. Tier-1 ownership-aware link／run／debug evidence、differential cleanup／failure trace、代表的Rust／C比較、対訳guide、formatter／LSP変更、fuzzingを完了する。Acceptanceはmilestone完了／stabilizationではない。

## Test計画

| 分野 | 必要なevidence |
| --- | --- |
| 互換性 | RFC 0006の全source fixture／schema-1 golden byte維持、予約名migrationの正確なdiagnostic |
| Copy／move | Scalar／入れ子Copy aggregateを再利用可能、Text／Vec／shared／weakのmove後利用拒否、local全体の再初期化、partial move拒否 |
| Loan | 推論／明示read、exclusive update、shared／exclusive reborrow、未使用lexical loanのconflict、field重複、moved owner、borrowed parameterのreturn／storage、join／back-edge |
| Region | 同一arena cycle、入れ子arena escape、cross-arena replacement、arenaからshared／container／function-resultへのescape、read loan中のmutation拒否、stale generation再利用拒否 |
| Core | UTF-8不正offset、byteの0／255／256、負／最大index、空／pop／map置換、allocator保持、全allocating operationのpublication前failure |
| Generic上限 | 組み込みarity、key型拒否、inline再帰、有限indirection再帰、具体的specialization再利用、depth／work／allocation／instantiation境界 |
| Shared runtime | 明示clone／drop、retain付きweak upgrade、別aliasのread／replace conflict、入れ子container経由cycle preflight、rollback／race、count枯渇 |
| Cross-mode | VM／JIT／AOTで同じvalue／effect／drop順序、failure reason、span、resource accounting、両tier-1 hostのdirect／response-file実行 |
| Tooling／trust | Recovery／format往復、revision-safe loan diagnostic、不正ownership MIR／artifact／handle、generic／type／region graphのfuzz、ABI拒否の文書化 |

## 未解決事項

Repository ownerは2026-10-17 JSTにこのsource／core profileを明示的に受理しました。字句的whole-owner borrow、compiler専用組み込みgeneric collection、checked whole-payload shared mutationを含み、このprofileのblocking questionは残っていません。Cleanup／allocator／FFI validityと厳密ownership wire encodingは、依存実装前にそれぞれの判断が必要であり、Milestone 2完了に任意のfollow-upではありません。

Decision：repository ownerが**2026-10-17 JSTにaccepted**。判断前のreview outcomeはdecision-readyでした。Implementation、stabilization、計測済みergonomics／performanceは主張しません。

Follow-up decision：最初のsource実行でallocating resultからowned payloadを得られないことが判明し、**2026-10-18 JSTに実験段階の委任でRFC 0009をaccepted**としました。その明示操作とfailure／互換性規則でcatalogを拡張し、元のmatch規則は維持します。

## 情報源

- [RFC 0009](0009-whole-sum-ownership-extraction.md)：受理済みsum全体取り出しのfollow-upと再現済みsource blocker。2026-10-18 JST作成／更新・確認。
- [RFC 0004](0004-owned-values-and-cyclic-graphs.md)：受理済みownership／category／cycle／arena方針。2026-10-17 JST最終更新、同日確認。
- [RFC 0006](0006-source-frontend-and-portable-programs.md)：実装済みsource／Copy／portable program基準。2026-10-17 JST最終更新、同日確認。
- [Architecture](../architecture.md)、[roadmap](../roadmap.md)：project契約と残りoutcome。2026-10-17 JST最終更新、同日確認。
- `crates/semantics/src/graph.rs`：比較用ownership state model、`98bbb08` で2026-10-17 JST最終更新。`crates/codegen-clif/src/program.rs`、`tests/native_program.rs`：copy-value native実装／evidence、commit `926fb3b`、2026-10-17 JST commit。それぞれのprototype／copy境界を証明し、このsource拡張は証明しません。
- 本提案は外部言語について新しい事実／performanceを主張しません。構文、有限core API、migration、limit、延期は受理済みrepository契約から導くproject判断で、比較designは代替案であって独立計測結果ではありません。
