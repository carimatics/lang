<!-- translation-of: ../../../docs/rfcs/0002-prototype-control-flow.md; canonical-language: en -->

# RFC 0002：Prototype integer／structured control-flow semantics

[English](../../../docs/rfcs/0002-prototype-control-flow.md)

- Status：proposed
- Authors：Project contributors
- Created：2026-10-16
- Last updated：2026-10-16
- Tracking issue：
- Supersedes：
- Superseded by：

## 概要

2つ目のsemantic pathを追加せず、既存のtyped MIRへloweringできる最初のsource-level subsetを定義します。このsubsetは、`i64` parameter／result、符号付き10進literal、wrapping addition、signed less-than comparison、mutable local binding、`if`／`else`、`while`、明示的な `return` を含みます。

```text
fn sum_below(value: i64, cutoff: i64) -> i64 {
    let mut current: i64 = value;
    let mut total: i64 = 0;

    while 0 < current {
        if current < cutoff {
            total = total + current;
        }
        current = current + -1;
    }

    return total;
}
```

`sum_below(5, 3)` のresultは `3` です。Loopは `5`、`4`、`3`、`2`、`1` を訪れますが、`2` と `1` だけを加算します。これは既存MIR、VM、JIT、AOTのprototype workloadと一致します。

## 動機

Repositoryにはすでに、constant、wrapping `i64` addition、signed comparison、明示的branch、back-edge、block parameter、returnを扱うbackend非依存のtyped MIRとbytecodeがあります。これらのoperationはVMとCranelift pathで等価に動作しますが、どのstructured constructがそのMIRを生成するかを定義するsource programはまだありません。

Milestone 0は、prototype subsetのwritten semanticsが完成して初めて終了します。Parserだけを選ぶのでは不十分です。Operator overflow、evaluation order、control-flow joinをまたぐmutation、loopの再評価、return要件はexecution mode間で観測可能だからです。Libraryやmacroは、第2のfrontendにならずにこれらのcore ruleを定義できません。

## 目標

- 既存arithmetic／CFG prototypeへ、1つの正確なsource-level interpretationを与える。
- Deterministicなtyping、evaluation order、overflow、mutation、loop behaviorを定義する。
- Structured control flowをtyped MIR block／block parameterへloweringする間もsource spanを保持する。
- このsubsetの停止するprogramについて、VM、JIT、AOTで等価なresultを要求する。
- Lossless parser、formatter、diagnostics engine、LSPが共有できる小さなgrammarを維持する。

## 非目標

- 一般的なinteger type、implicit numeric conversion、floating point、bit operation、user-defined operator。
- Immutable local declaration、boolean local declaration、`break`、`continue`、loop value、pattern matching、recursion。
- Function call、host call、aggregate value、allocation、ownership、borrowing、destruction、async execution、effect。
- Stable native ABI、component ABI、serialized MIR、bytecode formatの定義。
- 最終的なlanguage nameまたはこの狭いsubset外のsyntaxへのcommitment。

## Guide-level design

Functionは0個以上の `i64` parameterと1つの `i64` resultを宣言します。Parameterはimmutableです。Mutable localには明示的な `i64` typeとinitializerが必要です。

```text
fn minimum(left: i64, right: i64) -> i64 {
    if left < right {
        return left;
    } else {
        return right;
    }
}
```

`+` はtwo's-complement wrapping additionを行います。Panicもundefined behaviorも発生しません。例えば `9223372036854775807 + 1` は `-9223372036854775808` と評価されます。

`<` は2つの `i64` valueをsigned integerとして比較し、内部 `bool` を生成します。`if` または `while` のconditionには `bool` だけを使用できます。Comparison chainingはこのsubsetに含まれません。

`if` は正確に1つのbranchを実行します。省略された `else` はempty blockとして動作します。`while` は各iterationの前にconditionを評価し、conditionがfalseになると直ちに終了します。実行されたbranchまたはloop iteration内のassignmentは、後続statementと次のloop conditionから見えます。

Reachableなすべてのfunction exitは、`i64` valueを伴う明示的な `return` を実行しなければなりません。Function末尾へのfall-throughはcompile-time errorです。Compilerは `while` loopがinfiniteであることを証明する必要はありません。

## Reference semantics

### Syntaxとprecedence

初期grammarは次のとおりです。

```text
function    := "fn" IDENT "(" parameters? ")" "->" "i64" block
parameters  := parameter ("," parameter)*
parameter   := IDENT ":" "i64"
block       := "{" statement* "}"
statement   := "let" "mut" IDENT ":" "i64" "=" expression ";"
             | IDENT "=" expression ";"
             | "if" expression block ("else" block)?
             | "while" expression block
             | "return" expression ";"
expression  := comparison
comparison  := addition ("<" addition)?
addition    := primary ("+" primary)*
primary     := SIGNED_DECIMAL | IDENT | "(" expression ")"
```

