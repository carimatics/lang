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

package 名前空間の明確化、2026-10-29 JST に実験的委任で採択: root の `mod helper;` は、有効な直接依存 `helper` が宣言されていればその依存を束縛し、なければ従来どおり local file module を読み込む。明示宣言は既存 module 名前空間を使用するため、宣言／import の重複は既存診断を維持する。`use helper::answer;` で公開 item を import でき、入れ子の local module は `root::helper::answer` から参照する。依存内部を含む各 package の `root::` はその package の entry module を起点とする。共有依存は compilation ごとに一つの module identity を持ち、別 package は同じ local／推移依存名を使用できる。依存の `main` は通常関数であり、選択 root の `main` だけが実行 entry となる。既存可視性を横断する全 item に適用し、`pub mod helper;` は明示的な公開 API edge であって未宣言依存への ambient access ではない。非 active 依存は module 束縛を供給しない。新しい source token や単独 file の挙動変更はない。source keyword と予約 `core` 名前空間を module 宣言に使うと、既存 source 診断を維持する。

compiler host は package root の forest と宣言 edge、および不変の parse 済み source snapshot を供給する。名前空間を source の連結や生成 source の再 parse で実装してはならない。compiler 内部の修飾表示名は source から記述できない prefix で依存 root を区別してよいが、source path と診断 span は原文を保持する。local file module は各 package entry を含む source directory を使い、別依存の directory や process の作業 directory を使わない。最初の実行 checkpoint は到達可能 input を snapshot 化できるが、lock replay は下記の完全 source inventory を compilation 前に必要とする。RFC 0006 の明示 `mod` と可視性検査を再利用し、合成宣言なしで canonical diamond を共有できるため、この明確化を採択する。forest loading、名前重複／可視性診断、VM／native 同値性は実行検証を必要とし、両 host の挙動は CI まで未検証とする。

entry を含む directory は外側の権限を通じて開き、その package の source-directory capability として保持する。entry と追加 source file はこの handle 相対で開き、source directory 外へ出る file symlink は、外側 manifest 権限内を指す場合も拒否する。directory alias は handle 取得前に外側権限内で解決できる。開いた通常ファイルの metadata で hardlink を含む source 重複を識別し、可変の最終 filename は canonicalize しない。この source 受入明確化を実装前の 2026-10-29 JST に採択する。元の表示 path／byte を保持して、RFC 0006 の狭い source-root 境界を package loader に適用する。移行する既存 package 実行形式はない。

Windows 実装修正、2026-10-29 JST: 開いた `File` の metadata に対して、公開 `cap-fs-ext::MetadataExt` の device／inode API を使う。同名の `cap-std` trait は固定した stable toolchain で Windows handle field を公開しない。host identity の管理だけを変え、新しい unsafe code、guest grant、source syntax、serialized field はない。両 host の hardlink 拒否を必要とする。128-bit ReFS identifier に対する 64-bit inode projection は拡張の文書化済み制限であり、projection の重複は拒否し、二つ目の source を別 identity として実行しない。

実装前の 2026-10-29 JST に取得寿命の改訂を採択: directory capability は input 取得中だけ保持する。最初の source 準備で上限付き完全 inventory を一度取得し、受入失敗時を含めて全 filesystem handle を解放する。同じ Graph の後続解析は固定した path／byte／alias のみを使用し、file 編集、削除、project rename に依存しない。新しい filesystem 状態には新しい Graph を必要とする。取得失敗も cache 化し、可変 input を再 open しない。Windows CI で、解析後も directory handle を保持すると既存の rename 成功 fixture が拒否されることを確認した。この改訂は不変 compiler data と一時的な I/O 権限を分離し、rename 条件と元の byte／span 契約を維持する。source parse／typing は取得後に元の診断を出し、filesystem／resource 受入失敗は package 診断として先に返せる。runtime／serialized version の変更はない。

dependency の省略可能な `targets` は、完全一致の target triple `aarch64-apple-darwin`、`x86_64-pc-windows-msvc` の空でないリストとする。省略時は両方を含み、重複と未知の triple はエラーとする。解決は対応する target を明示的に一つ選び、既定値は host とする。target 変更は lock replay を無効化する。非アクティブな依存 edge を除外する前に全宣言を検証する。

予定するコマンドは `lang new DIR`、`lang build`、package 対応の `lang run`、`lang test`、`lang package`、`lang fmt`、`lang lint`、`lang doc`。適用可能な package コマンドは `--manifest-path`、`--target`、`--offline`、`--locked`、`--message-format human|json` を受け付ける。最初の検査操作 `lang package check-manifest FILE` は依存 I/O を許可しない。既存のファイル明示 CLI は従来の挙動を維持する。

