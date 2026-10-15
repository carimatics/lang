<!-- translation-of: ../../../docs/rfcs/0000-template.md; canonical-language: en -->

# RFC 0000：タイトル

[English](../../../docs/rfcs/0000-template.md)

- Status：proposed
- Authors：
- Created：YYYY-MM-DD
- Last updated：YYYY-MM-DD
- Tracking issue：
- Supersedes：
- Superseded by：

## 概要

User-visible changeと、最小限で有用なexampleを記述します。

## 動機

どのような具体的問題があるかを示します。代表的なcode、measurement、compatibility evidenceを含め、libraryまたはtooling changeでは不十分な理由を説明します。

## 目標

- 目標

## 非目標

- 非目標

## Guide-level design

Diagnosticsとよくある間違いを含め、userがfeatureをどのように利用するか説明します。

## Reference semantics

次を仕様化します。

- Syntaxとname resolution
- Type checkingとinference
- Evaluation orderとside effect
- Ownership、borrowing、destruction、allocation
- Error、panic、unwinding、cancellation
- Concurrencyとmemory-modelへの影響
- `unsafe`、FFI、capability boundary
- Constant／compile-time executionでの動作

## 実行モード

次のモードにおける等価な動作と、正当化された制限を説明します。

- Bytecode VM
- Native AOT
- Optional JIT
- Embedded host callとcallback
- 該当する場合はWebAssembly／component execution

## ABIとcompatibility

Source compatibility、native ABI、component ABI、bytecode、serialized IR、package metadata、edition、migration toolingを扱います。

## Diagnosticsとtooling

Parser recovery、formatter behavior、error code、fix-it、LSP behavior、debugger display、documentation、lint ruleを記述します。

## Performanceとcost model

Allocation、reference counting、copy、dynamic dispatch、code size、compile time、runtime check、optimization opportunityを特定します。Userが依存できる保証とimplementation detailを区別します。

## Securityとtrust boundary

Untrusted source、macro、bytecode、plugin、manifest、package input、capability、resource exhaustion、denial of serviceを検討します。

## 代替案

少なくとも次を比較します。

- 変更しない
- Library／toolingによる解決
- 最も有力な競合言語設計
- 保証を減らした単純な設計

## 実装計画

実装をテスト可能なvertical sliceに分割します。影響を受けるcompiler stage、runtime、standard library、LSP、DAP、formatter、linter、package manager、documentationを特定します。

## テスト計画

必要に応じてcompile-pass／fail case、VM／native differential test、ABI fixture、fuzzing、benchmark、platform coverage、migration testを含めます。

## 未解決事項

- 質問

## 情報源

Primary sourceと公開／更新日を記載します。公開日がないLiving Documentationでは、確認日を記録します。