Parenthesesが最も強くbindし、`+` はleft-associative、`<` はより低いprecedenceを持ちchainできません。`SIGNED_DECIMAL` はoptionalな先頭 `-` を含みます。一般的なunary negationは含みません。`i64` range外のsigned decimal valueは、parse時にwrapせず拒否します。

### Name resolutionとtyping

- Parameterは外側のfunction scopeに属し、各blockはnested lexical scopeを導入する。
- Localはdeclarationから、それを含むblockの末尾までvisibleである。Nameはuse前にdeclareする必要がある。
- Nestedまたはdisjoint blockを含め、同じfunction内のparameter／local nameをdeclarationで再利用できない。このsubsetはredeclarationとshadowingの両方を拒否する。
- Parameterはimmutableである。Assignmentのleft sideには `let mut` localだけを置ける。
- Literal、parameter、mutable local、addition operand、assignment value、return valueのtypeは `i64` である。
- `<` は2つの `i64` operandを受け取り、typeは `bool` である。
- `if` と `while` のconditionには `bool` typeが必要である。
- このsubsetで `bool` は内部expression typeであり、userはboolean parameter、result、localをまだdeclareできない。

### Evaluationとcontrol flow

- Statementはsource orderで実行する。
- Binary operandはleft-to-rightで評価する。Callやeffectを追加した時点で観測可能になり、その後も変更してはならない。
- Assignmentはright-hand sideを完全に評価してからlocal valueを置き換える。
- `if` conditionは1回評価し、その後、選択されたblockを正確に1つ実行する。
- `while` conditionは、iterationを実行する可能性があるたびに1回評価する。Trueならbodyを実行してconditionへcontrolを戻し、falseなら後続statementへcontrolを移す。
- `return` はexpressionを評価し、直ちにfunctionを終了する。Unconditional return後のstatementはunreachableであり、runtime behaviorを変えずwarningを生成すべきである。
- `i64` additionはmathematical sumの下位64 bitを返し、two's-complement signed `i64` として解釈する。
- Signed comparisonは `i64::MIN` から `i64::MAX` までの数学的な順序を使用する。

Loweringは各assignmentに新しいSSA valueを作成します。`if` joinまたは `while` back-edgeをまたいでliveなvalueは、typed block argumentとしてblock parameterへ渡します。これはcompiler representation ruleであり、source-visibleなaliasingやallocationではありません。

### Ownership、error、concurrency、constant execution

`i64` と内部 `bool` はcopy valueです。Language heap storageをallocateせず、destructorもborrowもなく、aliasing／concurrency behaviorを導入しません。このRFCは `unsafe`、FFI、capability、panic、unwinding、cancellation、recoverable-error boundaryを追加しません。

Unbounded loopは停止しない可能性があります。Bytecode VMは別途仕様化されたinstruction-fuel interface、cooperative interruption flag、絶対monotonic deadlineで停止できますが、native／JIT executionには現在同等のenforcementがありません。Fuel exhaustion、interruption、deadline expirationはembedding／runtime outcomeであり、source `return` が生成するvalueではありません。

初期frontendはsource-level constant evaluationを行いません。Constant evaluationを追加する場合は、同じwrapping addition、signed comparison、control-flow semanticsを使用し、deterministicなresource limitを適用しなければなりません。

## 実行モード

- **Bytecode VM：** Structured control flowは検証済みregister bytecodeへloweringする。すべてのinstruction／terminatorは既存のfuel costを維持し、register countにはoptional value-slot limitを引き続き適用し、optionalな共有interruption flagまたは絶対monotonic deadlineをVM safe pointで観測する。
- **Native AOT：** 同じ検証済みMIRをnative object codeへloweringする。Wrapping additionとsigned comparisonは、すべての `i64` inputについてVMと一致しなければならない。
- **Optional JIT：** AOTと同じnative lowering contractを使用し、同じdifferential input tableを通過しなければならない。
- **Embedding：** Argumentとresultは現在の型付き `i64` prototype boundaryを使用する。このRFCはhost callまたはstable ABIを追加しない。
- **WebAssembly／component：** ここではmappingを定義しない。後続component RFCが同じfunctionを公開する場合、これらのobservable resultを維持しなければならない。

Backend optimizationは、effect導入後のstatement order、wrapping result、branch selection、source programが許すtermination behaviorを維持する場合に限り、blockの再構成やredundant workの削除を行えます。

## ABIとcompatibility