### Package command の transaction profile

package command 実装前の 2026-10-29 JST、実験段階の委任で採択する。package 形式は `lang new DIR [--name NAME] [--message-format human|json]` と `lang build|run|test [--manifest-path FILE] [--target TRIPLE] [--locked] [--offline] [--message-format human|json]`。manifest option がなければ `./Lang.json` だけを使い、祖先を検索しない。target の既定値は supported host triple。run／test は既存の fuel、service-work、host-call、frame、cell 上限も受け付ける。重複／未知／値なし option は usage error。既存の file 明示 `run`、`run --artifact`、`check`、`compile`、`native`、REPL、整数形式の挙動を保持する。位置引数 source または `--artifact` は従来の `run` を選び、package 専用 option と位置引数 source の混在は拒否する。source のない run は package 形式を選ぶ。

`new` は明示的な schema 1 package を version `0.1.0`、edition `2026`、entry `src/main.lang`、test `tests/smoke.lang` で生成し、両 main は 0 を返す。package 名は `--name`、または manifest の名前文法をすでに満たす directory basename とする。不正 basename には明示 name を必要とし、暗黙変換しない。完成した新規 directory を排他的に公開し、空 directory も含む既存 destination を拒否する。既存 project を変更せず、dependency を推測／取得しない。scaffold file は UTF-8／LF。build／new／test の成功は出力なし。run は実行／公開成功後に i64 の結果と LF を出力し、exit 0 とする。test は 0 が成功、非 0 または runtime failure は診断付き exit 1。追加の成功 JSON record はなく、JSON mode は既定どおり stderr 診断を対象とする。

build／run は選択 package entry を既存の検証済み portable artifact profile で compile し、Copy、ownership、structured task／service を含む。test は各明示 test entry を、その entry の固定 source-directory scope、同じ dependency forest、元の byte／span で compile し、source 合成や再読を行わない。test 0 件でも package graph を型検査するが、実行可能 main は要求しない。test は最初の失敗で停止する。既存 VM engine と allocator／control 契約を再利用し、別の package interpreter を作らない。native executable／link 統合は A06 に残し、この portable checkpoint で完了と主張しない。

test 名前空間の明確化: A04 実装前の 2026-10-29 JST、実験段階の委任で採択する。選択した package の test だけを manifest 順に実行する。別 file の test entry はその解析の root 名前空間となり、選択 package の有効な直接依存と、選択 package を指す test 専用 edge `_package` を持つ。例として `mod _package; fn main()->i64 { return _package::answer() + -42; }` は package main を実行せず公開 answer を検査する。先頭 underscore は source 構文では有効、manifest dependency 名では不正なので、この固定 alias は宣言依存と衝突しない。通常の公開 module access だけを与え、private access や runtime capability は追加しない。元 package の `root::` と nominal identity はそれぞれ保持する。宣言 test が package entry 自体なら、canonical file を重複 load せず既存 root を再利用する。通常の module 重複／循環規則は維持する。追加 test root は既存 module 上限に算入する。source 合成や保存する package edge は追加せず、将来この挙動を変更する際は compiler identity を無効化する。新しい assertion 構文や source wrapper と比べ、既存の main／可視性／module 契約を利用できる。固定 scope／自身の公開 export／private 拒否／profile／失敗 fixture で検証し、使いやすさや性能の向上は仮定しない。

実装の明確化、2026-10-29 JST: entry ごとに新しい VM instance と指定した entry 単位の実行 control を使う。compile 中に保持する test artifact も同じ 64 MiB output 上限に制限する。実行 fixture に基づき、上の例を既存の加算／符号付き literal 構文へ修正した。

失敗時の公開保護のため、canonical lock byte の SHA-256 を generation key とする。build／run generation は `target/lang/<key>/build/` に `inputs.lock` と `main.lbc` を置き、test generation は `target/lang/<key>/test/` に `inputs.lock` と `tests/<manifest-index>.lbc` を置く。`inputs.lock` は完全一致 copy で、test 0 件でも generation は空にならない。この配置は tool の private output で、source／component ABI ではない。同じ親に完成 generation を private stage 化し、排他的に公開して従来 generation を置換しない。既存 generation は新たに検証した期待 file／byte と一致してから再利用し、cache で source／lock 検証を代替しない。lock copy を含む操作ごとの全 stage output を 64 MiB に制限し、従来の source／format 上限を維持する。

