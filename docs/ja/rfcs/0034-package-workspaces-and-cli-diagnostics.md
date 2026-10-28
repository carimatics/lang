<!-- translation-of: ../../rfcs/0034-package-workspaces-and-cli-diagnostics.md -->

# RFC 0034: パッケージワークスペースと CLI 診断

[English](../../rfcs/0034-package-workspaces-and-cli-diagnostics.md)

- Status: accepted
- Authors: project owner, Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: [作業ボード](../milestone-4.md)の M4 G1–G4
- Supersedes: none
- Superseded by: none

## 概要

明示的にバージョンを付けた `Lang.json` パッケージ／ワークスペースマニフェスト、ローカル依存解決、`Lang.lock`、改行区切り JSON の CLI 診断を導入する。これはホストのツールチェーン契約であり、ソース式、型付き MIR、実行時 capability、ABI は変更しない。

2026-10-29 JST、RFC 0001 の実験的委任に基づいて accepted とした。完全一致のローカルバージョン、明示的なファイルシステム権限、不明な形式の拒否は、最初の再現可能なパッケージプロファイルとして実装可能である。レジストリ転送、依存名前空間の統合、両ホストでの再現性、ツールの使いやすさは、対応する実行証拠が得られるまで未検証とする。採択は実装完了や安定化を意味しない。

## 動機

既存ソース CLI はルートファイルをコンパイルできるが、パッケージの識別、許可された依存グラフの解決、検証済みソーススナップショットの再実行はできない。M4 では editor や installer の基盤を広げる前に、作成・依存・build・run・test の fixture が必要である。M5 のレジストリを暗黙のパッケージ権限源にはしない。

## 目標

- 決定的なローカルパッケージグラフと、完全一致のオフライン lock 再実行。
- バージョン付きで上限のある厳密な manifest／lock パーサーと安定した診断 envelope。
- 既存ソースコンパイラ、VM／native パイプライン、ソース診断の再利用。

## 非目標

- レジストリ／Git／HTTP 依存、バージョン範囲、build script、macro、feature 統合、自動ダウンロード。
- ソース言語 edition、native／component ABI、既存 artifact 形式の変更。
- installer 署名、editor protocol、profiling 契約。これらは別の M4 設計を必要とする。

## 利用者向け設計

`Lang.json` は UTF-8 JSON とし、コメントと重複キーを認めない。パッケージの例:

```json
{
  "schema": 1,
  "package": {"name": "hello", "version": "0.1.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {"helper": {"path": "deps/helper", "version": "0.1.0"}},
  "tests": ["tests/smoke.lang"]
}
```

workspace root は `"workspace": {"members": ["app", "helper"]}` を持ち、同時に package を持ってもよい。manifest は package または空でない workspace を宣言する。dependencies／tests は package を必要とする。パス区切りはホストによらず `/` とする。entry／test／member は空でない相対パスとし、空、`.`、`..` の要素を認めない。dependency path だけは workspace 内の兄弟を指す `..` を認めるが、resolver は読取り前に canonical な権限境界を検証する。絶対パス、バックスラッシュ、コロン、NUL を拒否する。

package 名と dependency alias は `[a-z][a-z0-9_]{0,63}` に一致する。version は十進 u32 要素三つに限り、先頭ゼロ、prerelease、build metadata を認めない。edition は `2026` のみとする。library root も binary と同じ `entry` を使用し、package orchestration がコンパイル entry を選ぶ。既存の `mod`／`use`、可視性、所有権規則を適用する。dependency alias は宣言された直接依存だけを公開する。同じ推移依存 alias を複数の依存が使用する場合も同様である。名前空間統合は元のソースバイト／span を保持し、名前衝突を診断し、未宣言 import を防ぐ。

dependency の省略可能な `targets` は、完全一致の target triple `aarch64-apple-darwin`、`x86_64-pc-windows-msvc` の空でないリストとする。省略時は両方を含み、重複と未知の triple はエラーとする。解決は対応する target を明示的に一つ選び、既定値は host とする。target 変更は lock replay を無効化する。非アクティブな依存 edge を除外する前に全宣言を検証する。

予定するコマンドは `lang new DIR`、`lang build`、package 対応の `lang run`、`lang test`、`lang package`、`lang fmt`、`lang lint`、`lang doc`。適用可能な package コマンドは `--manifest-path`、`--target`、`--offline`、`--locked`、`--message-format human|json` を受け付ける。最初の検査操作 `lang package check-manifest FILE` は依存 I/O を許可しない。既存のファイル明示 CLI は従来の挙動を維持する。

## 厳密な意味論

