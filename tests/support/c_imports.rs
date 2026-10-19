//! Real compiler-produced C callbacks, reused by the source execution fixtures.
use super::{Artifact, HeaderOptions, Kind, Limits, Target, artifact, source};
use lang_codegen_clif::c_abi::{allocator::CAllocator, scalar::NativeInvoker};
use lang_runtime::c_abi::context::{
    self as c,
    config::{self, Binding},
    scalar::Scalar,
};
use lang_runtime::{allocation as a, execution as e};
use std::{
    cell::Cell,
    ffi::{CString, c_void},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const IMPORTS: &str = r#"
extern "C" fn increment(value: i64) -> i64;
extern "C" fn flag(value: bool) -> bool;
extern "C" fn notify(value: core::Unit) -> core::Unit;
extern "C" fn combine(a: i64, b: bool, c: core::Unit, d: i64, e: bool, f: i64, g: bool, h: i64, i: i64, j: bool, k: i64, l: bool) -> i64;
"#;
#[path = "c_imports_native.rs"]
mod native;
#[repr(C)]
#[derive(Default)]
struct Stats {
    effects: Cell<u64>,
    allocations: Cell<u64>,
    frees: Cell<u64>,
    fail: Cell<u32>,
    wrong: Cell<u32>,
    refuse_at: Cell<u64>,
}
#[derive(Clone, Copy)]
enum Refusal {
    None,
    First,
    At(u64),
}
impl From<bool> for Refusal {
    fn from(value: bool) -> Self {
        if value { Self::First } else { Self::None }
    }
}
impl Refusal {
    fn apply(self, stats: &Stats) {
        match self {
            Self::None => {}
            Self::First => stats.fail.set(1),
            Self::At(at) => stats.refuse_at.set(at),
        }
    }
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dlopen(name: *const std::ffi::c_char, mode: std::ffi::c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> std::ffi::c_int;
}
#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(
        handle: *mut c_void,
        name: *const u8,
    ) -> Option<unsafe extern "system" fn() -> isize>;
    fn FreeLibrary(handle: *mut c_void) -> i32;
}
struct Library {
    handle: *mut c_void,
    directory: PathBuf,
}
// A whole C struct/union assignment need not preserve inactive storage. The
// trusted fixture obeys profile 1 on either C compiler by copying active fields
// and explicitly zeroing unused union bytes; ordinary padding stays irrelevant.
fn c_copy_fields(
    artifact: &Artifact,
    id: lang_runtime::c_abi::TypeId,
    from: &str,
    to: &str,
) -> String {
    match &artifact.types()[id.0].kind {
        Kind::I64 | Kind::Bool => format!("{to} = {from}; "),
        Kind::Unit => format!("{to}.reserved = {from}.reserved; "),
        Kind::Record { fields, .. } => {
            if fields.is_empty() {
                return format!("{to}.reserved = {from}.reserved; ");
            }
            fields
                .iter()
                .map(|f| {
                    c_copy_fields(
                        artifact,
                        f.ty,
                        &format!("{from}.f_{}", f.name),
                        &format!("{to}.f_{}", f.name),
                    )
                })
                .collect()
        }
        Kind::Enum { cases, .. } => {
            let mut code = format!(
                "{to}.tag = {from}.tag; memset(&{to}.payload, 0, sizeof({to}.payload)); switch ({from}.tag) {{ "
            );
            for (tag, case) in cases.iter().enumerate() {
                code.push_str(&format!("case {tag}: "));
                if case.payload.is_empty() {
                    code.push_str(&format!(
                        "{to}.payload.c{tag}.reserved = {from}.payload.c{tag}.reserved; "
                    ));
                }
                for (index, id) in case.payload.iter().enumerate() {
                    code.push_str(&c_copy_fields(
                        artifact,
                        *id,
                        &format!("{from}.payload.c{tag}.p{index}"),
                        &format!("{to}.payload.c{tag}.p{index}"),
                    ));
                }
                code.push_str("break; ");
            }
            code.push_str("default: return 1; } ");
            code
        }
    }
}
impl Library {
    fn new(artifact: &Artifact) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "lang-c-imports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let unit = artifact
            .types()
            .iter()
            .find(|ty| matches!(ty.kind, Kind::Unit))
            .map(|ty| format!("lang_type_{}", ty.digest));
        let mut code = if unit.is_none() {
            "typedef struct { uint8_t reserved; } fixture_unit;\n".into()
        } else {
            String::new()
        };
        code.push_str(
            &include_str!("c_imports.c")
                .replace("UNIT_TYPE", unit.as_deref().unwrap_or("fixture_unit")),
        );
        for function in artifact
            .functions()
            .iter()
            .filter(|f| f.name.rsplit("::").next() == Some("inspect"))
        {
            let ty = &artifact.types()[function.parameters[0].0];
            code.push_str(&format!("\nEXPORT uint32_t c_inspect(lang_context *context, void *user, const lang_type_{digest} *value, uint8_t enabled, int64_t *out) {{ if (!context) return 1; ++((fixture_stats *)user)->effects; *out = value->f_number + value->f_flag + enabled; return 0; }}\n",digest=ty.digest));
        }
        for function in artifact
            .functions()
            .iter()
            .filter(|f| f.name.rsplit("::").next() == Some("aggregate_many"))
        {
            let mut parameters = Vec::new();
            let mut values = Vec::new();
            for (i, id) in function.parameters.iter().enumerate() {
                let ty = &artifact.types()[id.0];
                let (parameter, value) = match ty.kind {
                    Kind::I64 => (format!("int64_t v{i}"), format!("v{i}")),
                    Kind::Bool => (format!("uint8_t v{i}"), format!("v{i}")),
                    Kind::Unit => (
                        format!("const lang_type_{} *v{i}", ty.digest),
                        format!("v{i}->reserved"),
                    ),
                    Kind::Record { .. } => (
                        format!("const lang_type_{} *v{i}", ty.digest),
                        format!("v{i}->f_number"),
                    ),
                    _ => panic!("fixture parameter"),
                };
                parameters.push(parameter);
                values.push(value);
            }
            let digest = artifact.types()[function.result.0].digest;
            code.push_str(&format!("\nEXPORT uint32_t c_aggregate_many(lang_context *context, void *user, {}, lang_type_{digest} *out) {{ if (!context) return 1; ++((fixture_stats *)user)->effects; out->f_number = {}; out->f_flag = 1; return 0; }}\n",parameters.join(", "),values.join(" + ")));
        }
        for function in artifact.functions().iter().filter(|f| {
            f.name
                .rsplit("::")
                .next()
                .is_some_and(|name| name.starts_with("echo"))
        }) {
            assert_eq!(function.parameters.len(), 1);
            assert_eq!(function.parameters[0], function.result);
            let ty = &artifact.types()[function.result.0];
            let name = function.name.rsplit("::").next().unwrap();
            let extra = match name {
                "echo_bad_bool" => "out->f_flag = 2;",
                "echo_bad_tag" => "out->tag = UINT32_MAX;",
                "echo_bad_reserved" => "out->reserved = 1;",
                "echo_bad_unused" => {
                    "((unsigned char *)&out->payload)[sizeof(out->payload) - 1] = 1;"
                }
                "echo_failure" => "return UINT32_MAX;",
                _ => "",
            };
            let copy = c_copy_fields(artifact, function.result, "(*value)", "(*out)");
            code.push_str(&format!("\nEXPORT uint32_t c_{name}(lang_context *context, void *user, const lang_type_{digest} *value, lang_type_{digest} *out) {{ if (!context) return 1; ++((fixture_stats *)user)->effects; memset(out, 0xa5, sizeof(*out)); {copy} {extra} return 0; }}\n",digest=ty.digest));
        }
        for function in artifact
            .functions()
            .iter()
            .filter(|f| f.name.rsplit("::").next() == Some("arity"))
        {
            let parameters = (0..function.parameters.len())
                .map(|i| format!("int64_t v{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let sum = (0..function.parameters.len())
                .map(|i| format!("v{i}"))
                .collect::<Vec<_>>()
                .join(" + ");
            code.push_str(&format!("\nEXPORT uint32_t c_arity(lang_context *context, void *user, {parameters}, int64_t *out) {{ if (!context) return 1; ++((fixture_stats *)user)->effects; *out = {sum}; return 0; }}\n"));
        }
        for function in artifact.functions() {
            match function.name.rsplit("::").next().unwrap() {
                "make_pair" => {
                    let digest = artifact.types()[function.result.0].digest;
                    code.push_str(&format!("\nEXPORT uint32_t c_make_pair(lang_context *context, void *user, lang_type_{digest} *out) {{ if (!context) return 1; ++((fixture_stats *)user)->effects; out->f_number = 56; out->f_flag = 1; return 0; }}\n"));
                }
                "consume_pair" => {
                    let digest = artifact.types()[function.parameters[0].0].digest;
                    code.push_str(&format!("\nEXPORT uint32_t c_consume_pair(lang_context *context, void *user, const lang_type_{digest} *value) {{ if (!context || value->f_number != 57 || value->f_flag != 1) return 1; ++((fixture_stats *)user)->effects; return 0; }}\n"));
                }
                _ => {}
            }
        }
        let mut checks = String::new();
        for function in artifact.functions() {
            let name = function.name.rsplit("::").next().unwrap();
            checks.push_str(&format!(
                "lang_callback_{} check_{name} = c_{name};\n",
                function.digest
            ));
        }
        let header = artifact
            .header(HeaderOptions::default(), Limits::default())
            .unwrap();
        let source = directory.join("callbacks.c");
        std::fs::write(&source, format!("{header}\n{code}\n{checks}")).unwrap();
        let library = directory.join(if cfg!(target_os = "windows") {
            "callbacks.dll"
        } else {
            "callbacks.dylib"
        });
        let output = if cfg!(target_os = "windows") {
            Command::new("cl")
                .current_dir(&directory)
                .args(["/nologo", "/std:c11", "/W4", "/WX", "/LD"])
                .arg(&source)
                .arg(format!("/Fe{}", library.display()))
                .output()
                .unwrap()
        } else {
            Command::new("cc")
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib"])
                .arg(&source)
                .arg("-o")
                .arg(&library)
                .output()
                .unwrap()
        };
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        #[cfg(target_os = "macos")]
        let handle = {
            let name = CString::new(library.as_os_str().as_encoded_bytes()).unwrap();
            // SAFETY: only this just-compiled trusted fixture is loaded. The
            // nul-terminated path is live and RTLD_NOW resolves its C imports.
            unsafe { dlopen(name.as_ptr(), 2) }
        };
        #[cfg(target_os = "windows")]
        let handle = {
            use std::os::windows::ffi::OsStrExt;
            let name: Vec<_> = library.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: the live terminated path names only our trusted fixture.
            unsafe { LoadLibraryW(name.as_ptr()) }
        };
        assert!(!handle.is_null());
        Self { handle, directory }
    }
    fn function(&self, name: &str) -> config::ErasedFunction {
        let name = CString::new(name).unwrap();
        #[cfg(target_os = "macos")]
        {
            // SAFETY: this retained image contains the exact C definitions
            // compiled above. POSIX dlsym supplies their code addresses; the
            // shared context subsequently stores a typed erased code pointer.
            let pointer = unsafe { dlsym(self.handle, name.as_ptr()) };
            assert!(!pointer.is_null());
            unsafe { std::mem::transmute(pointer) }
        }
        #[cfg(target_os = "windows")]
        {
            // SAFETY: a live owned module and an exact terminated export name.
            let function = unsafe { GetProcAddress(self.handle, name.as_ptr().cast()) }.unwrap();
            // SAFETY: FARPROC and the erased C code pointer have the asserted
            // profile-1 x64 representation. It is not invoked erased.
            unsafe { std::mem::transmute(function) }
        }
    }
    fn allocator(&self, stats: &Stats) -> config::Allocator {
        // SAFETY: c_allocate/c_deallocate definitions in the C fixture have
        // exactly these generated descriptor callback signatures.
        let allocate = unsafe {
            std::mem::transmute::<config::ErasedFunction, config::Allocate>(
                self.function("c_allocate"),
            )
        };
        let deallocate = unsafe {
            std::mem::transmute::<config::ErasedFunction, config::Deallocate>(
                self.function("c_deallocate"),
            )
        };
        config::Allocator {
            version: 1,
            size: 32,
            user: (stats as *const Stats).cast_mut().cast(),
            allocate: Some(allocate),
            deallocate: Some(deallocate),
        }
    }
}

#[test]
fn aggregate_c_imports_execute_records_and_sums_from_source() {
    let cases = [
        (
            "pub struct Pair { flag: bool, number: i64 } extern \"C\" fn echo_pair(value: Pair) -> Pair; fn main() -> i64 { unsafe { let mut value: Pair = echo_pair(Pair { number: 42, flag: true }); if value.flag { return value.number; } return 0; } }",
            42,
        ),
        (
            "pub struct Inner { number: i64, flag: bool } pub struct Outer { empty: core::Unit, inner: Inner, enabled: bool } extern \"C\" fn echo_nested(value: Outer) -> Outer; fn main() -> i64 { unsafe { let mut value: Outer = echo_nested(Outer { enabled: true, inner: Inner { flag: true, number: 43 }, empty: core::Unit {} }); return value.inner.number; } }",
            43,
        ),
        (
            "pub enum Choice { Empty, Number(i64, bool) } extern \"C\" fn echo_choice(value: Choice) -> Choice; fn main() -> i64 { unsafe { match echo_choice(Choice::Empty) { Choice::Empty => { return 44; }, Choice::Number(_, _) => { return 0; }, } } }",
            44,
        ),
        (
            "pub enum Choice { Empty, Number(i64, bool) } extern \"C\" fn echo_choice(value: Choice) -> Choice; fn main() -> i64 { unsafe { match echo_choice(Choice::Number(45, true)) { Choice::Empty => { return 0; }, Choice::Number(number, flag) => { if flag { return number; } return 0; }, } } }",
            45,
        ),
        (
            "extern \"C\" fn echo_option(value: core::Option<i64>) -> core::Option<i64>; fn main() -> i64 { unsafe { match echo_option(core::Option::Some(46)) { core::Option::Some(number) => { return number; }, core::Option::None => { return 0; }, } } }",
            46,
        ),
        (
            "extern \"C\" fn echo_option(value: core::Option<i64>) -> core::Option<i64>; fn main() -> i64 { unsafe { match echo_option(core::Option::None) { core::Option::Some(_) => { return 0; }, core::Option::None => { return 47; }, } } }",
            47,
        ),
        (
            "pub struct Pair { number: i64, flag: bool } extern \"C\" fn echo_result(value: core::Result<core::Option<Pair>, core::BufferError>) -> core::Result<core::Option<Pair>, core::BufferError>; fn main() -> i64 { unsafe { match echo_result(core::Result::Ok(core::Option::Some(Pair { number: 48, flag: true }))) { core::Result::Ok(value) => { match value { core::Option::Some(pair) => { if pair.flag { return pair.number; } return 0; }, core::Option::None => { return 0; }, } }, core::Result::Err(_) => { return 0; }, } } }",
            48,
        ),
        (
            "extern \"C\" fn echo_error(value: core::BufferError) -> core::BufferError; fn main() -> i64 { unsafe { match echo_error(core::BufferError::Allocation(core::AllocError::Overflow)) { core::BufferError::Allocation(error) => { match error { core::AllocError::Overflow => { return 49; }, core::AllocError::Limit => { return 0; }, core::AllocError::OutOfMemory => { return 0; }, } }, core::BufferError::Bounds => { return 0; }, core::BufferError::InvalidByte => { return 0; }, } } }",
            49,
        ),
        (
            "pub struct Empty {} extern \"C\" fn echo_empty(value: Empty) -> Empty; fn main() -> i64 { unsafe { let mut value: Empty = echo_empty(Empty {}); return 50; } }",
            50,
        ),
        (
            "extern \"C\" fn echo_utf8(value: core::Utf8Error) -> core::Utf8Error; fn main() -> i64 { unsafe { match echo_utf8(core::Utf8Error::Invalid(51)) { core::Utf8Error::Invalid(offset) => { return offset; }, } } }",
            51,
        ),
        (
            include_str!("../../examples/analysis/c_aggregate_imports.lang"),
            84,
        ),
        (
            "pub struct Pair { number: i64, flag: bool } extern \"C\" fn inspect(value: Pair, enabled: bool) -> i64; fn main() -> i64 { unsafe { return inspect(Pair { number: 52, flag: true }, true); } }",
            54,
        ),
        (
            "pub struct Pair { number: i64, flag: bool } extern \"C\" fn make_pair() -> Pair; fn main() -> i64 { unsafe { let mut result: Pair = make_pair(); if result.flag { return result.number; } return 0; } }",
            56,
        ),
        (
            "pub struct Pair { number: i64, flag: bool } extern \"C\" fn consume_pair(value: Pair) -> core::Unit; fn main() -> i64 { unsafe { let mut result: core::Unit = consume_pair(Pair { number: 57, flag: true }); return 57; } }",
            57,
        ),
    ];
    for (text, expected) in cases {
        let text = if expected == 84 {
            text.to_owned()
        } else {
            format!("{IMPORTS} {text}")
        };
        let owner = source(&text);
        let artifact = artifact(&text, target(), Limits::default()).unwrap();
        let library = Library::new(&artifact);
        let normal = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
        assert_eq!(normal.value, Some(expected), "{normal:?}");
        let declarations = owner
            .failure_flow()
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let layouts =
            lang_runtime::layout::Layouts::compile(declarations.types(), Default::default())
                .unwrap();
        let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
            owner.failure_flow(),
            &layouts,
            &artifact,
            Default::default(),
            4096,
        )
        .unwrap();
        assert_eq!(
            native::observe(&compiled, &artifact, &library, e::Control::default(), false),
            normal
        );
        for fuel in 0..=normal.usage.fuel {
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..Default::default()
            });
            assert_eq!(
                native::observe(&compiled, &artifact, &library, control, false),
                observe_vm(&owner, &artifact, &library, control, false),
                "fuel {fuel}, expected {expected}"
            );
        }
        for work in 0..=normal.usage.service_work {
            let control = e::Control::default().and_service_work(Some(work));
            assert_eq!(
                native::observe(&compiled, &artifact, &library, control, false),
                observe_vm(&owner, &artifact, &library, control, false),
                "work {work}, expected {expected}"
            );
        }
        for at in 1..=3 {
            let refusal = Refusal::At(at);
            let vm = observe_vm(&owner, &artifact, &library, e::Control::default(), refusal);
            assert!(vm.fault.is_some(), "allocation {at}");
            assert_eq!(vm.effects, 0, "allocation {at}");
            assert_eq!(
                native::observe(
                    &compiled,
                    &artifact,
                    &library,
                    e::Control::default(),
                    refusal
                ),
                vm,
                "allocation {at}, expected {expected}"
            );
        }
    }
}

