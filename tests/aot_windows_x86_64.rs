use lang::linker::LinkTarget;
use lang::{prototype_add_function, prototype_host_callback_function, prototype_sum_to_function};
use lang_codegen_clif::{Triple, emit_object_for_target};
use object::{Architecture, BinaryFormat, Object, ObjectSymbol};

#[test]
fn emits_windows_x86_64_coff_with_expected_symbols() {
    let target = LinkTarget::WindowsX86_64Msvc
        .triple()
        .parse::<Triple>()
        .expect("the Windows x86-64 target triple is valid");
    let add = emit_object_for_target(&prototype_add_function(), target.clone())
        .expect("emits the add function for Windows x86-64");
    let callback = emit_object_for_target(&prototype_host_callback_function(), target.clone())
        .expect("emits the host-callback function for Windows x86-64");
    let sum = emit_object_for_target(&prototype_sum_to_function(), target)
        .expect("emits the sum function for Windows x86-64");

    let add_object = object::File::parse(add.bytes()).expect("parses the add COFF object");
    assert_eq!(add_object.format(), BinaryFormat::Coff);
    assert_eq!(add_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(&add_object, add.function_symbol()));

    let callback_object =
        object::File::parse(callback.bytes()).expect("parses the callback COFF object");
    assert_eq!(callback_object.format(), BinaryFormat::Coff);
    assert_eq!(callback_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &callback_object,
        callback.function_symbol()
    ));
    assert!(has_undefined_symbol(&callback_object, "lang_host_i64_0"));

    let sum_object = object::File::parse(sum.bytes()).expect("parses the sum COFF object");
    assert_eq!(sum_object.format(), BinaryFormat::Coff);
    assert_eq!(sum_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(&sum_object, sum.function_symbol()));
}

fn has_defined_symbol(object: &object::File<'_>, name: &str) -> bool {
    object
        .symbols()
        .any(|symbol| symbol.name() == Ok(name) && !symbol.is_undefined())
}

fn has_undefined_symbol(object: &object::File<'_>, name: &str) -> bool {
    object
        .symbols()
        .any(|symbol| symbol.name() == Ok(name) && symbol.is_undefined())
}
