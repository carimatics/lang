<!-- translation-of: ../../rfcs/0036-checked-package-formatting.md; canonical-language: en -->

# RFC 0036: 検証付き package formatting

[English](../../rfcs/0036-checked-package-formatting.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B02
- Supersedes: None; extends RFCs 0034/0035
- Superseded by: None

## 概要

実装監査、2026-10-30 JST: `0b6591d`でcomment／Unicode保持、check-mode無書込み、stale plan拒否、permissionとfile単位失敗cleanupを実装と受入testへ照合した。検証対象は`package_format, formatter`。localの全baseline（174 suite／2,514 test、失敗0）、fmt／全Clippy／docに成功し、macOS 15とWindowsのpackage／連続導入tool journeyにも成功した。採択時の未検証記述は履歴であり、この契約をimplementedとする。stabilizedを意味せず、下記の互換性境界／non-goalを維持する。

共通 manifest／member／target／locked／offline／message-format option を持つ `lang fmt [--check]` を追加する。固定した選択 graph の全 source／test を既存 lossless CST formatter で整形する。check は無書込みで要変更を報告し、通常 mode は検証済み source 置換を適用する。

実装前の 2026-10-29 JST、RFC 0001 の実験段階の委任で採択する。既存 parser／formatter の再利用で言語挙動を保つ。新たな host 書込み境界には明示 snapshot guard と実行可能な stale／alias／失敗 test が必要。性能や file 間 transaction の保証は推測しない。

## 動機

LSP は不変 document snapshot を整形できるが、dependency と宣言 test を扱う package CLI がない。周囲の directory を formatter に渡すと、受入 source 集合と stale 検査を失う。別 formatter ではなく既存 CST 出力の host adapter を加える。

## 目標

- 選択 source graph の token、comment、既存 formatter policy を維持する。
- edit 公開前に上限付き計画全体を parse／準備し、不正 source や検出した stale input を拒否する。
- 明示 filesystem 権限と source permission を保持する。

## 非目標

- manifest 整形、source 構文／型検査の変更、semantic lint 修正、editor rename。
- filesystem 全体の transaction、crash recovery、非協調的外部 writer の排除。
- source 変更後の lock 自動再生成。

## 利用者向け設計

```text
lang fmt --manifest-path Lang.json --package app --check
lang fmt --manifest-path Lang.json --package app
```

共通選択／上限／診断は RFC 0034 を維持する。`--check` は fmt 専用で、native／SDK、archive-list、実行上限 option は無効。成功時は無出力。check は整形を要する file ごとに schema 1 `FMT0001`／exit 1 を出し、stdout と全 file を保持する。不正 UTF-8 は元 byte range 付き `FMT0002`、元 CST 構文診断は安定 code／path／byte span を保持する。parse／formatter resource 拒否は `PKG0004`。

## 厳密な意味論

元 package graph、manifest、完全 source inventory を一度取得する。既存 CST parser、comment／token owner、work／output 上限を再利用し、整形だけのために型の正しさは要求しない。全 file を準備してから edit し、preserved-invalid 出力を成功と扱わず構文 error を拒否する。整形後 byte 合計は 64 MiB に制限する。未選択 workspace source と manifest は整形しない。

plan は元 source／metadata byte と物理 input identity を不変で所有する。directory binding は archive profile 1 と同じ canonical 条件を要求する。最終 source-file link も拒否し、manifest link は読めるが置換しない。manifest、lock、生成 artifact、guest resource は編集しない。

apply は別途保持した権限 root の directory capability を明示的に要求する。各 canonical input parent を open／保持し、staging 前に全 source／受入 manifest の通常 file、identity、原 byte を検証する。欠落／置換／変更は `PKG0007`、alias／未対応 file type は `PKG0003`。check は staging／公開なしで同じ live 検証を行う。

通常 mode は変更 source の private な同 directory temporary file を準備し、元 permission と output 上限を保つ。最初の source 置換直前に完全 input 集合を再検証する。保持した parent capability で file ごとに atomic 置換し、失敗時は未公開 temporary を cleanup する。source 変更で古い lock が stale になることを維持し、書換えない。

検証 barrier は検出した stale plan を source 置換前に拒否する。parent handle は directory rename 時も権限を保ち、後の表示 path 置換をたどらない。公開は file 単位 atomic で、batch 全体ではない。後続 filesystem 失敗では commit 済み prefix を報告し、未 commit の原 file を保持する。検証後の外部同時書込みは通常の filesystem race に従い、全体 compare-and-swap を主張したり rollback で新しい data を上書きしたりしない。外部 writer をまたぐ rollback は試みない。

## 実行 mode

VM／native／JIT／embedding／component の意味論と span は不変。guest code の compile／実行や guest filesystem capability の付与は行わない。LSP formatting は既存 document／version policy を維持する。

## ABI と互換性

source、MIR、bytecode、native、SDK、C、archive format は不変。既存 schema 1 envelope に安定 formatting code を追加する。不透明 Rust plan API は実験段階で、新しい filesystem 状態には新しい plan を必要とする。既存明示 file command と package archive／build／run／test を保つ。

## 診断と tooling

元 UTF-8 source 位置を保持する。準備中の全 syntax 失敗は公開前に出し、stale 検査は `PKG0007`、filesystem 操作は stage／path／commit 済み prefix 情報付き `PKG0005`。check／write 成功時に artifact record は出さない。既存の決定的 4-space formatter policy を使い、lint／doc／editor／DAP の挙動を変えない。

## 性能と cost model

固定 source buffer と file ごと一度の CST 整形を再利用する。plan は原 byte と上限付き置換 text を保持する。live 検証は上限付き input を読み、staging は変更 file だけを書く。時間／size の改善は主張せず、directory／file 数は既存 package 上限を保つ。

## Security と信頼境界

利用者の呼出しは明示 directory capability 内の取得 source path に対する host edit だけを許可する。private plan field で任意 path の偽造 edit を防ぐ。公開前に identity／byte を再検証し、alias／特殊 file を拒否し、対応環境では nonblocking 受入を使い、permission を保持する。process／network／FFI 権限や metadata の実行可能性を推測しない。

## 代案

- 変更なしでは CLI formatting が欠ける。
- 周囲を巡回する formatter recipe は固定選択と stale 検査を失う。
- 新 formatter engine は既存の検証済み CST／comment policy を重複させる。
- editor edit の返却だけなら簡単だが CLI 適用を提供しない。
- file 間 journal／locking は強いが、この成果を超える crash／concurrency 意味論を増やす。明示した file 単位公開と部分 I/O 失敗報告を使う。

## 実装計画

1. 不変 graph から上限付き整形計画を作り、comment／token 保持と不正 source 拒否を示す。
2. 保持 parent capability で全 input 検証、permission 保持、temporary cleanup を行って適用する。
3. fmt／check 診断を接続し、stale／alias／readonly／失敗と既存 package 互換を検証後、全 baseline と両 host CI を実行する。

## テスト計画

実 root／dependency／test file を整形し、Unicode／comment を保持し、通常の lock 再生成後に run／test を replay して、冪等性と check の無書込みを示す。不正 syntax／UTF-8、変更／置換した manifest／source は source 公開前に拒否する。directory／file alias、保持 parent 権限、readonly／output 失敗、permission、temporary cleanup を検証する。既存 CST test と package／archive／native suite を保ち、local baseline と実 Windows／macOS CI を要求する。有限 test で外部 writer race の排除を証明しない。

## 未解決事項

限定 host adapter に blocking question はない。crash-atomic batch、協調 lock、一般の安全な refactoring は別途設計する。

## 出典

- [CST formatter](../../../crates/syntax/src/format.rs)、[formatter test](../../../crates/syntax/tests/formatting.rs)、[LSP adapter](../../../src/lsp.rs)、[検証付き edit preview](../../../crates/diagnostics/src/lib.rs): source に埋込み更新日はなく、2026-10-29 JST に確認した。
- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md) と [RFC 0035](0035-source-package-snapshots.md)、更新 2026-10-29。source 権限、不変 input、診断、canonical directory profile。
