<!-- translation-of: ../../rfcs/0012-native-ownership-continuations.md; canonical-language: en -->

# RFC 0012: Native ownershipの継続とmanaged frame

[English](../../rfcs/0012-native-ownership-continuations.md)

- ステータス: accepted
- 著者: Codex、ownerの実験段階の委任に基づく
- 作成日: 2026-10-18
- 最終更新日: 2026-10-20
- Tracking issue: M2-N08、M2-N01〜N07、M2-G01〜G03
- 置き換えるRFC: なし。RFC 0008のprivate native実行契約を具体化
- 置き換えたRFC: なし

拡張（2026-10-19）：[RFC 0014](0014-explicit-source-panic.md)は明示的Panicを含むunit全体にprivate revision 3を選びます。packet／statusとhelper index 0–33を保ち、動的panic helper 34を末尾に追加します。それ以外のunitはrevision 1/2を維持します。

拡張（2026-10-20）：[RFC 0015](0015-c-native-object-bridge.md)は明示C対応objectのprivate bridge revision 1、artifact別symbol、object内import thunkを定めます。既存packet／helperと通常／JIT namespaceは維持します。

## 概要

Ownership MIRを、既存の2 pointer／i32 statusのmachine conventionを使うnative継続にcompileします。上限付きhost driverがcompile済み関数addressをdispatchし、managed valueはruntimeの予約済みframeに保持します。Guestの算術、分岐、継続選択は機械コードです。DriverはMIR／bytecodeを解釈せず、VMへfallbackしません。

この決定はN01の前に不足していたownership call／context／公開契約を補います。Ownership backend、C ABI adapter、platform実行、performance、debugger gateの完了を意味しません。

## 動機

現在のnative APIはRFC 0006のCopy `VerifiedUnit` valueをi64 cellへ平坦化します。Ownership実行は独立検証済みfailure MIR、`layout::Layouts`、`FramePlan`／`FrameStorage`、service token、初期化順序、retirement work領域を使います。32 byte tokenのcopyはownership移転ではありません。Aggregateのpayload byteにはcleanup metadataが含まれません。

Native関数を直接再帰callすると、受理済みguest frame／cell budgetとは別のplatform stack上限が生まれます。予約済みlogical frameと一定深さのdispatch loopで、再帰ownership callのadmissionとretirement境界を明示します。これはprojectの設計判断であり、速度上の優位性は主張しません。

## 目標

- RFC 0007／0008のsource意味論、limit、failure優先順位、返り値escrow、解放順序、allocator権限をVM／native間で保つ。
- Runtime layoutとproofをCraneliftから独立させ、既存Copy native ABIを維持する。
- Guest call dispatch中の暗黙のhost allocationを避け、実装可能なnative境界を定める。

## 非目標

- Source ABIの安定化、生のmanaged tokenのC公開、C profile 1、bytecode schema 1／2、component ABI、package形式の変更。
- 新構文、ユーザーgeneric、async中断、実行中native frameのthread間移動、native sandbox。
- 実現性実験がownership sourceや生きたmanaged valueを実行するという主張。

## ガイドレベルの設計

Sourceは変わりません。[再帰owner fixture](../../../examples/scripts/owned_calls.lang) はBytesを `carry` へmoveし、`length` にborrowさせます。現在のVMは `1` を表示します。Nativeも同じ結果、各ownerの一度の解放、失敗／limitの一致を満たす必要があります。このRFCでnative CLI commandは追加しません。

未対応のnative operationやadmission設定は、guest／provider effectの前に構造化diagnosticで拒否します。Source operationのerrorは元spanを維持し、host設定のerrorは拒否したlimitと、利用可能な場合は関連entry spanを示します。対応済みprefixだけを実行したり、VM実行に置き換えたりしません。

## 参照意味論

### Private revision 1 packet

Tier-1 targetは64 bit pointerを使います。Machine signatureはtarget Cの `fn(input_pointer, output_pointer) -> i32` を維持します。Ownership symbolはwhole-unit内private namespace `lang_owned_r1_f<function-index>` を使い、Copyの `lang_unit_f<index>` と区別します。以下のpacketはtarget nativeのC alignmentを使い、Rust valueの直列化や独立link可能interfaceではありません。

| Input field | Offset／幅 | 意味 |
| --- | --- | --- |
| context | 0／pointer（8 byte） | 不透明で排他的にadmitされたinvocation context |
| frame | 8／u64 | Context内のlogical frame identity |
| continuation | 16／u32 | そのframeの関数内の検証済み継続 |
| reserved | 20／u32 | Zero |

Inputのsize／alignmentは24／8です。Outputのsize／alignmentは16／8です。

