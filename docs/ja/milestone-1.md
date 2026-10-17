<!-- canonical-language: en; japanese-translation: ja/milestone-1.md -->

# Milestone 1 機能ガイド

[English](../milestone-1.md)

最終更新／確認：**2026-10-17 JST**。Milestone 1はformatterとLSP統合を含め、**24/24成果、89/89点**で完了しました。RFC 0006は **implemented** で、stabilizedではありません。Projectは引き続きpre-alphaです。

## 試してみる

Repositoryで固定したRust 1.99.0 toolchainを使います。Repository rootで実行します。

```bash
cargo run -- run examples/scripts/answer.lang
cargo run -- repl
cargo build
```

Scriptは `42` と改行を出力してexit 0です。Compile／runtime失敗はstderrに診断を出してexit 1、不正なcommand usageはexit 2です。Scriptの戻り値は出力結果であり、processのexit codeではありません。

実行fileは `target/debug/lang`（Windowsでは `target/debug/lang.exe`）です。利用可能なcommandは `lang --help` で確認できます。従来の `lang 20 22` も元のscalar VMとJIT prototypeを比較します。

## 小さな型付きprogramを書く

Source languageは実際にparseしたprogramを名前解決、型検査、typed MIR、portable VMへ通して実行します。

| 領域 | 利用できるbehavior |
| --- | --- |
| Source | BOMなしUTF-8、ASCII identifier、decimal signed `i64` literal、行comment、nested block comment。Lossless syntaxとrecovery |
| Scalar | `i64`、`bool`、wrapping `+`、signed `<`、boolean literal。暗黙変換なし |
| Function | 明示parameter／result型、forward／mutual call、再帰、左から右のargument評価、明示 `return` |
| Local state | `let mut name: Type = expression;`、value全体の代入、lexical visibility。Parameterとpattern bindingはimmutable |
| Control flow | `if`／`else`、`while`、statement `match`。Function exitとcondition型を検証 |
| Data | Nominalなcopy-only `struct`／`enum`、構築、field読取り、nested pattern、上限付き網羅性検査と到達不能code警告 |
| Module | `mod`、`pub`、`use`、qualified pathと可視性。重複、循環、root escape、public signatureのprivate型を診断 |
| 診断 | Stable source code、primary／related span、note、fix preview、不完全sourceからの復帰、元のREPL位置、runtime call stack |

Local名は別branchやmatch armを含めてfunction全体で一意にする必要があり、shadowingはできません。Record fieldは任意の順で書けます。Expressionは記述順に評価し、storage／表示には宣言順を使います。Recordとenum payloadの型graphは非循環である必要があります。Matchはstatementで、expressionの結果を生成しません。

既存の[answer.lang](../../examples/scripts/answer.lang)でmodule、call、record、variant、網羅的patternを試せます。

```text
mod math;

struct Pair { left: i64, right: i64 }
enum Answer { Missing, Found(Pair) }

fn total(answer: Answer) -> i64 {
    match answer {
        Answer::Missing => { return 0; },
        Answer::Found(Pair { left: a, right: b }) => { return a + b; },
    }
}

fn main() -> i64 {
    return total(Answer::Found(Pair { left: math::increment(20), right: 21 }));
}
```

同じdirectoryの[math.lang](../../examples/scripts/math.lang)は `pub fn increment(value: i64) -> i64 { return value + 1; }` をexportします。Scriptにはrootの `main() -> i64` が必要です。File moduleはprocessのworking directoryによらず、明示source rootから解決します。`mod math;` は `math.lang`、そのmodule内の子 `mod inner;` は `math/inner.lang` を選びます。

`lang run FILE` は既定でFILEの親をsource rootにします。`--source-root DIR` で変更できます。Symlink targetを含め、読込み前にcanonical containmentを確認します。このcompilerのfile authorityはguest runtime capabilityとは別です。

## REPLで試す

1回に1つの宣言またはexpressionを入力します。

```text
> fn increment(x: i64) -> i64 { return x + 1; }
> increment(41)
42
> struct Pair { left: i64, right: i64 }
> Pair { right: 21, left: 21 }
Pair { left: 21, right: 21 }
> :reset
> :quit
```