#[test]
fn aggregate_c_output_refusals_preserve_failure_and_discard_results() {
    use lang_runtime::c_abi::validation::Error as V;
    for (name, ty, declaration, value, kind) in [
        (
            "echo_bad_bool",
            "Pair",
            "pub struct Pair { number: i64, flag: bool }",
            "Pair { number: 42, flag: true }",
            0,
        ),
        (
            "echo_bad_tag",
            "Choice",
            "pub enum Choice { Empty, Number(i64) }",
            "Choice::Empty",
            1,
        ),
        (
            "echo_bad_unused",
            "Choice",
            "pub enum Choice { Empty, Number(i64) }",
            "Choice::Empty",
            2,
        ),
        (
            "echo_bad_reserved",
            "Empty",
            "pub struct Empty {}",
            "Empty {}",
            3,
        ),
        (
            "echo_failure",
            "Pair",
            "pub struct Pair { number: i64, flag: bool }",
            "Pair { number: 42, flag: true }",
            4,
        ),
    ] {
        let text = format!(
            "{declaration} extern \"C\" fn {name}(value: {ty}) -> {ty}; fn main() -> i64 {{ unsafe {{ let mut value: {ty} = {name}({value}); return 999; }} }}"
        );
        let owner = source(&text);
        let artifact = artifact(&text, target(), Limits::default()).unwrap();
        let library = Library::new(&artifact);
        let types = owner
            .failure_flow()
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
        let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
            owner.failure_flow(),
            &layouts,
            &artifact,
            Default::default(),
            4096,
        )
        .unwrap();
        let vm = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
        assert_eq!(vm.value, None);
        assert_eq!(vm.effects, 1);
        assert!(!vm.poisoned);
        assert!(
            matches!(
                (kind, vm.fault.unwrap().kind),
                (0, c::Kind::ForeignValue(V::InvalidBool { .. }))
                    | (1, c::Kind::ForeignValue(V::InvalidTag { .. }))
                    | (2, c::Kind::ForeignValue(V::UnusedUnion { .. }))
                    | (3, c::Kind::ForeignValue(V::Reserved { .. }))
                    | (
                        4,
                        c::Kind::ForeignStatus {
                            code: u32::MAX,
                            known: false
                        }
                    )
            ),
            "{name}: {vm:?}"
        );
        assert_eq!(
            native::observe(&compiled, &artifact, &library, e::Control::default(), false),
            vm
        );
    }
}

