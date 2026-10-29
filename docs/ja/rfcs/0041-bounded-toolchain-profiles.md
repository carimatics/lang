<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0041-bounded-toolchain-profiles.md -->

# RFC 0041: 上限付きtoolchain profile

[English](../../rfcs/0041-bounded-toolchain-profiles.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M4-E01–E04
- Supersedes: none
- Superseded by: none

## Summary

`lang profile compile --manifest-path Lang.json`で通常のpackage compilerを観測し、上限とversionを持つJSON reportを返す。runtime profile／allocator trace／rebuild説明は別の検証済みreport kindとして後続する。この採択で実装やG8を完了しない。

## Motivation

既存pipelineは不変package入力、構造化診断、検証済みportable artifactを提供する。2026-10-30 JSTのexact-SDK単独実験はhelloを既存frontendとlower／verifyでcompileし、それぞれ2.684 msと0.212 msを観測した。観測境界の証拠であり、benchmark／overhead推定／高速化ではない。費用比較には再現可能な入力IDと明示的な計測範囲が必要である。

## Goals

- 別compile経路やguest実行なしで実compiler操作を観測する。
- package権限、診断、意味上の結果、既存上限を保つ。
- reportのversion／上限を定め、経過時間の観測と決定的counterを区別する。

## Non-goals

- optimizer変更、remote telemetry、source自動送信、統計的性能主張。
- CPU時間、parser／type checker内部への細分化、compile cacheの主張。
- 新source構文、実行grant、保存program schema、public C ABI変更。

## Guide-level design

`lang profile compile [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--message-format human|json]`は厳密に`./Lang.json`とhost targetを既定とする。成功時は完全なreport一件をstdoutへ出し、既存code／spanを持つ診断はstderrへ出す。artifact公開、Lang.lock作成／置換、main実行、host import grantは行わない。明示workspace選択は通常package契約に従う。

## Reference semantics

compile reportは`schema: 1`、`kind: "compile"`、`clock: "host_monotonic_elapsed_ns"`、`compiler_id`、`target`、`input_digest`、`source_files`、`source_bytes`、`artifact_bytes`、`elapsed_ns`、`phases`を持つ。各phaseは`name`と`elapsed_ns`を持ち、順序は厳密に`package_capture`、`frontend`、`lower_verify_encode`とする。

captureはgraph admission、入力固定、通常のcanonical lock計算を含む。frontendはpackage module parse／bind／type／ownership解析を含む。lower／verify／encodeは既存の完全portable compilerを呼ぶ。粗い連続境界であり、elapsed_nsはchecked加算で求め、report encodeや無関係な管理処理を含めない。実際に観測した操作より細かい費用を表示しない。input digestは取得したcanonical lock bytesのSHA-256、compiler_idは既存host SDK build identityであり、互換性の観測であって認証ではない。

checked monotonic差分とu64 nanosecond変換／加算を使い、不正な観測を拒否する。0時間は有効で、clock精度を証明しない。reportは最大4 MiB、compile phaseは厳密に3件、sourceは最大4,096 file／32 MiB、artifactは64 MiBとする。これは既存compiler上限に加えるreport policyである。完全JSONを検証してから出力し、compile失敗時に成功した部分reportを出さない。readerは未知／重複field、未対応schema／kind／clock、不正ID／target、phase順序／件数、overflow、上限超過を拒否する。report読込みはsource／実行権限を与えない。

runtime／trace／rebuild kindは実装前にRFC具体化と実行証拠を必要とする。native帰属をVM replayで代用せず、provider counterを架空のallocation eventに置き換えず、不完全な観測を完全traceと呼ばない。rebuild説明はtimingからcache挙動を推測せず、実入力変更を特定する。

### Runtime観測の具体化（2026-10-30 JST）

exact-SDKの`target/m4-e02-charge-point-probe.log`は同じ所有権付きvector programを元VMと実compile済みnative JIT entryで実行する。fuel上限0／1／2／8で元usageとfault spanが一致し、完了時は両方9を返し、fuel20／host call0／service work16／peak frame1／cell30となる。元allocatorは両方とも全storageをretireする。既存charge／span境界の証拠であり、production profileやWindows受入ではない。

execution Controlに信頼済み任意の借用point recorderを加える。元fuel charge成功後、効果前に、その操作の既存optional source spanだけを記録する。poll、fuel／service／host accounting、cleanup、主失敗順を保つ。両configuration比較へrecorder identityを含め、ControlのSend／Syncを維持する。recorderはuser callbackを持たず、実行前に有限bufferを予約し、同期した所有metadataだけを使い、記録中に確保しない。既定4,096 sample、最大65,536、sampling間隔1–65,536で成功chargeのN件ごとを選ぶ。spanなしも含め成功chargeをすべて数える。buffer／counter／lock失敗は観測errorとして保持し、guestの中断／再実行はせず、snapshot／公開を拒否して完全prefixと主張しない。

`lang profile run`は`--engine vm|native`を選び、既定vm、明示package／host targetのadmissionを使いhost importはgrantしない。初期VM modeは通常Copy／ownership、native modeは元のownership JIT driverとそのcompile code ownerで、VMへfallbackしない。task／C／component bindingと非計測Copy nativeのsource-point profileは効果前に拒否する。native AOT instruction profileは未対応とし、実JIT／native fixtureが初期native runtime観測profileを満たす。元の有限fuel／frame／cell上限を使い、guest capabilityを増やさない。

`cargo run -- profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine native`で実行し、同じ取得済みsourceには`--engine vm`、決定的な間引きには`--sample-every 2`を使う。所有権付きvector／call fixtureはstorageをretireして42を返す。`--observations N`で有限buffer容量を選ぶ。runtime上限は通常の`--fuel`、`--frames`、`--cells`、`--service-work`を使う。未sample件数や統計的に偏りのないhot spotは推定しない。

runtime reportはschema 1／kind runtimeを使い、engine／意味profile／sampling間隔／成功charge総数／sample／帰属不能件数／source byte-span別sample数／元usage／結果／host経過時間を明示する。entry計測は元実行／preflight／cleanupとinstrumentationを含み、code生成／準備はその前に行う。sample数は課金位置の頻度であり、CPU時間、完了操作数、推定instruction費用ではない。間隔1でbuffer内なら全成功chargeを観測する。不明spanは帰属不能とし、sourceは同じ取得済み受理programを使う。report pathは相対の観測で、IO権限にならない。完全reportを4 MiBに制限し、公開／読込み前にID／受理source範囲／合計／有限counterを検証する。compile reportのfield／encodingは変更しない。

runtime schema-1 fieldは`schema`、`kind`、`clock`、`compiler_id`、`target`、`input_digest`、`engine`（`vm`／`native_jit`）、`profile`（`copy`／`ownership`）、`measurement`（`successful_fuel_charges`）、`sample_every`、`total_charged_points`、`sampled_points`、`unattributed_points`、`sources`、`points`、`usage`、`result`、`elapsed_ns`とする。sourceは`id`、相対`path`、`bytes`、`digest`、point集計は`source`、`start`、`end`、`samples`を持つ。usageは`fuel`、`host_calls`、`service_work`、`peak_frames`、`peak_live_cells`を保つ。sourceはID順、集計はsource／start／end順で重複を認めない。sample合計はfloor(total／interval)および集計sample＋帰属不能sampleと一致し、fuelは総charge数に一致する。native JITにはownershipを必要とする。readerは未知／重複field、不正span／件数、合計不一致を拒否する。guest実行失敗は通常診断経路で報告し、成功runtime reportを返さない。

RFC 0001の実験委任で採択する。有限実験が実native codeで共有span、厳密counter、cleanupを示した。観測にはhost lock／memory／timing overheadが増え、deadlineは引き続き実host経過時間を含む。private Rust metadata／layout準備費用は変わり得るが、保存形式とguest fuel／service／provider accountingは変えない。時間やoverheadの同値は主張しない。元結果／usage同値、sample帰属、上限超過、両host証拠をE02で検証する。E03 allocator eventとE04 rebuild説明は別の具体化に残る。

### 論理allocator traceの具体化（2026-10-30 JST）

実行可能な`target/m4-e03-allocator-boundary-probe.log`は、許可request4／論理allocation3／retirement3に対し、providerのattempt3／allocation2／retirement2を観測した。0-size ownerはprovider呼出しなしでも数え、許可済み64-byte OOMは累積要求を112 bytesへ増やすが成功peak48 bytesを増やさない。最終liveは0。RFC 0008（最終更新2026-10-20）がこの意味を既に定めている。provider callbackだけでは完全なcounter traceを提供できない。

E03の残るreport／CLI／reader（3 pt）より前にE03R（2 pt）を公開する。元root Allocatorへの任意の上限付き所有ledgerと最初の実VM／native fixtureを実装する。最初の許可request前に結び、再設定、非fresh状態、borrowed viewは拒否する。borrowed Memory viewは元rootへ転送し、そのrootが一度だけ記録する。元generation ID、要求size／alignment、権威ある操作後Statsを保持する。許可allocation成功、元live返金後の許可OOM／invalid-storage失敗、0-size owner／内部resizeを含む成功retirementを記録する。許可requestを生まない純粋なpreflight拒否／checkはevent列の範囲外。provider callbackから欠落eventを再構成しない。

guest entry前にbufferを予約する（既定4,096、最大16,384 event）。regionやbyte本文を保持せずmetadataだけを持ち、記録中にcallbackや確保を呼ばない。trace errorは別に保持し、request／counter／handle／provider呼出し／guest control／主失敗順／cleanupを変えず、不完全snapshot／公開を拒否する。allocator identityと元borrowed-view権限を保つ。trace設定／metadata保持でhost allocation／layout／lock費用が増えるがguest要求bytes外でありprocess RSSと呼ばない。既存public C／保存program／SDK形式は変更せず、private Rust SDK identityには通常の再buildが必要となる。

E03では厳密なfield／replay契約を具体化した後、別の検証済みreport kindを公開する。境界は元entry returnとし、その時点の元allocator実Statsと完全論理event ledgerを照合し、失敗後に残るlive ownerも含める。cleanupを強制せず、0-size ownerを物理確保として捏造せず、直前chargeからsource／宣言位置を推測せず、host allocator破棄全体を観測したと主張しない。VM／native traceは元engine、同じallocator／control、元failure／retirement経路を使う。

RFC 0001の実験委任でこの前提を採択する。実験が欠けた観測境界を示し、既存RFC 0008がaccounting／cleanup契約を提供する。元操作／identity／counter同値、0／OOM／invalid storage／resize／borrowed転送、有限buffer失敗、両host entry fixtureは未検証実装要件に残る。E03Rの新規2 planning pointはE03の消化やG8完了を意味しない。

## Execution modes

compile profilingは通常portable compilerが受理する全意味profileを、元のentry／preflight規則で使う。VM／native AOT／JIT／component codeは実行しない。runtime modeは明示instrumentation／計測上限を持つE02／E03の要件に残る。

## ABI and compatibility

reportは新しいschema-1 host artifactである。既存source／edition／MIR／bytecode／package／lock／SDK／C／component形式は変更しない。未知の将来report kindをreaderは拒否する。非互換compile report変更には新schemaを必要とし、既存programの移行は不要である。

## Diagnostics and tooling

通常package／source診断を再利用する。CLI usageはexit 2、compile／report検証／観測失敗はexit 1を維持する。source本文や環境中の絶対pathをreportへ出さない。初期CLIは永続profile fileを書かず、呼出側がstdoutをredirectできる。formatter／LSP／DAPの意味は変更しない。

## Performance and cost model

clock読込み、canonical lock IDの保持、report確保、JSON encodeにhost workが増える。timingはinstrumentationとmachine scheduling／I/Oの影響を含み、guest CPU時間と呼ばない。比較にはこのdelivery外の反復制御実験が必要である。通常の非profile commandに新timer／reportを加えない。

## Security and trust boundaries

明示manifest／package／target admissionと上限付き不変sourceを再利用し、editor overlayやancestor探索を使わない。compileはguest capabilityを増やさない。reportは信頼しない観測であり、credentialや実行manifestではない。parser入力を制限し、有限counter／IDを利用前に検証する。

## Alternatives

- 変更なしでは独自benchmarkに利用者向け観測契約がない。
- 外部process時間は有用だが、単独ではcompiler境界の分離やpackage入力ID固定ができない。
- 汎用trace frameworkは細かい観測が可能だが、最初の実行可能reportより先に統合とbuffer管理を増やす。
- 全体stopwatch一件は簡単だがcapture／frontend／lowerの違いを隠す。E01には既存3境界で十分である。

## Implementation plan

E01でcompile report／readerと実package CLI fixtureを実装する。E02で実VM／native runtime workを観測し、E03で実allocator／retirementを上限付きbufferへ記録し、E04で実rebuild入力を説明する。各残成果を実装前に再監査し、全4成果を対応hostで検証するまでG8は未完了とする。

## Test plan

通常／profile compileのartifact長／入力IDを比較し、実宣言依存とworkspace選択をcompileする。source診断と従来出力を保ち、不正version／phase／件数／overflow reportを拒否する。正確なelapsed値をassertしない。実装にはconsistencyと完全baselineを実行し、両host CI証拠を保持する。

## Open questions

2026-10-30 JST、RFC 0001の実験委任で採択する。E01 compile reportはsource `e2f0c1d`／最終head `4ee6043`の完全local baselineと両host CIで検証した。E02最終head `7baee25`は完全local baseline（170 suite／2,485 test）、全体CI `40951375087`、native CI `40951374893`に成功し、実VM／nativeの結果、usage、元span観測が一致した。E03Rの元allocator trace実装とE03／E04のreport／rebuild契約・証拠は引き続き必要。単発timingを性能比較に使わない。

## Sources

- [Rust 1.99.0 Instant](https://doc.rust-lang.org/1.99.0/std/time/struct.Instant.html): page更新日は非公開、2026-10-30 JST確認。monotonic計測はsteady clockを保証せず、checked差分で不正な観測を検出できる。
- RFC 0034／0035（ともに最終更新2026-10-29）、RFC 0008（2026-10-20）、現package／frontend／artifact／allocator APIを2026-10-30 JSTに確認。codeに埋込更新日はない。report上限／field／CLI挙動はproject判断である。
- 無視対象`target/m4-e01-compiler-cost-probe.log`、2026-10-30 JST観測。対応SDKとhelloの実compile一回であり、性能比較は主張しない。