| Output field | Offset／幅 | 意味 |
| --- | --- | --- |
| frame | 0／u64 | Admit済みの次のlogical frame |
| function | 8／u32 | Compile済みwhole-unit function index |
| continuation | 12／u32 | その検証済み継続 |

Status **0** は最外invocationのreturn、**1** は次の継続のdispatch、**2** は終端failureを表します。Output packetの公開はstatus 1のみです。Status 0／2はpacketを変更せず、結果と構造化failureはcontext所有storageに置きます。他のstatusは不正です。Calleeの成功returnはcallerへdispatchし、最外returnにはなりません。

Packetを構築するのは信頼されたgatewayのみです。生成codeのcall前にprogram／context identity、生存frame identity、function／continuation境界、reservedのzero、instance poison状態、非再入を確認します。Dispatch前には、提案されたtarget全体をcontextの準備済み遷移と照合します。不正遷移は閉じた状態で失敗します。Sourceやdecode済みartifactは生pointerや権限を供給しません。生成entry guardは不正な継続をeffect前に拒否します。

### 明示C import用のprivate revision 2

RFC 0001の実験段階の委任に基づき、**2026-10-19 JST** に改訂を受理します。明示的なC対応ownership unitは **lang_owned_r2_f<function-index>** を使います。通常のownership unitはrevision 1と既存symbolを維持します。Revision 2でも、前述のinput／output packetのsize・field offset、2 pointer／i32 machine signature、終端／dispatch statusは同じで、外部C export署名ではありません。

対応runtimeはrevision 1のhelper prefix（index 0–32の署名／意味／offset）を保ち、宣言済みC import用helperをindex **33** へ追加します。Revision 1 codeは元のprefixだけを参照するため、runtimeは拡張済みtableを保持したままrevision 1も実行できます。Revision 2のcode生成とfunction metadataはrevision 2 namespaceを使い、通常のrevision 1／object gatewayがそのsymbol名だけを受け入れてC実行を有効化してはいけません。

新helperは静的に準備したimport identity、引数／result cell位置、source spanを受け取ります。RFC 0008のcontext gate越しに宣言済みforeign serviceを実行し、MIR control flowをdecodeしません。Guest／providerのeffect前に、実code owner、C metadata全体と明示grant、contextへ一度だけ結び付けた実accounting allocator、一致するcontrol／共有workを要求します。C値の完全な事前検査後にhost callを1回だけ計上し、raw foreign statusをguest panicやnative継続statusと分けて保持します。Sourceへ公開する前にresultを検証／copyし、通常のfailure cleanupとpoison規則を維持します。初期実装はscalar／Unit importに対応し、未対応import profile全体を作用前に拒否します。Aggregate importと外部export／AOT対応は引き続きM2要件です。

根拠と代替案：C実行を拒否し続けてもM2を達成できず、operationをdecodeするlibrary経路はnative loweringを満たしません。Versionを変えずhelperを追加すると契約変更が隠れます。通常のrevision 1 unitを全て改名しても意味論は変わらず移行だけが増えます。Revision 1 prefixを保つ別のrevision 2 C対応namespaceにより、通常objectを維持しながら拡張を確認できます。C対応codeは対応runtimeと再buildし、過去に生成したcodeのlabelだけを変更しません。今後の非互換packet／status／helper／layout変更にも別途RFCとprivate revisionを要求します。C profile 1のdescriptor／digest／header、source構文、schema 1／2とそのC識別拒否は不変です。

2026-10-19 JST確認の実現性evidence：生成typedefと照合したC compiler産callbackをsource VM／nativeから呼び42／59を返し、256引数で32,896を得ました。全有限fuel／work境界、foreign失敗、code／allocator provenanceも対象fixtureがあります。改訂前prototypeは判断根拠であり、最終revision 2やWindowsのevidenceではありません。Revision 2の再検証、native Windows／MSVC、汎用aggregate／export／factory統合と性能計測は未検証です。この改訂から性能や追加のsource／FFI権限は主張しません。

### 予約済みframeとruntime operation

Contextは検証済みfailure MIR identity、source map、compiled-address table、grant、allocator、control／usage状態、service、logical frame stack、返り値escrow、failure reportを所有します。Host wrapper poolとreport bufferはすべて最外guest entry前に準備します。Guest payloadとretirement／work storageはVMと同じactivation位置で、引数移転前に `FrameStorage::admit` でadmitし、先行確保するguest allocation poolへ移しません。Provider requestの順序／size、試行課金、rollbackを保ちます。Compile／layout／frame budgetは累積し、proof stageは既存usageをresetしません。Overflowや未対応容量は、要求limitを暗黙に下げずentry前に失敗します。