#[test]
fn c_imports_transfer_256_mixed_scalar_and_aggregate_arguments() {
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut expected = 0;
    for i in 0..256 {
        let (ty, value, cost) = match i % 4 {
            0 => (
                "Pair",
                format!("Pair {{ number: {}, flag: true }}", i + 1),
                i + 1,
            ),
            1 => ("i64", format!("{}", i + 1), i + 1),
            2 => ("bool", "true".into(), 1),
            _ => ("core::Unit", "core::Unit {}".into(), 0),
        };
        parameters.push(format!("v{i}: {ty}"));
        arguments.push(value);
        expected += cost;
    }
    let text = format!(
        "pub struct Pair {{ number: i64, flag: bool }} extern \"C\" fn aggregate_many({}) -> Pair; fn main() -> i64 {{ unsafe {{ let mut result: Pair = aggregate_many({}); if result.flag {{ return result.number; }} return -1; }} }}",
        parameters.join(", "),
        arguments.join(", ")
    );
    let owner = source(&text);
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let types = owner
        .failure_flow()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let vm = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
    assert_eq!(vm.value, Some(expected));
    assert_eq!(vm.effects, 1);
    assert_eq!(
        native::observe(&compiled, &artifact, &library, e::Control::default(), false),
        vm
    );
}
impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: every callback/context/provider declared after this retained
        // image has been dropped, and each owned image is released once.
        #[cfg(target_os = "macos")]
        assert_eq!(unsafe { dlclose(self.handle) }, 0);
        #[cfg(target_os = "windows")]
        assert_ne!(unsafe { FreeLibrary(self.handle) }, 0);
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}
fn target() -> Target {
    if cfg!(target_os = "macos") {
        Target::MacosAarch64
    } else {
        Target::WindowsX86_64
    }
}
fn bindings<'a>(artifact: &'a Artifact, library: &Library, stats: &Stats) -> Vec<Binding<'a>> {
    artifact
        .functions()
        .iter()
        .map(|function| Binding {
            version: 1,
            size: 80,
            name: Some(function.name.as_bytes()),
            name_bytes: function.name.len() as u64,
            signature: function.digest,
            function: Some(
                library.function(&format!("c_{}", function.name.rsplit("::").next().unwrap())),
            ),
            user: (stats as *const Stats).cast_mut().cast(),
            flags: config::NATIVE_FFI,
        })
        .collect()
}
fn function(artifact: &Artifact, name: &str) -> usize {
    artifact
        .functions()
        .iter()
        .position(|f| f.name.rsplit("::").next() == Some(name))
        .unwrap()
}

