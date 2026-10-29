<!-- canonical-language: en; english-source: ../spec-improvement-study.md -->

# 仕様改善調査：Goose・メモリドメイン・GraalVM

[English](../spec-improvement-study.md)

- 調査日・最終更新日：2026-10-30 JST。
- レビュー判定：**needs prototype/measurement**。
- プロジェクト基準：[`df55f6e`](https://github.com/carimatics/lang/commit/df55f6ebcf33edb824b7a96e878ab9da5540228d)。[GraalVM調査](graalvm-study.md)を含む`main`のsnapshot。
- 範囲：Goose調査、複数メモリドメインの分析、それらと現行lang契約を照合した検討の証拠・判断基準を保存する。必要な知見・例・出典を本書に収録し、別のローカルレポートを前提にしない。
- 状態：研究上の提言であり、受理済みRFC、実装完了の主張、milestoneの開始、新しいdelivery見積りではない。

## 推奨

既定の基礎として所有権を維持する。最初に最適化の適合条件と保持理由を明確にし、次にnativeコードの選択的な削除、不変なコンパイル結果の再利用、構築先への直接構築を測定する。packedな不変データやoptional traced heapは、必要性を示すworkloadがある場合に検討する。

三つの調査は相補的である。Gooseは配置と構築、GraalVMは到達性・準備・観測、メモリドメイン検討は異なる回収方式を組み合わせる境界条件を提供する。共通typed MIRの置換、mandatory GC、Truffle中心のruntimeへの書換え、stale参照の安全性低下を正当化するものではない。

| 候補 | 推奨 | 採用条件 |
| --- | --- | --- |
| 最適化の適合契約 | 最優先 | 振舞いを変える前に既存の失敗・コスト・所有権・権限・provenance規則を整理する。 |
| 到達性・所有権・割当の説明 | 読取り専用reportから開始 | 静的facts、論理課金、実測コストを区別し、解析と出力量を制限する。 |
| entry/exportを起点とするnative削除 | 限定実験 | 入力全体を先に検証し、call以外の依存とartifact/debug対応を維持する。 |
| 不変code/planの再利用 | 測定優先 | binding/code ownerの寿命と、instanceごとの独立した権限・stateを保つ。 |
| 構築先への直接構築 | 限定したMIR実験 | 評価、部分失敗時のcleanup、return escrow、論理課金を維持する。 |
| packedデータと切り離せるfrozen segment | workloadを定めて実験 | 既存Bytes＋所有権移動と比較し、境界検証と所有権を成立させる。 |
| optional traced参照 | 要件と試作が必要 | 別の型/category、制限したbridge、回収・コスト・失敗契約。shared/arenaを黙って変えない。 |
| 全面的なGoose stack、既定sharedのtracing化、Graal/Truffleへの書換え | この証拠では採用しない | 互換性・runtime変更を正当化する優位性が示されていない。 |

## 現行プロジェクトの制約

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)は将来の明示的なtraced型を許容するが、基礎となる`shared`表現へのtracing採用を退けている。owned値はmoveし、strong sharingは明示する。weak back-edgeとlexical arenaは異なるgraph寿命に対応する。別途設計されたadapterなしに、arena参照はescapeしたり別arena/shared heapへ渡ったりできない。数値slotの再利用でstale handleが後のallocationへ別名化してはならない。arenaが保証するのはbulk lifetimeとallocation逆順の破棄であり、特定のbump allocatorやpacked layoutではない。[L1]

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)は実際の初期化順の逆順cleanup、exactly-onceの義務、未公開のreturn escrow、失敗の優先順位、明示的allocator authorityを定義する。[RFC 0013](rfcs/0013-service-byte-work-budget.md)は論理service課金とeffect前のadmissionを定義する。[RFC 0024](rfcs/0024-send-share-and-service-authority.md)のSend/Shareはlogical task向けであり、全managed/host型のOSスレッド安全性を意味しない。[RFC 0033](rfcs/0033-component-v1-compatibility.md)はprivateなsource/SDK表現と独立してcomponent C v1を固定する。[L2–L5]