宣言は候補program全体のanalysisとMIR verificationに成功した場合だけcommitします。定義や評価に失敗しても以前の定義を利用できます。永続化するのは定義textで、expressionのvalueやmutable globalは保持しません。Expressionごとに新しい実行counterを使います。診断、fix preview、stack traceは、以前の定義も含めた元の番号付きsubmissionを指します。

括弧／波括弧やnested commentが閉じていなければ複数行入力できます。対話入力は `>`／`..` promptを表示し、pipe入力には表示しません。`:reset` は定義を消去します。`:quit` または通常EOFは処理済みsubmission errorがあってもexit 0、不完全EOFと致命的IO／入力上限errorはexit 1です。File moduleはsessionに `--source-root DIR` を渡した場合だけ有効です。

## Editorを接続してsourceを整形する

LSP clientにbuild済み実行fileと引数 `lsp` を設定します。Repository rootから `cargo run --quiet -- lsp` でも起動できます。標準出力にはframingされたprotocol messageだけを出します。

| LSP機能 | Prototype契約 |
| --- | --- |
| Lifecycle | `initialize`、`initialized`、`shutdown`、`exit`。UTF-16位置を明示advertise |
| 同期 | Full document変更、増加versionの検証、open／close overlay |
| 診断 | 共有parser／module／type query、不完全source、複数file位置、capabilityに従うrelated informationとversion、close時の消去 |
| Workspace | 明示local rootは最大1つ。Watched-file refreshとoptionalなdynamic watcher登録 |
| Formatting | `textDocument/formatting` はcanonicalな4-space whole-document editを返し、不正fileを保持 |

Open bufferはdisk内容より優先します。Importされたopen fileは親moduleの文脈で検証します。Bufferのcloseは公開診断を消去し、権限があれば残る依存rootに対してdisk内容を復帰させます。Closed dependencyのerrorはopen rootのmodule宣言へ付け、対応clientには元のrelated locationも示します。Initializeのrootがなければdiskのsource fileを読みません。外部disk変更でcache済みclosed fileをrefreshするには、clientからfile-change通知を送る必要があります。

Formatterは実token／commentのbyteと意味上のsyntaxを保持し、冪等で、不正sourceを変更しません。Formatting optionは検証しますが、languageのcanonicalな4-space方針は固定です。Embeddingは `lang_syntax::format::format` を直接呼べます。単独の `lang fmt` command、code action、completion、navigation、incremental text edit、DAPは後続tooling作業です。

## Compilerとportable VMを組み込む

公開APIはfrontend semanticsをCraneliftから独立させます。

| API | 用途 |
| --- | --- |
| `lang::frontend::{analyze_file, analyze_virtual, compile_script}` | Source snapshot／診断の保持、typed program、script entry検証、設定可能なcompile limit |
| `lang::repl::Session` | Transactionalなitem定義と、元submission位置を持つexpression評価 |
| `lang_semantics::database::Database` | Parser／module／診断cache、reverse invalidation、immutable snapshot、古いjobのpublication guard |
| `lang_semantics::program_lowering::{lower, lower_library}` | Optionalなscript entryまたはlibrary方針のverified typed unit |
| `lang_bytecode::unit::{encode, decode}` | Canonicalなversion付きschema-1 artifactと共有decoder／verifier budgetでの検証 |
| `lang_vm::unit::{execute, execute_entry}` | Typed value、明示host binding、上限付きguest frame、outcomeとstructured failure |

Schema 1は明示little-endian field／tagを持ち、Rust layoutから独立しています。Decodeは未知version／opcode、noncanonical table、不正UTF-8／span、invalid type／control flow、resource超過を拒否します。Canonical encodingはIDをdeterministicに再配置します。CLI artifact writer／loaderはまだなく、library APIを使います。実行可能なembedding exampleは[source VM test](../../tests/source_vm.rs)、[codec test](../../crates/bytecode/tests/unit_codec.rs)、[database test](../../crates/semantics/tests/database.rs)を参照してください。

Sourceのhost宣言は `extern fn name(value: i64) -> i64;` です。Embeddingは正確な型の `extern "C" fn(i64) -> i64` をregisterし、qualified import名にhost-function IDをbindします。未使用importも含めた全宣言をguest実行前に検証します。Script／REPL CLIは空registryなので、host importを宣言したprogramは実行できません。Registry callbackはtrusted native codeです。Source profileにambient filesystem、network、environment、process accessはありません。

