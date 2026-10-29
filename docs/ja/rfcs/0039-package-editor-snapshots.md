<!-- translation-of: ../../rfcs/0039-package-editor-snapshots.md -->

# RFC 0039: package editor snapshot と検証付き edit

[English](../../rfcs/0039-package-editor-snapshots.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-C01–C07
- Supersedes: none
- Superseded by: none

## 概要

既存 UTF-16 LSP server に package 対応 navigation、completion、semantic token、検証付き rename／code action を加える。semantic answer は不変の許可 package／input／overlay epoch と元 source span に対応する。edit は client 向け提案で、filesystem に直接書き込まない。

## 動機

既存 server は明示 source root 一つの revision-safe 診断と CST formatting を持つが、package namespace navigation と安全な semantic edit はない。package frontend は dependency identity、privacy、typed local／field fact を解決済みであり、それを使えば第 2 の language resolver を避けられる。

## 目標

- 広い editor 基盤より先に、実宣言 package dependency へ navigation する。
- source 変更、open overlay、watched-file refresh を通じて definition／reference identity を保持する。
- scope／privacy／type 対応 completion と source に対応する semantic token を提供する。
- 観測した snapshot 全体と client 文書 version に対して rename／fix 提案を検証する。

## 対象外

- 新しい source semantics、serialized IR field、compiler／runtime ABI、guest 実行。
- 独立 workspace folder 複数、registry download、任意 filesystem read。
- 直接 source write、file resource 操作、rich snippet edit、外部 writer 全体への atomic 保証。
- 不正／未許可 source について有効な semantic binding を捏造すること。

## ガイドレベル設計

明示 local workspace root 一つと既存 full-document synchronization を保持する。root に `Lang.json` があれば package mode は共通の選択 root／member／target 契約を使い、明示 initialization option で相対 `manifestPath`、`package`、`target` を選べる。manifest と package 許可 path は全て client 許可 workspace 内に保ち、祖先検索や権限拡張はしない。選択／既定 manifest がなければ既存 standalone 挙動を使える。

open source overlay は許可 file の bytes を memory 内で置換し、既存 module／dependency 境界を正とする。新しい disk module は refresh／admission で入る。未許可の unsaved file には unavailable を返し、filesystem 権限を与えない。dirty manifest overlay は有効 metadata の save／refresh まで package semantic answer を無効化する。未選択 workspace member を暗黙に active package にしない。

definition／references、completion、semantic-token capability は実装が入った時点でのみ広告する。未対応／不正／古い request は既存 JSON-RPC error 処理を保つ。通常 Location は LocationLink capability を要さず、position／range は既存 UTF-16 を使う。

protocol 境界では LSP に従い、行長を超す character offset をその行末へ clamp する。存在しない行や UTF-16 surrogate 内部は拒否する。内部 byte-offset API は引き続き範囲内の UTF-8 文字境界を要求する。

## 参照セマンティクス

### input epoch

snapshot は固定 manifest／source bytes、SourceMap、検査済み program、module／name graph、open 文書 version、単調に変わる epoch を所有する。package capture と既存 compiler quota に加え、open overlay bytes／count、indexed occurrence、query work、符号化 response bytes を制限する。input key は canonical な許可 identity で、alias から同一 identity に相反する overlay text を入れない。

C01 の既定上限は overlay 256 件／16 MiB、symbol 65,536 件、occurrence 262,144 件、index が所有する name／path の論理 bytes 64 MiB、index／resolution step 10,000,000 件。byte 上限は count 上限を補い、厳密な heap allocation 測定ではない。protocol response 上限は統合時にも必要。

未使用 input を含め、許可 manifest／source bytes 全てを epoch が保持する。manifest と物理 identity を共有する source input は overlay 前に拒否し、editor buffer で package metadata を source として再解釈させない。closed 診断 URI は所見がある間だけ保持し、所見が消えたとき空 publication を一度送る。

文書 open／change／close と該当 watched-file 変更で依存 semantic answer を無効化する。open overlay は disk refresh より優先する。一つの epoch で計算し、現在 epoch と関連文書 version が一致する場合のみ公開する。失敗時は semantic navigation／edit fact を無効化し、parser 診断や lexical token fallback は保持できる。古い成功 program を変更済み input の結果として返さない。

### symbol identity

symbol は snapshot 内の宣言 identity と kind、元宣言名 span で表す。item／case は ModuleGraph、local／parameter／pattern と field target は解決済み／typed HIR を使う。use はその正確な宣言と元 name token span を指す。ID は安定 serialized metadata ではない。

local binding を module-name lookup より優先する。comment、field 名、別 scope の同名を text 検索で分類しない。global path は既存 resolver の privacy／alias 規則と共有の有限 resolution budget を使う。不明／曖昧／未証明 binding に推測 target を返さない。物理 occurrence を重複除去し、元 path／byte range で整列し、検証済み UTF-8 span から UTF-16 への変換は protocol 境界だけで行う。

### query

definition は宣言名 span へ移動し、references は任意で宣言も含める。completion は lexical scope と検査済み receiver type を使い、privacy を適用し、plain label／text edit の上限付き決定的 list を返す。不正／未完成 source には scope／identity が証明された fact だけを使い、非公開 member を捏造しない。

semantic token legend は namespace、type、enum member、function、parameter、variable、property、keyword、comment、string、number、operator に固定する。複数行 token は CR／LF を除いて行境界で分割し、整列・非重複の UTF-16 length と検証済み delta 符号を出力する。初期は全文 token で、未対応 delta／range capability を広告しない。構文 error 時は semantic identity を捏造せず lexical 分類を使える。

### 検証付き edit 提案

prepare／rename は証明済み source item、case、field、local binding を扱い、名前は keyword 以外の合法 identifier とする。未解決／core／外部宣言、重なる edit、衝突、未対応 alias／module／file rename は拒否する。正確な宣言／use 集合への置換を作り、memory 内複写に適用し、再解析して byte offset 対応付けの下で binding の対応を比較する。意図した symbol の名前だけを変え、他の binding、type／privacy／ownership／effect fact の意味を保持する。型検査成功だけでは意図しない capture がないことを証明できない。

返却前に関連 closed input を refresh／検証し、open version／epoch を再確認する。LSP null version は client の stale-edit guard を持たないため、編集対象 file は全て open で具体的 version を持つ必要がある。複数 file の text edit は `workspaceEdit.documentChanges` と `failureHandling` の `transactional` または `textOnlyTransactional` を要求する。version 付き非重複 TextDocumentEdit を返し、条件が足りなければ不足理由を示して部分 edit を返さない。atomicity は広告された client 契約内で client 適用が提供する。server の直接 write、resource 操作、提案後の任意変更に対する保証はない。

初期 code action は明示 document formatting と、検証 edit 証明に成功した場合だけの未読 binding への `_` prefix。formatter と未読 binding rule を再利用し、initializer の effect／ownership を保持し、必要なら証明済み代入 use も全て rename する。unsafe 挿入、allocator 変更、失敗の握り潰し、effect のある initializer 削除は自動化しない。

## 実行モード

editor 解析は VM／native／task 共通 frontend 契約を使う。bytecode 実行、native link、SDK compile、host callback、Cargo process は不要。runtime／concurrency／memory 挙動は不変。

## ABI と互換性

実験的 tooling API と標準 LSP method の追加のみ。source syntax、ABI、lock／bytecode／serialized IR schema は不変。symbol sidecar と epoch は memory 内の exact-analysis data に保つ。既存 standalone 診断／formatting と既定 UTF-16 同期を保持し、artifact migration は不要。

## 診断とツール

元 source 診断 identity と安定 package／CLI error を保つ。不正 parameter、snapshot 外 position、UTF-16 surrogate 内部、stale request、resource 拒否を構造化 protocol error にする。authority 外の ambient path を message に漏らさない。符号化 location／completion／token／edit を response 返却前に制限し、部分成功 prefix を返さない。

## 性能とコストモデル

上限付き不変 source／CST／HIR を再利用し、選択 package の load 済み graph のみ集める。node／token 訪問、resolver step、type／scope 走査、response bytes を課金する。高コストな rename は提案前に関連 semantic graph 全体を検証する。epoch ごとに cache し、古い計算結果を破棄する。prototype から latency、incremental 性能、memory benchmark は推論しない。

## セキュリティと信頼境界

initialize は明示 local root のみ許可する。package resolution／canonical identity guard と open-overlay admission が全 I/O を制約する。URI／position／version を access 前に検証する。request から network／process／runtime capability を与えない。rename／code action は明示 client 適用を要する version 付き提案で、atomicity は上記範囲に限る。

## 代替案

- 変更なしでは package navigation と安全な semantic edit がない。
- text／name 一致は小さいが scope、field、comment、dependency privacy を混同する。
- editor 専用 resolver は language semantics を重複し、実行系から乖離し得る。
- filesystem 直接 edit や version なし closed-file edit は選択した stale／transactional 契約を満たせない。
- incremental compiler 再構成は将来 cost を下げ得るが、上限付き snapshot sidecar ならそれを前提にせず最初の実 navigation fixture を得られる。

## 実装計画

C01 で本契約を採択し、許可済み不変 package snapshot と実 dependency definition navigation を追加する。C02 は definition／references、overlay、watched refresh を接続する。C03 は scope／type 対応 completion、C04 は semantic token。C05 は rename と client edit 保証、C06 は二つの検証 code action。C07 は protocol／package／resource／lifecycle 統合 fixture を実行する。実装 doing は一つに保ち、実 task／見積の増大は code 前に監査する。

## テスト計画

実 sibling package function／type、import／alias／privacy、local／global shadowing、field／pattern identity、UTF-8／UTF-16／CRLF 境界、不正 source、open／change／close、closed-file refresh、stale 公開を検証する。rename は有効な複数 file 変更、capture／衝突、version／transactional capability 不足、古い input、不正名、拒否時 file 保持を含める。token 順、response 上限、end-to-end wire framing を検査する。両 host CI と必須 baseline を実行し、benchmark／fuzz campaign は主張しない。

## 未解決事項

RFC 0001 の実験委譲で 2026-10-29 JST に採択。既存 resolver／HIR identity と明示 snapshot／edit 境界は合理的に妥当。有限 prototype は `helper::answer` の bytes 41..47 から依存宣言 7..13 を解決し、provider 変更後も元 bytes を保持し、同名 local を解決済み HIR で除外した。全 overlay／LSP／edit 統合や cost の証明ではなく、C01–C07 で実 fixture が必要。open-file／transactional client 要件は実験的 product tradeoff。

## 出典

- [公式 LSP 3.18 仕様](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)。publisher が current と表示。公開／更新日未掲載、2026-10-29 JST 確認。UTF-16 交渉、Location、TextDocumentEdit version、transactional／textOnlyTransactional 失敗処理は protocol の事実。
- RFC 0001（更新 2026-10-28）、RFC 0034／0036／0037（2026-10-29）、現在の module／binding／HIR／database／LSP 実装と ignored resolver prototype を 2026-10-29 JST に確認。code に埋込み公開／更新日はない。snapshot 上限、初期 action、より厳しい edit 前提は project 判断。
