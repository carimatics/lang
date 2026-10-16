use lang::linker::LinkTarget;
use lang::{
    prototype_add_function, prototype_host_allocation_burst_function,
    prototype_host_allocation_churn_function, prototype_host_allocation_variable_function,
    prototype_host_callback_function, prototype_sum_below_function, prototype_sum_to_function,
};
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
    let allocation_burst =
        emit_object_for_target(&prototype_host_allocation_burst_function(), target.clone())
            .expect("emits the host-allocation burst function for Windows x86-64");
    let allocation_churn =
        emit_object_for_target(&prototype_host_allocation_churn_function(), target.clone())
            .expect("emits the host-allocation churn function for Windows x86-64");
    let allocation_variable = emit_object_for_target(
        &prototype_host_allocation_variable_function(),
        target.clone(),
    )
    .expect("emits the variable host-allocation function for Windows x86-64");
    let sum_below = emit_object_for_target(&prototype_sum_below_function(), target.clone())
        .expect("emits the conditional sum function for Windows x86-64");
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

    let allocation_burst_object = object::File::parse(allocation_burst.bytes())
        .expect("parses the host-allocation burst COFF object");
    assert_eq!(allocation_burst_object.format(), BinaryFormat::Coff);
    assert_eq!(allocation_burst_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &allocation_burst_object,
        allocation_burst.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_burst_object,
        "lang_host_i64_2"
    ));

    let allocation_churn_object = object::File::parse(allocation_churn.bytes())
        .expect("parses the host-allocation churn COFF object");
    assert_eq!(allocation_churn_object.format(), BinaryFormat::Coff);
    assert_eq!(allocation_churn_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &allocation_churn_object,
        allocation_churn.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_churn_object,
        "lang_host_i64_3"
    ));

    let allocation_variable_object = object::File::parse(allocation_variable.bytes())
        .expect("parses the variable host-allocation COFF object");
    assert_eq!(allocation_variable_object.format(), BinaryFormat::Coff);
    assert_eq!(
        allocation_variable_object.architecture(),
        Architecture::X86_64
    );
    assert!(has_defined_symbol(
        &allocation_variable_object,
        allocation_variable.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_variable_object,
        "lang_host_i64_4"
    ));

    let sum_below_object =
        object::File::parse(sum_below.bytes()).expect("parses the conditional sum COFF object");
    assert_eq!(sum_below_object.format(), BinaryFormat::Coff);
    assert_eq!(sum_below_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &sum_below_object,
        sum_below.function_symbol()
    ));

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