Syntaxとsemanticsはproposedのままであり、このRFCが明示的にacceptされるまでcompatibility guaranteeを持ちません。Accept後にwrapping arithmetic、condition typing、evaluation order、loop／assignment behaviorを変更する場合は、repositoryのlanguage-compatibility processが必要です。Edition導入後にsourceの意味を変える場合は、editionまたはmigration planも必要です。

MIR ID、block layout、bytecode register、native calling convention、object symbol、debug encodingは、implementation detailまたは個別にversion管理されるformatのままです。このRFCはそれらをstable interfaceにしません。

## Diagnosticsとtooling

最初の実装は、次のconditionにstable diagnostic identityを割り当てます。

| Proposed code | Condition | 必須primary span |
| --- | --- | --- |
| `SRC0001` | Decimal literalが `i64` range外 | Literal |
| `SRC0002` | Nameがunknownまたはdeclaration前にuseされた | Offending name |
| `SRC0003` | Declarationがparameterまたはlocal nameを再利用した | New declarationとoriginal declaration note |
| `SRC0004` | Assignment targetがimmutable parameter | Assignment targetとparameter declaration note |
| `SRC0005` | `+`、assignment、`return` が非 `i64` valueを受け取った | Offending expression |
| `SRC0006` | `if` または `while` conditionが `bool` ではない | Condition expression |
| `SRC0007` | Reachableなfunction exitに `return` がない | Closing braceとfunction signature note |

Lossless parserはcomment、whitespace、delimiter、error tokenを保持します。Recoveryはsemicolonとclosing braceでsynchronizeし、functionの残りを破棄しません。Formatterは上記のbrace／semicolon styleを出力し、idempotentです。LSP diagnosticsはparser／type-checker resultを再利用し、不完全なblockでも利用可能なままにします。Debuggerは、assignmentが異なるSSA valueへloweringされてもsource local nameを表示すべきです。利用できないoptimized valueは正直にlabelします。

これらのdiagnostic codeはproposalであり、このRFCがacceptされ、対応behaviorが実装されて初めてstableになります。

## Performanceとcost model

このsubsetはlanguage-level allocation、reference counting、dynamic dispatch、overflow branchを必要としません。Mutable localはsource abstractionであり、SSA valueまたはregisterに残せます。Userはstack slotやmemory addressへ依存できません。`while` conditionは各iteration試行ごとに1回実行され、実行された `+` と `<` はbackend optimization前にfixed-width integer costを持ちます。

既存countdown、2-value accumulator、conditional-sum benchmarkは、対応するMIR shapeについてlowering、compilation、code size、VM、JIT、AOTのevidenceを提供します。Cleanなschema 12 macOS AArch64 resultでは、通常／unset interruption／期限前deadline付きVM countdown medianが1.606 us／1.603 us／3.314 usでした。Interruption deltaは3.424 ns（0.213%）の減少で両rowの10.380%／0.263% RSDより小さく、deadline deltaは1.708 us（106.379%）です。これらのmicrobenchmarkはoverhead budgetや、将来のparser、type checker、optimizer、代表的applicationのperformance guaranteeを確立するものではありません。

## Securityとtrust boundary

Wrapping additionはすべてのinputにdefined behaviorを持ち、native signed-overflow undefined behaviorを発生させません。Sourceは引き続きuntrustedです。Parser、resolver、type checker、MIR validator、bytecode verifierはpanicせずmalformed structureを拒否しなければなりません。非常に深くnestしたsourceや巨大functionには、public exposure前に明示的なimplementation limitが必要です。

`while` はnon-terminationによるdenial of serviceを可能にします。VMのopt-in fuel／value-slot／cooperative-interruption／absolute-deadline controlは部分的なcontainmentを提供しますが、heap use、callback内部のpreemption、native／JIT execution、compiler resource exhaustionを扱いません。このRFCはplugin、filesystem、network、process、FFI、host capabilityを拡張しません。

## 代替案

### 変更しない

MIRを直接構築し続け、source semanticsを延期します。Implementation freedomは維持できますが、Milestone 0のwritten-semantics exit criterionを満たせず、backend-equivalence testがuserの記述できるlanguageから切り離されたままです。

### Libraryまたはtoolingによる解決

Loopとarithmeticをmacro、builder API、standard-library callでmodel化します。この層は、別のsource languageとして動作せずにparser recovery、static typing、operator overflow、evaluation order、backend-equivalent loweringを独自定義できないため、問題を解決しません。

### Expression-oriented control flowとchecked arithmetic

