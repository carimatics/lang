<!-- translation-of: ../../rfcs/0027-production-component-interface.md -->

# RFC 0027: Production component IDL・wire・embedding profile

[English](../../rfcs/0027-production-component-interface.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-22
- Last updated: 2026-10-22
- Tracking issue: M3-I01 through M3-I08, M3-R01 through M3-B02
- Supersedes:
- Superseded by:

## Summary

component IDL v1、rich component-contract schema 3、typed value-frame v1、独立した component C ABI v1 を定義します。[RFC 0003](0003-versioned-component-contracts.md) の capability・negotiation・所有権・failure・poison 規則を維持します。native host が検証済み source または検証した artifact を読み込み、型付き import と grant を供給して script export を呼び、同じ計上対象 invocation で nested guest callback を受け取ります。native body の演算/制御は生成 native code のままです。adapter は MIR 解釈へ fallback しません。

これは実装方針の決定であり、実装や安定化の証拠ではありません。既存 scalar/resource application prototype と M2 C profile 1 はこの profile の実装ではありません。実行と両 host の証拠は実装 Todo、公開 C の安定化は B02 が管理します。

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

生成 source は既存 frontend を使います。例えば `struct CounterKind {}`、`type Counter = core::ComponentResource<CounterKind>;` と `extern "component" fn apply(state: &mut Counter, request: Request) -> core::Result<i64, core::Text>;` です。versioned compiler binding input が qualified tag name→resource key、component import name→interface/member key、export member→既存 public source function name を対応させます。これは compiler data であり、ambient grant や別 source language ではありません。検証した IDL と全 mapping/signature/effect が一致する必要があります。tag は通常 data として命名/構築できますが resource owner を構築できず、検査付き component transfer だけが owner を生成します。component import は別 ABI/effect identity を持ち、Host/C import と置換しません。map と独立した resource/type/loan/effect の全事実を MIR/artifact 検証で保持します。I06 がこの拡張を実装し、通常/旧 profile は拒否を維持します。

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

完全な規範 surface は [component-abi-v1.h](../../component-abi-v1.h)、[layout vector](../../component-abi-v1-layouts.json) は両 tier-1 target の独立した期待 size/alignment と選択 offset を記録します。参照物は decision artifact であり library 実装ではありません。I04 が同じ surface と完全 layout/signature fingerprint を生成します。concrete struct は厳密 size/version と reserved-zero を要求し、view は規定の固定 layout leaf です。C padding ではなく named field だけを読み書きします。API table は両 target とも280 bytesで、全32 entry は non-null、利用できない mode は unsupported-profile を返します。C11/C++20 の target compile は header/layout 互換だけを証明し、Windows の runtime conformance ではありません。

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

I02 が有界 IDL parse/check/span、I03 が rich metadata/value codec・独立 canonical vector・厳密 report tag/vector・不正/budget test、I06 が source/MIR component-resource/call/effect proof と未対応拒否、I04 が型付き C/guest binding と完全 header/layout vector、I05 が interface/version negotiation を供給します。R01–R03 で実 resource transfer/loan/cleanup/failure、P01/P02 で実 typed grant と有界 metadata/adapter work を接続します。E01–E04 は host/script call・同期 nested callback・元 context の failure・mixed stack/cancellation、L01–L04 は合算 limit を実証します。I07/I08 は B01 前に serialized task/component と native-package profile を明示的に供給します。B01/B02 は両 host conformance/C migration 安定化、X/Z は runnable plugin example・cost・最終 guide を担当します。各1–3点の delivery を実行証拠で再監査し、欠けた前提は可視化してから実装し、source fixture を横断基盤だけで置換しません。

## Test plan

独立 JSON/binary vector、byte/count/depth/work/allocation の厳密上限/1不足、重複/未知 field、不正 Unicode/name/ref/effect/loan、型循環、canonical 順序、不正/truncated/trailing frame、旧 schema 拒否を検証します。構造 projection・SemVer/prerelease/build 曖昧さ・exact-lock replay を codec と独立に比較します。全 preflight 拒否で元 ownership を保ち、一度の commit/stale index、shared/exclusive 競合、nested owner、逆順 cleanup を証明します。生成 wrapper の実 C caller で検証 source/artifact/callback を VM/JIT/別リンク AOT から呼び、root task/channel/service fixture を含めます。値・usage・commit state・resource/heap inventory・span・logical stack・primary/suppressed/omitted report・poison を対応する全 control/provider/割込み境界で比較します。callback grant 拒否・再入 policy・失効/別 thread token・再帰 budget・host allocation・commit/callback 前後取消も検証します。macOS AArch64/Windows x86-64 MSVC で isolated installed package と migration を実行します。fuzz compile/lint を campaign とせず、campaign/計測は別の記録付き証拠とします。

## Open questions

参照 header/report tag の review 後、blocking な意味論の質問はありません。WIT adapter・非同期 native callback・再帰 value schema・sendable external resource kind は optional follow-up で、v1 受理を暗黙に弱めません。未検証仮説は compiler/runtime 再利用 cost、生成 C の ergonomics、callback/task 統合、両 host の実行/startup です。委任受理から性能/実装結果を推論しません。

Review outcome: **RFC 0001 の実験期委任に基づき2026-10-22 JST、production 実装前に accepted**。厳密な宣言文書と独立 version namespace で既存 component/resource/cleanup 機構を拡張し、旧 C/artifact profile を保ちます。shared/exclusive resource access・同期 nested callback・元 invocation 計上・明示 failure/ownership phase により、suspension loan の緩和や native VM fallback なしに production policy を決定します。既存実行 probe 2件、最終 header の各 target 112 C layout assertion と両 tier-1 target の C++20 syntax が成功しました。これは decision/互換性の証拠で、production 実行や Windows runtime の証拠ではありません。source/MIR/resource/callback 統合・完全な生成 fingerprint・serialized task/native-package rollout・cost・両 host 実行は未検証の実装条件として Todo に残します。

## Sources

- Repository の事実: component enum/application fixture・独立 MIR constructor・task boundary test の code は `7381e09` で確認し、2026-10-22 JST に検証しました。code に更新日の埋込みはありません。参照 RFC 0003 の follow-up は2026-10-22更新、RFC 0006 は2026-10-17、0008/0015/0017/0019/0020 は2026-10-20、0023 は2026-10-22です。既存 application/task boundary probe は production embedding の証拠ではありません。
- [WIT specification](https://github.com/WebAssembly/component-model/blob/d6a17ea9f828f44c4659d70c35822ab53d2a8a1d/design/mvp/WIT.md): 文中の更新日なし、最新 path commit `d6a17ea9` は2026-09-15 17:03 UTC、2026-10-22 JST確認。宣言 interface type と owned/borrowed resource は参照事実であり、この RFC の JSON/access mode/tag は project decision です。
- [Canonical ABI](https://github.com/WebAssembly/component-model/blob/2e7895c76905cd3fb7e6be4ac632258972b1f688/design/mvp/CanonicalABI.md): 文中の更新日なし、最新 path commit `2e7895c7` は2026-10-13 19:06 UTC、2026-10-22 JST確認。typed lifting/lowering・resource table/borrow scope・gated concurrency が参照事実であり、ここでの native layout/再入 policy は Canonical ABI conformance の主張ではありません。
- [Concurrency explainer](https://github.com/WebAssembly/component-model/blob/0220da84209397d9067c19cac83dae969aa85a7b/design/mvp/Concurrency.md): 文中の更新日なし、最新 path commit は2026-10-15、2026-10-22 JST確認。gated concurrency・取消・再入を説明しており、v1 の同期 native callback policy は project decision です。
- [JSON RFC 8259](https://www.rfc-editor.org/rfc/rfc8259): 2017年12月公開、2026-10-22 JST確認。JSON syntax/UTF-8 interchange が参照事実であり、重複/未知 field 拒否と IDL 上限は project 要件です。
- [Rustonomicon FFI](https://doc.rust-lang.org/nomicon/ffi.html) と [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html): 更新日未公表の living document、2026-10-22 JST確認。foreign unwind/lifetime 義務と SemVer precedence/互換規約を参照し、RFC 0003 のより厳密な build-sensitive exact lock を維持します。
