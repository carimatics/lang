<!-- translation-of: ../milestone-2.md; canonical-language: en -->

# Milestone 2 実装Todo

[English](../milestone-2.md) · [完了条件](roadmap.md#milestone-2ownershipとnative-aot)

更新・確認日：**2026-10-19 JST**。初回監査の基準commit：`0c2c012`。

**81 done · 0 doing · 25 todo · 60 remaining pt**

**次: M2-N03 → 最小のnative Bytes割当／変更／長さ取得fixtureを実行し、共有service接続をText／Vec／Mapへ拡張。**

**直近の完了 — N02**（`a3478bd`、2026-10-19 JST）：通常のloanとplace／借用case projectionがnative guest callを越えて動き、準備済みroot／field置換も共有storage／cleanup規則に従います。借用変更、旧／新owner解放、RHS副作用、独立に再検証した参照先変更をVMと比較しました。Fuel／割当失敗でもframe解放前に登録とcaptureを消去します。Mach-O／COFF metadataと累積budgetの厳密境界も成功しました。全 **85 suite・1352 test** とfmt／Clippy／docが成功しました。**80 → 81 done · 63 → 60 remaining pt**、**task総数106件は不変**です。G14は未完了です。

## 進捗と見積もり変更

詳細Todo導入時の `8605416`（2026-10-18 JST）以降、検証済み成果は **35 → 81件（+46件）**、作業一覧は **87 → 106件（+19件）** になりました。新たに可視化した作業は既存要件の実装であり、各件が同じ工数を表すわけではありません。

| 詳細計画導入時の残り | 追加・再見積もりの差分合計 | 完了により取り除いた見積もり | 現在の残り |
| ---: | ---: | ---: | ---: |
| 134 pt | +49 pt | −123 pt | 60 pt |

**134 + 49 − 123 = 60 pt**

完了した作業と新たに判明した作業を分離しました。完了により見積もり123点分を取り除く一方、前提作業の可視化と再見積もりで49点増えています。これは計画上の単位で、時間や実工数の計測値ではありません。残り60点は暫定値で、native／FFIとplatform evidenceには引き続き実装前の実験が必要です。元の18完了条件は維持します。各成果の更新時にこの収支を検証し、追加分の合計は明示的なscope／見積もり監査がある場合のみ変更します。詳細計画の初期134点と、歴史的な完了条件の重み113点は別の指標です。

N02は作業やtaskを追加せず、最後の見積もり3点を完了しました。**63 − 3 = 60 pt** です。以前の分割でN09の導入と完了が同じ計画更新だったため、その初期見積もり1点は引き続き監査に保持します。Taskも元の要件も削除していません。

直近code baselineの実測（2026-10-19 JST）：workspace Clippy **26.9秒**、全test **339.7秒**、rustdoc **15.0秒** の実時間（shell time）。Cargo報告のtest compileは **23.7秒**、suite時間合計は **78.7秒** です。境界23 testのfocused実行は **2.37秒** です。検証所要時間であり、実稼働時間やruntime benchmarkではありません。文書だけの計画更新では `repository_consistency` を実行します。

現在のsource対応範囲：引数なしi64 mainでBytes／Text、arena allocation／read／replace／teardown、shared／weak allocation／clone／read／replace／upgrade、Vec new／len／push／pop／shared・mutable read、厳密なi64／Text keyのMap new／len／insert／get／removeを実行します。Storable payloadは入れ子collection、inline record／enum／Option／Result、再帰guest call越しに組み合わせられます。Shared publicationは推移的strong edge、arena storeは入れ子brandを検査し、projection／call越しにread custodyを保持します。Ownerはcleanupで保護したreturnとprepared replacementを通り、正常／失敗teardownを反復実行します。Ordinary loan、allocator authority、arena ownerは格納不可で、cross-region／shared-heap edgeとarena escapeを拒否します。Borrowed matchとpartial move禁止は維持します。V16／V15／V10は既存managed preview／推論／REPL profileに対応し、V13は明記したsource coverage内のprovisional-read loan終了を検証します。Indirectionを含むpreviewはV17です。V11はin-memory CFG argumentに対応しますが、現ownership source loweringは生成しません。Native N03／N04／N05とownership artifactはevidenceが必要で、source統合だけでexit gateを閉じません。

実装前監査（2026-10-19 JST）：旧native valueの3点をN09 record構築1点、N10 scalar／sum／control 2点、N02 loan・準備済み置換・VM総合比較3点へ分割し、残作業3点・task 2件を追加しました。3件とも検証済み成果になりました。Callee内の通常loanをcallerのreferenceへescapeさせるsourceは引き続き禁止し、capture後の参照先変更は独立に再検証した同一frameのMIRで確認します。N03の一般core serviceは未対応です。最小Bytes fixtureからText／Vec／Mapへ進め、独立した前提が実証されたら再見積もりします。V17、native／FFI／platform／conformance／性能／debug全成果、Z01の日英guideを維持し、完了条件は閉じません。

従来の完了条件は **18件中5件完了** です。完了成果にはcompiler／runtimeの前提作業と初期source実行fixtureを含み、core-libraryやbackend機能全体の完了を意味しません。

この文書を実装Todoの正本とします。Roadmapには従来の受入条件とその履歴上の重みを残します。件数は独立して検証した成果の数であり、各件の作業量が等しいという意味ではありません。残りpointは積み上げによる暫定見積もりで、native／C／platform統合の確度は低く、時間や完成時期の予測ではありません。完了済み作業に後付けのeffort pointは与えません。完了行の *残り* pointは0です。

## 従来の数値が止まった理由

`367218e`（2026-10-18 00:14:21 JST）から `0c2c012`（03:24:22）まで、完了条件の重みは34 / 113のままでしたが、実装9commitとroadmap 9commitが追加されました。記録されたpassing-test baselineは840件から930件になりました。差分は48file、10,407行追加、78行削除です。Commit日時が示すのは3時間10分の区間であり、実測した稼働時間ではありません。Test数や行数は変更の裏付けであり、生産性や完成機能の指標ではありません。

原因は次のとおりです。

1. 大きなend-to-endの受入条件を日々のTodoにも使っていました。たとえばdrop項目には、cleanup engine、生成range、構築順序、入れ子report、failure CFG、allocator、layout、本番実行が隠れていました。前提作業が完了しても数値が変わりませんでした。
2. RFC 0007／0008で要件が確定した後も、残作業を積み上げずに113 ptを妥当と繰り返し判断しました。Ownership VM実行、fallible allocationの統合、ownership artifactのencodingなどは主に本文中にしか現れていませんでした。**113 ptを作業量予測として使うことを取りやめます。** 残り79の受入条件の重みは、実装残作業の妥当な見積もりではありません。
3. Compiler／runtimeの基盤を横方向に進め、ソース実行が未対応のままでした。必要な前提作業もありますが、利用可能な経路を試す前に基盤を追加し続けることへの明示的な制限がありませんでした。設計検討の長期化も最初の具体的な変更を遅らせました。
4. 各実装で長い履歴reportと別のroadmap commitを追加し、活動記録は増えても現在のqueueが見えませんでした。Code sliceごとにfull baselineを再実行していましたが、所要時間は計測していないため、経過時間に対するtestの比率は断定できません。

初回の残り134 ptの作業一覧は欠けていた詳細見積もりを置き換えたもので、完了済み34 ptの付け直しや新しい完成率の作成ではありません。現在の残りpointは成果の完了で減り、再見積もりでは変更を明示します。新旧の総計は粒度が異なり、過去のcost超過率の計算には使えません。新たな作業が判明したら、task数と残りpointの増減を含めて明示的に再見積もりします。

## 作業ルール

最初の実行fixture後の監査（2026-10-18 JST）：RFC 0007の受理済みmove／match規則では、非Copyのcase bindingは読み取りloanです。生成したBytesをownedなmutable localへbindingすると、`SRC0014`（expected Bytes, found &Bytes）で拒否されることを確認しました。Allocating Option／Resultから変更／消費core命令へ進む経路を妨げるため、V04前にS01〜S03（8 pt）を追加し、明示的で互換性を持つ取り出し設計を判断してからcompiler／runtime contractを実装します。受理済みborrowed matchingを暗黙に変更しません。また新VMはCFG block argumentを明示的に拒否するため、一般guest-call conformance前にV11（2 pt）を追加します。V03で3 ptを減らし、新規4行で10 ptを加えるため、残作業は129から **136 pt**、**53 todo** へ変わります。S02／S03はS01で設計を選ぶまで暫定とし、大きい形式を選んだ場合はcode前に分割／再見積もりします。実行できたsource fixtureは完了成果として維持します。

S01判断時の監査（2026-10-18 JST）：RFC 0009は新しい消費match構文を作らず、既存generic-call構文とwhole-value argument規則を使う有限core call 3個を選びました。2 ptの設計行を完了し、残作業は **136 → 134 pt**、todoは **53 → 52件** です。S02はsignature／source／MIR検査の3 pt、S03は対応Bytes／Copy移譲とpanic cleanupの3 ptを維持します。新parser構文、任意enum取り出し、collection、native実行はこの2行の対象外です。Transfer実験まで見積もりは暫定とし、新前提が判明したら実装前に明示／分割します。S02に続くS03で次のsource owner取り出しfixtureを実行してから基盤を拡張します。

- 実装前に1行を選んで `doing` にし、同時進行は最大1件とします。1行は観測可能な1成果、残り1〜3 ptです。それより大きい、または前提が不明なら着手前に分割します。
- 計画開始から10分経ってもpatchまたは実行可能な実験がなければ、設計を広げる前にtaskを狭めて最小限の有用な実験を実行します。抽象化やhardeningの追加は、選択した成果または受理済み要件に必要な場合に限ります。取得できる検証所要時間を記録し、commit間隔を稼働時間とはみなしません。
- 直近は2件の前提作業（有限valueと上限付きframe）の後、Bytesソース実行fixtureを必須とします。別の前提が判明したら記録し、基盤作業を追加する前に順序を見直します。Fixtureを暗黙に先送りしません。
- Doneには行のoutcomeを満たすcommit（実装、または明示的に計画した設計判断）と対象を絞った検証evidenceが必要です。自分自身のcommitを参照できない場合は次の計画更新でcommitを記録し、evidenceが揃うまで仕掛かり状態を正直に保ちます。完了条件を満たすまで待たず、各成果の後にこの表を更新します。
- 対象を絞ったcheckから始め、まとまったcode成果ごとに必須full baselineを1回実行します。文書だけのfollow-upはrepository consistencyを確認し、追加code、失敗、未解決の懸念がある場合だけ広いcheckを再実行します。実装／roadmapのcommit順序は維持し、繰り返しの長文追記を行更新に置き換えます。監査記録の追記はscope、見積もり、依存、完了判断が変わった場合に限ります。
- ソース実行fixtureが増えない前提作業が2件続いたら、基盤の拡張を止め、次のvertical sliceを再検討します。漠然と「見積もりは妥当」とせず、具体的な阻害要因と残taskを報告します。
- 18件の完了条件を維持します。必要な実機tier-1実行を含め、全evidenceが揃った場合だけ完了にします。Localのobjectや設定済みCI jobはWindows native実行evidenceではありません。未完了のP1 debug作業はexit前に移動先と再見積もりを明示します。

V07着手前の監査（2026-10-18 JST）：[owned-call fixture](../../examples/scripts/owned_calls.lang) はsource解析が通りますが、実行はeffect前に `Call` を拒否します。現在の `FramePlan::reserve_after` は呼び出しごとに新しいhost wrapper vectorを確保し、最外guest entry前にhost poolを用意する既存実装contractを満たしません。V07前に再利用するwrapperのadmission／cancel用 **M2-V12（2 pt）** を追加します。**残り118 → 120 pt**、新規1行で **45 done／1 doing／46 todo** です。V07のowned／loan source call、再帰上限、return escrowは暫定3 ptで維持し、新たな前提はcode前に明示します。V11とV12で前提作業が2件続くため、その後は基盤を増やす前にV07のsource fixtureを実行する成果を必須にします。

V09着手前の監査（2026-10-18 JST）：provisional core readには未完了のVec／Map／shared／arena source adapterが必要ですが、C03／C06は既にV09へ依存しており、従来の複合行に依存cycleが隠れていました。Prepared replacementをV09へ残し、不足しているallocationなしのroot／field storage交換と、old cleanup前のpublication adapterを含めて **2 → 3 pt** へ再見積もりします。Provisional-readの成功／未使用／失敗armとVM／REPLのloan conformanceは、C03／C06／R04／R06／V10後の **V13（2 pt）** へ移します。両成果を維持し、**残り112 → 115 pt**、1行追加（計93行）、**48 done／1 doing／44 todo** です。V09では基盤を増やす前に実際のsource replacement fixtureを実行する成果を必須にし、元のexit gateは維持します。

V10着手前の監査（2026-10-18 JST）：pipeしたREPL実験で妥当なowning関数の定義が拒否され、その呼び出しがunknownになりました。後続のCopy式は `3` を表示します。`src/frontend.rs` はwrapper推論をCopy checkerへ固定し、`src/repl.rs` はitem commit前にownershipを拒否し、owned VM entryは引数なしi64 main専用です。RFC 0006にはCopy aggregate表示がありますが、RFC 0007／0008にはBytes／Textのpreview構文や表示後のresult retirementがありません。その判断を **V14（2 pt）**、上限付きmanaged-result preview／実行を **V16（3 pt）**、独立検証するownership式推論を **V15（3 pt）** として明示します。V10のsession／CLI／authority／error統合は **2 → 3 pt** へ再見積もりします。Bytes／TextをM2の全result familyと暗黙に扱わず、後続indirect-service resultのpreview設計／実装も実adapter後の **V17（3 pt）** へ明示します。**残り112 → 124 pt**、4行追加（計97行）、**49 done／1 doing／47 todo** です。順序はV14 → V16のsource実行 → V15 → V10とし、判断後はcompiler基盤を増やす前にowned-result fixtureを実行します。従来の成果とexit gateを維持し、この監査で新実装の完成は主張しません。

R01着手前の監査（2026-10-18 JST）：検証済みarena-tokenの `CopyPlace` だけを別のlive arenaの値へ差し替えた [source由来probe](../../tests/mir_regions.rs) が、type／move／loan-activity／loan／failure-CFG検証を通りました。同じsourceはHIRで拒否されますが、独立MIR region stageはありません。Loan topologyは意図的にnon-loan record fieldをopaqueに扱い、region value graphとcall-effect summaryを提供しません。3 ptのR01に4成果が隠れていたため、上限shape／path／owner／call境界の **R08（3 pt）**、関数内brand／loan／store／CFG転送の **R09（3 pt）**、再帰caller-projection／mutation／transient-store summaryの **R10（3 pt）** を明示します。3件の後に必須pipeline admission、構造化診断、偽造入力／resource／fuzz conformanceを行う **R01（2 pt）** を残し、元の拒否成果は維持します。**残り113 → 121 pt**、3行追加（計100行）、**53 done／1 doing／46 todo** です。R08は準備であり、region proofやarena実行／artifact有効化ではありません。R08／R09後は基盤拡張前にsource arena経路を再確認します。Probeではnested arenaでshared allocator parameterを再利用するとloweringでmoveされる不足も見つかったため、R06／R07のsource conformanceに修正を残します。新exit gateは完了扱いにしません。

R02 storage／次sliceの監査（2026-10-18 JST）：[型付きstorage実験](../../crates/runtime/tests/shared_storage.rs) で3つのOOM位置、quota境界、detach後のcleanupが成功しました。当初のhost Arc／opaque-edge engineでは型付きsource valueやguest-backed control／read metadataを直接扱えません。R03でtoken identity、上限付きgraph／read registry、cycle proof、反復child dispatchを新storageへ明示統合する必要があるため、実装前に **2 → 3 pt** へ再見積もりします。R02完了で **残り102 → 99 pt**、調整後は **100 pt**、**61 done／0 doing／39 todo** で、新規行はありません。R03の前提R02／D34は完了です。基盤2件のR02／R03後はR04のsource shared／weak fixtureを必須にし、新たな前提を追加する前にsource経路を見直します。G05／G06にはnative／source全体のevidenceが引き続き必要で、ownership artifact gateも未完了です。

R03 interaction監査（2026-10-18 JST）：実行できた型付きgraph probeはshared／weak fieldとBytes子孫を扱いますが、activeなVec／Map tokenは明示拒否します。Containerの再帰cleanup callbackから同じborrow中のheapへ安全に再enterすることもできず、入れ子edge／brand検証全体と共通の反復service drainには、実際のcollection adapter後に固有の統合が必要です。直接shared fieldを受理済みpayload全体とみなさず、C03／C06／R04／R06後の **C07（3 pt）** を明示します。V13／V17とN04もC07へ依存させます。既存のgeneric value scopeを維持し、**残り100 → 103 pt**、1行追加（計101行）、**61 done／1 doing／39 todo** です。C07は最初の実container／graph実験まで暫定です。直近はR03 → R04 source実行の順を維持し、元のgateは完了扱いにしません。

N01実装前監査（2026-10-18 JST）：既存native APIはCopyの `VerifiedUnit` cellのみを受け取り、ownershipはruntime `Layouts` と別管理の初期化／retirement metadataを使います。RFC 0008は2 pointer／statusのconventionを維持しますが、ownership context、返り値escrow、frame admissionを定めていません。明示的な決定と機械コード試作の **N08（2 pt）** を追加し、**残り66 → 68 pt**、**計103 → 104行**とします。元のN01〜N07の成果範囲はすべて維持します。今回と次の基盤成果の後は、ownership source fixtureを届けてからnative基盤を追加します。

## 完了済み実装

最初の35行はcommitからM2の実装成果を復元したものです。元のRFC 0007／0008判断は完了条件側に残します。以後の成果は、明示的に計画したfollow-up設計行を含め、計画時のIDを維持してdoneにし、各行に記したoutcomeだけを満たします。検証の詳細と限界は [architecture](architecture.md) と [roadmapの実装履歴](roadmap.md#milestone-2ownershipとnative-aot) を参照してください。

`Gate` はroadmapのchecklist順にG01〜G18です。1行が複数条件に貢献しても、作業を二重計上しません。

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-D01 | done | 0 | — | G13 | Copy profile全体のnative loweringと差分fixture。 | `926fb3b` |
| M2-D02 | done | 0 | — | G04 | Shared／weakのlifecycle、cycle publication、rollback、race。 | `ec16f8a` |
| M2-D03 | done | 0 | — | G03 | Ownership構文とformatter往復。 | `eb3b16f` |
| M2-D04 | done | 0 | — | G03 | HIRのownership型特性と上限。 | `81fab15` |
| M2-D05 | done | 0 | — | G03 | HIRのplaceとlexical arena scopeのbinding。 | `f6e0edb` |
| M2-D06 | done | 0 | — | G03 | 有限core-call signatureとprovenance template。 | `9342175` |
| M2-D07 | done | 0 | — | G03 | Core sum case、Unit value、pattern。 | `0115a46` |
| M2-D08 | done | 0 | — | G03 | 文脈を用いたbody／core型検査と網羅性。 | `8eda046` |
| M2-D09 | done | 0 | — | G05 | Lexical arena-owner authorityの型検査。 | `7213ae3` |
| M2-D10 | done | 0 | — | G03 | HIRの初期化とaffine move flow。 | `9263966` |
| M2-D11 | done | 0 | — | G03 | HIRのlexical loanとmutable access。 | `7e3d43d` |
| M2-D12 | done | 0 | — | G03 | 入れ子reference cellと推移的provenance。 | `83b5e36` |
| M2-D13 | done | 0 | — | G05 | ソースのarena brandとstorage flow。 | `3e6563a` |
| M2-D14 | done | 0 | — | G05 | 再帰guest callのregion summary。 | `b39b96a` |
| M2-D15 | done | 0 | — | G05 | Arenaのretain付きread、generation、逆順teardown。 | `02f349d` |
| M2-D16 | done | 0 | — | G03 | MIR ownership型graphの独立検証。 | `c6847a6` |
| M2-D17 | done | 0 | — | G03 | MIRのcanonical typeとsource declaration。 | `97672e4` |
| M2-D18 | done | 0 | — | G03 | MIR place／projectionの型検証。 | `51759b3` |
| M2-D19 | done | 0 | — | G03 | Ownership命令の型検証とCFG検証。 | `a3c5dce` |
| M2-D20 | done | 0 | — | G03 | MIRの初期化とmoveの独立解析。 | `9cf81bd` |
| M2-D21 | done | 0 | — | G03 | RHS effectをまたぐ代入先の事前評価。 | `9a57680` |
| M2-D22 | done | 0 | — | G03 | MIR loanの明示的な有効化／終了解析。 | `fd5f050` |
| M2-D23 | done | 0 | — | G03 | MIR reference provenanceの独立検査。 | `4c6c13e` |
| M2-D24 | done | 0 | — | G03 | MIR core-call contractとallocator要件。 | `4619937` |
| M2-D25 | done | 0 | — | G03 | Ownershipソースbodyの自動lowering。 | `39bb20e` |
| M2-D26 | done | 0 | — | G03 | Matchとprovisional-loan armのlowering。 | `ad40b7a` |
| M2-D27 | done | 0 | — | G03 | Driver／queryのownership解析とlang check。 | `d25706c` |
| M2-D28 | done | 0 | — | G06 | 順序付きcleanup engineとreturn escrow。 | `c437dc1` |
| M2-D29 | done | 0 | — | G06 | ソース生成cleanup rangeとruntime順序の接続。 | `6216b43` |
| M2-D30 | done | 0 | — | G06 | 実際のrecord初期化順序をloweringで保持。 | `a984e5e` |
| M2-D31 | done | 0 | — | G06 | 上限付きの入れ子field failure report。 | `9678072` |
| M2-D32 | done | 0 | — | G06 | Ownership failure／return CFGの生成。 | `3c3131f` |
| M2-D33 | done | 0 | — | G06 | Fallible instance allocatorと会計。 | `d7f85de` |
| M2-D34 | done | 0 | — | G06 | Managed layoutと事前確保したretirement storage。 | `f68a4da` |
| M2-D35 | done | 0 | — | G07 | Allocator付きBytes／Textと検証済みtoken cleanup。 | `82ed11b` |

## ソース実行を先行

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-V01 | done | 0 | M2-D34 | G08 | Managed Unit／Option／Result／error codec：入れ子往復、tag検証、厳密な上限。 | `71ece57` |
| M2-V02 | done | 0 | M2-V01 | G06 | 上限付きowned frameの受け入れ時、effect前にvalue／retirement storageを確保し、失敗rollbackを検証。 | `86c23ce` |
| M2-V03 | done | 0 | M2-V02,M2-D35 | G06,G07 | lang runでBytes new／lenのソースと通常dropを実行し、到達可能な未対応命令はeffect前に拒否。 | `ece8690` |
| M2-S01 | done | 0 | M2-V03 | G07,G08,G09,G10 | RFC 0009でSome／Ok／Err全体取り出しを受理。Borrowed match維持、明示panic／cleanupと互換性規則。設計のみ。 | `c4cb8b9` |
| M2-S02 | done | 0 | M2-S01 | G08 | RFC 0009の3操作を型検査し独立検証済みMIRへlower。Copy／move、loan、region、入れ子sumのcompile pass／fail。Runtimeはgateを維持。 | `b562c0f` |
| M2-S03 | done | 0 | M2-S02 | G06,G08 | 有限Bytes／Copy sumと3種のwhole-payload取り出しをtransfer確保なしで実行。Source fixture、panic／fuel／OOM cleanup、managedな入れ子順序／span移譲を検証。 | `e584902` |
| M2-V04 | done | 0 | M2-V03,M2-S03 | G07,G08 | Source Bytes get／set／push、UTF-8変換、Text byte長を実行。Bounds／OOM、変換確保数、fuel／interruption cleanup、Copy cell loop再利用を検証。 | `8b24c94` |
| M2-V05 | done | 0 | M2-V04 | G07 | Text concat／clone／to-bytes／equalityとBytes cloneを実行。独立owner、allocator identity、typed OOM／quota error、fuel／loop cleanupを検証。 | `752a369` |
| M2-V06 | done | 0 | M2-V03,M2-S03 | G06,G08 | 入れ子Bytes／Text／Copy record／enum／sum ownerとreference projectionを実行。Pack用確保なし、記述逆順の1回限りdrop、OOM／panic／fuel、loop cleanupを検証。 | `92d225a` |
| M2-V12 | done | 0 | M2-V11 | G06 | Host拡張なしでmain／callee regionをadmit／release。厳密budget、size 0課金、OOM rollback／再試行、cumulative request、callback前拒否を検証。 | `05ebf0b` |
| M2-V07 | done | 0 | M2-V06,M2-V11,M2-V12 | G06 | Owned guest call／resultを再帰上限とreturn escrow付きで実行。 | `15436f0` |
| M2-V08 | done | 0 | M2-V07,M2-D32 | G06 | Single／callの生成failure exitで、body／fuel／OOM／interruptionとcleanup error／panic注入時の入れ子drop順序／span、保護return、順序付き上限report、poison／再利用、全解放を検証。 | `a7af470` |
| M2-V09 | done | 0 | M2-V04,M2-V06 | G07,G08 | Root／投影先置換でRHS effect越しに確定済みtargetを保持し、移譲用allocationなしでold cleanup前に公開。Caller所有権、field順序／span、失敗cleanup、再試行を維持。 | `0311bd0` |
| M2-V13 | done | 0 | M2-C03,M2-C06,M2-C07,M2-R04,M2-R06,M2-V10 | G08 | Provisional core readの成功／未使用／失敗armとloan終了をsource VM／REPL実行で検証。 | `9d9d2ac` |
| M2-V14 | done | 0 | M2-V08,M2-V09 | G08 | 委任でRFC 0010を受理。Owned REPL preview形式、result lifetime、明示allocator authority、上限出力、failure優先順位を決定しsource contractを検証。実装は未完了。 | `b09cc46` |
| M2-V16 | done | 0 | M2-V14,M2-V08,M2-V09 | G08 | Namedな引数なしowned resultを上限付きmanaged previewとresult全retirementで実行し、source fixtureと失敗根拠を示す。 | `040c082` |
| M2-V15 | done | 0 | M2-V14 | G08 | 生成ownership式wrapperの具体的resultを通常のHIR／MIR move／loan／region検証越しに推論し、user関数の推論は追加しない。 | `3ecef3b` |
| M2-V10 | done | 0 | M2-V08,M2-V09,M2-V15,M2-V16 | G08 | REPL ownership item／式、明示allocator authority、元sourceへのfailure mapping、cleanupで分離したcanonical resultをscript実行意味論へ統合。 | `834abe8` |
| M2-V17 | todo | 3 | M2-V14,M2-V16,M2-V10,M2-C03,M2-C06,M2-C07,M2-R04,M2-R06 | G08 | 明示follow-upを判断してindirect-service resultの上限付きpreviewを実装し、Map順序とshared／weak／arena access制約を維持。VM／REPL根拠を示す。 | — |
| M2-V11 | done | 0 | M2-V06 | G08,G14,G16 | 上限付きadmission済みsnapshotでCopy／affine CFG argumentを同時転送。Cycle／fan-out／enum／owner oracle、loan再束縛、cell／quota／OOM、fuel cleanupを検証。 | `1dcf260` |

## Shared、arena、collection

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-R08 | done | 0 | M2-D24 | G05 | 通算budget下で有限region type／slot／projection topology、arena-owner identity、region関連call境界をadmit。再帰shape edgeを展開せず保持し、region proofは行わない。 | `24fb835` |
| M2-R09 | done | 0 | M2-R08 | G05 | Local MIR操作、core value、CFG join／loopでarena brand、reference／capture path、storage lifetimeを独立転送し、直接のcross-region／escapeを拒否。 | `ee5b12e` |
| M2-R10 | done | 0 | M2-R09 | G05 | Caller projection、正常return時mutation、全transient storeの再帰guest-call summaryをHIR factsなしで導出し、推移的／循環call escapeを拒否。 | `28f2080` |
| M2-R01 | done | 0 | M2-D24,M2-R08,M2-R09,M2-R10 | G05 | Failure／実行admission前に独立MIR arena-brand／call-effect proofを必須化し、構造化span診断と偽造入力／resource／fuzz conformanceを維持。 | `dd0b006` |
| M2-R02 | done | 0 | M2-D02,M2-D33 | G06 | 型付きshared control／payload storageをguest fallible allocationに接続し、payload／retirement全領域のpreflight／rollbackを会計。Graph／source統合はR03／R04へ続く。 | `15834b9` |
| M2-R03 | done | 0 | M2-R02,M2-D34 | G06 | Production shared／weak token、上限付きgraph／read registry、cycle検証を型付きstorageへ統合し、test registryなしに子孫を反復retire。 | `20ade1f` |
| M2-R04 | done | 0 | M2-R03,M2-V08 | G06,G14 | Shared／weakソース命令、upgrade、cycle拒否、rollbackを実行。 | `830dc26` |
| M2-R05 | done | 0 | M2-D15,M2-D33 | G05,G06 | Arena payload allocationと子孫retirementにguest budgetと失敗rollbackを適用。 | `cb7a72b` |
| M2-R06 | done | 0 | M2-R01,M2-R05,M2-V08 | G05 | Arena allocation／read／replace／region再enter（reset）／teardownをソースから実行し、stale referenceと逆順dropを検証。 | `e0f6bb4` |
| M2-R07 | done | 0 | M2-R06,M2-V07 | G05 | Guest callを通じ、arena escape／call-brandのcompile-failとruntime結果を照合。 | `cecd6e8` |
| M2-C01 | done | 0 | M2-D33,M2-D34 | G09 | Vec storage／new／len／push growthをallocator-awareかつfailure-atomicに実装。 | `78a422a` |
| M2-C02 | done | 0 | M2-C01 | G09 | Vec read／mutable-read／take／replacementで移動／置換elementを正確に1回retire。 | `1271ad8` |
| M2-C03 | done | 0 | M2-C02,M2-V09 | G09 | Vecソース命令をmove／loan／growth／failure fixtureと副作用前のservice別payload preflightで実行。 | `3dca725` |
| M2-C04 | done | 0 | M2-D35,M2-D34 | G10 | Map storage／登録付きread lookupで受理済みの厳密key equalityとallocation budgetを実装。 | `348dc42` |
| M2-C05 | done | 0 | M2-C04 | G10 | Map insert／replacement／removalをfailure-atomicにし、同値の既存keyを保持して旧key／valueを1回だけretire。 | `677bb66` |
| M2-C06 | done | 0 | M2-C05,M2-V09 | G10 | Mapソース命令を独立したequality／replacement／failure oracleで検証。 | `fb13cfd` |
| M2-C07 | done | 0 | M2-C03,M2-C06,M2-R04,M2-R06,M2-C08,M2-C09 | G05,G06,G09,G10 | 上限付きcross-service edge／brand検証と反復cleanupで入れ子Vec／Map／shared／arena値を統合し、source cycle／escape、逆順drop、失敗caseを検証。 | `f89b323` |
| M2-C08 | done | 0 | M2-C03,M2-C06 | G05,G06,G09,G10 | 元initializer spanを保持する上限付きread-only入れ子Vec／Map走査を実装し、改ざん／work・depth制限／無effectを検証。 | `cb698d9` |
| M2-C09 | done | 0 | M2-C08,M2-R03,M2-R05 | G05,G06,G09,G10 | 再入・host再帰・guest中のhost pool拡張なしにサービス間の反復cleanupを実装し、逆順／失敗を検証。 | `559b176` |

## Ownership artifactとC境界

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-A01 | done | 0 | M2-V08 | G14,G16 | Ownership artifactのversion／encodingを互換性と検証上限付きでRFC受理記録にする。 | `6c9f12b` |
| M2-A02 | done | 0 | M2-A01 | G14,G16 | Ownership型／命令のencode／decodeに不正入力budgetとfuzz targetを追加。 | `1311acd` |
| M2-A03 | done | 0 | M2-A02 | G16 | Owned source artifactをload、独立検証、実行し、往復時のbehavior一致を確認。 | `2dfea13` |
| M2-F01 | todo | 2 | M2-D27 | G11 | 受理済みunsafe／source境界規則をsource診断とcompile-fail fixtureで強制。 | — |
| M2-F02 | todo | 3 | M2-F01,M2-V08 | G11 | C境界のvalidity／capability／poison preflightとunwind containmentを失敗fixtureで検証。 | — |
| M2-F03 | todo | 3 | M2-F01 | G12 | 厳密なC descriptor／digest／headerを生成し、受理済みvectorでlayoutを検証。 | — |
| M2-F04 | todo | 3 | M2-F02,M2-F03 | G12 | C importでscalar／aggregate引数、allocation context、failure statusを扱う。 | — |
| M2-F05 | todo | 3 | M2-F04 | G12 | C exportで規定status／resultを返し、入れ子cleanup failureをcontain。 | — |
| M2-F06 | todo | 2 | M2-F05 | G11,G12 | Headerを独立compileし、native macOSで境界validity fixtureを実行。 | — |
| M2-F07 | todo | 2 | M2-F06 | G11,G12 | Native Windowsで対応するC境界fixtureを実行し、成功CI evidenceを記録。 | — |

## Native、packaging、最終evidence

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-N08 | done | 0 | M2-V07,M2-A02 | G14 | private native ownership call／context／公開契約を決定し、機械コードのABI試作を実行。 | `dc3df29` |
| M2-N01 | done | 0 | M2-V07,M2-A02,M2-N08 | G14 | Owned call／return conventionとmanaged layoutをemitし、source Bytes引数／返り値fixtureを含める。 | `0ff2618` |
| M2-N09 | done | 0 | M2-N01 | G14 | Nativeのowned record構築、入れ子field順、record全体のcallでcleanupを保持し、共有／VM fixtureと一致。 | `f199b19` |
| M2-N10 | done | 0 | M2-N09 | G14 | Nativeのscalar／controlとsum構築・検査・payload全体抽出をVM fixtureに一致させる。 | `5d335a9` |
| M2-N02 | done | 0 | M2-N10 | G14 | Nativeのplace／loan projection・準備済み置換と、owner move／record／sum／loanのVM総合conformance。 | `a3478bd` |
| M2-N03 | todo | 3 | M2-N02,M2-C03,M2-C06 | G07,G08,G09,G10,G14 | Native core dispatchで同じBytes／Text／Vec／Map fixtureを実行。 | — |
| M2-N04 | todo | 3 | M2-N02,M2-C07,M2-R04,M2-R07 | G05,G14 | Native shared／arena dispatchでlifecycleとbrand検査を保持。 | — |
| M2-N05 | todo | 3 | M2-N02,M2-V08 | G06,G14 | Nativeの生成failure／return exitをVMのcleanup順序とfuel behaviorに一致させる。 | — |
| M2-N06 | todo | 2 | M2-N03,M2-N04,M2-N05 | G14 | Native macOSでowned programのdirect／response-file object link／runを検証。 | — |
| M2-N07 | todo | 2 | M2-N06 | G14 | Native Windowsで対応owned programを実行し、成功CI evidenceを記録。 | — |
| M2-P01 | todo | 2 | M2-N06 | G15 | 両tier-1でlinker discovery／出力path失敗に対処可能な診断を返す。 | — |
| M2-P02 | todo | 3 | M2-P01,M2-N07,M2-F05 | G15 | Startup／runtime／context依存をpackage化し、clean installでlink／run成功。 | — |
| M2-Q01 | todo | 3 | M2-A03,M2-N07,M2-F07,M2-V10 | G16 | 複合機能の代表ソースfixtureでVM／nativeの成功／失敗conformanceを検証。 | — |
| M2-Q02 | todo | 2 | M2-Q01,M2-P02 | G16 | 両tier-1 CIで代表conformanceとresource-limit scenarioの成功を記録。 | — |
| M2-B01 | todo | 2 | M2-Q01 | G17 | 同等Rust／C workloadとharnessで比較前に意味的一致を確認。 | — |
| M2-B02 | todo | 2 | M2-B01,M2-P02 | G17 | CleanなmacOSでruntime／startup／memory／sizeとばらつきを記録。 | — |
| M2-B03 | todo | 2 | M2-B01,M2-P02 | G17 | 同等のclean Windows計測とばらつきを記録。 | — |
| M2-B04 | todo | 1 | M2-B02,M2-B03 | G17 | 結果の解釈、再現command、残るperformance上の限界を公開。 | — |
| M2-G01 | todo | 3 | M2-N06 | G18 | Native owned-valueのsource span／locationをemitし、macOS debug fixtureで検証。 | — |
| M2-G02 | todo | 3 | M2-N07 | G18 | 対応するWindows native debug metadataをemitし、source locationを検証。 | — |
| M2-G03 | todo | 2 | M2-G01,M2-G02 | G18 | 両tier-1実機でdebuggerのstep／backtrace evidenceを記録。 | — |
| M2-Z01 | todo | 1 | M2-Q02,M2-B04,M2-G03 | G05,G06,G07,G08,G09,G10,G11,G12,G14,G15,G16,G17,G18 | 全exit evidenceを監査し、英日M2 capability guideを公開。 | — |

## 情報源と確認日

- [Native place／loan lowering](../../crates/codegen-clif/src/owned.rs)、[source fixture](../../examples/analysis/native_owned_loans.lang)、[place／VM test](../../tests/support/native_places.rs)（`a3478bd`）：2026-10-19 JST更新・検証。N02の既存見積もり3点を完了しました。Core service、exit conformance全体、単独object実行、platform evidenceは未完了です。
- [進捗validator](../../tests/repository_consistency.rs)（`00cff53`）：更新・検証日2026-10-18 JST。見積もり収支と未完了行の一致、成果／task数の差分、roadmap集計、英日一致を検証します。全workspace 1326 testとfmt／Clippy／rustdocが成功しました。この追跡方法の保守をM2実装行の完了には数えません。
- 詳細Todoの履歴 `8605416..926ccfe`：2026-10-18 JST付の48 revisionを2026-10-18 JSTに再監査。行の状態変化だけでは完了107点／追加39点になります。V14とR08は最初のcommitでdoneでしたが、V10／R01の実装前監査に2／3点の見積もりが残っているため、両側に5点を加えて完了112点／追加44点としました。新たな成果41件をすべて含み、削除された行はありません。導入と完了が同じ更新でも、その見積もりを保持します。
- Local Git履歴 `926fb3b`〜`0c2c012` と差分 `367218e..0c2c012`：commit日は2026-10-17／18 JST、確認日は2026-10-18 JST。
- [Architecture](architecture.md)、[roadmap](roadmap.md)、そこから参照する実装／test：更新・確認日2026-10-19 JST。初回監査の930 testは過去のbaselineであり、現在の検証済みbaselineは上記に記載しています。
- 受理済み [RFC 0007](rfcs/0007-source-ownership-and-core-values.md)：最終更新2026-10-18、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)：最終更新2026-10-17、両方を今回のtask一覧のため2026-10-18 JSTに確認しました。Web調査や外部の生産性benchmarkは使用していません。
- [RFC 0012](rfcs/0012-native-ownership-continuations.md) と [native ABI実験](../../crates/codegen-clif/src/owned_abi_probe.rs)（`dc3df29`）：更新・検証2026-10-18 JST。N08の明示的見積もり2点は、そのcheckpointで追加・完了しました。
