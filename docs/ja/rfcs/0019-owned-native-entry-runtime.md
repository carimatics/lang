<!-- translation-of: ../../rfcs/0019-owned-native-entry-runtime.md; canonical-language: en -->

# RFC 0019: 通常owned native entry runtime

[English](../../rfcs/0019-owned-native-entry-runtime.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P05
- Supersedes: none; extends RFCs 0012/0013/0014/0018
- Superseded by: none

## 現在の実装状態

2026-10-20 JSTに[M2 capabilityと検証根拠](../milestone-2-capabilities.md)と照合済みです。本RFCの範囲はimplementedであり、stabilizedではありません。以下の当初計画、decision時点の記述、日付付きcheckpointはrollout履歴として読みます。そこで別作業としていたnative／SDK／platformの現在状態は上記ガイドを参照してください。今回発見したRFC 0012の論理debugger検証不足は同ガイドの訂正で解消し、本RFCの非目標や規範的要求は変更しません。

## Summary

実際のcompile／link済みcontinuationと既存managed primitivesを使う、通常ownership program用の具体的Rust runtime入口を追加します。引数なし・i64結果の検証済みfunction ID、既存cleanup Instance、計上付きAllocator<P>、上限付き準備／control設定を明示的に受け取ります。値・usage・cleanup report、または元の構造化native／設定／allocation失敗とusage／report／stackを返します。実行中のsource分析、機械語生成、JIT生成、guest MIR評価は行いません。

Review結果：runtime実装前の**実験的委任に基づくaccepted、2026-10-20 JST**。既存C NativeFactoryは全managed service familyを実行し、P03／RFC 0018の記録でもtest runtimeによる通常linked実行ができます。現StateはC専用NativeProgram trait、context Provider、C export result transferに結合しているため、providerを汎用化してprimitives／pool準備を共有し、C dispatchは実際のC特殊化に残します。System providerの通常runtimeと全差分matrixは受理時点で未検証です。

実装の証拠（2026-10-20 JST）：System／汎用providerの通常entryで、managed／panicの10 sourceを実JIT／保存済みlinked continuationから実行できます。全fuel／work／provider拒否prefix、動的control、元stack／report、retry、作用前の拒否、host panic時poisonを検査し、既存C testで元context特殊化を維持します。SDK／C context配布と両host clean installは別作業で、対訳architecture／実装Todoに証拠を記録します。

## Motivation

P05はfileからnative planを復元できますが、通常ownerの実行にはまだrepoのtest runtimeが必要です。Fixtureを配布すればtest／frontend依存が残ります。架空のC context／exportや合成MIR wrapperはidentityを混同するかentryのfuel／workを変えます。通常用runtimeを複製すれば動作中のC実装から乖離します。

## Goals

- 元の通常entryと全現行ownership／core familyをproduction primitivesで実行。
- 設定control／allocator計上、cleanup、panic code／span、guest stack、instanceの永続poisonを保持。
- C権限を通常実行に与えず、C／nativeのstorageとguest call機構を共有。

## Non-goals

- 新構文、意味論operation、portable／native記録形式、machine packet／helper、公開C／component ABI、安定plugin ABI。
- 既定のC unit／host binding有効化、VM fallback、SDK install、P06／P07／P02完了扱い。
- Copy nativeのtrusted同期・非計量契約の変更。

## Guide-level design

RFC 0018に従い意味論／記録をdecodeして対応するtrusted imageをbindし、そのprogramと明示entry IDで通常runtimeを呼びます。Bind時のunsafeな正確なcode／image／toolchain条件は維持し、code ownerと検証domainを完了まで借用します。Sealedなbackend program interfaceが準備済みlayout／usageを与え、JIT／linked ownershipの元continuationを呼びます。通常／C実行でstorage、loan、collection／graph、arena inspection、必須retirementを共有します。

Hostは実allocator providerを渡し、Systemや失敗記録providerにC contextは不要です。C exportは既存NativeFactoryと元C allocator／contextを使用します。Decodeしたdataからnative pointer、context、grantを得ることはありません。

## Reference semantics

Guest／provider作用前に、不正entry／domain、引数あり、非i64結果、未対応のC interface全体、poison、未対応control、準備予算超過を拒否します。Pool準備には有限のlogical frame／live cell制限が必要で、通常設定に明記した有限defaultを用意しても構いません。RFC 0012／0013のcontrolと共有service workを適用し、同じ設定でのsafe point／失敗優先順位をVMに揃えます。未対応設定で部分実行しません。

最初のguest命令前にframe／loan／capture／logical stack／heap／arena／inspection／retirement storageを上限内で準備します。Guest frame領域はadmission時に元の計上allocatorから取得し、root引数は空です。生成cleanupとledger完了までreturn escrowを保持し、frame解放前に検証済みi64を読み、全pool／heapを一度だけ閉じます。必須cleanupは非計量で独立失敗後も継続し、最初の失敗／panicを優先して副次失敗も記録します。Boundary／host panicは一時的Rust wrapperだけでなくcleanup instanceを永続poisonし、正常な失敗なら同instanceでretryできます。

内部Store／Drain／StateとprimitivesをP: allocation::Providerへ汎用化します。C引数／結果transferとimport呼出しは実ContextProvider identityを検査するC特殊化／adapterに維持します。明示的な内部entry mode／domain型で通常root結果とC export metadataを分離。通常program interfaceをsealedとし、外部callbackやdecode済みmetadataが実行code ownerを実装することはありません。共有plan検索で機械語を変更／再生成しません。

## Execution modes

VM意味論をoracleとします。通常nativeは実compile済みcontinuationをdispatchし、型付きstorage primitivesを使い、MIR／bytecodeを解釈しません。JIT／linked通常codeで新runtimeを共有します。C exportの公開APIとcontext／allocator gateを保ち、同じ汎用primitivesを呼びます。Component／WebAssemblyとasyncは不変です。

## ABI and compatibility

Wire tag、構文、private native呼出し規約は変更しません。Native記録1とportable schema 1／2／3／4は維持し、通常実行でC unit全体の拒否を維持します。新Rust backend embedding APIは実験的で、versionを越えるSDK ABIではありません。対応compiler／runtimeで再compileします。Copyと既存C embeddingの動作は維持します。

## Diagnostics and tooling

構造化した準備／設定、allocation、native失敗、cleanup情報を返し、失敗時もentry／function ID、元span、panic code、順序付きreport、source stack、消費usageを保持します。Parser／formatter／LSPは不変で、debugger steppingの証拠には数えません。

## Performance and cost model

Provider汎用化でmonomorphizationが増える可能性がありますが、primitives共有により実装の複製を避けます。Pool準備と意味論検証のwork／memoryは残り、native codeが算術／制御、runtime serviceがstorage／retirementを担当します。B01–B04で起動／実行／sizeを測定し、改善を仮定しません。

## Security and trust boundaries

RFC 0018のtrusted native codeとunsafe bindingを維持します。C allocator／context identityの捏造、capability抑制、通常runtime経由のcallback許可を行いません。作用前に上限を検査し、unwindで必須retirementを迂回したりpoison済みstateを再使用したりできません。Region／ownerとcode借用は呼出しを越えて逃げません。

## Alternatives

- 変更なし／test runtime配布：repo専用依存とproduction起動の不足を残す。
- VMを包むlibrary：native bodyを実行しないため拒否。
- 架空C export／contextやguest wrapper：権限の混同やfuel／stack変更があるため拒否。
- 通常runtime複製：意味論乖離と保守負担が増えるためprovider汎用primitivesを共有。

## Implementation plan

1. Managed storage／providerの汎用化とC固有dispatch分離。既存C fixtureを維持。
2. 通常entry／complete処理を追加し、Bytesを使うsource entryを実native codeで実行。
3. 保存記録linked起動、全managed family、guest再帰／loan、panic／cleanup、必要な全limit／provider prefix、retry、元code／allocator保持を検証。対訳文書を更新し、この検証後だけP05完了を監査。

## Test plan

代表sourceで値、fuel／host call／work、peak frame／cell、panic／span／stack、report、割当／解放をVMと比較します。正常／失敗root return、有限fuel／work／frame／cell、interruption／deadline、全provider拒否、cleanup失敗／host panic／poison、同instance retryを検査。不正entry／domain／C unit／設定を作用前に拒否します。既存C／Copy回帰と両host証拠を維持し、重点検証後に全baselineを実行します。Testから性能を主張しません。

## Open questions

意味論のblocking questionはありません。正確なRust API名と内部構成は実装に従えますが、entry形状、作用、control、wire形式、互換性の約束の変更前にRFCを改訂します。SDK／公開startup ABIは別作業です。

## Sources

- [Native continuation契約](0012-native-ownership-continuations.md)、2026-10-20更新。
- [Service work](0013-service-byte-work-budget.md)、2026-10-20 JST確認。
- [Native記録transport](0018-native-compiler-records.md)、実装5e0d6a1、2026-10-20 JST。
- [既存runtime](../../../crates/codegen-clif/src/c_abi/runtime.rs)と[factory](../../../crates/codegen-clif/src/c_abi/runtime/factory.rs)、2026-10-20 JST確認。State::chargeは既にControl::chargeへ委ね、C専用program traitは必須Digestを返します。
- [実装Todo](../milestone-2.md)、2026-10-20 JST更新。Projectの証拠／決定であり、新通常runtimeは受理時点で未検証です。
