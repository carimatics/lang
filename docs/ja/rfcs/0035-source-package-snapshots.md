<!-- translation-of: ../../rfcs/0035-source-package-snapshots.md; canonical-language: en -->

# RFC 0035: 決定的な source package snapshot

[English](../../rfcs/0035-source-package-snapshots.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B01
- Supersedes: None; extends RFC 0034
- Superseded by: None

## 概要

実装監査、2026-10-30 JST: `0b6591d`で決定的archive headerと元bytes、信頼済み展開後の実行、固定source replay、失敗時の保持を実装と受入testへ照合した。検証対象は`package_archive`。localの全baseline（174 suite／2,514 test、失敗0）、fmt／全Clippy／docに成功し、macOS 15とWindowsのpackage／連続導入tool journeyにも成功した。採択時の未検証記述は履歴であり、この契約をimplementedとする。stabilizedを意味せず、下記の互換性境界／non-goalを維持する。

既存 manifest／member／target／locked／offline／message-format option を持つ `lang package [--list]` を追加する。選択 source snapshot 全体を一度取得し、公開せず予定 file を一覧表示するか、原 byte、lock、version 付き descriptor の決定的な非圧縮 TAR を公開する。

実装前の 2026-10-29 JST、RFC 0001 の実験段階の委任で採択する。既存の上限付き inventory、lock 検証、transaction output API を使う限定的 tooling 成果である。TAR 相互運用、展開／replay、resource 上限、両 host 挙動は実行証拠を要し、採択は実装や安定化を意味しない。

## 動機

build／run／test は検査済み package を実行できるが、受入 input を一覧／archive 化できない。周囲の directory を archive 化すると output／private file の混入、変更中 source の再読、lock の上限付き権限の喪失が起こる。別の filesystem 巡回を作らず固定 inventory を再利用する。これは target／compiler 固定の実験的 source snapshot であり、M5 registry 公開ではない。

## 目標

- 固定した選択 graph、宣言 test、workspace metadata を正確に検査／archive 化する。
- 原 byte、決定的 UTF-8 path 順、信頼した展開後の検査済み offline replay を保つ。
- 失敗時保護 generation と拒否時の旧 output 保持を再利用する。

## 非目標

- registry 公開、署名、圧縮、自動 dependency download、信頼しない archive の展開。
- archive input の実行、言語意味論の変更、guest capability の付与。
- 最初の archive profile での directory alias 表現。

## 利用者向け設計

```text
lang package --manifest-path Lang.json --package app --list
lang package --manifest-path Lang.json --package app --locked --offline
```

`package check-manifest` は既存の検査専用 route を維持する。引数なし `package` は既定 `./Lang.json` から archive を作る。`--list` は package 専用で、native／SDK や実行上限 option は無効。共通の選択／target／usage／拒否は RFC 0034 を維持する。

human 一覧は escape した権限相対 path を一行ずつ表示する。JSON 一覧は field 順 `{schema:1,kind:"package-files",files:[...]}` の object 一つとし、各 file は順に `path`、`kind`（`manifest`／`source`／`lock`／`descriptor`）、`bytes`、`sha256` を持つ。予定 archive の一覧なので生成 `Lang.lock` と `.lang-snapshot.json` も含める。一覧は lock、target directory、archive を作らず、locked 一覧は stdout 前に既存 lock を検証する。

package は `target/lang/<lock-sha256>/package/source.tar` と `inputs.lock` を公開し、locked でなければ最後に root lock を commit する。成功時は既存の human path または schema 1 artifact JSON record を使い、任意 output path 権限を追加しない。

## 厳密な意味論

一覧／符号化前に canonical lock と source inventory を取得する。選択 graph の全 package manifest、catalogue の全 workspace manifest、取得した source／test file を原 byte で収録し、受入 scope の未使用 file も含める。同一 metadata path は統合し、role／path 衝突は拒否する。周囲の lock を後で再読せず、canonical な期待 lock を収録する。

archive profile 1 は canonical directory binding を要求し、workspace member alias、有効 dependency directory alias、宣言 entry／test の directory alias、source-directory alias を `PKG0003` で拒否する。既存 build／run／test の alias 挙動は変えない。link の合成や manifest 書換えなしで plain-file archive を replay するための制約である。既存の包含、通常 file、循環、物理 source identity 検査も archive profile より先に適用する。固定した通常 byte として受入済みの最終 file link は archive link にする必要がない。

archive profile では `.lang-snapshot.json`、`Lang.lock` とその配下を予約し、両 host とも最初の path component を ASCII 大小文字を区別せず比較して、受入 input と衝突したら拒否する。信頼した展開時の file／directory と case-folding の衝突を防ぐ。descriptor の canonical JSON field 順は `format`（`lang-source-snapshot`）、`schema`（1）、`target`、`compiler_build_id`、`lock_sha256`、2-space indent と末尾 LF とする。source manifest／lock version は維持する。

全論理 file entry を UTF-8 path byte 順に整列する。default feature を無効にした `tar` 0.4 で GNU 互換 TAR の通常 file header を生成する。mode 0644、uid／gid／mtime 0、空の owner／group 名を指定し、filesystem 由来 metadata は使わない。library の決定的な long-name 拡張を使い、directory、link、device、xattr、圧縮 header、source の実行 permission は出さない。標準 archive 終端を生成する。符号化時に source parse、型検査、guest 作用、process 起動、source 再読を行わない。

既存 graph 上限と、統合前で最大 643 論理 file（package manifest 256、workspace manifest 129、source file 256、lock／descriptor）を維持する。失敗可能 writer で符号化 byte を 64 MiB に制限し、一覧 record も同じ file／path inventory で制限する。source snapshot は不変符号化 byte と description を所有する。新たに取得した期待 archive に対する検証は、外部 file を parse／展開したり権限を与えたりせず、上限付き byte の完全一致で行う。

## 実行 mode

VM／native／JIT／embedding／component の意味論と span は不変。信頼した test は生成 plain-file archive を所有 directory に展開し、元 lock で選択 graph を replay してよい。製品用の archive 実行 API は導入しない。

## ABI と互換性

descriptor が実験的 archive profile を独立 version 化し、TAR は容器であって認証機構ではない。既存 source／IR／bytecode／native／SDK／C format は維持する。lock は target／compiler identity を完全一致で保ち、target／toolchain 間の source 配布は後の registry 設計に残す。将来の profile 変更は明示した互換／migration 判断を必要とし、reader は descriptor／checksum から権限を推測してはならない。

## 診断と tooling

既存 schema 1 診断を使う。不正 option は `CLI0001`／exit 2、parse する未対応 metadata version は `PKG0002`、未対応 alias／予約 path は `PKG0003`、resource は `PKG0004`、filesystem 受入は `PKG0005`、lock／archive／cache 不一致は `PKG0007` とする。error で一覧／artifact stdout を出さない。`check-manifest`、明示 file 実行、native build 互換を維持する。本 delivery で formatter、lint、LSP、DAP の挙動を変えない。

## 性能と cost model

取得 buffer と整列 metadata を再利用し、上限付き hash と TAR output buffer を使う。圧縮や別の filesystem 走査は不要。符号化／比較は上限付き path sort を除いて受入 byte に線形で、性能や size 改善は主張しない。header を明示 field から作るため、crate の任意 xattr 挙動は無効にする。

## Security と信頼境界

manifest／source byte は既存上限下の信頼しない data。元の directory capability で取得し、output 前に locked input を検証し、generation cache／alias 検査と最終 lock transaction を再利用する。通常の相対 path と alias 拒否により filesystem link を合成しない。信頼しない展開 API、provider discovery、guest 実行、暗黙 network／process capability は追加しない。TAR library／標準 tool は source snapshot を認証しない。

## 代案

- 変更なしでは package 検査／archive 統合が未完了。
- shell TAR recipe は周囲の file を再読し、固定 graph／transaction 契約を適用できない。
- 独自 JSON／binary container は明示 framing を持つが、標準 tool の代わりに新たな data encoding と decoder を増やす。
- directory link の保持は柔軟だが、別の仮想／展開 binding 契約を要する。profile 1 では拒否する。
- 制約しない directory archive は簡単だが、上限付き input identity と完全一致 replay を失う。

## 実装計画

1. 固定 file catalogue／TAR 符号化と実 sibling 展開／replay fixture を追加する。
2. package／list を既存 request／diagnostic／output に接続し、alias、stale input、衝突、resource、cache の拒否を検証する。
3. local baseline と両 host CI を検証し、B01 の最終 3 point を記録してから、完了 native 成果を変えず package fmt／lint／doc へ進む。

## テスト計画

繰返し archive 全 byte と展開した原 file byte を比較する。信頼した展開後に宣言 sibling import と test を実行し、locked／offline replay も確認する。編集／削除／rename 後の source 固定、UTF-8 順、決定的 header、list／失敗時の無書込み、旧 generation 保持、alias／escape／予約 path、resource 拒否を検証する。既存 package／native test を保ち、重点 test 後に全 baseline と実 Windows／macOS CI を実行する。期待 byte の完全一致は一般的な TAR fuzz campaign ではない。

## 未解決事項

plain-file snapshot profile の blocking question はない。directory link、圧縮、toolchain 間 registry source、信頼しない展開は後の設計／証拠を必要とし、B01 の暗黙機能にはしない。

## 出典

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md)、更新 2026-10-29。package 権限、固定 inventory、lock、CLI、公開の契約。
- [tar-rs](https://github.com/composefs/tar-rs)、Cargo が選択した version 0.4.46。Cargo manifest の最新 commit は 2026-05-18、確認日は 2026-10-29 JST。問い合わせた registry endpoint から crate release の公開／更新日を取得できなかった。default xattr 対応は無効。container／alias／budget は上記の project 判断であり、library の主張ではない。