output-directory 権限は source capture と別に保持する。書込み前に、物理 alias を含む受入 manifest／source input と予約 output の衝突を拒否する。locked mode は型検査前に lock を検証し、全 artifact の compile／検証と run／test 実行を終えてから generation を公開する。root `Lang.lock` を唯一の可変 commit point として最後に公開し、`--locked` は書き換えない。失敗時は従来 lock とその build／test generation を保持する。private temporary stage は除去するが、lock 公開失敗時に完成した未参照の不変 cache generation が残る場合はある。同時実行が採用した可能性があるためで、rollback で他操作の公開済み generation を削除してはならない。offline mode は許可した local I/O だけを行い、network fallback はない。

理由: 固定 artifact path と lock の別々の置換では atomic な commit point を一つにできない。不変 generation と最後の lock 公開により、複数 file の rollback journal なしで従来 output を保持する。固定した可変 output path の維持、lock の先行公開、lock 検証後の可変 path からの再 compile ではこの契約を満たさない。未実装だった CLI／output 挙動の明確化であり、既存 lock／artifact schema は変更しない。両 host の排他的 directory 公開、同時再利用、test-entry／profile 統合、resource／失敗 cleanup は A04 の実行証拠が必要で、性能改善は仮定しない。

### Workspace 選択と lock schema 2

workspace 実装前の 2026-10-29 JST、実験段階の委任で採択する。build／run／test に `--package NAME` を追加する。明示 workspace manifest では、name がなく root package があればそれを選び、workspace 専用 root は name を要求する。package identity で名前を選択し、root package があれば暗黙 member `.` に含める。明示した全 member manifest を一度検証し、package を必須にして、canonical directory または package 名の重複を `PKG0006` で拒否する。nested workspace 宣言を再帰展開しない。選択不足／未知名は `PKG0006`、不正 command option は `CLI0001`／exit 2 を維持する。standalone の名前選択は同 package と一致する必要があり、包含 directory 権限を追加しない。祖先検索や先頭／全 member の暗黙選択は追加しない。

明示 manifest の directory を権限と output root に保つ。workspace root は sibling を含む同 directory 内の宣言 dependency を許可し、既存の包含、通常 file、循環、完全一致 version、resource 検査を維持する。選択 dependency graph の解決前に root／member metadata を固定し、検査済み manifest を再読せず同 byte を使う。選択 graph の source entry だけを準備する。別 package の source 走査配下に入る全 catalogue member root は除外し、明示した source scope は保持する。既知 manifest path は metadata input とする。未選択 member を含む全受入 manifest を output alias／予約 path 検査に含める。source capture は引き続き handle を閉じ、compile は固定 input だけを使う。workspace 操作は root の `Lang.lock` と `target/lang/<key>/` generation を共有し、standalone member 操作は自身の権限／output root を使う。

standalone lock は schema 1 と完全一致 encoding／checksum fixture を維持する。workspace 操作は schema 2 とし、top-level field 順を `schema`、`target`、`compiler_version`、`compiler_build_id`、`workspace`、`packages` とする。package record／checksum は schema 1 の field と encoding を維持する。必須 workspace record は `manifest_path`、`manifest_sha256`、`members`、`selected_root`、`checksum`。各 member は `path`（宣言 directory、または暗黙 `.`）、`root`（canonical な権限相対 directory）、`manifest_path`、`manifest_sha256`、`name`、`version` を持ち、UTF-8 `path` 順に整列する。root package があればこの list に含める。root／member metadata は root 一つと既存の 128 member 上限、各 manifest 64 KiB に制限する。lock 1 MiB、UTF-8 JSON の 2-space indent と末尾 LF を維持し、path は schema 1 と同じ正規化を使う。

workspace checksum は `LANGWORKSPACE1` と NUL、little-endian u64 の一意 metadata-file 数で始める。root／member manifest file を canonical な権限相対 path 順に並べ、root package の manifest は一度だけ数える。各 file は u64 UTF-8 path 長、path byte、u64 原内容長、原 byte を与える。続いて u64 member 数、整列した各 member の `path`、`root`、`manifest_path`、`name`、`version` をそれぞれ u64 UTF-8 長付きで加え、最後に長さ付き `selected_root` を加える。SHA-256 は小文字 hex。metadata を言語 source と扱ったり package identity を作ったりせず、未選択 manifest、宣言／canonical binding、選択を固定する。schema 2 の replay 利用前に独立計算した canonical fixture を公開する。