manifest schema 1 は必須 field の欠落、未知 field、全 object 階層の重複キー、未対応 schema／edition、不正な name／version／path／condition、末尾の余剰入力を拒否する。package identity の既定値や dependency path の推測は行わない。dependencies／tests は省略時に空、workspace は省略時に存在しない。省略可能な field も存在する場合は宣言された型を必要とし、明示的 null は省略とは扱わない。manifest は 64 KiB、dependency／member／test リストはそれぞれ 128 件までとする。JSON のネストには serde_json の再帰上限を維持する。

マージ前の入力受入改訂、2026-10-29 JST に実験的委任で採択: file 検査は通常ファイルだけを認め、directory、device、FIFO を `PKG0005` で拒否する。Unix は nonblocking で開いてから開いた handle の metadata を検査し、writer のない FIFO が受入検査を止めないようにする。既存 native SDK reader の実装パターンを使う。上限付き CLI 実験で従来の blocking open を再現した。stream 入力はこの profile の対象ではなく、manifest bytes／version を変えず file 受入を明確化する。増大する通常ファイルにも byte 上限と sentinel を維持する。

解決は明示的な manifest から開始し、そのディレクトリを権限 root とする。明示的に選んだ包含 workspace は兄弟 member を許可できる。canonical な dependency root、entry、source file はその root 内に制限し、symlink による逸脱を拒否する。ローカル dependency identity は canonical root と宣言された name／version からなる。schema 1 では alias と依存 package 名の一致を必要とする。alias 順に edge を解決し、循環、内容が矛盾する重複 identity、完全一致 version の不一致、source 欠落を拒否する。上限は package 256、module 256、source 合計 16 MiB、edge 4096 とし、繰返し訪問も計上する。解決中に build script や dependency code は実行しない。

`Lang.lock` schema 1 は選択 target、toolchain／package edition、権限 root 相対の `/` パスを使って整列した package identity、完全一致 version、整列した active edge、source／manifest の SHA-256 checksum を記録する。hash は原バイトを対象とし、辞書順のパスとバイトに長さを付ける。host の絶対パス、mtime、filesystem 列挙順を identity に含めない。各 package source root 下の全 `.lang` source を含め、生成物、入れ子の dependency root、symlink 逸脱の除外／拒否を明示する。compiler の source-root 読取りは検証した input に制限する。lock 実装は replay に使う前に canonical なバイト符号化と fixture を文書化する。

通常の解決は完全検証後に lock を transaction として作成／更新する。`--locked` は欠落／古い lock を拒否して書き換えない。`--offline` は許可されたローカルファイルだけを認め、ネットワーク操作を一切行わない。両方を指定した場合、コンパイル前に全 identity、edge、checksum を検証する。不一致を古い source や cached binary で代替しない。同じディレクトリの一時ファイルと atomic publication を用い、失敗時は従来の lock／build output を保持する。同時 source 変更は publication を無効化するか、検証した不変 snapshot を使用する。content digest は完全性を与えるが、発行者認証ではない。

ソース評価順序、型、所有権／破棄、キャンセル、並行処理、定数評価、foreign capability は採択済み意味論を保持する。package loading が与えるのは host compiler の読取り権限だけで、guest runtime の権限は明示的なままとする。

## 実行モード

VM、native AOT、JIT は同一の検証済みソースグラフを消費する。package test は `fn main()->i64` を持つ明示的 source entry とし、0 が成功、非 0 が失敗。実行時失敗も test 失敗とする。native build は既存の target／SDK／link admission を使い、toolchain input を記録する。portable artifact だけでは再現可能な native binary の gate を満たさない。embedded／component artifact は明示的 binding と独立した version を保持する。

## ABI と互換性

影響する互換性 domain は `package` と `tooling`。manifest、lock、diagnostic schema は bytecode、native SDK、component C v1 から独立して version 管理する。不明 version は推測移行せずエラーとする。移行する既存 package manifest はない。schema 1 を無効化する将来変更には RFC 改訂と明示的 migration／refusal test を必要とする。既存 source command と人間向け診断の互換性を維持する。

## 診断とツール

JSON mode は stderr の各行に完全な JSON object を一つ出力し、人間向け文章を混ぜない。schema 1 diagnostic object は `schema: 1`、`kind: "diagnostic"`、`code`、`severity: "error"|"warning"|"note"`、`message`、`path`（文字列または null）、`span`（null または当該 path の UTF-8 byte offset `{start,end}`）を持つ。追加の文脈情報は場当たり的 field ではなく将来 version で定義する。source diagnostic は既存の安定 code と元の span を保持する。package error は `PKG0001` 不正入力、`PKG0002` 未対応 version、`PKG0003` 不正 contract field、`PKG0004` resource limit、`PKG0005` filesystem／authority、`PKG0006` dependency graph、`PKG0007` lock mismatch。usage error は `CLI0001`、専用診断を持たない tool／runtime failure は `CLI0002`。exit status は usage 2、source／tool／runtime failure 1、成功 0 を維持する。program result は stdout とする。command ごとの成功 record は実装前に明示的に文書化する。manifest の意味検証で位置を省略してよいが、offset を捏造してはならない。

