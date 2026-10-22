use lang_diagnostics::SourceMap;
use lang_syntax::{SyntaxKind, parse};

#[test]
fn component_imports_use_the_lossless_foreign_declaration_shape() {
    let text =
        "extern \"component\" fn ping(value: i64) -> i64; fn main() -> i64 { return ping(42); }";
    let mut sources = SourceMap::new();
    let source = sources.add("component.lang", text).unwrap();
    let tree = parse(&sources, source, Default::default()).unwrap();
    assert!(tree.diagnostics().is_empty(), "{:?}", tree.diagnostics());
    assert_eq!(tree.reconstruct(&sources).unwrap(), text);
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == SyntaxKind::ForeignImport)
            .count(),
        1
    );
}

#[test]
fn component_exports_use_public_guest_functions_and_native_imports_are_synchronous() {
    for text in [
        "pub extern \"component\" fn ping(value:i64)->i64{return value;}",
        "extern \"component\" async fn ping(value:i64)->i64;",
        "extern \"Component\" fn ping(value:i64)->i64;",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add("invalid-component.lang", text).unwrap();
        let tree = parse(&sources, source, Default::default()).unwrap();
        assert!(!tree.diagnostics().is_empty(), "{text}");
        assert_eq!(tree.reconstruct(&sources).unwrap(), text);
    }
}