独立した schema 2 fixture は [`tests/fixtures/package-lock/schema2-workspace.json`](../../../tests/fixtures/package-lock/schema2-workspace.json) で、`tests/package_workspace.rs` が検証する。workspace checksum は `a672b45db910fc7f615e083b7fa3f33fca975bd716640a00a4ff5e726a49134a`、test では compiler build identity だけを置換する。schema 2 replay 実装前に独立計算した fixture である。

上限付き schema header を読んで対応する厳密な schema 1／2 shape を decode し、重複／未知 field と未対応 version を拒否する。純粋な encoding 検査は replay 権限を与えない。locked 検証は型検査前に、新たに取得した完全な canonical record と比較する。workspace manifest、member manifest、binding、選択、有効 source の変更は旧 output を書き換えず拒否し、catalogue 検証だけのために無効／未選択 source file を読まない。test は A04 の名前空間／control 契約で選択 package の宣言だけを実行する。両 supported target の完全一致 target-condition graph と compiler identity を維持する。

理由と互換性: 実装前の実行 probe は仮想 root と standalone sibling access を拒否し、root package の schema 1 lock は未選択 member manifest 変更後も受入可能だった。schema 1 にこの input の型付き保存欄がない。workspace record は input の役割と旧 standalone fixture を保ち、wrapper では欠落した権限／metadata を固定できず、source／package record の流用は契約を曖昧にする。既存 standalone encoding は引き続き対応する。旧 binary は workspace schema 2 を拒否し、workspace の旧 package-only lock は通常の unlocked 再生成を必要とする。自動 migration は行わない。manifest／source 構文、artifact schema、guest capability、native ABI は不変。member alias の包含、metadata 固定／handle 解放、配置変更、両 target replay、失敗時保護は実行証拠を必要とし、native package 再現性は A06 に残す。性能改善は仮定しない。

### Native package build profile

CLI 実装前の 2026-10-29 JST、実験段階の委任で採択する。`lang build --native --sdk SDK` は同じ固定済み選択 package graph を compile し、既存 native program profile を生成して明示した対応 SDK で既定 startup を link する。二つの option は組で必須、build 専用で、不正 usage は `CLI0001`／exit 2 を維持する。完成 native executable は現在 host 向けとし、別 target は output 前に `PKG0003` で拒否する。既存 Copy／owned／task startup と、C／component の明示 host 不足拒否を維持する。build は guest entry を実行せず、source 再読や Cargo／network 操作を行わない。

linker は private snapshot から検証済み SDK manifest の原 byte を返す。native program file、`sdk-input.json`、executable `application`／`application.exe`、`inputs.lock`、canonical `build.json` を `target/lang/<lock-key>/native-<sdk-manifest-sha256>/` に一括公開する。SDK identity は受入済み manifest の原 byte を hash 化し、その検証済み file table がコピーした SDK input を固定する。native／portable generation は独立に再利用する。Unix の実行 permission を保持し、cache 受入でも検査する。128 output／64 MiB の package generation 上限と最終 lock transaction を再利用し、compile、SDK／toolchain／link、size、古い lock、cache 拒否で旧 output を保持する。

`build.json` は 2-space indent と末尾 LF の UTF-8 JSON で、field 順は `schema`（1）、`target`、`compiler_build_id`、`lock_sha256`、`sdk_manifest_sha256`、`rustc_release`、`rustc_commit`、`package_manifest_sha256`、`executable_sha256`。identity／toolchain data は固定 compiler input と検証済み SDK／link 操作から得る。cache replay は期待する完全 canonical output byte と比較し、receipt は実行権限を与えない。native build 成功時は権限相対 executable path を表示し、JSON message format は field 順 `schema`（1）、`kind`（`artifact`）、`path` の object 一つを出す。error は既存診断を維持する。

理由: 既存 compiler／SDK／link API の合成により、別 backend／startup engine を作らず元 entry、span、native 信頼、互換性を保持する。手作業の二 command recipe は package dependency の固定や lock と一緒の output commit を提供しない。portable generation 内に native output を置くと SDK 依存 input を混同する。source／lock／native／SDK schema は維持し、独立 version の build receipt と CLI profile を追加する。再現性 probe が示した startup／debug-map staging path の修正は、RFC 0020 に実装前記録した。executable 全 digest、source 不要 startup、失敗時保持、SDK 不一致、両 host 挙動を test で検証する。再現性は SDK と外部 native toolchain input を固定した範囲に限り、toolchain 間や host 間の byte 一致を主張しない。A06 の task list は既存受入／公開機構を使う単一 3-point 実行成果として review した。