`if` とloopをvalue-producing expressionにし、immutable rebindingを優先し、`break value` をsupportし、overflow時に `+` をtrapさせるかerrorを返します。Compositionalityを改善し、意図しないoverflowを可視化できるため、これは最も有力なcompeting designです。一方でpanic／error semantics、joinとnon-local exitのより複雑なtyping、現在のprototypeで検証していないruntime checkが必要です。このvertical sliceではなく、後続の一般RFC候補として残します。

### Forward-only control flow

`if` と `return` をsupportしますが、`while` とassignmentを省きます。より小さい一方、CFG prototypeを動機付けたtest済みback-edge、loop-carried value、fuel behavior、代表的なbounded workを表現できません。

## 実装計画

1. 正確なgrammarのためのlossless parserとsyntax nodeを追加し、valid、incomplete、recovered inputのsnapshotを用意する。
2. Nameをresolveし、source spanとproposed diagnostic identityを保持しながら、`i64`、内部 `bool`、mutation、condition、明示的returnをtype-checkする。
3. Function、`if`、`while` を既存typed MIRへloweringする。Joinとback-edgeをまたぐassigned valueをblock argument／parameterで表現する。
4. 既存MIR-to-bytecode／MIR-to-Cranelift pathを再利用し、どちらのbackendにもsource固有behaviorを追加しない。
5. Formatter round-trip／idempotence test、不完全program向けLSP diagnostics、VM／native debugging向けsource mapを追加する。
6. Frontend performanceを主張する前に、parsing、analysis、loweringを個別に測定する。

## テスト計画

- Precedence、negative literal、nested block、missing semicolon／brace、後続statementへのrecoveryを扱うparser snapshot。
- Add、minimum、countdown、2-value accumulation、conditional sum、zero iteration、nested `if`、joinをまたいで保持されるassignmentのcompile-pass test。
- Proposed diagnostic codeごとのcompile-fail test。Primary／secondary spanとstable machine-readable fieldを検証する。
- Signed comparison、wrapping addition、branch target、back-edge、typed block parameterを証明するMIR snapshot。
- Zero、negative value、`i64::MIN`、`i64::MAX`、wrapping overflow、両conditional path、bounded loopを含むVM／JIT differential table。
- Representativeなsource-generated objectについて、macOS AArch64とWindows x86-64のAOT link-and-run coverage。
- Non-terminating source loopが、次のguard対象side effect前のVM safe pointで停止することを証明するfuel／interruption／deadline test。
- Formatterのparse／format／parse equivalenceとidempotence test。
- Untrusted sourceをacceptする前のparser／verifier fuzzingと明示的なsize／depth-limit test。

## 未解決事項

- Immutable `let` bindingとuser-visibleな `bool` localを、この最初のfrontend sliceまたはfollow-upのどちらに含めるべきか。
- Wrapping `+` を維持しながら、後続editionでどのchecked-arithmetic syntaxを追加すべきか。
- `break`、`continue`、value-producing loop expressionを後続control-flow RFCでまとめてdesignすべきか。

レビュー結果：この狭いprototype subsetについて **ready for decision**。対応するMIR shapeにはVM／JIT differential、AOT、benchmark evidenceがすでにあるため、追加のprototype blockerはありません。Repository ownerによるacceptanceは引き続き必要です。Immutable binding、user-visible boolean storage、checked-arithmetic spelling、loop expressionはoptional follow-upです。

## 情報源

- `docs/rfcs/0001-project-charter.md`、project charter／compatibility policy、最終更新 2026-10-16 12:50 JST。
- `docs/architecture.md`、compiler／execution architecture proposal、最終更新 2026-10-16 12:50 JST。
- `docs/roadmap.md`、Milestone 0 exit criteria／current evidence、最終更新 2026-10-16 12:50 JST。
- `crates/mir/src/lib.rs`、typed CFG／validation prototype、最終更新 2026-10-16 01:25 JST。
- `crates/bytecode/src/lib.rs`、verified register bytecode prototype、最終更新 2026-10-16 01:25 JST。
- `crates/vm/src/lib.rs`、VM execution／resource-control prototype、最終更新 2026-10-16 11:20 JST。
- `crates/codegen-clif/src/lib.rs`、Cranelift lowering prototype、最終更新 2026-10-16 01:36 JST。
- `src/lib.rs`、differential prototype workload、最終更新 2026-10-16 11:42 JST。
- `src/bin/prototype-bench.rs`、schema 13 control／allocation／heap／process-memory harness、最終更新 2026-10-16 12:50 JST。
- `benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json`、clean schema 12 macOS AArch64 evidence、2026-10-16 12:24:45 JSTに計測。

このRFCは外部の事実に関する主張を追加しません。Design proposalはrepositoryの実行可能prototypeとproject decisionから導出しています。