#[derive(Debug, Eq, PartialEq)]
struct Observed {
    value: Option<i64>,
    fault: Option<c::Fault>,
    usage: e::Usage,
    effects: u64,
    summary: c::ErrorSummary,
    poisoned: bool,
}
fn observe_vm(
    owner: &lang_semantics::analysis::OwnershipProgram,
    artifact: &Artifact,
    library: &Library,
    control: e::Control<'_>,
    fail_allocation: impl Into<Refusal>,
) -> Observed {
    let stats = Stats::default();
    fail_allocation.into().apply(&stats);
    let flow = owner.failure_flow();
    let program = c::Program::new(
        artifact,
        flow,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = bindings(artifact, library, &stats);
    let context = program
        .create(config::Config {
            version: 1,
            size: 80,
            allocator: Some(&allocator),
            limits: Some(&limits),
            imports: Some(&bindings),
            import_count: bindings.len() as u64,
            artifact: artifact.digest(),
            flags: 0,
        })
        .unwrap();
    // SAFETY: fixture C prototypes were checked with the generated typedefs;
    // the explicit image/user/context remain live throughout the invocation.
    let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
    // SAFETY: this same image's allocator owns unique complete writable blocks
    // and matches its exact original requests without unwind or reentry.
    let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
    let mut usage = e::Usage::default();
    let work = e::ServiceWork::new(control.service_work_limit());
    let mut value = None;
    let result = context.run(
        &program,
        control,
        &work,
        &mut usage,
        c::EntryPoint::Guest,
        &mut |entry| match lang_vm::unit::owned::execute_entry_c(
            flow,
            entry,
            allocations.managed_mut(),
            &mut invoker,
        ) {
            Ok(outcome) => {
                value = Some(outcome.value);
                Ok(())
            }
            Err(failure) => Err(failure.context_fault()),
        },
    );
    assert_eq!(allocations.managed().stats().live_allocations, 0);
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
    let fault = result.err();
    Observed {
        value: if fault.is_none() { value } else { None },
        fault,
        usage,
        effects: stats.effects.get(),
        summary: context.last_error().unwrap(),
        poisoned: context.is_poisoned().unwrap(),
    }
}

#[test]
fn real_c_source_vm_native_controls_match_every_fuel_and_work_prefix() {
    let text = format!("{IMPORTS} fn main() -> i64 {{ unsafe {{ return increment(41); }} }}");
    let owner = source(&text);
    let flow = owner.failure_flow();
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
        flow,
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let normal = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
    assert_eq!(normal.value, Some(42));
    assert_eq!(
        native::observe(&compiled, &artifact, &library, e::Control::default(), false),
        normal
    );
    for fuel in 0..=normal.usage.fuel {
        let control = e::Control::with_limits(e::Limits {
            fuel: Some(fuel),
            ..Default::default()
        });
        assert_eq!(
            native::observe(&compiled, &artifact, &library, control, false),
            observe_vm(&owner, &artifact, &library, control, false),
            "fuel {fuel}"
        );
    }
    for work in 0..=normal.usage.service_work {
        let control = e::Control::default().and_service_work(Some(work));
        assert_eq!(
            native::observe(&compiled, &artifact, &library, control, false),
            observe_vm(&owner, &artifact, &library, control, false),
            "work {work}"
        );
    }
    let no_calls = e::Control::with_limits(e::Limits {
        host_calls: Some(0),
        ..Default::default()
    });
    assert_eq!(
        native::observe(&compiled, &artifact, &library, no_calls, false),
        observe_vm(&owner, &artifact, &library, no_calls, false)
    );
    let interrupted = e::InterruptionFlag::new();
    interrupted.interrupt();
    let control = e::Control::default().and_interruption(&interrupted);
    assert_eq!(
        native::observe(&compiled, &artifact, &library, control, false),
        observe_vm(&owner, &artifact, &library, control, false)
    );
    let control = e::Control::default().and_deadline(std::time::Instant::now());
    assert_eq!(
        native::observe(&compiled, &artifact, &library, control, false),
        observe_vm(&owner, &artifact, &library, control, false)
    );
    let oom = observe_vm(&owner, &artifact, &library, e::Control::default(), true);
    assert_eq!(oom.effects, 0);
    assert!(!oom.poisoned);
    assert!(oom.fault.is_some());
    assert_eq!(
        native::observe(&compiled, &artifact, &library, e::Control::default(), true),
        oom
    );
}

#[test]
fn c_source_supports_the_full_256_parameter_profile_on_vm_and_native() {
    let parameters = (0..256)
        .map(|i| format!("v{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (1..=256)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "{IMPORTS} extern \"C\" fn arity({parameters}) -> i64; fn main() -> i64 {{ unsafe {{ return arity({arguments}); }} }}"
    );
    let owner = source(&text);
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let vm = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
    assert_eq!(vm.value, Some(32_896));
    assert_eq!(vm.effects, 1);
    assert_eq!(vm.usage.host_calls, 1);
    assert_eq!(
        native::observe(&compiled, &artifact, &library, e::Control::default(), false),
        vm
    );
}

#[test]
fn named_scalar_c_example_returns_59_on_vm_and_native() {
    let text = include_str!("../../examples/analysis/c_scalar_imports.lang");
    let owner = source(text);
    let artifact = artifact(text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let vm = observe_vm(&owner, &artifact, &library, e::Control::default(), false);
    assert_eq!(vm.value, Some(59));
    assert_eq!(vm.effects, 3);
    assert_eq!(
        native::observe(&compiled, &artifact, &library, e::Control::default(), false),
        vm
    );
}

#[test]
fn source_admission_rejects_same_abi_wrong_code_and_replaced_allocator_before_effects() {
    let text = format!("{IMPORTS} fn main() -> i64 {{ unsafe {{ return increment(41); }} }}");
    let owner = source(&text);
    let other = source(&text.replace("increment(41)", "increment(42)"));
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let other_types = other
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = lang_runtime::layout::Layouts::compile(types, Default::default()).unwrap();
    let other_layouts =
        lang_runtime::layout::Layouts::compile(other_types, Default::default()).unwrap();
    let compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let other_compiled = lang_codegen_clif::owned::CCompiledProgram::compile_scalar_imports(
        other.failure_flow(),
        &other_layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    for native in [false, true] {
        let stats = Stats::default();
        let code: &dyn c::CodeOwner = if native {
            &compiled
        } else {
            owner.failure_flow()
        };
        let program = c::Program::new(
            &artifact,
            code,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let descriptor = library.allocator(&stats);
        let limits = config::LimitsDescriptor::default();
        let bindings = bindings(&artifact, &library, &stats);
        let context = program
            .create(config::Config {
                version: 1,
                size: 80,
                allocator: Some(&descriptor),
                limits: Some(&limits),
                imports: Some(&bindings),
                import_count: bindings.len() as u64,
                artifact: artifact.digest(),
                flags: 0,
            })
            .unwrap();
        // SAFETY: retained fixture code/user objects and generated C typedef
        // checks satisfy the explicit callbacks' native lifetime/type contract.
        let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
        let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
        let mut usage = e::Usage::default();
        let work = e::ServiceWork::new(None);
        let failed = context
            .run(
                &program,
                e::Control::default(),
                &work,
                &mut usage,
                c::EntryPoint::Guest,
                &mut |entry| {
                    if native {
                        let (_, session) = entry.parts();
                        other_compiled
                            .prepare(session, &invoker, allocations.managed())
                            .map(|_| ())
                    } else {
                        lang_vm::unit::owned::execute_entry_c(
                            other.failure_flow(),
                            entry,
                            allocations.managed_mut(),
                            &mut invoker,
                        )
                        .map(|_| ())
                        .map_err(|failure| failure.context_fault())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(failed.kind, c::Kind::WrongProgram);
        assert_eq!(stats.allocations.get(), 0);
        assert_eq!(stats.effects.get(), 0);
        assert_eq!(usage, e::Usage::default());
        let provider = allocations.managed_mut().revoke().unwrap();
        let mut replacement = a::Allocator::new(provider, allocations.managed().limits()).unwrap();
        assert_ne!(replacement.identity(), allocations.managed().identity());
        assert_eq!(
            context
                .bind_allocator(replacement.identity())
                .unwrap_err()
                .kind,
            c::Kind::WrongProgram
        );
        let failed = context
            .run(
                &program,
                e::Control::default(),
                &work,
                &mut usage,
                c::EntryPoint::Guest,
                &mut |entry| {
                    if native {
                        let (_, session) = entry.parts();
                        compiled
                            .prepare(session, &invoker, &replacement)
                            .map(|_| ())
                    } else {
                        lang_vm::unit::owned::execute_entry_c(
                            owner.failure_flow(),
                            entry,
                            &mut replacement,
                            &mut invoker,
                        )
                        .map(|_| ())
                        .map_err(|failure| failure.context_fault())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(failed.kind, c::Kind::WrongProgram);
        assert_eq!(stats.allocations.get(), 0);
        assert_eq!(stats.effects.get(), 0);
        assert!(!context.is_poisoned().unwrap());
    }
}

#[test]
fn native_thunks_call_compiler_produced_c_and_preserve_original_allocator() {
    let text = format!("{IMPORTS} fn main() -> i64 {{ unsafe {{ return increment(41); }} }}");
    let owner = source(&text);
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let library = Library::new(&artifact);
    let stats = Stats::default();
    let program = c::Program::new(
        &artifact,
        owner.as_ref(),
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = bindings(&artifact, &library, &stats);
    let context = program
        .create(config::Config {
            version: 1,
            size: 80,
            allocator: Some(&allocator),
            limits: Some(&limits),
            imports: Some(&bindings),
            import_count: bindings.len() as u64,
            artifact: artifact.digest(),
            flags: 0,
        })
        .unwrap();
    // SAFETY: the compiler checked every C callback against the generated
    // typedef. Library, user cells and context stay live through all calls.
    let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
    let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
    // SAFETY: same live callbacks, but the second accounting identity must be
    // refused so cumulative quotas cannot be reset within this context.
    assert!(unsafe { CAllocator::new(&context) }.is_err());
    assert!(allocations.belongs_to(&context));
    let zero = allocations
        .managed_mut()
        .allocate(a::Request::new(0, 1).unwrap())
        .unwrap();
    allocations.managed_mut().release(zero).unwrap();
    assert_eq!(stats.allocations.get(), 0);
    let bytes = allocations
        .managed_mut()
        .allocate(a::Request::new(7, 1).unwrap())
        .unwrap();
    assert_eq!(allocations.managed().bytes(bytes).unwrap(), &[0; 7]);
    allocations
        .managed_mut()
        .bytes_mut(bytes)
        .unwrap()
        .fill(0x3f);
    allocations.managed_mut().release(bytes).unwrap();
    assert_eq!(stats.allocations.get(), 1);
    assert_eq!(stats.frees.get(), 1);
    assert_eq!(stats.wrong.get(), 0);
    stats.fail.set(1);
    assert!(matches!(
        allocations
            .managed_mut()
            .allocate(a::Request::new(8, 8).unwrap()),
        Err(a::Error::OutOfMemory)
    ));
    assert_eq!(allocations.managed().stats().live_allocations, 0);
    let mut usage = e::Usage::default();
    let work = e::ServiceWork::new(None);
    context
        .run(
            &program,
            e::Control::default(),
            &work,
            &mut usage,
            c::EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                assert_eq!(
                    session.invoke_scalars(
                        function(&artifact, "increment"),
                        None,
                        &[Scalar::I64(41)],
                        &mut invoker
                    )?,
                    Scalar::I64(42)
                );
                assert_eq!(
                    session.invoke_scalars(
                        function(&artifact, "flag"),
                        None,
                        &[Scalar::Bool(1)],
                        &mut invoker
                    )?,
                    Scalar::Bool(1)
                );
                assert_eq!(
                    session.invoke_scalars(
                        function(&artifact, "notify"),
                        None,
                        &[Scalar::Unit],
                        &mut invoker
                    )?,
                    Scalar::Unit
                );
                assert_eq!(
                    session.invoke_scalars(
                        function(&artifact, "combine"),
                        None,
                        &[
                            Scalar::I64(3),
                            Scalar::Bool(1),
                            Scalar::Unit,
                            Scalar::I64(5),
                            Scalar::Bool(0),
                            Scalar::I64(7),
                            Scalar::Bool(1),
                            Scalar::I64(11),
                            Scalar::I64(13),
                            Scalar::Bool(1),
                            Scalar::I64(17),
                            Scalar::Bool(0)
                        ],
                        &mut invoker
                    )?,
                    Scalar::I64(59)
                );
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(usage.host_calls, 4);
    assert_eq!(usage.fuel, 0);
    assert_eq!(stats.effects.get(), 4);
}

#[test]
fn vm_executes_c_source_and_preserves_foreign_failures_and_allocator_cleanup() {
    for (body, expected, raw, bad_bool) in [
        ("unsafe { return increment(41); }", Some(42), None, false),
        (
            "unsafe { let mut notice: core::Unit = notify(core::Unit {}); if flag(true) { return combine(3, true, core::Unit {}, 5, false, 7, true, 11, 13, true, 17, false); } return 0; }",
            Some(59),
            None,
            false,
        ),
        ("unsafe { return increment(-1); }", None, Some(2), false),
        (
            "unsafe { return increment(-2); }",
            None,
            Some(u32::MAX),
            false,
        ),
        (
            "unsafe { if flag(false) { return 1; } return 0; }",
            None,
            None,
            true,
        ),
    ] {
        let text = format!("{IMPORTS} fn main() -> i64 {{ {body} }}");
        let owner = source(&text);
        let flow = owner.failure_flow();
        let artifact = artifact(&text, target(), Limits::default()).unwrap();
        let library = Library::new(&artifact);
        let stats = Stats::default();
        let program = c::Program::new(
            &artifact,
            flow,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let allocator = library.allocator(&stats);
        let limits = config::LimitsDescriptor::default();
        let bindings = bindings(&artifact, &library, &stats);
        let context = program
            .create(config::Config {
                version: 1,
                size: 80,
                allocator: Some(&allocator),
                limits: Some(&limits),
                imports: Some(&bindings),
                import_count: bindings.len() as u64,
                artifact: artifact.digest(),
                flags: 0,
            })
            .unwrap();
        // SAFETY: the image's C callbacks were checked against generated C
        // typedefs, and code/user cells outlive the context and its adapters.
        let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
        let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
        let mut usage = e::Usage::default();
        let work = e::ServiceWork::new(Some(100_000));
        let mut saved = None;
        let result = context.run(
            &program,
            e::Control::default().and_service_work(Some(100_000)),
            &work,
            &mut usage,
            c::EntryPoint::Guest,
            &mut |entry| {
                let result = lang_vm::unit::owned::execute_entry_c(
                    flow,
                    entry,
                    allocations.managed_mut(),
                    &mut invoker,
                );
                let failure = result.as_ref().err().map(|failure| {
                    failure
                        .report
                        .as_ref()
                        .and_then(|report| {
                            c::Fault::from_report(report, failure.panic_code.unwrap_or(0))
                        })
                        .unwrap_or_else(|| match failure.reason {
                            lang_vm::unit::owned::Reason::Foreign(fault) => fault,
                            _ => c::Fault::new(c::Kind::Execution(
                                lang_runtime::cleanup::ExecutionFailure::Fault,
                            )),
                        })
                });
                saved = Some(result);
                failure.map_or(Ok(()), Err)
            },
        );
        if let Some(expected) = expected {
            assert!(result.is_ok(), "{result:?}");
            let outcome = saved.unwrap().unwrap();
            assert_eq!(outcome.value, expected);
            assert_eq!(outcome.usage, usage);
            assert_eq!(outcome.usage.service_work, work.used());
            assert!(usage.fuel > 0);
            assert!(usage.host_calls > 0);
        } else {
            let failure = result.unwrap_err();
            assert_eq!(failure.status(), c::Status::Runtime);
            if let Some(code) = raw {
                assert_eq!(
                    failure.kind,
                    c::Kind::ForeignStatus {
                        code,
                        known: code <= 6
                    }
                );
            }
            if bad_bool {
                assert!(matches!(
                    failure.kind,
                    c::Kind::ForeignValue(lang_runtime::c_abi::validation::Error::InvalidBool {
                        value: 2,
                        ..
                    })
                ));
            }
            let vm = saved.unwrap().err().unwrap();
            assert!(matches!(
                vm.reason,
                lang_vm::unit::owned::Reason::Foreign(_)
            ));
            assert!(vm.report.is_some());
            assert!(vm.panic_code.is_none());
            assert_eq!(vm.usage, usage);
            assert!(!context.is_poisoned().unwrap());
        }
        assert_eq!(allocations.managed().stats().live_allocations, 0);
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert!(stats.allocations.get() > 0);
        assert_eq!(stats.wrong.get(), 0);
    }
}
