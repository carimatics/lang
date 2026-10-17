<!-- translation-of: ../../../.agents/skills/language-design-rfc/SKILL.md; canonical-language: en; informational-only: true -->

# Language Design RFC Skill

[English canonical Skill](../../../.agents/skills/language-design-rfc/SKILL.md)

- Name：`language-design-rfc`
- Description：このrepositoryのlanguage semantics、compiler IR、memory model、concurrency、ABI、runtime、developer toolingに関するRFCをdraftまたはreviewします。User-visibleまたはcompatibility-sensitiveなdesign changeに使用し、承認済みdesignに従う通常の実装では使用しません。

Accepted behaviorとしてhypothesisを提示せず、semantic／implementation tradeoffを明確にした、decision-readyなRFCまたはreviewを作成します。

## Context

Draftまたはreviewの前に次を読みます。

- `README.md`
- `docs/architecture.md`
- `docs/roadmap.md`
- External designを比較する場合は `docs/research.md`
- 提案に影響する既存RFC

新しいRFCは `docs/rfcs/0000-template.md` から開始します。[RFC 0001の実験段階の委任](../rfcs/0001-project-charter.md#実験段階のrfc判断)に従い、ある程度妥当と判断できる設計は個別のユーザー承認なしで受理し、日付、判断理由、未検証の仮定を記録して実装を続けます。未解決の設計は `proposed` を維持します。後から設計変更を実装する前にRFCを明示的に改訂します。

## 必須分析

提案が実際に影響する次の観点を扱います。

- User-facing syntaxとguide-level example
- Static／dynamic semantics
- Type inference、trait、effect、error behavior
- Ownership、allocation、destruction、aliasing、unsafe invariant
- Data-race freedom、cancellation、atomic、scheduling
- Bytecode VMとnative AOTのequivalence
- Embedding、FFI、component ABI、callback、versioning
- Diagnostics、formatter、linter、LSP、DAP、documentation
- Compile time、runtime、memory、binary size、hidden cost
- Security boundary、capability、malformed input、resource limit
- Compatibility、edition、migration、rollout
- Implementation stageとobservable test

Non-goalを明示します。No change、library／tool solution、最も有力なcompeting design、より単純なalternativeを比較します。

## Evidence

Current language、standard、toolに関する主張にはprimary sourceを使用します。Publication／update dateを記録し、Living Documentに公開日がない場合は、その旨とverification dateを記録します。Sourced fact、project choice、unresolved hypothesisを分離します。

Ergonomics、compile time、runtime performance、call overhead、code sizeに関する主張には、小さなprototypeとbenchmarkを使用します。Language popularityからこれらの特性を推論しません。

## Project invariant

- FrontendをLLVM、Cranelift、VM、platform-specific APIから独立させる。
- Lowering全体でsource spanとstable diagnostic identityを保持する。
- Typed MIRをexecution mode間のshared semantic contractとして扱う。
- Native source-language ABIをunstableとして扱い、独立してversion管理されるcomponentはcomponent interfaceを使用する。
- User-authored pluginはtrusted native loadingではなく、capability-restricted bytecodeまたはWebAssemblyを既定とする。
- 最初のfrontendとcomponent conformance suiteが安定するまで、2つ目のsource languageを追加しない。

## Review結果

Reviewの最後に、`accepted under experimental delegation`、`ready for decision`、`needs revision`、`needs prototype/measurement`、`conflicts with an accepted invariant` のいずれかを示します。Reviewで受理できると判断したら委任に基づいて受理し、通常のowner承認待ちにはしません。Blocking questionをoptional follow-upと分けて記載します。
