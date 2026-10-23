<!-- translation-of: ../../rfcs/0027-production-component-interface.md -->

# RFC 0027: Production component IDL・wire・embedding profile

[English](../../rfcs/0027-production-component-interface.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-22
- Last updated: 2026-10-23
- Tracking issue: M3-I01 through M3-I09, M3-R01 through M3-B02
- Supersedes:
- Superseded by:

## Summary

component IDL v1、rich component-contract schema 3、typed value-frame v1、独立した component C ABI v1 を定義します。[RFC 0003](0003-versioned-component-contracts.md) の capability・negotiation・所有権・failure・poison 規則を維持します。native host が検証済み source または検証した artifact を読み込み、型付き import と grant を供給して script export を呼び、同じ計上対象 invocation で nested guest callback を受け取ります。native body の演算/制御は生成 native code のままです。adapter は MIR 解釈へ fallback しません。

この profile は accepted です。下記の IDL/codec checkpoint で一部を実装していますが、source/MIR component proof・実 grant/resource/callback・production embedding・両 host 実行は実装 Todo に残ります。既存 scalar/resource application prototype と M2 C profile 1 は別 profile のままです。公開 C の安定化は実 conformance 後に B02 が管理します。

## Motivation

`7381e09` の `ComponentContractValueType` は I64・Bool・Own・Borrow だけを受理します。代表 application test は scalar/resource の結果と lock replay を実行しますが、canonical table は backend が生成した host data を包み、source MIR の component resource/call ではありません。既存テストは2026-10-22 JSTに成功しました。独立 MIR constructor graph に task/channel/service role はありますが、nominal component resource はありません。RFC 0023 と実行した task boundary test は旧 codec の明示拒否を維持します。これらは基盤であり、rich IDL・実 production adapter・serialized task profile そのものではありません。

WIT と Canonical ABI は interface/resource の有力な外部参照です。現仕様は gated concurrency feature も含み、WIT を同期専用とは仮定しません。native profile には具体的な source 対応・grant・shared/exclusive access・C lifetime・取消 policy の決定が必要です。scalar prototype を包む library だけでは compiler/trust 境界を確立できません。

## Goals

- canonical resource ownership/access を含む scalar・record・variant・sequence value の有界な独立検証契約。
- lookup/副作用前の明示 interface projection・exact lock・capability。
- 実 source/MIR/VM/生成 native/別リンク統合、共有計上、logical mixed stack、nested callback、協調取消。
- 旧形式を維持し、未対応 profile を拒否する versioned metadata/artifact/native-package rollout。
- 安定化前の両 tier-1 host における実 C caller と、値・所有権・failure・quota・cleanup の等価性。

## Non-goals

WIT 構文/binary 互換、WebAssembly component target、detached task、非同期 native callback stack、guest task の OS-thread 実行、任意の foreign exception 回復、Rust/native-source layout の安定化、循環 wire object identity、ambient filesystem/network/process authority は対象外です。IDL は宣言文書であり、別の実行 source language ではありません。

## Guide-level design

IDL v1 は UTF-8 JSON です。BOM・comment・重複 object key・未知 field・無視する拡張を認めません。宣言 grammar の JSON 数値は `idl` の厳密な整数 token `1` だけで、SemVer は text です。既存 identifier/SemVer validation を適用します。1ファイルで1 component package と閉じた interface projection 集合を記述します。

```json
{
  "idl": 1,
  "package": "demo:counter",
  "version": "1.0.0",
  "interfaces": [{
    "key": "demo:counter/api",
    "resources": ["counter"],
    "types": [{"name": "request", "record": [
      {"name": "delta", "type": "i64"},
      {"name": "label", "type": "text"}
    ]}],
    "functions": [{
      "name": "apply", "effect": "sync",
      "params": [
        {"name": "state", "type": {"borrow-mut": "counter"}},
        {"name": "request", "type": "request"}
      ],
      "result": {"result": {"ok": "i64", "err": "text"}},
      "capability": "demo.counter.apply"
    }]
  }],
  "imports": [],
  "exports": ["demo:counter/api"]
}
```

空 array を含め、示した key はすべて必須です。type 宣言は `name` と `record`・`variant`・`alias` のいずれか1個を持ちます。variant は `{ "name": identifier, "type": Type }` の空でない順序付き array で、payload なしは `unit` を使います。record field と parameter は順序付き `{name,type}` です。各 scope で名前は一意です。alias の payload は Type です。function の field は例のとおりで、`effect` は `sync` または `task`、result は1個です。複数の論理結果には record を使います。

Type は primitive string（`unit`・`i64`・`bool`・`text`・`bytes`）、local/qualified named value type、または constructor を1個だけ持つ object です: `{"list":Type}`、`{"option":Type}`、`{"result":{"ok":Type,"err":Type}}`、`{"own":Resource}`、`{"borrow":Resource}`、`{"borrow-mut":Resource}`。resource と value name は別 namespace です。完全名は `namespace:package/interface.name`、local name は包含 interface 内だけで解決します。resource は宣言済み resource に解決する必要があります。borrow 形は直接 parameter のみに許し、borrowed result・保存/aggregate borrow は受理しません。owned resource は record/variant/list に含められます。alias/value の依存は list/option/result を介する場合も DAG とします。再帰 data は循環 value schema ではなく resource を介して渡します。

import は `{ "interface": InterfaceKey, "version": Requirement, "members": [MemberName] }` で、projection は空でなく member は一意です。Requirement は `{"exact":SemVer}` または `{"interval":{"min":SemVer,"max":SemVer,"prerelease":Bool}}` で、text range の推測はしません。参照先 interface 宣言が member の型と capability を供給します。export は自 package の interface 宣言を名指しし、その member を公開します。interface key・type/resource name・capability name は RFC 0003 の保守的な ASCII grammar に従います。path は data であり file access ではありません。対応する source import が未使用でも、grant の欠落/不一致は error です。

生成 guest 宣言は i64/bool/Unit、Text/Bytes、Vec、Option/Result、record/variant を既存 source family に対応させます。nominal opaque component-resource owner と直接 shared/exclusive resource parameter は独立した source/MIR 受理を必要とし、整数・C pointer・利用者の capability assertion ではありません。owner は一度 move され、退役が必要です。v1 の resource ownership は Send/Share を持ちません。resource loan は suspend・escape・capture できません。通常の source async 規則も適用します。async source entry に対応するのは `task` export だけで、その引数/結果は独立に Send を満たす必要があります。native host import は同期です。実行前に source/MIR proof と生成宣言の全 effect/type が一致しなければなりません。

生成 source は既存 frontend を使います。`struct CounterKind {}` のような tag 宣言と `core::ComponentResource<CounterKind>` の直接型注釈を使い、`extern "component" fn apply(state: &mut core::ComponentResource<CounterKind>, request: Request) -> core::Result<i64, core::Text>;` と記述します。version 付き compiler binding input は qualified tag name を resource key、component import name を interface/member key、exported interface member を既存 public source function name に対応させます。これは compiler data であり、ambient grant や第2 source language ではありません。検証済み IDL は全 mapping・signature・effect と一致する必要があります。tag は通常 data として命名/構築できますが、resource owner を構築できず、検証済み component transfer だけが owner を生成します。component import は Host/C と異なる ABI/effect identity を持ち、代用できません。map と独立した完全な resource/type/loan/effect facts は MIR/artifact 検証を通じて保持します。I09 が opaque 型 proof/拒否、I06 が検証済み binding/call/effect を接続し、通常/旧 profile は拒否を維持します。

source mapping 改訂（2026-10-22 JST）: I09 の実行可能な実験で、既存 frontend に型別名宣言の grammar がないことを確認しました。以前の例示 `type Counter = ...;` を直接型注釈に置き換え、binding の前提として新たな型別名機能を追加しません。tag は nominal identity であり resource の保存 payload ではないため、field の性質は opaque owner に伝播しません。未実装 generator の表記変更であり、移行対象の受理済み source program・artifact bytes・runtime ABI はありません。完全な検証済み map/call/effect 境界は I06 に維持します。

binding 表現決定（2026-10-22 JST）: compiler input v1 は qualified nominal tag name から検証済み ResourceTypeKey への resource map、qualified source import name から interface/member key への import map、interface/member key から qualified public guest function name への export map を持ちます。未知 version・重複 map・未解決参照・必須 import/export member 欠落・対応のない component import/resource owner を、未使用 metadata も含めて拒否します。名前は一度だけ MIR DefinitionId/ImportId/FunctionId に解決し、独立 MIR checker は検証済み IDL semantic graph・検証済み MIR・raw ID map だけを受け取ります。全参照・resource key・ABI・順序付き parameter/result 型・sync/task effect を HIR typing claim に依存せず再検証します。検証済み component unit は完全 contract・code・map を一緒に保持します。既存 C-ABI metadata 境界と同じ backend 非依存 semantic data であり、core frontend crate に runtime/backend 依存を追加しません。IDL graph を再利用し、第2 declaration schema・owner registry・汎用 mapping framework は設けません。preparation work/storage は有限かつ累積とし、全拒否を実行/権限取得前に行います。 この component-unit profile では全 external source import に component binding を必須とし、source C export は受理しません。通常の Host/C profile は別に維持します。scalar のみの component export も共有 ownership MIR を明示選択し、通常 source の profile 推論は変更しません。

source signature 照合は nominal identity を保持し、到達する非 alias IDL record/variant key の反復は source nominal 定義と一貫した一対一対応を持つ必要があります。順序付き field/case と payload 型を一致させ、parameter 名や source 型名は signature identity に含めません。field/case 名は IDL 表記そのもの、または `component_` に完全 IDL 名の UTF-8 bytes の小文字16進表記を続けたものです。generator はこの prefix 外の有効な非予約 source identifier に限って元表記を使い、それ以外は符号化します。hyphen・keyword・prefix 衝突を、source escape/alias の追加なしで表現できます。Unit の variant payload は空 source case または Unit payload 1個に対応できます。比較前に IDL alias を解決します。resource loan は直接の shared/exclusive 引数だけとし、resource/task/capability 制約を独立して必須検証します。未実装 binding format/generator の具体化であり、既存 artifact・native package・C profile の受理や移行を伴いません。serialized revision は I07 が担当します。

generator 表現決定（2026-10-23 JST）: 生成 guest global 名は `component_<kind>_<hex>` とし、kind は resource tag が `r`、nominal data が `t`、import が `i`、export が `e`、hex は `namespace:package/interface#local-name` の UTF-8 bytes の小文字16進表記です。保守的な key grammar は構成名に区切り文字を含めないため、interface/role をまたいでも単射になります。parameter 名は位置に応じた `p0`、`p1` などとし、alias は既存 source 型注釈へ展開します。export 出力は public sync/async signature と version-1 map を供給し、body は実装側が与えます。panic stub は生成しません。順序付き field/case の表記は上記規則に従います。これは tooling 出力であり、signature identity や runtime grant ではありません。

生成 C ABI は参照 surface の閉じた compiler-owned 記述1つから導出します。全 field・size/alignment/offset と、callback 5個・API entry 32個・query の signature を各対応 target について記録します。declaration template の名前位置は `@` とし、fingerprint は host typedef の展開や Rust layout でなく descriptor の厳密な表記を使います。`H(parts)` は各 part の byte 長を u64 little-endian で書いて bytes を続けた列の SHA-256 と定義します。callback signature は `H("LCAB1-signature", target, name, template)`、API entry は同じ domain/target/name に return type・`LANG_COMPONENT_V1_CALL`・comma 区切り parameter type を続けて hash します。query は完全 template を使います。aggregate SHA-256 stream は `H("LCAB1-layout",target)`、宣言順に各 `H(record-name,u64le(size),u64le(alignment))` と各 field の `H(field-name,template,u64le(offset))`、最後に callback/API/query 順の全 signature digest から成ります。これは target 記述の fingerprint であり、native code 互換性や sandbox の証明ではありません。独立した target compiler 検査は参照宣言・全 offset・関数型を対象にします。生成は累積要求 storage/work と出力/中間 text bytes で制限し、失敗時は artifact を公開しません。既存 source・C ABI・wire bytes・package の変更はなく、header freeze と実際の両 host 実行は B02/B01 に維持します。RFC 0001 の実験的委任によりこの具体化を受理し、C wrapper の使い勝手と runtime 統合は未検証です。

C 値表現決定（2026-10-23 JST）: 生成 contract header は canonical contract の SHA-256 を名前空間に使います。非巡回型には決定的な postorder 名を付け、record field・variant case・parameter は位置による C 名として C/C++ keyword を避けます。scalar は固定幅整数、byte/text view は pointer/length、list は型付き pointer/count、sum は検証対象 tag と型付き payload union を使います。これは native tooling 表現であり wire struct ではありません。呼出側が供給する有限 buffer と整列 scratch arena に符号化 frame と復号 list object を置き、text/byte view は入力 frame を借用します。復号は完全な canonical 検証後だけ root 出力を公開します。0-byte 値でも work/elements/depth と scratch/text 範囲を検査します。header は型付き argument/result codec と、同じ version 付き API への root/context call forwarding を供給します。result handle は呼出側が明示所有し、復号 view は frame/result または scratch の解放で失効します。contextual frame の resource index は claim にすぎず、data-only mode では拒否し、非0検査も実際の adapter provenance/preflight を代用しません。生成 call outcome は no-commit/committed/unknown を区別し、result 検査失敗を no-commit と解釈しません。符号化や呼出前拒否では入力 owner を変更せず、呼出側は stale copy を再使用せず報告された commit state に従います。実際の resource transfer/loan/drop 権限は R01–R03 が担当し、生成側に第2 owner registry は設けません。外部 ABI/wire の変更はなく、この C 表現は B02 まで実験的です。

構造的 C 型 identity 改訂（2026-10-23 JST）: alias 解決後に同値な list/Option/Result constructor は child 型 identity が一致すれば同じ生成 C 型を共有します。nominal record/variant key・resource key・access mode は区別します。出現箇所ごとの匿名 struct では、layout が一致しても同じ IDL 値を parameter/result 間で受け渡す際に pointer 型が非互換になるためです。cast なしの直接 C 代入と復号 list storage の再利用をテストします。未公開 generator 表現の修正であり、C ABI v1・LCVL/LCSG bytes・既存 artifact は変更しません。同じ実験的委任により実装改訂前に受理します。

## Reference semantics

### Validation, binding and authority

有界 JSON syntax、閉じた field/name 検査、全 type/resource/effect 検証、import/export projection、canonicalization の順に検証します。未使用宣言を含む不正 metadata を拒否します。import 解決は RFC 0003 の順序を再利用します: inventory lookup 前の grant、fallback なし exact lock、一致する version/prerelease、完全な構造 projection、同順位の曖昧さを拒否する決定的な最高 version 選択です。不変の検証済み binding と実 code owner を保持し、registry の変更で既存 instance を暗黙に rebind しません。

record field の名前/順序/型、variant case の名前/順序/payload、resource key と ownership/access、function effect、parameter/result の順序/型、capability は完全一致させます。parameter の綴りは diagnostic 情報であり、構造 ABI identity ではありません。provider の未要求 member の追加は互換です。value alias は比較前に解決し、named value key と到達する定義を projection 検証に含めます。resource key は version 選択後だけ RFC 0003 の version 非依存 lineage を持ち、実 instance/table provenance は別に検査します。

`borrow` は shared runtime token、`borrow-mut` は exclusive token を取得します。全 call の preflight は transfer 前に nested owned resource leaf と直接 borrow 引数をすべて辿り、stale/別 instance/別型 handle、owner 重複、own/borrow alias、loan 競合、destination capacity 不足、不正 wire、control 未達を拒否します。shared alias の反復は許し、exclusive alias は競合します。commit 前に backing storage と destination entry を予約します。commit は destination の確立と旧 index の退役を不可分に行い、allocate/callback/destructor を実行しません。preflight 拒否は live ownership を保ちます。消費した work/allocation counter や再利用しない identity は巻き戻しません。index は wrap せず、deserialize だけで authority になりません。

runtime resource kind は登録した provider 実装ごとに封印します。検証済み interface binding は static resource key を選択した runtime kind に対応させ、preflight は static key に加えてその kind と実 table/instance provenance を検査します。互換 package version だけで別実装の backing bytes を再解釈できません。resource 公開は元 provider/code owner と context 計上済み backing allocation を保持します。別 component は有効な owner の move/lend/drop ができますが、backing へアクセスするのは登録実装だけです。read/write は現 scope の検査付き shared/exclusive view lease を使い、nested reborrow は実 parent loan authority の下だけで許します。view は結果 transfer 前に終了し、callback return/suspension を越えません。serialize した key/index で実装 authority を供給しません。

全 dynamic borrow scope を内側から終了してから owner を cleanup します。未 return/transfer owner は取得の逆順に退役し、未 commit 結果は callee 所有です。commit 後の failure は所有権を巻き戻しません。宣言された `result` error は値です。panic/trap/control failure は明示 failure flow を使い、選択した primary/callsite を保持して、必須 cleanup を非計量で継続し、suppressed failure の順序と omitted 数を保ちます。destructor は unlink 後一度だけ試み、retry しません。不完全 cleanup・invariant 違反・host callback panic/契約違反・destructor failure は instance を poison します。通常の宣言 error、interruption/cancellation、guest panic/trap の後は、完全 cleanup と invariant 再検査が済んだ場合だけ再利用できます。RFC 0003 の failure 優先順位を保ち、cleanup で先行 failure を置換しません。

### IDL and metadata bounds

IDL の既定上限は source 1 MiB、depth 64、宣言4,096、field/case/parameter/projection 要素の累計65,536、string あたり64 KiB、compiler metadata の要求 storage 16 MiB、preparation work 1,000,000です。function arity は最大256です。各上限は独立した設定可能な有限値で、allocate/traverse 前に変換・算術・計上を検査します。parser recovery は有界で diagnostic 専用とし、error 後に部分的な有効 contract を返しません。別の計上 framework ではなく既存 Budget/resource diagnostic を使います。

canonical contract は `LCMP`、little-endian u32 schema `3`、kind byte `0` に続き、count 付き section を package key/full SemVer、resource key、named value 宣言、interface 宣言、requirement、exact lock、export interface key の順で格納します。string/key/SemVer/requirement/lock は schema 2 の表現を再利用します。resource/value 宣言は完全 key 順、interface/requirement/lock/export は interface key 順、member は名前順です。field/case/parameter は宣言順を保ちます。count と string byte length は u32。重複・非 canonical 順序・未知 tag/field・trailing bytes は error です。

value 宣言 kind は `0` record と named field、`1` variant と named payload case、`2` alias と Type です。value-type tag は `0` i64、`1` bool、`2` own と完全 resource key、`3` shared borrow と key、`4` exclusive borrow と key、`5` unit、`6` text、`7` bytes、`8` list と Type、`9` option と Type、`10` result と ok/err Type、`11` named と完全 value key です。interface は key と member、member は名前・effect byte（sync `0`、task `1`）・順序付き named parameter・result Type 1個・capability を持ちます。requirement は interface key・正規化 requirement・名前順 required member、exact lock は schema 2 の完全 key/version/digest を使います。export はソート済み interface key です。全参照は閉じた宣言集合で解決し、map iteration/backend 型に依存した canonical 出力を禁止します。

schema 3 report kind `1` は policy u8（redacted 0、sanitized 1）、primary Record、u32 suppressed count と Record、poisoned u8、omitted u64、参照 header 順の Usage u64 counter 9個、u32 frame count と外側からの Frame を格納します。boolean/presence は厳密に0/1で、存在する optional data をその後へ格納します。Record は category u8・phase u8・diagnostic string・optional message string・optional `(limit-kind u8,limit u64,attempted u64)`・optional origin Frame です。Frame は mode u8（VM 0、JIT 1、AOT 2、host 3、adapter 4）、module/provider digest[32]、function/member id u32、optional Span `(source-id,start,end)` 各u32です。span は start ≤ end とし、保持した検証済み manifest による解決を data decode と分離し、path を開きません。必須 cleanup 中の omitted は u64::MAX で飽和し、他 counter の枯渇は次の副作用前に拒否します。category は panic 0・trap 1・destructor 2・cancellation 3・interruption 4・limit 5・admission 6・host-contract 7、phase は call 0・cleanup 1・admission 2です。suppressed は cleanup failure、destructor/host-contract failure と instance-poisoned は poisoned=true を必要とします。limit record は numeric data 必須、他 category は禁止です。redacted は message を禁止します。未知値・category/phase/diagnostic の不整合・上限超過は閉じて拒否し、schema 2 の bytes は変更しません。

次の閉じた diagnostic 集合は固定 C admission diagnostic ID も定義します。0は C diagnostic なしであり、failure Record では不正です。各行は許可する category/phase を示します。cleanup 再入は destructor failure です。host-origin panic は poison し、guest panic/trap 後の再利用には完全 cleanup が必要です。

| C ID | Diagnostic | Category / phase |
| --- | --- | --- |
| 1 | component.invalid-request | admission / admission |
| 2 | component.unsupported-version | admission / admission |
| 3 | component.missing-capability | admission / admission |
| 4 | component.incompatible-interface | admission / admission |
| 5 | component.invalid-metadata | admission / admission |
| 6 | component.invalid-wire | admission / admission |
| 7 | component.resource | admission / admission |
| 8 | component.reentry | admission / admission |
| 9 | component.unsupported-profile | admission / admission |
| 10 | component.unknown-export | admission / admission |
| 11 | component.instance-poisoned | admission / admission |
| 12 | component.limit | limit / admission or call |
| 13 | component.call.cancelled | cancellation / call |
| 14 | component.call.interrupted | interruption / call |
| 15 | component.host-contract | host-contract / call or cleanup |
| 16 | component.boundary.panic | panic / call |
| 17 | component.boundary.trap | trap / call |
| 18 | component.boundary.destructor-failure | destructor / cleanup |

limit-kind 0–16は順に fuel・service-work・host-calls・heap-bytes・live-allocations・frames・live-cells・live-tasks・canonical-indices・preparation-work・metadata-bytes・value-elements・serialization-bytes・depth・decoder-allocation-bytes・suppressed-records・trace-frames です。optional suppressed/trace capacity は omitted 数で証拠を打ち切り、必須 cleanup を停止しません。C error の numeric field は component.limit 以外では0です。

### Typed value frame v1

frame header は `LCVL`、u32 version `1`、kind byte（parameter tuple `0`、result `1`）、32-byte signature digest、u32 body length です。直後に body を置き、padding/trailing data はありません。digest は function projection、すなわち完全 function key・effect・capability・順序付き parameter Type・result Type・到達 named 定義/resource を canonical key 順に SHA-256 化します。parameter の綴りと無関係な宣言を除き、参照は完全 key で符号化します。加算的 provider 互換を保ち、body 解釈前に選択した signature を要求します。

signature digest の preimage は `LCSG`、u32 version 1、完全 function-key string、effect byte、capability string、u32 parameter count と名前なし Type、result Type、到達 resource key のソート済み count 付き集合、到達する非 alias value 宣言の同様の集合です。schema 3 の string/Type/宣言表現を使い、集合収集/符号化前に alias を解決します。参照は完全 key です。この厳密な bytes を SHA-256 化し、無関係な宣言や parameter 綴りで互換 projection の digest を変えません。

| Type | Body encoding |
| --- | --- |
| Unit | No bytes |
| i64 | Eight little-endian two's-complement bytes |
| bool | One canonical byte, exactly 0 or 1 |
| Text / Bytes | u32 byte length then UTF-8 / arbitrary bytes |
| list | u32 element count then elements in order |
| record | Fields in declaration order |
| variant | u32 case ordinal then selected payload |
| option / result | One byte: None/Ok = 0, Some/Err = 1; selected payload follows |
| own / borrow / borrow-mut | Nonzero u32 canonical boundary index, only with an active typed call context |

data-only encode/decode は resource を含む schema を拒否します。context 付き frame の index は未信頼の申告であり、元 table に対する全 call preflight 成功まで owner/loan を与えません。resource index を永続 metadata・保存 data value・portable package に入れません。result transfer は caller の全表現を検証/allocate してから commit します。生成 binding は failure 時の引数 ownership commit 有無を公開し、commit 前拒否では owner を元のまま返し、後の drop で受け取った各 owner を明示解放します。buffer 解放だけで別に commit 済みの resource owner は解放せず、instance close は全 call/borrow 終了後に残 owner を drain します。

既定上限は frame あたり64 MiB、depth 128、value 要素累計1,000,000、decoder 要求 buffer 128 MiBと、既存 preparation/work 上限です。予約/参照追跡前に length・残入力の最小量・checked multiplication・累計 count・UTF-8・tag・signature を検証します。value/cleanup は反復的に辿ります。nested call と allocation/byte/work budget を共有し、frame ごとに invocation counter を reset しません。wire bool の厳密性は、旧 scalar prototype で維持する RFC 0003 の scalar-lane lifting と別です。

### Embedding, callbacks and cancellation

opaque engine/module/instance/result handle を持つ別の `lang_component_v1_*` C surface を使います。公開 struct は u32 size/version と reserved-zero field を持ち、length/counter は固定 u64 で host size への変換前に検査します。byte view は `(const uint8_t*,u64)`、null は length 0 のみです。owned buffer は engine allocator と対応する release API で扱います。handle/code/user は規定の owner lifetime/thread affinity を保ち、callback は型付き C function pointer です。Rust layout・native frame address・HostFunctionId・unwind payload を公開しません。

必要な操作は API/version query、engine create/destroy、source/検証 artifact からの module load、保持した code/user owner 付き provider/import 登録、instance create/close、export call、callback-context nested call/check/allocate/release、result/report inspection/release、resource drop、invocation interruption です。request/response と API table の size/version を明示検査します。参照 C header が surface を今固定し、I04 が生成と独立した完全 layout/fingerprint vector を executable 公開前に供給します。B02 が両 host conformance/migration 後だけ安定化します。field 欠落・未知 version・reserved bit は公開前に拒否し、拒否した request は成功 output slot を変更しません。既存 C profile 1 の signature と再入拒否は維持します。

受理した call は値（宣言 Err を含む）、または origin・usage・commit state・report・logical stack を持つ構造化 boundary failure を返します。metadata/grant/signature/preflight failure は guest/host callback を呼ばず owner を transfer しません。型付き生成 wrapper は canonical frame を encode/decode し、call-local buffer/borrow と同じ全 call resource adapter を使います。raw pointer validity と code/user lifetime は信頼する native caller の責務であり、未信頼 source/metadata/bytes は完全に検証します。

root call は元 allocator identity、grant/binding、fuel/service-work/host-call counter、heap/live-allocation/live-cell/frame/task 上限、割込み flag、failure state、logical stack を1個の invocation context に保持します。native→VM/JIT/AOT call と nested callback は新 budget/provider ではなく同じ context を消費します。engine/module preparation は別の有界 compiler budget を持ちます。callback は root API で active instance に入れず、現 callback context の dynamic child token を使います。token は callback return で失効し、owner thread に束縛され、preflight/commit/cleanup 中は使えません。保持や偽造で有効期間を伸ばせません。

再入は既定で拒否します。登録時に nested guest call を明示許可できますが、child entry にも grant・recursion/frame 上限・active resource loan・poison・元 failure state を適用します。child は同期 export のみで、nested task export は副作用前に拒否します。root task export は既存 structured task tree の完了まで C に戻らず、同期 host callback を呼べます。厳密な source/MIR suspension-loan 検査を維持し、C callback stack や source loan を task suspension に残しません。再入で producer に service state を見せたり owner isolation を迂回しません。

interruption は協調 invocation flag であり、task cancellation は既存 ancestry/handler-boundary protocol です。call admission/commit 前、guest safe point、callback return 直後に、共有 flag と既存 control 順を検査します。callback は供給 context で確認/伝搬でき、任意の blocking native code を強制 preempt はしません。commit 後取消は rollback せず callee 所有引数/結果と descendant を退役します。観測した primary cancellation/interruption 後に成功値を返しません。必須 borrow 終了・owner cleanup・failure projection は非計量です。後の境界でも先行 primary と suppressed 順序を保ちます。logical frame は guest mode/function/span、adapter 遷移、host callback を同じ parent invocation に束ね、report 解放まで code/source owner を保持します。

計上対象 host callback は供給 context allocator で allocate し、guest/callback が実 provider counter と元 release request を共有します。trusted native library が独立に行う任意の allocation は、この allocator が強制できる budget の外です。instance close は active call/loan を拒否してから一度 drain し、destructor failure を記録/poison します。engine destroy は live dependent handle を拒否します。暗黙 forced free・retry・counter reset は認めません。

## Execution modes

VM/JIT/AOT は同じ独立検証 component 宣言・binding・resource preflight/commit・control・cleanup 契約を使います。native body は演算/CFG を生成し型付き service を呼び、callback/package startup は隠れた VM/MIR evaluator を実行しません。verified script には明示 VM 実行も残します。native image は信頼対象で exact image/compiler/SDK 対応を必要とし、hash は native sandbox の証拠ではありません。後の WebAssembly target で adapter を作れますが、WIT/Canonical ABI conformance の証拠にはなりません。

## ABI and compatibility

IDL v1、LCMP schema 3、LCVL v1、component C ABI v1、source-program schema、private native packet/package revision は独立した version namespace です。component schema 1/2 と旧 scalar/resource API の解釈を保ちます。LANGBC schema 1–4 と M2 C/native-package reader は既存 profile と task/service 明示拒否を維持します。旧 schema へ tag を差し込んだり、通常 C import を component authority と再解釈しません。

serialized task/component 公開前に、I07 で次 LANGBC revision、全 task/channel/service/component 型と operation、source span、cleanup/claim/effect proof、厳密 tag、budget、互換 fixture の follow-up を受理する必要があります。I08 は private native task/component record、native-package metadata/startup/SDK 対応、両 host の installed link を別に改訂します。これらの必須 M3 delivery を source load/メモリ内 object の陰に隠しません。I06 が先に nominal resource/component-call の source/MIR proof と旧 profile 拒否を実装し、実 adapter/grant/callback は R/P/E に残します。新公開 source 宣言/diagnostic も受理前に対応する日英 decision を更新します。未対応 mode は provider/guest 副作用や output 公開前に拒否します。

### Component C ABI v1 reference

完全な規範 surface は [component-abi-v1.h](../../component-abi-v1.h)、[layout vector](../../component-abi-v1-layouts.json) は各 tier-1 target の独立した期待 size/alignment・全139 field offset・38 signature fingerprint を記録します。参照物は引き続き decision artifact であり library 実装ではありません。I04 は同じ surface と完全 fingerprint を導出し、独立 reference packer は規範 header と target compiler layout を読み取ります。concrete struct は厳密 size/version と reserved-zero を要求し、view は規定の固定 layout leaf です。C padding ではなく named field だけを読み書きします。API table は両 target とも280 bytesで、全32 entry は non-null、利用できない mode は unsupported-profile を返します。C11/C++20 の target compile は header/layout 互換だけを証明し、Windows の runtime conformance ではありません。

result output を持つ call/context_call/instance_close/provider_drop/resource drop は、受理した完了を status 0/1 と value/failure result handle で返します。他の操作は0で成功し、context_check は result なしで既に latch した failure を1で示します。callback invoke の0/1は value/report buffer です。他 status は invalid-request 2・unsupported-version 3・busy 4・stale 5・out-of-memory 6で、成功 output は変更せず、有効に初期化した error slot に固定 diagnostic/numeric field を返せます。optional input digest・source ID・byte span・detail code により primary source/IDL/artifact diagnostic を allocation なしで示します。flags 下位bitは none/source/IDL/artifact = 0/1/2/3、bit 2 は valid span です。未使用 field は0、source/IDL code は SRC/IDL の数値 identity を維持します。artifact detail 不明時は offset を保ってcode 0にできます。result kind は宣言 Err を含む値と boundary failure を区別し、bytes は LCVL または LCMP schema-3 report です。inspection は不変で view は result 解放まで有効です。失敗 buffer に部分的な値を公開しません。 instance_close/provider_drop/resource drop の成功時は result output を null にし、失敗時は report handle を返します。

engine mode は VM 0・JIT 1・trusted native AOT 2と有限 ceiling、flags bit 0 は host が明示的に sanitize した message の許可です。他bitは0、既定は redacted です。source format 0/LANGBC format 1は VM/JIT のみで load し、JIT が利用不能なら閉じて拒否します。native AOT は module_bind_native を使い、有界 package metadata と exact SDK 所有の opaque program/code owner を image/compiler/SDK 検査後に登録してから private data にアクセスします。互換性確認のため未知 native constructor を load/実行しません。provider/native 登録成功時だけ供給 owner を消費し、全 binding/value の解放後に release_owner を一度呼びます。失敗なら caller 所有を保ちます。allocator/owner-release は unwind しない infallible な native lifetime operation とし、code/user storage は最後の return まで保持します。

module load は source/artifact/IDL binding data を snapshot し、公開前に全 export/import を検証します。bind_export は requirement/member がちょうど1個の LCMP schema-3 consumer projection を受け取り、指定 instance の一致 export だけを束縛します。grant は宣言 capability で registry lookup を制限し、provider discovery に先行します。root call の control handle は一度だけ使用し、再使用で quota を reset しません。child context_call に別 control/allocator/grant parameter はありません。cross-thread は interrupt のみで、control lifetime はその操作を覆う必要があります。他は owner-thread です。active root は新 root call・registry 変更・close を拒否し、不変 result の inspection/release と許可した context 操作は可能です。

context token は engine・再利用しない invocation/scope ID・reserved field を持つ versioned value であり、stack address を保持しません。全使用で engine/thread・live scope・callback policy・phase を検査し、copy で lifetime を延長しません。engine 生存中の失効した copy は拒否します。owning opaque handle の解放後は pointer が無効であり、任意/double-free native pointer の検証を stale check が保証するわけではありません。public module handle 解放後も instance binding は owner を保持し、provider close は下記の live binding 条件に従います。instance close は active call/loan を拒否し、受理後は cleanup failure でも handle を消費して既存 export binding を無効化し、一度 drain します。残 binding/result handle は解放まで必要な code/report storage を保持します。engine destroy は dependent live handle を拒否します。

resource_publish は native caller backing bytes を instance の計上 allocator で copy し、provider/type identity を検証して成功時だけ canonical owner を公開します。context_resource_publish は現実装の resource へ live context buffer を move し、拒否時は buffer を保ちます。型ごとの登録 destructor は unlink 後に backing を一度受け取り、runtime が元 request で解放します。context_resource_read/write は現実装の authority と実 read/exclusive/owner 権限を要求し、view lease と parameter loan を返却 owner commit 前に終了します。caller が engine backing を直接 free してはいけません。cleanup 中の resource drop は同じ順序付き Session を使い、公開/再入を禁止します。context report scratch は予約済み/有界のままです。optional cleanup text の allocation 不可は omitted として記録し、primary の置換や destructor の省略をしません。 context_resource_keep/take は scope owner と provider の persistent holder の間を、全 preflight と新しい再利用しない index で不可分に move し、拒否時は ownership を保ちます。active loan と cleanup 中の操作は拒否します。persistent owner を返すには先に call scope へ take します。provider close は active call/live function binding を拒否してから登録を外し、persistent owner を一度 drain します。残 resource value/report は最後の解放まで code/user owner を保持します。成功 result は null、失敗は cleanup report で、受理した close は provider handle を消費します。v1 の backing 公開は登録した host resource kind を使い、script は opaque owner を保持/move/borrow/drop して型付き method を合成します。新 host report は call phase の panic/trap/host-contract のみで、宣言 error は値、control failure は検査した context から発生します。P02 受理前に component service-work event catalog を RFC 0013 に記録し、既存 program cost を維持します。

host callback は context allocation の value/report buffer を返し、adapter が所有権を消費します。残 temporary buffer/result は callback 終了で解放し、公開 resource backing は owner に保持します。先に latch した child/control failure は callback が成功を返しても元のまま伝搬し、この場合 CALL_FAILED は null を返せます。新callback report は検証して現 host call に帰属させ、先行 guest origin の偽装や primary 置換を許しません。不正 host result は host-contract failure と poison です。通常時の context allocation/control failure は invocation に latch し、optional cleanup-report allocation は primary を変えません。後の task suspension 前に callback の実 C stack は終了しています。result buffer 解放は独立に commit した resource owner を消費せず、生成 binding が明示 owner drop と commit 前 input owner の保存を行います。

## Diagnostics and tooling

元 IDL の byte span を正規化宣言・diagnostic・生成 source mapping まで保持します。安定 identity は IDL0001 syntax/UTF-8、IDL0002 重複/未知 field、IDL0003 name/reference、IDL0004 不正 type/effect/borrow、IDL0005 projection/version、IDL0006 resource bound、IDL0007 非互換 version、IDL0008 未対応 profile です。path と primary/related span を含め、構造 runtime failure を string に置換しません。JSON formatter は検証文書の決定的な tooling であり、binary canonicalization は空白/property 順と独立です。language formatter/LSP/debugger の source identity は既存契約を維持します。生成 binding に ownership/access・capability・effect・commit state・buffer release を記載し、diagnostic を IDL/source callsite に対応させます。

## Performance and cost model

parse・graph/projection 検証・hash・negotiation binding・生成 wrapper は有界 preparation work/storage を消費します。canonical value に UTF-8 検査/copy/allocation、resource call に table 検証/予約/ownership/loan bookkeeping、callback に logical frame/control check が必要です。不変の検証 plan は再利用できても、動的 grant/provenance/loan/limit を cache で消しません。直接 native 最適化には観測等価性が必要です。zero-copy・overhead・code-size・startup 改善は主張しません。Z01 で M2 workload を維持しつつ実 concurrency/boundary cost を測り、テスト時間を benchmark としません。

## Security and trust boundaries

IDL・schema・value frame・script artifact・provider metadata は未信頼の有界 data です。grant を provider discovery より先に検査します。metadata name・digest・serialized index・unsafe source scope は authority を与えません。互換性を調べるため native constructor を実行しません。native C caller/provider は信頼対象で foreign exception を封じ込めます。既知の unwinding Rust callback のみ catch_unwind を使い、panic payload の disposal も封じ込めます。abort は回復できません。cleanup 再入と context 再作成による poison reset を禁止します。source proof/logical capability は in-process native plugin sandbox ではありません。

## Alternatives

- 変更なし: scalar/resource prototype だけでは要求する rich interface を表現/実行できません。
- library/tool wrapper: 型付き wrapper 生成には有用ですが、独立 MIR effect・provenance・callback control・artifact 検証には不足します。
- 完全 WIT/Canonical ABI: gated async を含む最も有力な相互運用案ですが、Wasm target/linear-memory/task model を必要とし、この native C/source 対応そのものは定義しません。現時点で互換を主張せず、将来 adapter を残します。
- 独自の実行 interface language/pointer native struct: 別 frontend または host layout/lifetime 依存を導入します。厳密な宣言 JSON と型付き有界 frame は copy/validation cost と引き換えに小さい明示互換契約を与えます。

## Implementation plan

I02 が有界 IDL parse/check/span、I03 が rich metadata/value codec・独立 canonical vector・厳密 report tag/vector・不正/budget test、I09 が opaque resource type/capability/move proof と未対応拒否、I06 が検証済み source/MIR component binding/call/effect、I04 が型付き C/guest binding と完全 header/layout vector、I05 が interface/version negotiation を供給します。R01–R03 で実 resource transfer/loan/cleanup/failure、P01/P02 で実 typed grant と有界 metadata/adapter work を接続します。E01–E04 は host/script call・同期 nested callback・元 context の failure・mixed stack/cancellation、L01–L04 は合算 limit を実証します。I07/I08 は B01 前に serialized task/component と native-package profile を明示的に供給します。B01/B02 は両 host conformance/C migration 安定化、X/Z は runnable plugin example・cost・最終 guide を担当します。各1–3点の delivery を実行証拠で再監査し、欠けた前提は可視化してから実装し、source fixture を横断基盤だけで置換しません。

## Test plan

I06 binding checkpoint（2026-10-22 JST）: `lang::frontend::component::analyze` は version 1 の qualified source name を MIR ID に解決します。受動的な `component::bindings::Unit` は完全な検証済み IDL・ownership/task code・map を一緒に保持します。独立 MIR check は選択された全 resource key・source ABI・nominal record/variant identity・順序付き field/case/payload・data constructor・parameter/result・sync/task effect を照合し、比較前に IDL alias を解決します。native import は同期のみ、loan は直接 resource 引数のみです。未使用 import/resource と全必須 IDL projection を含め、重複・未解決・欠落・private・role 不一致・偽造 MIR を拒否します。CBND0001–0007 診断は元 code/contract span と map index を保持します。compiler preparation は code/IDL/名前/照合の work・requested-storage counter を累積共有し、保持した depth fact で上限を検証します。scalar export は共有 ownership MIR を明示選択し、通常 source の推論は変更せず core frontend crate に runtime/backend 依存を追加しません。SourceMap identity は同名/同内容の古い snapshot があっても実 HIR module origin から取得します。version/count は source parse 前に拒否します。静的 facts は provider・owner・lease・実行権限を与えません。旧 Host/C・runtime/task・bytecode・native-package 経路は明示拒否を維持します。実 call/grant/transfer/callback と serialized/installed profile は I04/P01/R01–R03/E01–E04/I07/I08 に残ります。

I09 opaque 型 checkpoint（2026-10-22 JST）: source と独立 MIR は `core::ComponentResource<Tag>` を nominal な move-only owner として受理し、retirement を要求して Send/Share を許しません。nominal tag は同一性を示し物理 payload ではないため、field の storage/region 性質を伝播しません。元 source span と tag privacy を lowering 後も保持します。通常 tag 構築では owner を生成できず、不正 tag ID・二重消費・偽造 retirement を拒否します。現行全 runtime layout・ownership VM entry・schema-2/3/4 writer・native package は未使用 resource metadata も実行/出力前に拒否します。型/proof の受理であり、検証済み resource-key binding・component import ABI/call/effect proof は I06、実 transfer/adapter は R01 に残ります。既存 source grammar に型別名がないため generator は直接型注釈を使います。

I03 実装 checkpoint（2026-10-22 JST）: production wire module に LCMP3 contract/report・alias 解決済み LCSG1 projection・LCVL1 data-only frame を実装しました。wire 23件と IDL 回帰13件で、独立 bytes/digest・完全な version/lock/type・厳密順序/tag・未使用の不正 graph・全 quota 境界・1,024段の値・byte を持たない element・累積 preparation usage・旧 bytes の一致を検証します。独立 reference packer は17 golden entry を再生成します。preparation Session の上限は累積 MIR Budget と入れ子 metadata cap を使い、保持する検査済み data-depth fact で profile 上限の迂回を防ぎます。API は実行 binding・runtime allocator identity・grant・owner・loan を与えません。data-only codec は resource を含む signature を拒否し、contextual index は R01 の実 whole-call preflight へ接続します。実 provider/control 接続は P02、embedding/callback は E01–E04 に残ります。RFC 全体は accepted であり implemented/stabilized ではありません。

I02 実装 checkpoint（2026-10-22 JST）: runtime IDL module に有限な JSON/schema/graph 検証・span 保持・宣言の正規化を実装しました。公開 integration 13件、parser 3件、正常/不正 JSON fixture、有界な `component-idl` 実行例でこの段階を検証します。検査済み wrapper は runtime grant・binding・owner・callback 権限を与えません。RFC の残部は後続 delivery の証拠待ちの accepted であり、implemented/stabilized ではありません。

独立 JSON/binary vector、byte/count/depth/work/allocation の厳密上限/1不足、重複/未知 field、不正 Unicode/name/ref/effect/loan、型循環、canonical 順序、不正/truncated/trailing frame、旧 schema 拒否を検証します。構造 projection・SemVer/prerelease/build 曖昧さ・exact-lock replay を codec と独立に比較します。全 preflight 拒否で元 ownership を保ち、一度の commit/stale index、shared/exclusive 競合、nested owner、逆順 cleanup を証明します。生成 wrapper の実 C caller で検証 source/artifact/callback を VM/JIT/別リンク AOT から呼び、root task/channel/service fixture を含めます。値・usage・commit state・resource/heap inventory・span・logical stack・primary/suppressed/omitted report・poison を対応する全 control/provider/割込み境界で比較します。callback grant 拒否・再入 policy・失効/別 thread token・再帰 budget・host allocation・commit/callback 前後取消も検証します。macOS AArch64/Windows x86-64 MSVC で isolated installed package と migration を実行します。fuzz compile/lint を campaign とせず、campaign/計測は別の記録付き証拠とします。

## Open questions

参照 header/report tag の review 後、blocking な意味論の質問はありません。WIT adapter・非同期 native callback・再帰 value schema・sendable external resource kind は optional follow-up で、v1 受理を暗黙に弱めません。未検証仮説は compiler/runtime 再利用 cost、生成 C の ergonomics、callback/task 統合、両 host の実行/startup です。委任受理から性能/実装結果を推論しません。

Review outcome: **RFC 0001 の実験期委任に基づき2026-10-22 JST、production 実装前に accepted**。厳密な宣言文書と独立 version namespace で既存 component/resource/cleanup 機構を拡張し、旧 C/artifact profile を保ちます。shared/exclusive resource access・同期 nested callback・元 invocation 計上・明示 failure/ownership phase により、suspension loan の緩和や native VM fallback なしに production policy を決定します。既存実行 probe 2件、最終 header の各 target 112 C layout assertion と両 tier-1 target の C++20 syntax が成功しました。これは decision/互換性の証拠で、production 実行や Windows runtime の証拠ではありません。source/MIR/resource/callback 統合・完全な生成 fingerprint・serialized task/native-package rollout・cost・両 host 実行は未検証の実装条件として Todo に残します。

I04 generation checkpoint（2026-10-23 JST）: `lang::component_bindings` は検証済み metadata から、有限予算の guest 宣言・version-1 compiler input・元 IDL origin・完全 C ABI 記述・型付き C canonical-frame codec を導出します。生成テスト10件が成功し、全 data family/alias と sync/task signature・構造的 C 型 identity/cast なしの list 再利用・resource import move・keyword/qualified-name/header 衝突・preparation/C 値 quota の厳密上限/1不足・不正 frame・独立 Rust/C bytes を検証しました。生成 source fixture は独立 MIR を通り、VM/JIT で42を返し usage も一致します。C11/C++20 target compile は各 target の全139 field offset・38 signature を検査し、独立 Python/Clang reference packer が完全 vector を再現します。C mock call は forwarding と明示 no-commit/committed/unknown outcome を検査しますが、production engine・grant/resource/callback・Windows 実行の証明ではありません。最終 source 固定 baseline は136 suite / 2,059件 / 失敗0 / 既存 timing ignore 1 と、workspace/fuzz fmt・警告をエラー扱いにした Clippy・rustdoc・全14 fuzz target compile/lint が成功しました。production/validation source hash は一致し、fuzz campaign は主張しません。

確認用 example は `cargo run --example component-bindings -- guest tests/fixtures/component_generated_script.idl.json` で宣言/signature/map data を出力し、`guest` を `c` に置き換えると生成 header を出力します。最後に任意で `aarch64-apple-darwin` または `x86_64-pc-windows-msvc` を指定できます。guest JSON は tooling の確認用出力であり、compiler API は型付き version-1 `Input` を受け取ります。ここでは serialized executable/package profile を受理しません。export body は明示的に実装します。生成 C view は frame/scratch storage を借用し、result handle は対応 API で明示解放します。実 canonical 実行/resource 統合は board の後続 delivery に維持します。

I05 selection checkpoint（2026-10-23 JST）: `component::wire::select_provider` は検証済み consumer import 1件を、provider が export する interface と照合します。candidate metadata の調査前に必須 capability 名を確認し、未使用宣言/lock を含む完全 canonical contract に現在の上限を適用して、alias 解決済み LCSG preimage bytes を比較します。member/capability/effect/構造の不一致は元 consumer/provider の span と index を保持します。既存 exact/interval/prerelease/build-sensitive version policy を再利用し、互換最高版の選択・重複/曖昧 identity の拒否・fallback なしの exact-lock replay を計量します。明示的な比較/version 文字列生成の cost・scratch allocation・serialization は失敗時も同じ呼出側 Session を消費します。不変の選択結果は contract を借用し、主張された SHA-256 identity/member 対応を記録しますが、callable・登録 provider・code 所有権・resource 権限は供給しません。P01 がこれらを実 typed grant と保持した実装へ接続します。追加的な metadata API であり、source syntax・旧 negotiation の挙動・C ABI・LCMP/LCVL/LCSG bytes・package 受理は変更しません。focused 回帰12件と関連58テストが成功しました。source 固定の全 baseline は137 suite / 2,071件 / 失敗0 / 既存 timing ignore 1 と、workspace/fuzz fmt・警告をエラー扱いにした Clippy・rustdoc・全14 fuzz target compile/lint が成功しました。production/validation hash は一致し、campaign や実 grant/call は推論しません。

P01 binding 表現決定（2026-10-23 JST）: preparation request は独立 source/MIR unit と同じ検証済み文書 snapshot の wire contract を組にします。grant 解決前に完全な文書 identity を確認し、span/ID や metadata が異なる第2文書をその code に関する主張として扱いません。明示 native grant は interface/member・capability・完全 LCSG signature preimage を指定します。全 grant と未使用を含む全 source import を provider inventory query 前に検証し、未知/重複/欠落/不一致を拒否します。native 登録 record は検証済み exported member と固定 component-v1 invoke 関数 pointer・user pointer・借用した実 code/user owner を、既存 C context の lifetime pattern に従って組にします。不変の解決済み binding は record/owner を保持し、digest だけでは callable を構築できません。native caller は実 code/user storage を指定 owner の lifetime 内に保持する必要があります。pointer の有効性は trusted native 側の義務で、preparation 中に callback を呼びません。raw-pointer adapter と元 context での実行は resource/embedding delivery に維持します。この追加的 Rust binding API は C ABI v1 を変更せず、新たな wire/package profile も受理しません。検証済み文書の二重表現は明示的に制限する preparation cost であり第2 schema ではなく、後続 artifact 改訂で所有を統合できます。source facts・native 権限・lifetime を公開前に接続する必要性を理由に RFC 0001 の委任で受理し、callback 実行/artifact 統合は未検証に維持します。

P01 grant checkpoint（2026-10-23 JST）: `component::grants::prepare` は独立 source/MIR unit と、source identity・span・未使用 metadata まで厳密に同じ検証済み文書を組にします。明示された全 member/capability/完全 LCSG grant と未使用を含む全 source import を、最初の inventory 参照前に検証します。有限な native 登録は exported member・実 component-v1 invoke/user pointer・借用 code/user owner を保持します。選択は I05 を再利用し、不変 binding は source ImportId と選択した実装の provenance を保存して、全件成功後だけ公開します。単一 Session が snapshot 比較・canonical 再受理・grant・選択・binding storage を返却なしで計量します。focused 回帰8件は厳密上限/1不足 budget・lookup 前の不正 grant 拒否・callback/owner の不変・部分公開のない後段失敗を検証します。trusted native 登録/preparation であり、pointer の有効性は native caller の義務です。実 invocation・context 付き resource transfer・元 context の callback・serialized/installed profile は R01–R03/E01–E04/I07/I08 に維持します。source syntax・canonical bytes・C ABI v1 は変更しません。

R01/R04 runtime entry checkpoint（2026-10-23 JST）: native resource descriptor は実固定 component-v1 destroy/user callback と code owner を保持し、runtime kind identity を provider 登録に封印します。source tag は global な static-key 選択ではなく、実際に呼び出す import の選択済み provider で解決します。事前予約した resource-table locator と、独立 unit 検証付き非公開32-byte resource token が既存 engine への接続を支えます。通常 layout/runtime/C/wire/package profile は拒否を維持します。既存 VM/native root API は引数なし/i64 戻り値のため、R01 の完全な context 付き frame preflight/commit 前に、R04 で型付き component resource 引数/結果 entry を明示的に供給します。この追加的非公開 entry は受理済み source/所有権/allocator/context 契約に従い、新たな source language・evaluator fallback・serialized/native-package 受理・C ABI 改訂は受理しません。元 context の control・必須 cleanup・実 source/VM/native 証拠・後続の完全 resource/callback/C surface は必須に維持します。IDL の import projection 非空規則も変更しません。RFC 0001 の委任で実装順序を受理し、現状の source-kind/layout probe は resource source 実行ではありません。

R04 実行 checkpoint（2026-10-23 JST）: 検証済み `component::program::Program` と非公開 root transport が、直接所有する Resource 引数/結果と i64/bool leaf を既存の共通 task Machine に接続します。VM は source body を解釈し、native backend は body と continuation を生成します。実 resource は元 allocator の payload/header region・既存 generation table・provider ごとに封印した実装 identity・事前予約済み transfer plan を使います。引数 preflight は新 ticket を staging し、allocation/callback なしで commit します。commit 前の拒否は caller を保持し、commit 後の失敗は rollback せず退役させます。成功結果は callee の必須 cleanup と service close まで escrow に保持し、その後、同じ backing に新 caller ticket を公開します。退役は先に unlink して保持済みの実 destructor を呼び、失敗後も元 storage を解放し、主エラーを保存して cleanup failure で poison します。native bridge は安定した engine storage と元 control/allocator を使い、借用 callback scope は復帰時に失効します。この非公開 entry の現対応は直接 Resource と scalar root です。rich context 付き frame/入れ子 owner/直接 loan（R01）・完全 resource/callback/C 操作（R02/R03/E01–E04）・serialized/installed profile・有限 service-work 対応は別の必須事項に維持します。通常 layout/runtime/C/wire/package profile は拒否を維持します。

R01/R05 source value entry 決定（2026-10-23 JST）: 独立検証済み rich source signature は存在しますが、最初の入れ子 Resource record fixture は直接 root carrier により副作用前に拒否されます。既存 VM/native engine に非公開の型付き所有値 transport を追加し、引数 commit 前の全 staging 予約/cleanup と、caller 表現準備までの rich 結果 escrow を要求します。resource index は元 table に対して検証する未信頼 claim のままとし、通常 data-only/wire/C/package gate は閉じたままです。新 language・evaluator・registry・persisted format を導入せず、受理済み rich component 境界を実装します。RFC 0001 の委任により実装前に R05 を記録し、R01 の完全な context 付き preflight/直接 loan 要件を維持します。

R05 所有値 transport checkpoint（2026-10-23 JST）: context 付き引数 frame を全 decode し、provider の副作用前に resource index・key・alias・容量・元 caller claim を検証します。不透明な call 内 root locator は受理済みの不変 arena を指し、arena の置換で旧 locator は失効します。既存 layout・buffer・Vector を使う型付き reservation により、record/variant/Option/Result、Unit/i64/bool、Text/Bytes、list を staging します。反復 list 構築では未完成 element を公開済み parent element と分離し、失敗時は新規 data を退役させて commit 前の caller resource を保持します。rich 結果は canonical bytes と新 caller claim を全予約し、builtin data の退役・callee service close 後に所有権を commit します。明示 output Session は scalar/直接 Resource 結果にも同じ canonical 結果経路を適用します。resource 退役は元 allocator・native 実装・code owner を保持します。source fixture は VM/native の実成功、多段 list、途中の割り当て失敗を検証します。R01 の完全な context 付き直接 loan/preflight conformance、後続 native 操作、統一 service-work 計量、serialized/installed profile は必須に維持し、data-only codec は Resource index を拒否します。

## Sources

- R05 repository evidence: この RFC は2026-10-23更新、所有値 transport/source 実行と入力固定の全 baseline は2026-10-23 11:04 JST に macOS AArch64 で検証しました（141 suites / 2,112 passed / 失敗0 / 既存 timing ignore 1）。code に更新日表記はありません。全14 fuzz target の compile/lint が成功し、campaign・完全 loan 対応・Windows runtime conformance は主張しません。

- P01 リポジトリ証拠: 本 RFC の更新日は2026-10-23。typed grant・厳密 snapshot の組合せ・実 native callback/code-owner 保持・source 固定の全 baseline を2026-10-23 04:53 JST、macOS AArch64で検証しました。code に更新日の埋込みはありません。preparation/登録は callback 呼出しや Windows runtime conformance ではなく、全14 fuzz target の compile/lint が成功し campaign は行っていません。
- I05 リポジトリ証拠: 本 RFC の更新日は2026-10-23。rich selection・元 span 診断・完全 canonical 再受理・既存 version policy・回帰12件・拡張 wire fuzz target を2026-10-23 03:51 JST、macOS AArch64 で確認/実行/検証しました。code に更新日の埋込みはありません。次の P01 監査に向け既存 schema-2 instantiation と代表 application を調査しましたが、prototype binding は production typed grant を立証しません。
- I04 リポジトリ証拠: 本 RFC の更新日は2026-10-23。generator・元 span map・独立 reference packer/vector・生成 C/source fixture・example command を2026-10-23 02:39 JST、macOS AArch64 で確認/実行/検証しました。code に更新日の埋め込みはありません。両 target の C11/C++20 compile は cross compile の証拠です。最終 baseline は構造的 C 型 identity 修正後のもので、古い lang-syntax check metadata を意味論/SDK 検証の変更なしに再構築しました。
- Repository の事実: component enum/application fixture・独立 MIR constructor・task boundary test の code は `7381e09` で確認し、2026-10-22 JST に検証しました。code に更新日の埋込みはありません。参照 RFC 0003 の follow-up は2026-10-22更新、RFC 0006 は2026-10-17、0008/0015/0017/0019/0020 は2026-10-20、0023 は2026-10-22です。既存 application/task boundary probe は production embedding の証拠ではありません。
- [WIT specification](https://github.com/WebAssembly/component-model/blob/d6a17ea9f828f44c4659d70c35822ab53d2a8a1d/design/mvp/WIT.md): 文中の更新日なし、最新 path commit `d6a17ea9` は2026-09-15 17:03 UTC、2026-10-22 JST確認。宣言 interface type と owned/borrowed resource は参照事実であり、この RFC の JSON/access mode/tag は project decision です。
- [Canonical ABI](https://github.com/WebAssembly/component-model/blob/2e7895c76905cd3fb7e6be4ac632258972b1f688/design/mvp/CanonicalABI.md): 文中の更新日なし、最新 path commit `2e7895c7` は2026-10-13 19:06 UTC、2026-10-22 JST確認。typed lifting/lowering・resource table/borrow scope・gated concurrency が参照事実であり、ここでの native layout/再入 policy は Canonical ABI conformance の主張ではありません。
- [Concurrency explainer](https://github.com/WebAssembly/component-model/blob/0220da84209397d9067c19cac83dae969aa85a7b/design/mvp/Concurrency.md): 文中の更新日なし、最新 path commit は2026-10-15、2026-10-22 JST確認。gated concurrency・取消・再入を説明しており、v1 の同期 native callback policy は project decision です。
- [JSON RFC 8259](https://www.rfc-editor.org/rfc/rfc8259): 2017年12月公開、2026-10-22 JST確認。JSON syntax/UTF-8 interchange が参照事実であり、重複/未知 field 拒否と IDL 上限は project 要件です。
- [Rustonomicon FFI](https://doc.rust-lang.org/nomicon/ffi.html) と [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html): 更新日未公表の living document、2026-10-22 JST確認。foreign unwind/lifetime 義務と SemVer precedence/互換規約を参照し、RFC 0003 のより厳密な build-sensitive exact lock を維持します。