Payload、初期化／順序metadata、logical loan、service identity、反復cleanup領域は既存runtime layoutを使います。Packetにowning valueは入りません。生tokenのcopyやnative stack spillでownerは増えません。Scalar算術／比較／分岐と継続選択は機械命令へlowerします。型付き・上限付きruntime helperが、admit済みlocationのload／store、managed rootの移転、projectionの解決、実serviceの呼出しを担います。Operationをdecodeするinterpreterにはしません。

Guest callは引数消費前にcallee activationと返り値escrowを予約し、検証済み引数移転をcommitし、caller継続とsource call siteを記録してcalleeへdispatchします。Driverは封じたtableからcompiled addressを選択します。Returnは結果を準備し、生成failure／cleanup edgeを実行し、必要なcleanup成功後にcallerの予約済みdestinationへ公開します。準備／cleanup／公開の失敗は既存failure MIRに従い、不完全なreturnは観測できません。解放は初期化順序の逆で、元のfault／call／cleanup spanを維持します。

Guest再帰はlogical frameを使い、driver再帰やC call stackの増大に依存しません。1つのnative継続は、call／return／終端failureまで通常のbasic blockを実行できます。Helper呼出しには上限付き／反復serviceとcleanup algorithmを使います。Host callbackの受理済み再入制限は維持します。

### Limit、failure、権限

Owned native profileは計測命令を挿入します。Fuel課金／poll位置、host-call課金、frame／cell admission、中断、allocator結果、usage reportは、入力／grant／limitが同じ場合に共有VM契約と一致させます。Runtime helperはユーザー宣言host callに数えません。既存の非計測Copy profileは変更せず、明示的に区別します。

構造化failureをcontextへ記録します。生成failure CFGがcleanupを実行してから終端status 2を返します。Provider／host callbackのpanicはprivate C境界をunwindできません。Adapterで封じ込め、受理済みfailure優先順位を保ち、不変条件を復元できない場合はinstanceをpoisonします。Poisonからguest実行を再開しません。Unsafeなpointer型消去は信頼されたgateway／helper shimに限定し、packet範囲、生存期間、排他性、非保持の不変条件を記録します。

Source型検査、推論、評価順序、lexical loan、arena brand、並行性／atomicity、明示的capabilityは受理済みの意味を保ちます。この同期private実行契約はconstant評価やasyncの挙動を追加しません。Runtime helperは明示grantされたallocator／host権限のみを受け取り、native実行は引き続きtrusted codeでありcapability sandboxではありません。

## 実行モード

Bytecode VMは独立した意味論比較経路を維持します。Native AOT／JITは同じ継続lowering、packet、layout、runtime helperを使います。AOTは実機械関数をemitし、対応する信頼されたdriver／runtimeをlinkします。Compile済み継続はVMへ入りません。C exportは同期ownership gatewayを出入りし、C profile 1の検証／公開を適用します。内部packetはC export signatureではありません。Wasm／componentの対応は延期したままです。

## ABIと互換性

影響domainは1つのtoolchain build内の `native-abi` です。既存Copy symbol、2 pointer／status convention、型付きhost import、source diagnostic、C profile 1、schema 1 golden byte、schema 2 ownership artifactは維持します。各native ownership unitとruntimeは同時buildし、独立compileされたownership objectの互換性は約束しません。Invocation前にprivate revision／program identityを確認します。Packet／status／helper／layout契約を後で変える場合は、出力前に明示的RFC改訂とprivate symbol／descriptor revisionが必要です。ユーザーsource移行は不要です。

## Diagnosticとtooling

各継続の元FunctionId、source span、call site、logical parentをprivate context／debug descriptorに保持します。Native検証／admission／未対応operationの失敗は構造化し、既存runtime failure identityを保ちます。Parser、formatter、LSPの文法変更は不要です。Debug metadataはguest source位置でnative codeをstepできる必要があります。Hardware frameはguest call chain全体を表さないため、debugger統合はdescriptorからlogical chainを公開します。Driverだけの生backtraceではG03を完了しません。実際のmacOS／Windowsでのstep／backtrace evidenceが引き続き必要です。

## Performanceとcost model

Call／returnには固定packet store、admission／runtime処理、1回のdispatch loop遷移が必要です。Owner移転には必要なmetadata／loan／cleanup処理が残ります。Host wrapperはentry前に準備し、managed payload／metadataとservice allocationはVMと同じ失敗可能なactivation／operation位置を維持します。生成native codeとcompile metadataには明示compile budgetを設けます。Cranelift内部allocationはtrusted compiler境界であり、host memory全体を完全計上した上限ではありません。Dispatchにhost container拡張を隠しません。Scalar blockはdriverへ戻らず実行でき、fuel／poll effectは最適化で消去できません。Dispatch overhead、code size、最適化品質、platform costは既存M2 benchmark gateまで未計測です。

## Securityとtrust boundary

