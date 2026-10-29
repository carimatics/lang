<!-- translation-of: ../../rfcs/0038-package-api-documentation.md -->

# RFC 0038: package API 文書

[English](../../rfcs/0038-package-api-documentation.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B04
- Supersedes: none
- Superseded by: none

## 概要

`lang doc` は選択 package の検査済み frontend program から、単独で offline 閲覧できる HTML API reference を生成する。public type／function の元 signature、隣接 `///` text、解決済み type link、元 source 位置を保持する。既存の失敗時保護を備えた generation／lock protocol で `target/lang/<lock-digest>/doc/index.html` を公開する。

## 動機

package 利用者には、guest code を実行せず、生成 page の横に source file を保持せず調べられる API が必要。text 抽出だけでは dependency type や同名宣言の解決済み type identity を失う。

## 目標

- 選択 package の load 済み dependency／module graph に含まれる検査済み public function、record、enum case を文書化する。
- 入れ子の constructed type を含め、signature／field／payload の nominal type を解決済み definition ID で link する。
- 元 path、byte range、Unicode source text、決定的な offline output を保持する。
- committed lock／generation の置換前に不正 input／output 失敗を拒否する。

## 対象外

- Markdown、comment 内 link 構文、doctest 実行、検索 service、JavaScript。
- semantic lint の強制、private-item API 一覧、自動 browser 起動。
- 安定した serialized API metadata schema や compiler build 間で安定した fragment ID。

## ガイドレベル設計

`lang doc` は既存の共通 manifest／member／target／locked／offline／message-format option を受け付ける。通常の選択 package を解析し、実行可能 `main` は不要で、宣言 test や host call は実行しない。宣言 test は固定 lock input に含む。成功時は HTML artifact path、または既存 schema-1 artifact JSON record を出力する。解析／output 失敗は exit 1、不正 option は exit 2。

HTML に title／navigation、public type／function section、元 signature／宣言 text、隣接 line 文書、解決済み参照 type link、source link を持たせる。source panel は private 実装 text も含む元の load 済み source file を埋め込むため、この source 公開を意図する場合だけ生成文書を配布する。元 file を削除しても文書は使用できる。

## 参照セマンティクス

通常の不変 package frontend 結果を再利用する。既存 source 診断と warning を保持し、解析 error が一つでもあれば公開しない。検査済み public bit が true の definition／function のみを含む。module 名を navigation context とし、public field／case data は既存 type 可視性契約を使う。

function signature は元 signature span、record／enum 宣言は元 span から複写する。直前に連続する `///` line を plain text として集め、marker と任意の空白一つを除く。他の comment と空行で group を終える。comment 内容を markup／link 構文として再解釈しない。

解決済み type 参照は `Type::Nominal`、入れ子の `TypeConstructor::arguments()`、component-resource nominal tag を訪問する。文書化した public definition のみ link し、builtin／private definition に link を捏造しない。綴りではなく typed ID で解決する。function parameter／result、record field、enum payload を参照 root とし、item ごとに target を重複除去する。自己／再帰 type 参照で宣言を再帰展開しない。

表示 item は qualified name／kind、load 済み source panel は元 path で並べる。この決定的 inventory で文書内 numeric fragment ID を割り当て、global source ID に依存しない。全 internal link に出力先を持たせる。source link は元 path と UTF-8 start／end byte offset を含み、全 slice を SourceMap で検証する。path は label で、ambient file URL にしない。

## 実行モード

VM／native／task program 共通の frontend semantics を使う。実行形式への lowering、SDK／linker、Cargo、network、guest 実行、host callback は不要。runtime、ownership、concurrency、cleanup の挙動は不変。

## ABI と互換性

command と `doc` output generation の追加。source／ABI／bytecode／lock schema は不変。HTML は人が読む exact-build artifact で、安定 metadata wire format ではない。internal anchor 番号は compiler／input graph で変わり得るが、一つの生成 file 内の link は完結する。edition migration は不要。

## 診断とツール

CLI／source／package 診断 code と schema 1 を再利用し、render 上限は `PKG0004`、filesystem 失敗は `PKG0005`、古い lock／cache 不一致は `PKG0007`。公開前に元の状態を保持する。formatter／linter／LSP／DAP は不変。

## 性能とコストモデル

escape 済み source text を含む生成 HTML を、公開前に 64 MiB で制限する。既存の集約 output 上限には generation lock bytes も含む。renderer work は 4,000,000 step に制限し、item、type 参照、source／escape 訪問を課金する。constructed type graph は item ごとの visited set で走査する。時間／memory benchmark は主張しない。

## セキュリティと信頼境界

source／comment／path は信頼しない text として HTML 特殊文字を全て escape し、numeric internal fragment を使い、source 由来 URL／script／event attribute／CSS を埋め込まない。固定 inline style のみ許す制限 CSP 付き static 文書を出力する。capability 相対 output staging、input identity guard、private file、cache bytes 検証、lock 最終 commit を再利用する。render／source 失敗と cache output 不一致は前回の committed output／lock を保持する。network／browser 権限は与えない。

## 代替案

- 変更なしでは package API 文書を得られない。
- 独立 source scanner は frontend が解決した dependency／type identity を保持できない。
- 外部文書 engine は、この delivery に不要な markup／plugin 実行境界を追加する。
- signature のみの text 出力は小さいが、解決済み navigation と単独で使える source 位置を満たさない。

## 実装計画

1. 検査済み frontend program／source map から上限付き static HTML renderer を作る。
2. doc command routing と既存 staged generation 公開を追加する。
3. dependency／type／source／escape／配置変更／不正 output fixture、baseline、両 host CI を実行する。実 scope が変わらなければ B04 の review 済み 3 pt を保つ。

## テスト計画

実 CLI による root／dependency type／function、入れ子 type link、同名宣言、元 Unicode comment／byte 位置、private API 一覧への非掲載を検証する。全 fragment link を独立に parse し、対応する一意 ID を確認する。HTML injection text、配置変更時の決定性、source 削除、main／guest 実行不要、不正 source／古い lock、壊れた cache output で lock／前回 file を保持することを検査する。render byte／work 上限の丁度と一つ下を実行する。既存 command／repository 検査、全 baseline、両 host CI を実行する。

## 未解決事項

RFC 0001 の実験委譲で 2026-10-29 JST に採択。証明済み frontend identity に基づく上限付き static artifact は、新しい実行／serialized-metadata 境界なしで B04 を満たす。plain line 文書と exact-build fragment は実験的 product 判断。rich markup、安定 API metadata、private-item 制御は後の設計を要する。

## 出典

- RFC 0001（更新 2026-10-28）、RFC 0034（2026-10-29）、現在の検査済み HIR／SourceMap／output API を 2026-10-29 JST に確認。実装 file に埋込み公開／更新日はない。上記 limit／output 方針は project 判断で、外部比較や測定事実ではない。
