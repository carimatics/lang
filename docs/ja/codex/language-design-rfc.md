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

新しいRFCは `docs/rfcs/0000-template.md` から開始します。Repository ownerが明示的にacceptするまでstatusを `proposed` のままにします。

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

Reviewの最後に、`ready for decision`、`needs revision`、`needs prototype/measurement`、`conflicts with an accepted invariant` のいずれかを示します。Blocking questionをoptional follow-upと分けて記載します。