独立region／loan検証を含む完全検証済みownership failure MIRのみを受け入れます。Foreign入力はlive managed valueを作る前に各gatewayで検証します。生context address、frame ID、生成address、private descriptorはportable artifactやsource valueへ入りません。実行codeのownerは全callより長く生存し、error時に未公開codeを解放します。Helperはpacket pointerを保持せず、一時host referenceを公開しません。Native pluginはtrustedのままであり、このprofile自体はfilesystem、network、environment、process、FFI権限を付与しません。

## 代替案

- **変更なし／Copy API：** managed metadata、loan、返り値escrowを表現できません。
- **VMを包むlibrary：** 現sourceを実行できますが、native ownership loweringの実装にはなりません。
- **直接再帰native call：** 通常のhardware backtraceを保ち、call costを減らせる可能性がありますが、別のnative stack admission／overflow契約とplatform evidenceが必要です。後で選ぶ場合は明示改訂が必要です。
- **DecodeしたMIR operationごとのhelper：** interpreter dispatchを再利用できますが、機械コードloweringの目的を満たしません。
- **選択した継続方式：** 上限付きruntime primitiveを共有し、実operation／制御flowをemitします。明示call／return dispatchとlogical debug chainのcostを受け入れます。

## 実装計画

1. **N08：** 今回の決定と、機械コード／target objectの限定的な実現性実験。
2. **N01：** 本番packet／context／frame／helper admissionとnative function／return layout出力を実装します。N08／N01の後は、さらに基盤を増やす前にownership source実行を示します。独立した新前提が実証された場合は実装前に分割します。
3. **N02〜N05：** managed move／aggregate／loan、core service、shared／arena operation、生成failure／cleanup／controlをlowerします。受理済みpayload familyとsource結果をすべて維持し、scalar限定の代替にしません。
4. **N06／N07、F／P／Q／B／G／Z行：** 実際のmacOS／Windowsでのobject／link／run、C境界、packaging、複合conformance、計測、debugger evidence、最終英日capability guideを引き続き必要とします。

## 検証計画

`owned_abi_probe` はhost上で2つの生成関数を実行します。MIR／bytecode evaluatorなしで1,000回のcall／return列が各回42を計算し、不活性な32 byte dataを保ち、不正継続ではstate／outputを変更しません。Runtime Bytes layoutとaggregate retirement metadataを確認し、aarch64 macOS Mach-Oとx86-64 Windows COFF objectを出力してsymbolを検査します。Packet／dispatchの実現性を示すもので、live ownership、source lowering、C validity、performance、target link／run、target debuggerの実証ではありません。

本番testでは、再帰owned／loan source call、全provider拒否位置／fuel prefix／中断位置、不正context／frame／遷移、moved／initialized状態、解放順序／failure優先順位、返り値escrow、poison済みinstance拒否、無allocationのwrapper再利用、受理済みservice／payload全組合せ、VM／JIT／AOT一致を検証します。既存schema 1／2互換fixtureを保ちます。Native adapterには限定unsafe境界testと実tier-1実行／debug evidenceが必要です。Fuzzは上限付き検証済みmetadataに限定し、任意の非信頼native codeを実行しません。

## 未解決事項

N01を妨げる契約上の未決事項はありません。Performance、本番helper統合、platform linker／runtime統合、debuggerでの再構成は未検証の実装仮定です。既存行／gateを開いたままにして、実験に基づき再見積もりします。受理でその完了を認めません。

RFC 0001の実験段階の委任により **2026-10-18 JSTにaccepted** とします。理由は、既存machine signatureとmanaged runtime不変条件を維持し、上限のないnative再帰stackを避け、具体的host／target object試作があり、source／native／platform全体の要件を保持するためです。本番実験が仮定と矛盾する場合は明示的に改訂します。

## 情報源

- [RFC 0001](../../rfcs/0001-project-charter.md)、実験段階の委任：最終更新2026-10-17、確認2026-10-18 JST。
- [RFC 0007](../../rfcs/0007-source-ownership-and-core-values.md)：最終更新2026-10-18、[RFC 0008](../../rfcs/0008-cleanup-allocators-and-c-boundaries.md)：最終更新2026-10-17、両方の確認2026-10-18 JST。[RFC 0011](../../rfcs/0011-ownership-artifacts.md)：最終更新2026-10-18、確認2026-10-18 JST。
- Localの [Copy native lowering](../../../crates/codegen-clif/src/program.rs)、[managed layout](../../../crates/runtime/src/layout.rs)、[frame admission](../../../crates/runtime/src/layout/frame.rs)、[ABI実験](../../../crates/codegen-clif/src/owned_abi_probe.rs)：検査・検証2026-10-18 JST。Cranelift 0.136.3はrepository lockfileで解決された版です。Local APIの公開／更新日はここでは不明で、確認日2026-10-18 JSTを記録します。外部performanceの主張やWeb調査は使っていません。