## 厳密な意味論

manifest schema 1 は必須 field の欠落、未知 field、全 object 階層の重複キー、未対応 schema／edition、不正な name／version／path／condition、末尾の余剰入力を拒否する。package identity の既定値や dependency path の推測は行わない。dependencies／tests は省略時に空、workspace は省略時に存在しない。省略可能な field も存在する場合は宣言された型を必要とし、明示的 null は省略とは扱わない。manifest は 64 KiB、dependency／member／test リストはそれぞれ 128 件までとする。JSON のネストには serde_json の再帰上限を維持する。

マージ前の入力受入改訂、2026-10-29 JST に実験的委任で採択: file 検査は通常ファイルだけを認め、directory、device、FIFO を `PKG0005` で拒否する。Unix は nonblocking で開いてから開いた handle の metadata を検査し、writer のない FIFO が受入検査を止めないようにする。既存 native SDK reader の実装パターンを使う。上限付き CLI 実験で従来の blocking open を再現した。stream 入力はこの profile の対象ではなく、manifest bytes／version を変えず file 受入を明確化する。増大する通常ファイルにも byte 上限と sentinel を維持する。

解決は明示的な manifest から開始し、そのディレクトリを権限 root とする。明示的に選んだ包含 workspace は兄弟 member を許可できる。canonical な dependency root、entry、source file はその root 内に制限し、symlink による逸脱を拒否する。ローカル dependency identity は canonical root と宣言された name／version からなる。schema 1 では alias と依存 package 名の一致を必要とする。alias 順に edge を解決し、循環、内容が矛盾する重複 identity、完全一致 version の不一致、source 欠落を拒否する。上限は package 256、module 256、source 合計 16 MiB、edge 4096 とし、繰返し訪問も計上する。解決中に build script や dependency code は実行しない。

dependency I/O の明確化を 2026-10-29 JST に採択: package directory identity は directory 専用操作で解決する。snapshot には受理した権限 root 相対の file path を保持し、capability 相対の nonblocking な通常ファイル open 時に、解決先が権限内であることを検証する。受入前の file canonicalize や、別 handle を検証した後の可変 pathname の再 open に identity 検証を任せない。上限付き実験で file canonicalize 自体が FIFO を待ち得ることを確認した。manifest syntax／version／runtime 権限は変えない。

`Lang.lock` schema 1 は選択 target、toolchain／package edition、権限 root 相対の `/` パスを使って整列した package identity、完全一致 version、整列した active edge、source／manifest の SHA-256 checksum を記録する。hash は原バイトを対象とし、辞書順のパスとバイトに長さを付ける。host の絶対パス、mtime、filesystem 列挙順を identity に含めない。各 package source root 下の全 `.lang` source を含め、生成物、入れ子の dependency root、symlink 逸脱の除外／拒否を明示する。compiler の source-root 読取りは検証した input に制限する。lock 実装は replay に使う前に canonical なバイト符号化と fixture を文書化する。

通常の解決は完全検証後に lock を transaction として作成／更新する。`--locked` は欠落／古い lock を拒否して書き換えない。`--offline` は許可されたローカルファイルだけを認め、ネットワーク操作を一切行わない。両方を指定した場合、コンパイル前に全 identity、edge、checksum を検証する。不一致を古い source や cached binary で代替しない。同じディレクトリの一時ファイルと atomic publication を用い、失敗時は従来の lock／build output を保持する。同時 source 変更は publication を無効化するか、検証した不変 snapshot を使用する。content digest は完全性を与えるが、発行者認証ではない。

### Lock schema 1 の符号化

A03 実装前の 2026-10-29 JST に明確化を採択: lock の field 順は `schema`、`target`、`compiler_version`、`compiler_build_id`、`packages`。各 package は `root`、`name`、`version`、`edition`、`manifest_path`、`manifest_sha256`、`sources`、`dependencies`、`bindings`、`checksum` を持つ。source は `path`、`sha256`、edge は `alias`、`root` を持つ。binding は `kind`（`directory`、`entry`、`test`）、`scope`、`path`、`target` を持ち、宣言 entry／test path と固定 input key の対応、各 canonical source scope 内の directory alias と固定 target の対応を記録する。package record は権限相対 root、source は path、edge は alias、binding は `(scope, kind, path, target)` の UTF-8 辞書順で整列する。root／scope の `.` は権限 directory を示し、その他は `.`／`..` を含まない正規化した権限相対 `/` path とする。絶対 path や filesystem identity は保存しない。compiler version は crate version、build identity は既存 production-source／toolchain／profile fingerprint と選択 target の組とする。compiler identity が変われば通常の lock 再生成を必要とし、locked mode は拒否する。