## Resourceを制御し、失敗を調べる

Portable VMはRust再帰を使わず、明示guest frameを使います。既定値は次のとおりです。

| Limit | 既定値 |
| --- | ---: |
| Instruction／terminator fuel | 1,000,000 |
| 実行に入るhost callback | 1,000 |
| 同時guest frame | 256 |
| 全frameのlive guest value cell | 1,000,000 |

```bash
cargo run -- run --fuel 10000 --frames 64 --cells 65536 examples/scripts/answer.lang
cargo run -- repl --fuel 10000 --frames 64
```

Instruction／terminatorは1 fuelです。Guest callを拒否してもcall instructionを課金しますが、入っていないcalleeはfailure stackに含めません。初期frame／cell拒否はfuelを消費しません。Host-call budgetはcallbackに入る直前に課金します。Failureは理由、消費count、fault span、入ったcallを内側から外側へ報告します。Guest失敗前のhost副作用は観測可能なままで、transactionalではありません。

Embeddingはlimit、interruption flag、monotonic deadlineを選べます。同じsafe pointでは期限超過よりinterruptionを優先します。Trusted callbackはpreemptできず、超過は後続guest safe pointで検出します。Cell計数は論理guest value storageを測り、process全体のmemoryやcallback allocationを含みません。

Compiler、cache、formatter、codecの上限はVM fuelとは別です。例としてsource fileあたり4 MiB、nominal depth 64、valueあたり4,096 cell、portable artifactの既定上限64 MiBがあります。Decoderとunit verifierはwork／要求allocationの通算budgetを共有します。REPL value表示は65,536 byte上限です。完全な方針は各APIのlimit型を参照してください。

## Native対応と後続作業

Scalar／CFG source programにはVM／JIT differential testとnative object／source-map conformanceがあります。既存straight-line owned-resource identity fixtureも維持します。Nativeのsource guest call、record／enum操作、その他の拡張unit操作はcode公開前にstructured unsupported-feature errorを返します。新source profile全体の実行はVMが対応します。CLIにAOT build commandはまだありません。

2026-10-17のlocal source-native direct／response-file link／run evidenceはmacOS AArch64です。Windows向けintegration testは存在し、以前のMilestone 0 Windows evidenceも別途記録しています。この完了で新しいWindows source-native CI実行を主張しません。

Source string／float／array、generic、method、closure、再帰data type、参照、source resource生成／destructor、例外、async／task、production package manager、stable ABI、self-hostingはまだありません。Milestone 0のresource／component／concurrency modelはscopeを限定した実験で、これらのsource-language featureではありません。Milestone 2はownershipと広いnative AOT、後続はconcurrency／embedding、完全なtooling、ecosystem互換性、self-hosting、追加frontendを扱います。[Roadmap](roadmap.md)を参照してください。

## Evidenceと情報源の日付

2026-10-17の完了監査は **497 testの成功**、`cargo fmt --all --check`、警告をエラーとするClippy、workspace documentation生成を記録します。Parser／formatter、typed-HIR、portable-decoder／VM targetにはinstrumentationなしの10,000-run smoke evidenceがありますが、coverage-guided／sanitizer検証は主張しません。[README](../../README.ja.md#開発)のcommandで全baselineを再現できます。

| 情報源 | 公開／更新日または確認日 |
| --- | --- |
| [Milestone 1完了監査](roadmap.md#完了監査2026-10-17)、`402a5dc` までの実装commit | 最終更新／確認2026-10-17 JST。監査commit `5e69965` |
| [RFC 0002](rfcs/0002-prototype-control-flow.md)、[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)、[architecture](architecture.md) | 最終更新／確認2026-10-17 JST |
| Microsoft公式[LSP 3.18](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md)と[architecture](architecture.md#diagnosticslspdebugging)のlifecycle document | 公開／更新日の記載がないliving document。2026-10-17 JST確認 |
| [JSON-RPC 2.0](https://www.jsonrpc.org/specification) | 2010-03-26公開、2013-01-04最終更新、2026-10-17 JST確認 |

Language semanticsと数値limitはproject decisionで、対応behaviorはrepository実装／testで検証しています。外部protocolの事実は公式specificationに基づきます。後続milestoneは計画であり、実装済みの保証ではありません。