formatting／lint／doc は宣言され検証された package input を対象とする。editor overlay と debugger source mapping は、生成した連結ソースでなく元の path／span を使う。package syntax は source formatter を変更しない。後続 LSP／DAP／code action 設計は edit と snapshot の不変条件を明記する。

## 性能とコストモデル

parse の memory／work は manifest bytes と list 上限で制限する。graph traversal と hashing は明示的な package／edge／source budget を消費する。byte hashing と canonical sorting は観測可能な build work である。cache hit は source、target、toolchain identity の一致を必要とする。測定なしに時間や binary size の改善を主張しない。

## セキュリティと信頼境界

manifest、lock、dependency source は信頼しない。明示的 canonical 権限外への path traversal、symlink 逸脱、不正 UTF-8、不明 field／version、過大入力を無制限な構造の割当前に拒否する。解決中に shell 起動、registry fetch、native plugin load、guest capability の許可は行わない。ローカル path 検査を race に耐える隔離と見なしてはならない。同時変更下で output を公開する前に、不変の検証済み snapshot と handle 相対アクセスを必要とする。

## 代替案

- 変更なしでは M4 の再現性と導入 gate を満たせない。
- source command の shell wrapper では package graph の独立検証や診断契約を提供できない。
- Cargo のような TOML と範囲解決は有力だが、最初のローカル fixture の前に parser と solver／registry policy が増える。完全一致 JSON は既存 serde stack を使い、権限を検査しやすくする。使いやすさは未測定で、優位性を主張しない。
- 依存の連結や checksum の無視は単純だが、名前空間／span または再現性を失うため、この設計を満たさない。

## 実装計画

1. 厳密な manifest parser、構造化診断の検査、不正入力／上限 fixture。
2. 許可された決定的 local graph、完全一致 lock／replay test。
3. 作成／local dependency／build／run／test fixture と、変更 source の offline refusal。その後 native 再現性。
4. package／fmt／lint／doc 統合と、M4 の editor／debug／installer 作業。

観測可能な 1–3 pt delivery、検証、見積変更は [M4 ボード](../milestone-4.md)で管理する。

## テスト計画

許可 manifest の round-trip、重複、必須／未知 field、未対応 edition／version／target、不正 path、循環、version mismatch、権限逸脱、古い lock、resource exhaustion を検証する。実 CLI process で JSON line と exit status を確認する。最初の package fixture は project と local dependency を作成し、compile／run／test、完全一致 offline replay、従来 output を保持した変更 input の拒否を示す。VM／native の値と span、配置変更に依存しない lock identity、target 変更、両 tier-1 host、固定 toolchain input 下の binary digest 再現を確認する。実装後に parser／lock 境界を fuzz する。有限の不正入力 test を fuzz campaign と同一視しない。

## 未解決事項

- registry identity、signing、version range は M5 とし、暗黙の fallback にしない。
- installer／editor／profiling protocol はボード上の別の採択設計とする。
- JSON の使いやすさ、不変 package snapshot のコスト、再現可能 native link は証拠を必要とする。測定で設計変更が必要になれば明示的に改訂する。

## 出典

- repository の `src/main.rs`、`src/frontend.rs`、`crates/semantics/src/modules.rs`、`Cargo.lock`、M4 roadmap。2026-10-29 JST 確認。埋込みの公開／更新日は不明。
- repository の `src/native/manifest/snapshot.rs` の通常ファイル／nonblocking 読取りパターン。2026-10-29 JST 確認。埋込み更新日は不明。
- [RFC 0001](0001-project-charter.md)の最終更新日は 2026-10-28、実験的委任記録は 2026-10-17 JST。[RFC 0006](0006-source-frontend-and-portable-programs.md)は source／module 契約で、最終更新日は 2026-10-28。2026-10-29 JST 確認。本 RFC は project decision を定め、新たな比較性能の主張はしない。
- Cargo Book の [manifest format](https://doc.rust-lang.org/cargo/reference/manifest.html) と [workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)。両方とも更新日を公表しておらず、2026-10-29 JST 確認。TOML manifest、明示的 workspace member、共有 workspace lock を説明している。本 RFC の JSON／完全一致 local profile は project choice で、Cargo の保証を推論したものではない。
