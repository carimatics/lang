<!-- translation-of: README.md; canonical-language: en -->

# 権限を明示するcomponent plugin

[English](README.md)

固定したRust toolchainでrepository rootから実行します。

```bash
cargo run --example component-plugin-host -- vm
cargo run --example component-plugin-host -- jit
```

hostはtrusted native math pluginをリンクし、公開component-v1 APIから検証済みscript pluginをloadします。[verified.lang](verified.lang)は通常のsourceで、native `increment` importを呼び、負の入力ではpanicします。native incrementはi64の飽和加算です。2つのIDLが厳密なtyped境界を定義し、hostはsource名を明示的に対応付け、`example.math.increment`だけをgrantします。sourceをcanonical component artifactへcompileし、実行前の`module_load`でそのartifactを独立に再検証します。pluginの自動探索や暗黙grantは行いません。

両commandは次の観測結果を検査します。

- grantを省略するとinstance作成を拒否し、outputを変更せずnative callは0件です。
- 入力41は42を返します。入力−1はpanicを報告し、その後の入力0は同じinstanceで1を返します。
- fuel 0はnative plugin実行前に構造化された上限失敗を返します。
- native callは正確に2件です。保持した失敗resultがinstance/provider close後も実native ownerを保持し、最後のresult解放でownerを一度だけ解放します。
- runtime管理の割当ては0件へ退役します。

同じ公開API/native callback経路の有限release計測は[M3 benchmark手順](../../docs/ja/benchmarks.md#milestone-3-concurrencyとcomponent-entryのcost)を参照してください。任意の`--measure 21`引数で、この例の正しさ/寿命検査を維持したままJSONを出力します。

最後の出力は`VM:`または`JIT:`に続いて次の内容になります。

```text
script=42; native calls=2; grant denied; panic; reuse=1; fuel refused; owner released; live allocations=0
```

native実装はhost process内のtrusted codeです。元contextを協調的に検査し、そのcontextで割り当てたbufferを返します。runtime上限はfuel/service work各10,000、host call 8、管理heap 1 MiB、live allocation 256、frame 32、live task 1で、準備とcanonical decodeにも別の有限上限があります。固定scalar native codecは別の有界scratch Sessionを使います。これらの上限は任意のnative codeをsandbox化しません。scriptに渡るのは宣言済みtyped importだけで、filesystem・network・environment・process importは与えません。

これは実験段階のRFC 0027 interfaceです。Windows実行・実両host適合・C安定化は[M3表](../../docs/ja/milestone-3.md)に残ります。installed厳密SDKのAOT bindingは[native SDK guide](../../docs/ja/native-sdk.md)を参照してください。

出典: [host](../component-plugin-host.rs)、[script IDL](verified.idl.json)、[native IDL](native.idl.json)、[RFC 0027](../../docs/ja/rfcs/0027-production-component-interface.md)。codeに公開更新日記載はなく、この例とVM/JIT実行は**2026-10-27 JST検証**です。RFC 0027は**2026-10-27 JST更新**です。