このsnapshotでは、ユーザー定義generics、一般的なclosure、明示lifetime parameter、suspendを越えるborrowは将来の設計対象である。本書の提言を試すために第二のsource languageは必要ない。active milestoneはM4のtoolingであり、本書はそのexit条件へメモリモデル作業を加えず、別のdoing deliveryも作らない。[L0]

先行Goose比較のlang基準は`1a9aac9`、メモリドメイン比較は`fb970d6`だった。RFC 0004・0008・0013・0024・0033は、それらから`df55f6e`まで変更がない。統合検討では後者のarchitecture、roadmap、package/editor契約を再確認した。別セッションの未merge変更は対象に含めない。

## Gooseから得られる知見

調査したGoose snapshotは[`cce94b9`](https://github.com/aardappel/goose/tree/cce94b919d27458856ae2685974049eac78debcf)で、仕様は**v4 draft**である。以下はその設計に関する事実であり、langの受理済み動作ではない。[G1–G3]

| Gooseの仕組み | 有用な発想 | langでのtradeoff・非互換性 |
| --- | --- | --- |
| 複数のdata stack、address spaceの予約とcommitによるgrowth | addressが安定したscratch storage、まとめたallocation | 予約、platform挙動、回収、再帰、回復可能なOOM、foreign allocator契約は別途必要。 |
| rootに基づくlifetime推論 | 参照が生存する理由とescapeできない理由を説明する | lifetime引数を書かないことは、無制限のborrowやlifetime検査の欠如を意味しない。 |
| growth/containment制限、relative reference、packed layout | ノードごとのallocationを減らした自己完結データ | offset検証、alignment、動的fieldアクセス、変更制限、SIMDのコストを測る必要がある。 |
| 型・root・mutability・静的関数値・構築先による特殊化 | 特殊化を説明しcode増加を制限する | 到達する具体化ごとの再検査は、langの全入力検査、package境界、editorの部分解析の代替にはならない。 |
| result destinationへの直接構築 | 一部の物理的な一時コピーを省く | by-value copyがpayloadサイズに比例する場合は残り、初期化・失敗意味論も観測可能。 |
| 再利用できるpool slot | storage再利用とobject identityを分ける | Gooseでは古い参照が同型の新しい値を観測できる場合があり、langのstale handle契約と衝突する。 |
| 独立したthread programとflatなコピーmessage | 大きな単位での分離と一括転送 | langのstructured task、capability/resource境界、Send/Shareの代替ではない。 |

GooseのAOTはCを出力し、embedded JITはTinyCCを使う。この選択から、既存VM/Cranelift構成の置換がlangを改善するとは言えない。upstreamのbenchmark表も言語・compiler・workloadが異なり、本調査はlang対Gooseの速度比較を主張しない。best-of-threeや集計scoreはtail latency、メモリ、再現性の保証ではない。[G2–G3]

### 過去に実行したGoose probe

macOS 27.2 arm64、Apple clang 21.0.0、CMake 4.4.5で、追加built-in packageなしの固定Goose snapshotをRelease buildした。警告は出たがbuildは成功した。以下は2026-10-30の調査で実行した小規模probeであり、全test/benchmark campaignではない。

| Probe | 観測結果 | 意味 |
| --- | --- | --- |
| 最初の参照を保持し、配列へ100,000要素を追加 | JITとC AOTで`100001 1 99.5` | 選んだstack予約条件で、調査したgrowthは参照を維持する。 |
| pool slotをfree・再利用して古い参照を読む | JITとC AOTで`99 99 1` | 型・範囲の安全性と論理objectのidentityは別の保証。 |
| 配列をclearし、その要素参照を後で使う | checkが拒否、exit 1 | root/lifetimeの制約は存在する。 |
| nonfixed scratchがrecursive callを越えて生存する再帰 | checkが拒否、exit 1 | data stackの寿命制約が再帰を制限する。再入前に寿命が終わるscratchは別のケース。 |

表の順に、入力全体は次のとおり。

```text
// stable.goose
struct Item { id: i32, weight: f32 }
fn main() {
    var items: Item[>..] = [];
    let first .= items.push(Item { 1, 0.5 });
    for i: i32 in 2..100002 { items.push(Item { i, 0.0 }); }
    first.weight = 99.5;
    print(items.len, " ", first.id, " ", first.weight);
}

// reuse.goose
struct Slot { value: i64 }
fn main() {
    reusable var pool: Slot[>..] = [];
    let old .= pool.alloc_ref(Slot { 7 });
    pool.free(pool.index_of(old));
    let fresh .= pool.alloc_ref(Slot { 99 });
    print(old.value, " ", fresh.value, " ", pool.len);
}

// shrink.goose
fn main() {
    var xs: i64[>..] = [];
    let first .= xs.push(7);
    xs.clear();
    print(first);
}

// recursion.goose
recursive fn f(n: i64) -> i64 {
    var scratch: u8[>..] = [];
    scratch.push(1);
    if n == 0 { return scratch.len; }
    return f(n - 1) + scratch.len;
}
fn main() { print(f(2)); }
```

固定revisionの独立したGoose checkoutで再現する場合、4入力を記載の名前で保存して実行する。

```sh
cmake -S . -B build-study -DCMAKE_BUILD_TYPE=Release -DGOOSE_BUILTIN_LIBS=
cmake --build build-study --parallel 2
./build-study/goose run stable.goose --stack-reserve 8M
./build-study/goose run reuse.goose --stack-reserve 8M
./build-study/goose check shrink.goose
./build-study/goose check recursion.goose
# The two check commands above are expected to exit 1.
./build-study/goose --standalone -o stable.c stable.goose --stack-reserve 8M
clang -O2 stable.c -o stable-native -lm -pthread
./stable-native
./build-study/goose --standalone -o reuse.c reuse.goose --stack-reserve 8M
clang -O2 reuse.c -o reuse-native -lm -pthread
./reuse-native
```

Windows、sanitizer、長時間stress、upstream benchmark全体の結果を意味しない。

## メモリドメイン：すべての表現ではなく契約を共有する

所有権、tracing、region lifetime、物理layoutは独立した軸である。一つのruntimeは、将来のすべてのfrontendにRust風のlifetime記法を強制せず、複数方式を提供できる。一方、任意の生の相互参照、独立した即時回収、object移動、無制限のcycle、境界コストなしを同時には保証できない。

domain内では各方式の効率的な表現を保つ。境界では次を区別する。

| 操作 | 必要な契約 | 初期の制限 |
| --- | --- | --- |
| scope内のborrow | domain/type identity、生存、アクセス権、利用期間を覆うlease。移動可能ならpin/再解決 | ordinary loanはsuspendを越えない。reentryでownerをclose/reuseさせない。nonmoving storageでも生存保護は必要。 |
| 長期保持 | 明示的なexternal root、または参加holderのtraced strong edge | 整数handleだけでは保持にならない。初期はweak edgeや強制可能な所有方向でcross-domain cycleを制限。 |
| 所有権移譲 | 切り離せるallocation単位、元allocator、destination metadataの予約、atomic publication | 任意のarena内の要素をO(1)でpromoteしない。commit前の失敗では元ownerを有効に保つ。 |
| copy/snapshot | 独立したdestination値、検査された変換、明示的会計 | 小さい値、非互換layout、複雑な参照graphでは単純な場合が多い。細かなbridgeを多数使う方式と比較する。 |

object rootは回収を防ぐが、addressを固定するとは限らない。generation検査はstale identityを拒否するが、検査とdereferenceの間のclose/reuseを防ぐわけではない。アクセス権と排他性も別である。動的境界ではdomain/table、generation、provider/holder、thread、loanを検査する。静的に証明された局所的不変条件により安価なアクセスを可能にする余地はあり、全参照を汎用動的handleへ置き換える必要はない。[L1–L5, S4]

### cycle・回収・失敗

二つのcollectorが各bridge edgeを永続external rootとして扱うと、外部から到達不能な混合cycleも保持される。選択肢はcycle制限/weak back-edge、明示的domain teardown、参加graphの協調tracingである。独立したowned resourceを黙ってGC finalization対象にしない。Kotlin/Nativeの公式文書にも実際の制限がある。KotlinとObjective-Cのobjectを含むretain cycleは、そのGC/ARCの組合せでは回収できない。[S3]

将来のtraced categoryの最小候補はthread-confined・非移動・user finalizerなし。それでもroot scope、pointer map、collector workとallocationの予算、回収trigger、weak参照/OOM、cleanup規則が必要になる。GCはSend/Shareを成立させない。moving/concurrent collectionにはcompilerの協力、mutation barrier、safepoint/stack map、同期protocolの定義も要る。Wasmtimeのscoped rootやLLVM statepointは有用な一次資料だが、そのままlangの意味論になるわけではない。[S4–S5]

VM/nativeの物理的回収時刻を完全に一致させる必要はない。ただしweak参照、finalization、OOM、予算、host effectの観測差は仕様化し、検証しなければならない。決定的resource closeは明示する。mandatory retirementはguest予算を使い切っても完了させる。

### 抽象モデルで確認したこと

先行検討では4ノード、self edge以外の12本の有向edgeの全組合せ、全root部分集合を比較した。`2^12 * 2^4 = 65,536`件のgraph/rootで、tracingと独立した推移閉包oracleが一致した。acquire/read/releaseとclose/reuseの順序を保つinterleaving、正常・例外・模擬キャンセルの3 cleanup経路も列挙した。これは抽象protocolの検査であり、production collectorやメモリ安全性の証明ではない。

重要な反例は、そのモデルの元ファイルなしでも次のように理解できる。

| シナリオ | 反例・必要な結果 |
| --- | --- |
| 全bridgeをroot化 | A→B、B→Aがありexternal rootがない場合、union tracingは両方を回収する。一方のbridgeでも無条件rootにすると両方が残る。 |
| generationだけ検査 | 古いtokenの検証 → close → slotを値99で再利用 → cached addressをdereferenceすると99を観測し得る。検証と一体でleaseを取得し、lease中のclose/reuseを除外する。 |
| rootがaddressを固定すると仮定 | rootされたobjectはcompaction後も生存するがaddressは変わる。cached pointerにはpinまたは安全な再解決が必要。 |
| domain identityを無視 | 異なるdomainでgeneration値が一致し得る。一方のtoken/leaseで他方をread/releaseさせない。 |
| tracing中のmutationを無視 | Aのscan後にA→未markのBを追加すると、collectorのmutation protocolが保護しなければlive objectを見失い得る。 |
| generationをwrap | 有限のgeneration空間を使い切ったslotは退役させ、古いidentityを復活させない。 |
| frozen regionを公開 | payloadコピーなしをモデル化したのは、切り離せるpointer-free allocationから元ownerの解放権限を失わせる場合だけ。任意の内側objectのpromoteを示さない。 |

実native memory、atomic memory ordering、stack map、実際のcancel、compilerのeffect検査、性能はモデルの対象外だった。productionの証明にはそれらの層も必要になる。

## 統合案：三つの関係を区別する

| 関係 | root/edgeまたは権限 | 答える質問 | 代替できないもの |
| --- | --- | --- | --- |
| code到達性 | artifactのentry/export/REPL保持定義。call、spawn、callback、cleanup、helper | artifactにどのcodeを残す必要があるか | host操作を実行する権限 |
| objectの生存 | external root/owner、strong/weak edge、region所有、loan | 何をいつ回収できるか | 実行関数の選択 |
| アクセス権限 | host grant、provider/holder、instance、thread、scope内loan | 今この操作を実行してよいか | 存在、addressの有効性、code identity |

有用な箇所では、不変identity、provenance、独立に検証したfactsを共有する。しかし、nodeが存在するだけで権限を与える一つの無型graphに統合しない。source lifetime root、GC root、code entry rootは異なる概念である。到達性reportはimportを説明できても、grantできない。[L1–L5, S1]

closed-world仮定はartifactごとに定める。単独実行プログラムは選択entryと契約上の外部入口を起点にする。C library/componentはcallable export、callback、teardownを保持する。REPLは将来呼ばれる定義を保持する。選択されたtest artifactにはtest entryがある。sourceの`pub`、interface export、backendの`Linkage::Export`は同じではない。未知の間接call先には健全で保守的な集合を用いるか、最適化を見送る。[S1, G1]

## 最適化を広げる前に適合条件を明確にする

同じ整数を返すだけでは不十分である。最適化は適用対象の既存契約を保つか、実装前にRFCで明示的に改定しなければならない。

| 観測対象 | 既存の義務 | 帰結 |
| --- | --- | --- |
| admission | 入力全体の型・所有権・artifact検証、適用対象となるwhole-unit control対応 | 不正な未使用bodyを検証前に削除しない。RFC 0013の2026-10-19 amendmentは全関数の検査を明示する。 |
| 失敗/effect | 評価順、失敗identity/span、primary/secondary順、admit済みhost/provider prefix | 観測可能なcallbackを越えてcapability検査を移動せず、拒否後のeffectを実行しない。 |
| 所有権 | 実初期化順の逆順、exactly-once cleanup、未公開return escrow | 直接構築でも履歴、部分失敗cleanup、cleanup失敗後のresult破棄を保つ。 |
| fuel/service work | 論理課金点、used/requested、pollと失敗優先順位 | 命令融合や物理コピー削減で論理eventを消さない。wall-clock時間の同一性は要求しない。 |
| allocation | 元provider、size/alignment、live/cumulative会計、callback、OOM | 実allocator要求を消して仮想counterだけ残しても、effectや失敗結果は変わり得る。 |
| identity/権限 | domain、generation、holder/provider、loan、grant | 同じoffsetやcode hashはobject identityや認可の証拠ではない。 |
| provenance/形式 | semantic ID、source span、native/debug range、検証済みrecord関係 | 出力のcompact化には有効な対応付け、永続形式変更には独立したversioningが必要。 |

RFC 0013は論理service操作を数え、任意の実装上のmemory trafficを数えるわけではない。一時swap bufferは追加の論理transferにならない。一方、RFC 0008のallocation要求はhostを呼び、失敗し得る。したがって物理コピーの削減と、観測可能なallocationの削除は異なる提案である。[L2–L3]

最小の反例は`charge(8); allocate(8); publish; free(8)`。limit 7ではeffect前に拒否する必要があり、chargeを消すと失敗が成功へ変わる。limit 8でもchargeだけ残してallocate/freeを消すとallocatorのeffect列が変わる。統合検討の小さな説明用モデルでこれらのassertionを確認したが、langに実装したpassではない。

まず現行契約を保つ変換から始める。allocation elisionの実測効果がallocatorの観測性を変えるに値する場合は、会計・失敗・host契約を明示的に定める。「高速profile」といった名前を保証弱化の許可にしない。

元の検証済みsemantic unitとIDを保ち、別の出力選択を実験する。現行native record検証が全関数の対応を要求する場合があるため、machine codeを消して非互換な旧recordを流用しない。portable artifactの縮小は別の互換性判断である。

## 具体的な改善機会

### 保持理由とコストを説明する

最初のreportは、なぜ関数・owner・allocation・copy・特殊化・helperが残るのかを、元sourceのprovenanceとともに示す。静的facts、指定された論理課金、物理的実測、不明な推定を区別する。code/debug/metadata/object全体/最終linkedサイズを分け、live requested bytesとRSSも分ける。GraalVMのbuild reportは参考になる。[S2]

editor向け結果にはadmit済みpackage/input/overlay epochを再利用し、古いsemantic factsを新しいfile versionへ結び付けない。初期の内部読取り専用APIに新source構文は不要である。公開CLI/JSON reportにはversioning、決定的順序、program/compiler/target/profile identity、解析・出力上限、明示的truncationが必要になる。出力を信頼済みproofやcapabilityにしない。report中のsource本文やpathの外部公開が自動的に許可されるわけでもない。[L6]

### 不変な準備結果を再利用する

cacheを加える前にcomponentの繰返しコンパイルを測定する。key候補にはprogram digest、compiler build、target/CPU feature、backend設定、root/artifact kind、ABI/layout/schema、cost profile、解決済みbindingの前提がある。埋め込んだhost addressには対応provider/code ownerの保持が必要で、任意のinstance間でportableにはならない。

hitごとにgrant、allocator、予算、resource table、callback authority、thread条件、可変stateを独立に成立させる。認可済みinstanceのcacheは不変code cacheではない。result/resource retirementが終わるまで実行codeのownerを保持し、entry数/bytesを制限し、信頼できない永続入力は検証する。disk cacheやnative image layerより先にin-processで始める。[L5, GraalVM study]

### 直接構築と選択的なpacked配置

aggregateをdestinationへ直接構築する一つのMIR変換を試す。記述したfield評価順、初期化済みfieldのcleanup、atomic replacement、return escrowを保つ。一般的なsource上の部分初期化を許可する提案ではない。物理コピーを省いても論理transferのコストは保つ。[L2–L3, G1]

packedデータでは、まず実際の不変snapshot workloadを既存Bytes＋明示moveと比較する。その基準で不足する場合にだけ、切り離せるfrozen segmentを導入する。publication前にoffset、overflow、alignment、tag、type、owner寿命、禁止された外部参照やlive mutable loanの不在を検証する。移す解放責任は独立したallocation一つであり、任意のinterior pointerではない。密度とアクセスコストを別々に測る。packed layoutはalignmentやvectorizationを難しくし得る。別のversion付き設計なしにprivate layoutをstable component ABIやportable snapshotとして公開しない。

## langでの実行証拠

`df55f6e`を、Rust 1.99.0（`b940084d7`、2026-09-28）、Cargo既定dev profile、macOS 27.2（`26B123`）arm64でbuildした。workspaceとbuild出力は別セッションから隔離した。統合検討の最終probeはexit 0だった。

| Fixture | VM/checkの結果 | native定義body symbol | text bytes | object bytes | portable bytes | native record bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `base`：42を返すmainだけ | VMは42 | 1 | 44 | 944 | 173 | 1,102 |
| `dead`：43を返す未使用spareを追加 | VMは42 | 2 | 88 | 1,248 | 294 | 1,474 |
| `invalid`：未使用spareがmissingを呼ぶ | check/run/nativeのすべてexit 1、`SRC0002`、span `2:28..2:35`。objectなし | — | — | — | — | — |
| `export`：mainなし、C apiがhelperを呼ぶ | native生成成功、公開api symbolを保持 | 2とexport/context wrapper | 2,272 | 4,480 | 547 | 5,658 |

未使用関数による増加は**text 44 bytes、object全体304 bytes**であり、machine codeが304 bytes増えたのではない。先行GraalVM実験の出力削減機会を、新しいsnapshotでも確認した。linked executableの削減、起動、RSS、compile速度の改善、実装済み削除passを測ったものではない。C exportは生成・検査したが、linkしたC hostから呼び出してはいない。

### 自己完結した再現手順

記録したbaselineのclean checkoutで実行する。scriptは`target`内に独立した一時fixtureを作り、結果を検査・出力し、自分で作った一時directoryだけを削除する。以下のnative size/symbol検査はmacOS向けであり、Windowsには対応するtoolと新しい証拠を用いる。compiler/toolchainの変更でサイズは変わり得る。

```sh
cargo build --locked --bin lang --target-dir target/spec-study-build
python3 - <<'PY'
import json
import pathlib
import re
import subprocess
import tempfile

binary = pathlib.Path("target/spec-study-build/debug/lang").resolve()
fixtures = {
    "base": "fn main() -> i64 { return 42; }\n",
    "dead": "fn main() -> i64 { return 42; }\nfn spare() -> i64 { return 43; }\n",
    "invalid": "fn main() -> i64 { return 42; }\nfn spare() -> i64 { return missing(); }\n",
    "export": 'fn helper() -> i64 { return 41; }\npub extern "C" fn api() -> i64 { return helper() + 1; }\n',
}

def run(*args):
    return subprocess.run(list(map(str, args)), capture_output=True, text=True, timeout=30)

with tempfile.TemporaryDirectory(prefix="spec-study-", dir="target") as temporary:
    root = pathlib.Path(temporary).resolve()
    observations = []
    for name, source in fixtures.items():
        (root / (name + ".lang")).write_text(source, encoding="utf-8")
    for name in ("base", "dead", "export"):
        source, output = root / (name + ".lang"), root / name
        if name != "export":
            result = run(binary, "run", source)
            assert result.returncode == 0 and result.stdout.strip() == "42", result
        result = run(binary, "native", "--output", output, source)
        assert result.returncode == 0, result.stderr
        symbols = run("nm", "-g", output / "program.o")
        sizes = run("size", "-m", output / "program.o")
        assert symbols.returncode == sizes.returncode == 0
        match = re.search(r"Section \(__TEXT, __text\):\s+(\d+)", sizes.stdout)
        assert match, sizes.stdout
        defined = [line for line in symbols.stdout.splitlines() if re.search(r"\b[Tt]\b", line)]
        if name == "export":
            assert any("_lang_api_" in line for line in defined), defined
        else:
            assert len(defined) == (1 if name == "base" else 2), defined
        observations.append({"fixture": name, "text_bytes": int(match[1]),
            "file_bytes": {f: (output / f).stat().st_size
                           for f in ("program.o", "program.lbc", "program.native")},
            "defined_symbols": defined})
    for command in ("check", "run", "native"):
        args = [binary, command]
        if command == "native":
            args += ["--output", root / "invalid"]
        result = run(*args, root / "invalid.lang")
        assert result.returncode == 1, result
        assert "error[SRC0002]" in result.stderr and "2:28..2:35" in result.stderr, result.stderr
        observations.append({"command": command, "invalid_unused_body": result.stderr.strip()})
    assert not (root / "invalid" / "program.o").exists()
    print(json.dumps(observations, indent=2))
PY
```

統合検討では説明用のcode graphと独立した認可contextも確認した。callだけの走査ではcleanup/callback/spawn edgeが漏れ、`api`をrootとするlibraryはmainなしでhelperを必要とし、不変codeを共有しても必要grantのない別instanceは認可されなかった。これらは広すぎる設計への反例であり、production optimizer/cacheのtestではない。

## 導入順序と互換性

| 順 | 小さく観測可能なdelivery | 拡大前のgate |
| --- | --- | --- |
| 1 | 適合表と読取り専用Copy保持理由report | 未使用code、不正な未使用source、mainなしexportを区別。不明なedgeを説明しreport処理を制限。 |
| 2 | 閉じたCopy executable一件でnative出力を選択 | VM/JIT/AOTの値、失敗/span、fuel、record、debug対応。exportを保護しlinked binaryを別測定。 |
| 3 | component準備の計測と不変結果の再利用 | cold/hit/missコスト、peak memory、grant/provider/allocator、枯渇、再入、最後のcode ownerのretirement。 |
| 4 | 一つの直接構築変換 | 部分構築、replacement拒否、cleanup失敗、return escrow、service-work境界。物理コピー減少を示す。 |
| 5 | 一つのpacked/frozen snapshot workload | 既存Bytes/moveと比較。copy、allocation、live bytes、access時間、不正offset、失敗atomicityを測る。 |
| 6 | 必要性を示した長寿命cycle workloadのoptional GC | 限定bridge、weak/close/cycle方針、collector予算、VM/native適合、native memory safetyと両host検証。 |

3と4はbottleneck測定後に入れ替えてよい。これは提言でありmilestone pointではない。実装前に現在のboardで一つのdeliveryを選び、新たな前提を再見積りする。5/6をM4必須条件に加えない。

内部解析だけなら構文・ABIを変える必要はない。公開report、root manifest、永続profile/cache、新参照category、native/portable record変更には明示的な互換性判断が要る。component C v1の32 entryとhandle意味を保ち、非互換拡張は新version/namespaceにする。非対応artifactは実行前に拒否し、対応toolでsourceから再build/encodeする。host pointerやtrace descriptor addressをserializeしない。

代替案にも意義がある。変更しなければ強いbaselineを維持でき、library/toolだけで新メモリ意味論なしにreportや既存Bytesによるsnapshotを提供でき、協調回収より単純な制限付きbridgeで足りる場合もある。最も強い競合案である全面tracing、Goose型whole-program特殊化、Graal/Truffle runtimeは、プロジェクト固有の優位性が未測定のまま、大幅な意味論・実装コストを伴う。

実装前に解く設計事項は、最初のartifactのroot/export集合とnative record対応、cacheの完全なbinding前提、elisionを試す場合のallocator観測性、具体的なtraced workloadとcross-cycle方針である。高度なLSP表示、PGO、不変データの事前計算、moving GC、image layerは任意の後続作業であり、最初の実験の前提ではない。

## 出典一覧

すべての出典の確認日は**2026-10-30 JST**。repositoryの日付は指定snapshotまでの当該fileを最後に変更したGit commitであり、全段落をその日に再検証したという意味ではない。文書の公開・更新日が不明な場合にsnapshot日を代用していない。先行ローカル調査は必須参照先にせず、必要な内容を上にまとめた。

| ID | 一次資料・プロジェクト資料 | 公開・更新情報 |
| --- | --- | --- |
| G1 | [Goose仕様][G1] | 文書固有の公開更新日はなし。repository snapshot `cce94b9`、commitは2026-10-27 23:23 UTC（2026-10-28 JST）。v4 draft。 |
| G2 | [Goose実装解説][G2]、[README][G2a] | 文書固有の公開更新日はなし。G1と同じ固定snapshot日。 |
| G3 | [Goose benchmark結果][G3]、[summary][G3a] | 主表の測定は2026-10-16。両fileの最終変更は2026-10-25 08:10:53 UTC。langとの比較は未実施。 |
| S1 | [Native Image Basics][S1] | 公開・更新日表示なし。2026-10-30 JST取得。 |
| S2 | [Native Image Build Report][S2] | 公開・更新日表示なし。2026-10-30 JST取得。 |
| S3 | [Kotlin/Native ARC連携][S3] | ページlast-modified表示2025-04-17。2026-10-30 JST取得。 |
| S4 | [Wasmtime Rooted][S4] | 公開・更新日表示なし。2026-10-30 JST取得。 |
| S5 | [LLVM statepoint][S5] | living document、公開更新日なし。2026-10-30 JSTのメモリドメイン調査で確認。 |
| L0 | [Architecture](architecture.md)、[roadmap](roadmap.md) | `df55f6e`までの最終変更はいずれも2026-10-29 JST。 |
| L1 | [RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) | `df55f6e`までの最終変更2026-10-28 JST。 |
| L2 | [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) | `df55f6e`までの最終変更2026-10-20 JST。 |
| L3 | [RFC 0013](rfcs/0013-service-byte-work-budget.md) | 最終変更2026-10-24 JST。whole-unit admission amendmentは2026-10-19。 |
| L4 | [RFC 0024](rfcs/0024-send-share-and-service-authority.md) | `df55f6e`までの最終変更2026-10-27 JST。 |
| L5 | [RFC 0033](rfcs/0033-component-v1-compatibility.md) | `df55f6e`までの最終変更2026-10-27 JST。 |
| L6 | [RFC 0039](rfcs/0039-package-editor-snapshots.md) | `df55f6e`までの最終変更2026-10-29 JST。 |
| GraalVM study | [repositoryのGraalVM調査](graalvm-study.md) | 最終変更2026-10-30 01:14 JST、`d2cd8ba`。追加の固定Graal資料と実験provenanceを収録。 |

[G1]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/docs/goose_spec.md
[G2]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/docs/implementation.md
[G2a]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/README.md
[G3]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/bench/results.md
[G3a]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/bench/summary.md
[S1]: https://www.graalvm.org/latest/reference-manual/native-image/basics/
[S2]: https://www.graalvm.org/latest/reference-manual/native-image/overview/build-report/
[S3]: https://kotlinlang.org/docs/native-arc-integration.html
[S4]: https://docs.wasmtime.dev/api/wasmtime/struct.Rooted.html
[S5]: https://llvm.org/docs/Statepoints.html