SHA-256 digest は小文字 hex とする。package checksum は `LANGPACKAGE1` と NUL の byte から始め、続いて little-endian u64 の file 数を置く。file list は manifest と source input を path 順に整列し、input path の重複を拒否する。各 file は little-endian u64 の UTF-8 path 長、path byte、little-endian u64 の content 長、原 content byte を供給する。続けて little-endian u64 の binding 数を置き、整列した各 binding の `scope`、`kind`、`path`、`target` をそれぞれ little-endian u64 の UTF-8 byte 長と byte 本体で記録する。file ごとの hash は原 content byte だけを対象とする。canonical lock byte は上記 field 順、2 space indent、末尾 LF 一つの UTF-8 JSON とし、serialization fixture で固定する。parse は重複／未知 field と未対応 schema version を拒否し、replay は新たに検証した期待 lock と canonical byte の完全一致も要求する。lock 読取りは 1 MiB と検出 byte 一つまでとし、JSON nesting 上限を維持する。

lock 実装前の 2026-10-29 JST に binding integrity 改訂を採択: file checksum だけでは解決を固定できない。directory alias が inventory 内の二つの tree の間で向きを変えると、全 canonical file path／content が同じでも別の関数本体を選べる。inventory が重なる entry／test directory alias も同様に選択を変え得る。解決済み binding を保存して package checksum に含め、file／scope が重なる場合も宣言 entry／test と input の対応をすべて保持する。数値 inode／device は一時的な重複受入判定にのみ用いる。A03 では source file 未変更の alias retarget 拒否を実行し、従来の lock／output を保持する。未実装の schema 1 契約を変更するものであり、移行する既存 lock artifact はない。

完全 inventory は宣言 entry／test の各 source directory と明示 entry を対象とし、未使用の `.lang` も含む。重複する source root を統合し、入れ子の dependency package root、各 package root の `target` output directory、`.git`／`.codex` metadata directory を除外する。root の output directory は package orchestration 用に予約し、`src/target` module directory は source input として残す。source file を 256 module／16 MiB 上限に計上し、source 以外も含む訪問 directory entry 合計を 4096 に制限する。保持した directory capability で決定的に巡回し、逸脱 link、物理 source identity の重複、directory cycle を拒否する。lock 検証後の compilation はこの固定 inventory だけを使い、追加 source 読取りを行わない。上限と canonical byte は project choice であり、測定済み性能の主張ではない。配置変更、古い／未使用 source、target／compiler 不一致、不正 lock、resource、atomic publication の実行 fixture は A03 の残受入作業である。

schema 1 の serialization fixture は [`tests/fixtures/package-lock/schema1.json`](../../../tests/fixtures/package-lock/schema1.json) で、test 時に compiler build identity だけを置換する。manifest／source／package の SHA-256 値は独立に固定した。`tests/package_lock.rs` は、UTF-16 で逆順になる U+E000 と U+10000 path の UTF-8 順も固定する。これらの fixture は package CLI replay 導入前に canonical byte を文書化する。

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
- [cap-fs-ext 4.0.3 MetadataExt](https://docs.rs/cap-fs-ext/4.0.3/cap_fs_ext/trait.MetadataExt.html) と[実装](https://docs.rs/cap-fs-ext/4.0.3/src/cap_fs_ext/metadata_ext.rs.html)。version 公開日 2026-10-19、別の文書更新日は不明。2026-10-29 JST 確認。公開 device／inode 拡張は Windows に対応し、Windows では開いた file から構築した metadata を必要とする。
- [RFC 0001](0001-project-charter.md)の最終更新日は 2026-10-28、実験的委任記録は 2026-10-17 JST。[RFC 0006](0006-source-frontend-and-portable-programs.md)は source／module 契約で、最終更新日は 2026-10-28。2026-10-29 JST 確認。本 RFC は project decision を定め、新たな比較性能の主張はしない。
- Cargo Book の [manifest format](https://doc.rust-lang.org/cargo/reference/manifest.html) と [workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)。両方とも更新日を公表しておらず、2026-10-29 JST 確認。TOML manifest、明示的 workspace member、共有 workspace lock を説明している。本 RFC の JSON／完全一致 local profile は project choice で、Cargo の保証を推論したものではない。
