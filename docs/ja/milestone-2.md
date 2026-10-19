<!-- translation-of: ../milestone-2.md; canonical-language: en -->

# Milestone 2 実装Todo

[English](../milestone-2.md) · [完了条件](roadmap.md#milestone-2ownershipとnative-aot)

更新・確認日：**2026-10-19 JST**。初回監査の基準commit：`0c2c012`。

**98 done · 1 doing · 19 todo · 45 remaining pt**

**次: M2-F10 → C objectのaddressと正規field／tag／reserved／unused unionをpaddingを読まずに検証します（進行中、F02から配分した1点）。**

F02／F10の範囲配分（2026-10-19 JST）：Cのpaddingは未初期化の場合があるため、native adapterは入力C object全体をRustのu8 sliceへ変換できません。まずF10として、addressだけでnull／alignment／extent／overlapを事前検査し、通常paddingを要求しない上限付きread interfaceでfield／tag／reserved／unused unionを正規検証します。F03のC layoutと共有service-work計上を使い、raw addressの実際の生存・有効性はRFC 0008どおりtrusted native caller／adapterの義務とします。**F02初期3点から1点を配分**し、残る **2点** にcontext／binding／config全体の事前検査、owner／allocator provenance、poison／再入、callback containmentを維持します。F02はF10に依存し、F04／F05で両経路を実adapterに強制します。**117 → 118行、残り45点・追加72点・完了分161点は不変**です。F10をdoingとし、元の要件やplatform要件は削除しません。

F03完了／残る範囲の監査（2026-10-19 JST）：dee7f34でprofile 1のC layout、正確なfunction／type／artifact descriptorとSHA-256 fingerprint、callback／export／support宣言とlayout assertion付きC11 headerを完成しました。元のscalar vectorと独立aggregate／type／artifact vectorが両target・両方向で一致し、実C compilerでscalar／aggregate／Unit署名、headerの再読込み／共有型、両Clang targetの配置を検証しました。Resource／alias／depth fixtureで、artifact合計の早期上限とsource／MIR／展開後Cの一致した暗黙builtin payload境界も確認しました。最終検証：1,445 tests / 88 suites; zero failures, one manual timing test ignored; native 62 in 264.91s。Fmt／Clippy／rustdocも成功です。F03完了で **3点（45 → 42）** を取り除きます。同じ監査で既存要件の計上漏れも確認しました。受理済みRFC 0008のcore::drop<T>とcore::panic<T>は現行CLIでSRC0024となり、明示的な未完了成果がありません。Source型検査／発散、move／loan規則、VM／nativeのcleanupとpanic code／span検証をF09・暫定 **3点（42 → 45）** として追加し、実sourceのpanic／export fixtureのためF05の前提に加えます。受理済み要件の未実装であり、新言語機能やschema 1／2の変更ではありません。**97 → 98 done、116 → 117行、残り45点**。追加見積りは **69 → 72点**、完了で取り除いた点数は **158 → 161点** です。従来gateは **7/18** のままです。次は検証済みC metadataを使うF02で、F04-F07、F09、その他のM2要件も維持し、C実行／object／schema 2の拒否を保ちます。

F01のWindows evidence（2026-10-19 JST）：[全workflow 39207649118](https://github.com/carimatics/lang/actions/runs/39207649118)、head cdae1ee、job 117913869152が成功し、fmt／Clippy／全テスト／doc／prototype benchmarkを完了しました。F03より前のsnapshotであり、残るC相互運用／性能gateの完了を示すものではありません。

過去のF02／F03着手前監査（2026-10-19 JST）：runtimeのlayout.rsはmanaged／VM向けでC layoutではないと明記しています。製品用のLANGCABI descriptor／header生成器はなく、RFCのscalar fingerprintと手書きC layout fixtureだけがあります。F02には厳密な宣言digestの照合と実C field／reserved storageの検証が必要なため、F03をF02の前提に加え、先に実装します。範囲は上限付きprofile 1のfunction／type／artifact descriptorとSHA-256、target別C layout、layout assertion付きC11 headerで、固定RFC vectorと実C compilerで確認します。検証済みC対応MIRを再利用し、F02のcontext／capability／poison／unwind検査、F04 adapter、F05-F07の全要件を維持します。F03をdoing・3点とし、**97 done / 1 doing / 18 todo / 45 pt** とします。作業数、見積り、収支、元のgateは不変です。

F01完了と最初のC実行の監査（2026-10-19 JST）：819ee7cで、03239a1の明示的なC MIR検証（RFC改訂68240cc）へsource／HIRのC型検査とlexicalなunsafe scopeを接続しました。未使用scalar宣言を含む有効なC宣言は独立検証済みownership MIRへ到達し、unsafe呼出し、型、scope、move、loan、固定profile違反にはsource診断を出します。Unsafeはruntime branchやcapabilityを追加せず通常のcleanupを維持します。ローカル必須検証は **1,437 tests / 87 suites**、失敗0、手動timing testのみ1件ignored。Nativeは62件・255.76秒で、fmt／Clippy／rustdocも成功です。CLIでC例のcheck成功、unsafeなしのC callへのSRC0025と宣言span、C実行のfuel 0／host call 0での拒否を確認しました。**96 → 97 done、48 → 45 pt**。116行と追加分69点は不変で、完了で取り除いた点数は **158点** です。F01はdoneとし、従来gateは **7/18** のままです。最初の実C source callには、F02の明示的なC context／grant／validityとpoison／unwind事前検査、F03の正確なC layout／descriptor／header、続いてF04の型付きimport adapterが必要です。検証済み型graphと既存instance／allocation／controlを再利用し、通常host grantからC bindingは有効にしません。F05のexportとF06／F07のnative platform fixtureを維持し、schema 2もC識別を引き続き拒否します。この監査でscopeや見積りは追加しません。

F01着手前／F08分割の過去監査（2026-10-19 JST）：実行したprobeではunsafeという旧関数名が通り、extern "C"はSRC0008の字句エラーでした。ParserとHIRのHostImportは従来のi64引数1個のhost形式で、HIRにunsafe statementはありません。Accepted RFC 0008にC構文は定義済みで、SRC0024の共用もRFC 0007が明示するため、診断番号の再設計は不要です。Losslessなquoted-C import／exportとunsafe block、formatter、予約語移行、上限付きrecoveryをF08（初期2点、6c32322で完了）として追加します。未実装の意味解析は通常host importとして扱ったり無視したりせず拒否します。F01にはC型／unsafe／sourceとMIRの全要件を暫定3点で維持し、coding前に再度範囲を確認します。**115 → 116行、47 → 50 pt**。追加分は **66 → 69点**、完了で取り除いた **153点** は不変です。元のC／ABI／runtime／platform要件は削除しません。F08／F01のfrontend成果後、境界基盤を増やす前に最初のC source実行経路を確認します。

N07 native／CI上限の証拠（2026-10-19 JST）：516f663の[Windows全run 39198008813](https://github.com/carimatics/lang/actions/runs/39198008813)が33分56秒で成功しました。1,404 test／87 suite、native 60件（748.74秒）、31 source／62 executableのdirect／response-file standalone test、fmt／Clippy／rustdocと、cleanなschema-20 prototype benchmarkのuploadが成功しています。先行run 39194948229がbenchmark中に30分を超えた後、119f9ffの60分上限と5fe96deのbuild／実行分離を検証できました。Test build 4分49秒、実行16分20秒、docs 17秒、benchmark／release build 10分03秒で、いずれもCI時間です。N07／G14のnative証拠は完了とし、代表的M2の全体CI／性能の証明はQ02／B01-B04へ残します。F08は別の[Windows run 39200895976](https://github.com/carimatics/lang/actions/runs/39200895976)で検証し、N07のsnapshotで後続source変更の成功を主張しません。Task／見積もりの変更はありません。

F08の完了成果（2026-10-19 JST）：6c32322でF08を完了しました。正確なquoted-C import／export宣言とstatementのみのunsafe blockをlosslessにparse／formatし、comment、fixtureの全prefix、上限付きrecoveryを確認しました。型名を含む従来unsafe名はSRC0024となり、unsafeの自動fixは付けません。CLI checkは未対応のC意味境界に字句エラーではなくSRC0025を返し、従来のi64引数1個のhost importは維持します。**95 → 96 done、50 → 48 pt**。116行と追加分合計69点は不変で、完了で取り除いた見積もりは **161点** です。Frontendのfocused 352 test、workspace **1,421 test／87 suite**（native 61件、250.59秒、手動timing test 1件はignored）、fmt／Clippy／rustdocが成功しました。C bindingの意味規則、MIR接続、capability、実C adapterはF01-F07へ残し、従来gateは **7/18** を維持します。

N06着手前監査（2026-10-19 JST）：owned objectのbyte列と対応metadataは生成できますが、検証付きgatewayはJITのCompiledFunctionが保持するaddressだけをdispatchします。Code ownerの寿命を保持し、同じ検証付きprogram全体のgatewayを利用するtrusted linked-image境界と、実際のlinked owned codeの実行証拠をM2-N19（3点）として明示します。N06のmacOS direct／response-file全体conformance、P02のclean-install runtime packagingは維持します。VMへのfallback、packet／schema変更、sourceの部分対応で置き換えません。**114 → 115行、51 → 54 pt**、新たに判明した見積もり **63 → 66点**、完了で取り除いた146点は不変です。実装前にN19をdoingとして実装を開始しました。現在はd53aac0でN19を完了し、230予算でlinked Mach-OとVMが一致し、target／symbol／予算拒否とowner解放も検証済みです。**92 → 93 done、54 → 51 pt**、追加分66点は不変、完了で取り除いた見積もりは149点です。全workspace 1,414 test／87 suite、native 60件、fmt／Clippy／rustdoc成功（計測専用1件はignore）。N19完了時点では、次のN06のstandalone executable検証を暫定2点としていました。

N05完了／G06 exit監査（2026-10-19 JST）：`f6d9fc4`で現在の全managed service／payloadの計量をVM／nativeのcall、loan、viewへ接続しました。24件のsource programで有限予算の全prefixを比較し、他control／provider拒否、guest panic、secondary cleanup、source／artifact CLIの検証も成功しました。予算超過後も数値付きerrorと必須retirementを維持し、Rust contextの移行とserialized schemaが不変であることを文書化しました。**91 → 92 done、54 → 51 pt**。暫定3点を消化し、taskや見積もりは追加しません（**134 + 63 − 146 = 51**）。全baseline **1,410 test／87 suite**（手動計測1件は別途成功）、fmt／Clippy／rustdoc成功。D28–D34、V07／V08、C07、N05の生成cleanup・順序・returnの証拠と、owned execution 139件、native 56件に基づきG06を完了とします。従来gateは **6/18** となり、G14のmacOS／Windowsでのowned object実link／runは未完です。他gateはそれぞれの最終evidence reviewを維持します。

N05着手前監査／N18分離（2026-10-19 JST）：CLIで--service-work 37を指定するとunknown optionでexit 2となることを実行確認しました。Map／Vec／Shared／Arenaの4 runtime fileだけでも公開methodは74件あり、payloadの走査・copyとVM／nativeの接続、CLI／reportを暫定1点にまとめた見積もりは維持しません。N18のCLI／report監査を初期1点（doing）、N05のmanaged統合と全体conformanceを暫定3点とし、旧合計1 → 4点（+3）へ明示的に見直します。API数は工数の換算率ではありません。**113 → 114行、52 → 55 pt**、追加見積もり**60 → 63点**。N05の全service／payload、nativeのfailure／return、version付きreport移行、完全なVM／native conformance要件は維持し、managed eventの実装前監査を引き続き必要とします。

N17完了監査（2026-10-19 JST）：共通Bytes／Text計量とVM／nativeの有限予算programを実装（初期配分1点、085700c）。RFC 0013のunit全体の受理条件を先に明示改訂（143decc）し、未使用関数も含めた未対応eventをeffect前に拒否します。6 programの全予算prefixとguest callを検証し、fixtureは37単位で67を返します。Runtime境界test 3件、native 53件、全workspace 1389件／85 suite、fmt／Clippy／rustdoc成功。**89 → 90 done、53 → 52 pt**、追加見積もりは**60点のまま**、完了で取り除いた見積もりは**142点**。N05には全managed／payload・driver／CLI／report移行・完全なconformanceを残し、暫定1点は着手前に監査します。元のgateは5/18のままです。

N05縦slice配分（2026-10-19 JST）：既存2点をN17の共通Bytes／Text計量とVM／nativeのbuffer program実行（**初期見積もり1点**、doing）へ1点、残るN05のmanaged service・driver／report・全体conformanceへ **暫定1点** 配分します。**112 → 113行**、**53 ptは不変**、見積もり追加なしです。計量未完のserviceを使うprogramでは有限予算をeffect前に拒否し、RFC 0013の全成果を維持します。

N16完了／配分監査（2026-10-19 JST）：旧N05合計3点をN16のmeter／guard（**初期見積もり1点**、完了、`06c8e1d`）と、N05の全service接続／conformance（**暫定2点**）に配分します。**111 → 112行**、見積もり追加なし（追加分合計 **60点は不変**）、完了で **54 → 53 pt** です。共通APIで厳密／atomicなbatch課金とcleanup後のfailure数値を検証し、VM entry／preview／nativeで未完profileの有限予算をprovider effect前に拒否します。RFC 0013全event、driver／report移行、全VM／native service検証はN05に残します。

N15決定監査（2026-10-19 JST）：[RFC 0013](rfcs/0013-service-byte-work-budget.md) を実験段階の委任でacceptedとしました（`0a38beb`）。共通の累積event、構造化拒否、既知の依存costの一括確保、必須cleanupの課金免除、互換性規則を定義します。Map 20 testの成功は局所的な実現性の裏付けで、global実装／conformanceはN05の3点に維持します。N15の **初期見積もり1点** を同じ更新で追加・完了し、**110 → 111行**、**54 + 1 − 1 = 54 pt**、追加分合計 **59 → 60 pt**、完了分 **139 → 140 pt** です。元の要件は削除しません。

N05分割／N14完了監査（2026-10-19 JST）：着手前testでUTF-8の最大cell数がnative 49／VM 57と異なり、loan登録とreturn escrowが抜けていました。失敗の比較testではprimary spanも関数全体になっていました。N14（**初期見積もり3点**、完了）でcontrol／usageを共通化し、両不具合を修正し、未対応の準備済み容量設定をeffect前に拒否します。旧N05合計3点をN14制御3点と残るN05 service 3点へ分割し、完了前は **109 → 110行**、**54 → 57 pt**、追加分合計 **56 → 59 pt** です。N14の3点を完了し **57 → 54 pt** になりました。順序付きservice／payload全体のconformanceと設定可能なbyte-work admissionはN05に維持し、具体的なservice-work契約を実装前に設計文書で定義します。

N04完了監査（2026-10-19 JST）：字句arenaのowner・操作、collection／call越しのread custody、入れ子brand検査がnative／VM fixtureで成功しました。範囲は3点のままで、追加task・見積もり増はありません。**57 → 54 pt**、**109行は不変** です。順序付き失敗・制御とservice／payload全体のconformance、設定可能なbyte-work admissionはN05に残します。

N04分割・完了監査（2026-10-19 JST）：初回shared／arena sourceはVMで9を返し、nativeではshared core操作とarena字句開始が別々に未対応でした。旧N04合計3点をN13 shared／weak（**初期3点**、今回完了）とN04 arena 3点へ分割・再見積もりし、**108 → 109行**、完了前の残りを **57 → 60点**、追加分合計を **53 → 56点** としました。N13の3点完了で **60 → 57点** です。Sourceの排他アクセス規則は変えず、custody fixtureは所有clone経由で置換します。N04にはarenaの字句lifecycle、collection／call越しのread custody、入れ子brandを残し、service／payload全組合せと設定可能なbyte-work admissionはN05に維持します。

N03見積もり・依存関係監査（2026-10-19 JST）：keyの借用・消費、旧所有値の結果保護、切り離したkeyと拒否入力の解放の接続が必要なため、実装前にN03を **1 → 3点**、残りを **58 → 60点**、追加分合計を **51 → 53点** へ再見積もりしました。初回sourceはVMで49、nativeでUnsupportedとなり、現在はnativeでも成功します。最後の3点を完了し **60 → 57点**、108行は不変です。N04はN12／N03に依存し、collection／call越しのread custodyと入れ子brand検査を明示して維持します。N05と元のplatform／FFI／performance／debug・guide全要件を残します。

N12見積もり・依存関係監査（2026-10-19 JST）：当初のnative参照先はframe cellとallocator権限だけでした。Vector-backed projection／変更／置換と入れ子cleanupの接続が必要なため、実装前にN12を **1 → 3点**、残りを **59 → 61点**、追加分合計を **49 → 51点** へ再見積もりしました。初回sourceはVMで9、nativeでUnsupportedとなり、現在はnativeでも成功します。最後の見積もり3点を完了し **61 → 58点**、108行は不変です。N05にはN02／N11／V08に加えN12／N03／N04の依存を明示し、受理済みservice／payloadの全組合せ、順序付きexit、設定可能なbyte-work admissionを維持します。

**直近の完了 — N16**（`06c8e1d`、2026-10-19 JST）：Controlのservice-work設定、Usage counter、checkedなatomic batch、数値付き動的failureと共通変換を実装しました。Runtime 4 testで厳密／overflow／batchの拒否、polling／fuel不変、mandatory cleanup後のprimary数値とsecondaryを確認します。Entry／preview／nativeの有限予算0／1／u64::MAXは、service未接続の間はprovider effect・usageなしで拒否します。全 **85 suite・1385 test**、fmt／Clippy／doc成功。Service event接続はN05のままです。

N16検証時間：runtime重点4 testは報告値0.00秒、profile拒否の重点1 testは0.15秒、all-feature native suite 52 test／11.25秒。全体suiteのCargo compile 61秒、test報告時間合計88.29秒、ログ作成から最終書込まで752.13秒。Clippy 68秒、doc 28.71秒です。検証区間は実作業時間の計測値ではありません。

N17検証実測：Focused runtime 3件／0.00秒、有限buffer予算prefix 1件／0.69秒、全feature native 53件／11.29秒。全testのCargo compileは47.51秒、test報告時間の合計は88.20秒、log作成から最終書込みまで529.91秒。Clippy 45.34秒、rustdoc 19.28秒。検証の観測値であり、実作業時間やruntime benchmarkではありません。Build cacheの追加調査でdepsに645,863 entry／33.05 GiBがあり、一覧走査は12.26秒でした。Cargoのpackage単位の整理で生成物439,988 file／21.6 GiBを削除しました。同じ集計方法で整理後は228,369 entry／13.62 GiB、走査1.885秒です。各1回の走査はfilesystemの集計costを示し、検証実時間の残差すべての原因やsuite全体の高速化は示しません。起動のみの一覧表示2件は各0.00秒でした。Cache整理によるsource変更とfull baselineの再実行は不要で、plan更新後はrepository consistencyのみ実行します。

N07のローカル検証実測（2026-10-19 JST）：macOS baselineは1,415 test／87 suite成功、native 61件／255.64秒、計測専用1件ignore。全test実時間337.35秒、Cargo compile 0.60秒、suite報告時間合計334.29秒。Clippy実時間1.33秒、rustdoc 10.46秒、linked-image probe 1件／2.48秒。ローカル検証の時間で、Windows証拠・実作業時間・runtime benchmarkではありません。Planのみの変更はrepository_consistencyで検証します。

N07移植前の証拠確認（2026-10-19 JST）：当時の最新[Windows全workflow成功run](https://github.com/carimatics/lang/actions/runs/39007443435)はhead 88325a5、最終更新2026-10-17 05:54:49 UTCで、owned standalone matrix以前でした。この旧runをN07完了の証拠には使わず、上記の現行実装9589022のnative Test実行を確認して完了としました。

## 進捗と見積もり変更

詳細Todo導入時の `8605416`（2026-10-18 JST）以降、検証済み成果は **35 → 98件（+63件）**、作業一覧は **87 → 118件（+31件）** になりました。新たに可視化した作業は既存要件の実装であり、各件が同じ工数を表すわけではありません。

| 詳細計画導入時の残り | 追加・再見積もりの差分合計 | 完了により取り除いた見積もり | 現在の残り |
| ---: | ---: | ---: | ---: |
| 134 pt | +72 pt | −161 pt | 45 pt |

**134 + 72 − 161 = 45 pt**

完了した作業と新たに判明した作業を分離しました。完了により見積もり161点分を取り除く一方、前提作業の可視化と再見積もりで72点増えています。これは計画上の単位で、時間や実工数の計測値ではありません。残り45点は暫定値で、FFI、packaging、debug、性能には引き続き実装前の実験が必要です。元の18完了条件は維持します。各成果の更新時にこの収支を検証し、追加分の合計は明示的なscope／見積もり監査がある場合のみ変更します。詳細計画の初期134点と、歴史的な完了条件の重み113点は別の指標です。

N13／N14の **初期見積もり各3点**、N15の **初期見積もり1点**、N16の **初期配分1点** を保持します。N16は既存N05の3点から1点を配分し、N05に2点を残したため見積もり追加はなく、**54 − 1 = 53 pt** です。追加分合計は **72点**、完了で取り除いた見積もりは **155点** です。N03／N12の1 → 3点監査、N11／N09の初期各1点も保持し、元の要件は削除しません。

以前のcode baselineの実測（2026-10-19 JST）：workspace Clippy **28.3秒**、全test **362.5秒**、rustdoc **14.2秒** の実時間（shell time）。Cargo報告のtest compileは **24.4秒**、suite時間合計は **87.2秒** です。Native境界suiteは **40 test・10.77秒** で成功し、service失敗matrixはcompile済みcodeを再利用してruntimeを毎回作り直します。検証所要時間であり、実稼働時間やruntime benchmarkではありません。文書だけの計画更新では `repository_consistency` を実行します。

現在のsource対応範囲：引数なしi64 mainでBytes／Text、arena allocation／read／replace／teardown、shared／weak allocation／clone／read／replace／upgrade、Vec new／len／push／pop／shared・mutable read、厳密なi64／Text keyのMap new／len／insert／get／removeを実行します。Storable payloadは入れ子collection、inline record／enum／Option／Result、再帰guest call越しに組み合わせられます。Shared publicationは推移的strong edge、arena storeは入れ子brandを検査し、projection／call越しにread custodyを保持します。Ownerはcleanupで保護したreturnとprepared replacementを通り、正常／失敗teardownを反復実行します。Ordinary loan、allocator authority、arena ownerは格納不可で、cross-region／shared-heap edgeとarena escapeを拒否します。Borrowed matchとpartial move禁止は維持します。V16／V15／V10は既存managed preview／推論／REPL profileに対応し、V13は明記したsource coverage内のprovisional-read loan終了を検証します。Indirectionを含むpreviewはV17です。V11はin-memory CFG argumentに対応しますが、現ownership source loweringは生成しません。Nativeの生成exitは累積workのconformanceまで検証済みで、macOS standalone実行はN06で確認済みで、Windows／production driver／FFIの残要件はevidenceが必要です。Source統合だけでexit gateを閉じません。

実装前・分割監査（2026-10-19 JST）：初回Bytes sourceはVMで1を返し、native allocator／Bytes接続の不足を確認しました。公開buffer APIがstorage操作を提供し、collectionには別途managed loan viewが必要です。N03の3点をN11 allocator／Bytes／Text（初期1点、今回完了）、N12 Vec（1点）、N03 Map（1点）へ分割しました。**106 → 108 task**、**N11の1点完了前の残り60点は不変** です。N05は設定可能なservice byte-work admissionを引き続き含み、exit conformance全体の前提としてN11に依存します。RFC 0012はVM／nativeで設定済み制御を一致させ、未対応設定を拒否することを要求します。具体的なservice-work契約は実装前に設計文書で定義する必要があります。Native／FFI／platform／conformance／performance／debug全要件、V17、Z01の英日guideを維持します。

従来の完了条件は **18件中7件完了** です。G06のcompiler／VM／canonical native cleanupに加え、G14の両tier-1でのowned source object link／runを完了しました。全workflow成功とFFI、packaging、代表的conformance／性能、debug、最終guideは未完です。

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
| M2-F08 | done | 0 | M2-D27 | G11,G12 | AcceptedのC import／exportとunsafe blockをlosslessにparse／formatし、予約語を診断し、上限を維持して未完成の意味解析を拒否する。 | `6c32322` |
| M2-F01 | done | 0 | M2-D27,M2-F08 | G11 | 受理済みunsafe／source境界規則をsource診断とcompile-fail fixtureで強制。 | `819ee7c` |
| M2-F09 | todo | 3 | M2-F01,M2-N07 | G08,G11,G12 | 受理済みcore::drop／panicのsource型検査・発散・move／loan規則を実装し、VM／nativeのcleanupとpanic code／spanをsource fixtureで検証。 | — |
| M2-F10 | doing | 1 | M2-F03 | G11,G12 | C address／正規value検査でpaddingを読まず、alignment／extent／overlapと未知tag／非正規byteを作用前に拒否し、共有work計上とfailure fixtureを保つ。 | — |
| M2-F02 | todo | 2 | M2-F01,M2-V08,M2-F03,M2-F10 | G11 | C境界のvalidity／capability／poison preflightとunwind containmentを失敗fixtureで検証。 | — |
| M2-F03 | done | 0 | M2-F01 | G12 | 厳密なC descriptor／digest／headerを生成し、受理済みvectorでlayoutを検証。 | `dee7f34` |
| M2-F04 | todo | 3 | M2-F02,M2-F03 | G12 | C importでscalar／aggregate引数、allocation context、failure statusを扱う。 | — |
| M2-F05 | todo | 3 | M2-F04,M2-F09 | G12 | C exportで規定status／resultを返し、入れ子cleanup failureをcontain。 | — |
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
| M2-N11 | done | 0 | M2-N02,M2-C03 | G07,G08,G14 | Native allocator／Bytes／Text操作を共有runtimeへ接続し、source VM fixtureと一致させる。 | `1acb73a` |
| M2-N12 | done | 0 | M2-N11,M2-C03 | G09,G14 | Native Vec操作とscope付き要素loanをVM fixtureに一致させる。 | `649aa08` |
| M2-N03 | done | 0 | M2-N12,M2-C06 | G10,G14 | Native Map操作とscope付きvalue loanをVM fixtureに一致させる。 | `2e6a441` |
| M2-N13 | done | 0 | M2-N02,M2-N11,M2-N12,M2-N03,M2-C07,M2-R04,M2-R07 | G05,G14 | Native shared／weak操作でheap lifecycle、cycle拒否、collection／call projection越しのread custodyとcleanupを維持。 | `4e2c8d6` |
| M2-N04 | done | 0 | M2-N13,M2-N02,M2-N11,M2-N12,M2-N03,M2-C07,M2-R04,M2-R07 | G05,G14 | Native arenaの字句ownerと操作を接続し、collection／call projection越しのread custodyと入れ子brand検査を維持。 | `ea15b05` |
| M2-N14 | done | 0 | M2-N04,M2-N13,M2-N02,M2-N11,M2-N12,M2-N03,M2-V08 | G06,G14 | Nativeの実行上限・割込み・deadline・usageと所有値の失敗cleanupがVMの制御契約に一致。 | `a40e6eb` |
| M2-N15 | done | 0 | M2-N14 | G06,G14 | Service byte-workの課金・失敗・所有権・互換性をRFCで決定し、実現性を確認。 | `0a38beb` |
| M2-N16 | done | 0 | M2-N15,M2-N14 | G06,G14 | 共通service-workのcounter・batch課金・構造化failure・未対応profileの事前拒否を実装。 | `06c8e1d` |
| M2-N17 | done | 0 | M2-N16 | G06,G14 | 共通Bytes／Textのwork課金を接続し、対応programの有限予算・refusal・cleanupをVM／nativeで一致させる。 | 085700c |
| M2-N18 | done | 0 | M2-N17 | G06,G14 | Source／artifact／REPLのCLIでservice-work予算と数値付きfailureを扱い、report schemaの互換性を監査する。 | 419f6ac |
| M2-N05 | done | 0 | M2-N02,M2-N11,M2-N12,M2-N03,M2-N13,M2-N04,M2-V08,M2-N14,M2-N15,M2-N16,M2-N17,M2-N18 | G06,G14 | 設定可能なservice byte-work admissionを含め、受理済みservice／payloadの組合せ全体でnativeの生成failure／return exitをVMのcleanup順序とfuel behaviorに一致させる。 | `f6d9fc4` |
| M2-N19 | done | 0 | M2-N05 | G14 | Linked imageの寿命を保持した信頼済みbindingと既存gatewayによるowned program実行を、実objectで検証する。 | `d53aac0` |
| M2-N06 | done | 0 | M2-N03,M2-N04,M2-N05,M2-N19 | G14 | Native macOSでowned programのdirect／response-file object link／runを検証。 | `25de18a` |
| M2-N07 | done | 0 | M2-N06 | G14 | Native Windowsで対応owned programを実行し、成功CI evidenceを記録。 | `119f9ff` |
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
