use super::*;

const SOURCE: &str = include_str!("../../examples/analysis/c_exports.lang");

#[test]
fn ordinary_owned_object_api_preserves_the_explicit_c_export_boundary() {
    let owner = source(SOURCE);
    let flow = owner.failure_flow();
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let error = lang_codegen_clif::owned::emit_object_for_target(
            flow,
            &layouts,
            &target.parse().unwrap(),
            Default::default(),
            4096,
        )
        .err()
        .unwrap();
        assert!(matches!(
            error,
            lang_codegen_clif::owned::Error::UnsupportedCInterface { .. }
        ));
    }
}
