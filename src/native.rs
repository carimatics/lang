//! RFC 0020 whole-source native package production. This module never executes
//! guest code, grants imports or treats native hashes as authentication.
pub mod manifest;
pub mod sdk_package;
use crate::frontend::{self, CompileLimits, ScriptError};
use lang_codegen_clif::{self as clif, native_record::Record, owned as n, sdk};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{c_abi as abi, layout::Layouts};
use lang_semantics::analysis::Program;
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum Error {
    Target(String),
    Entry(ScriptError),
    CopyCodec(lang_bytecode::unit::CodecError),
    CopyCodegen(clif::program::Error),
    OwnedCodec(lang_bytecode::ownership::CodecError),
    OwnedCodegen(n::Error),
    CMetadata(abi::Error),
    CCodegen(n::CObjectError),
    Record(clif::native_record::Error),
    Limit {
        role: &'static str,
        bytes: usize,
        limit: usize,
    },
    Capacity,
    Manifest(serde_json::Error),
    Io {
        stage: &'static str,
        path: PathBuf,
        error: io::Error,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native package: {self:?}")
    }
}
impl std::error::Error for Error {}
fn io(stage: &'static str, path: &Path, error: io::Error) -> Error {
    Error::Io {
        stage,
        path: path.to_owned(),
        error,
    }
}
pub fn target(value: &str) -> Result<clif::Triple, Error> {
    match value {
        "aarch64-apple-darwin" | "x86_64-pc-windows-msvc" => {
            value.parse().map_err(|_| Error::Target(value.to_owned()))
        }
        _ => Err(Error::Target(value.to_owned())),
    }
}
pub struct File {
    pub role: &'static str,
    pub path: &'static str,
    pub bytes: Vec<u8>,
}
pub struct Package {
    files: Vec<File>,
}
impl Package {
    pub fn files(&self) -> &[File] {
        &self.files
    }
    fn push(
        &mut self,
        role: &'static str,
        path: &'static str,
        bytes: Vec<u8>,
        limit: usize,
    ) -> Result<(), Error> {
        if bytes.len() > limit {
            return Err(Error::Limit {
                role,
                bytes: bytes.len(),
                limit,
            });
        }
        let total = self
            .files
            .iter()
            .try_fold(bytes.len(), |a, f| a.checked_add(f.bytes.len()))
            .ok_or(Error::Capacity)?;
        if total > 256 * 1024 * 1024 {
            return Err(Error::Limit {
                role: "package",
                bytes: total,
                limit: 256 * 1024 * 1024,
            });
        }
        self.files.try_reserve(1).map_err(|_| Error::Capacity)?;
        self.files.push(File { role, path, bytes });
        Ok(())
    }
    /// Publish a complete fresh directory; no existing destination is replaced.
    pub fn publish(&self, output: &Path) -> Result<(), Error> {
        let stage = Stage::new(output)?;
        for file in &self.files {
            use std::io::Write;
            let path = stage.0.join(file.path);
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| io("component create", &path, e))?;
            out.write_all(&file.bytes)
                .and_then(|_| out.sync_all())
                .map_err(|e| io("component write", &path, e))?;
        }
        publish::rename(&stage.0, output).map_err(|e| io("exclusive publication", output, e))
    }
}
struct Stage(PathBuf);
impl Stage {
    fn new(output: &Path) -> Result<Self, Error> {
        if fs::symlink_metadata(output).is_ok() {
            return Err(io(
                "destination exists",
                output,
                io::ErrorKind::AlreadyExists.into(),
            ));
        }
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| io("output parent", parent, e))?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut stage = None;
        for _ in 0..128 {
            let path = parent.join(format!(
                ".lang-native-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let directory = fs::DirBuilder::new();
            #[cfg(unix)]
            let directory = {
                use std::os::unix::fs::DirBuilderExt;
                let mut directory = directory;
                directory.mode(0o700);
                directory
            };
            match directory.create(&path) {
                Ok(()) => {
                    stage = Some(path);
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io("staging directory", &path, e)),
            }
        }
        Ok(Self(stage.ok_or(Error::Capacity)?))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
mod publish;

pub fn compile(
    program: Program,
    sources: &SourceMap,
    limits: CompileLimits,
    target: clif::Triple,
) -> Result<Package, Error> {
    let abi_target = match target.to_string().as_str() {
        "aarch64-apple-darwin" => abi::Target::MacosAarch64,
        "x86_64-pc-windows-msvc" => abi::Target::WindowsX86_64,
        _ => return Err(Error::Target(target.to_string())),
    };
    let mut symbols = Vec::new();
    let mut header = None;
    let (semantic, record, profile, entry) = match program {
        Program::Copy(p) => {
            let code = frontend::compile_script(&p, sources, limits).map_err(Error::Entry)?;
            let semantic =
                lang_bytecode::unit::encode(&code, Default::default()).map_err(Error::CopyCodec)?;
            let canonical = lang_bytecode::unit::decode(&semantic, Default::default())
                .map_err(Error::CopyCodec)?;
            let entry = canonical.verified().unit().entry;
            let object = clif::program::emit_object_for_target(
                canonical.verified(),
                &[],
                Default::default(),
                target.clone(),
            )
            .map_err(Error::CopyCodegen)?;
            symbols.extend(object.functions.iter().map(|f| f.symbol.clone()));
            (semantic, object.into_record(target.clone()), "copy", entry)
        }
        Program::Ownership(p) => {
            let declarations = p
                .failure_flow()
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations();
            let c = declarations
                .imports()
                .iter()
                .any(|i| i.abi == m::ownership::declarations::ImportAbi::C)
                || declarations
                    .functions()
                    .iter()
                    .any(|f| f.abi == m::ownership::declarations::FunctionAbi::CExport);
            let entry = if c {
                None
            } else {
                frontend::check_script_entry(p.hir().loaned().moved().bodies().source(), sources)
                    .map_err(Error::Entry)?;
                declarations
                    .functions()
                    .iter()
                    .position(|f| f.name == "main")
                    .map(|i| m::FunctionId(i as u32))
            };
            let table = p
                .into_failure_flow()
                .into_source()
                .into_activity()
                .into_moved()
                .into_typed()
                .into_table();
            let semantic = lang_bytecode::ownership::encode(table, entry, Default::default())
                .map_err(Error::OwnedCodec)?;
            let canonical = lang_bytecode::ownership::decode(&semantic, Default::default())
                .map_err(Error::OwnedCodec)?;
            let flow = canonical.flow();
            let decl = flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations();
            let layouts = Layouts::compile(decl.types(), Default::default())
                .map_err(|e| Error::OwnedCodegen(n::Error::Layout(e)))?;
            let record = if c {
                let artifact = abi::Artifact::compile(decl, abi_target, Default::default())
                    .map_err(Error::CMetadata)?;
                header = Some(
                    artifact
                        .header(Default::default(), Default::default())
                        .map_err(Error::CMetadata)?,
                );
                let object = n::emit_c_object_for_target(
                    flow,
                    &layouts,
                    &artifact,
                    &target,
                    Default::default(),
                    4096,
                )
                .map_err(Error::CCodegen)?;
                symbols.push(object.slot_symbol().to_owned());
                symbols.extend(object.support_symbols().iter().cloned());
                symbols.extend(object.export_symbols().iter().map(|f| f.1.clone()));
                symbols.extend(object.thunk_symbols().iter().map(|f| f.1.clone()));
                for i in 0..flow.functions().len() {
                    symbols.push(
                        object
                            .function(m::FunctionId(i as u32))
                            .unwrap()
                            .metadata()
                            .symbol
                            .clone(),
                    );
                }
                object
                    .into_recorded(Default::default())
                    .map_err(Error::OwnedCodegen)?
            } else {
                let object =
                    n::emit_object_for_target(flow, &layouts, &target, Default::default(), 4096)
                        .map_err(Error::OwnedCodegen)?;
                for i in 0..flow.functions().len() {
                    symbols.push(
                        object
                            .function(m::FunctionId(i as u32))
                            .unwrap()
                            .metadata()
                            .symbol
                            .clone(),
                    );
                }
                object
                    .into_recorded(Default::default())
                    .map_err(Error::OwnedCodegen)?
            };
            (
                semantic,
                record.into_record(),
                if c { "c" } else { "owned" },
                entry,
            )
        }
    };
    let native = record
        .encode(&semantic, Default::default())
        .map_err(Error::Record)?;
    Record::decode(&native, &semantic, Default::default()).map_err(Error::Record)?;
    let mut package = Package { files: Vec::new() };
    let object_name = if abi_target == abi::Target::MacosAarch64 {
        "program.o"
    } else {
        "program.obj"
    };
    let build = sdk::build_id(&target);
    let bindings = bindings_source(&symbols, &build, &target)?;
    package.push("semantic", "program.lbc", semantic, 64 * 1024 * 1024)?;
    package.push("native_record", "program.native", native, 64 * 1024 * 1024)?;
    package.push(
        "object",
        object_name,
        record.into_object_bytes(),
        64 * 1024 * 1024,
    )?;
    package.push(
        "bindings",
        "bindings.rs",
        bindings.into_bytes(),
        8 * 1024 * 1024,
    )?;
    if let Some(header) = header {
        package.push(
            "c_header",
            "program.h",
            format!("// SDK build {build}\n{header}").into_bytes(),
            16 * 1024 * 1024,
        )?;
    }
    if profile != "c" {
        package.push(
            "main",
            "main.rs",
            main_source(profile).into_bytes(),
            8 * 1024 * 1024,
        )?;
    }
    let files: Vec<_> = package.files.iter().map(|f| serde_json::json!({"role":f.role,"path":f.path,"bytes":f.bytes.len(),"sha256":sdk::digest(&f.bytes)})).collect();
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({"format":"lang-native-program","version":1,"build_id":build,"target":target.to_string(),"profile":profile,"entry":entry.map(|i|i.0),"files":files})).map_err(Error::Manifest)?;
    package.push("manifest", "package.json", manifest, 64 * 1024)?;
    Ok(package)
}
/// Fallible formatting checks the byte budget before every reservation.
struct SourceWriter {
    text: String,
    limit: usize,
    error: Option<Error>,
}
impl fmt::Write for SourceWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = self.text.len().checked_add(text.len()).ok_or_else(|| {
            self.error = Some(Error::Capacity);
            fmt::Error
        })?;
        if bytes > self.limit {
            self.error = Some(Error::Limit {
                role: "bindings",
                bytes,
                limit: self.limit,
            });
            return Err(fmt::Error);
        }
        self.text.try_reserve(text.len()).map_err(|_| {
            self.error = Some(Error::Capacity);
            fmt::Error
        })?;
        self.text.push_str(text);
        Ok(())
    }
}
impl SourceWriter {
    fn append(&mut self, args: fmt::Arguments<'_>) -> Result<(), Error> {
        fmt::write(self, args).map_err(|_| self.error.take().unwrap_or(Error::Capacity))
    }
}
fn bindings_source(
    symbols: &[String],
    build: &str,
    target: &clif::Triple,
) -> Result<String, Error> {
    let mut out = SourceWriter {
        text: String::new(),
        limit: 8 * 1024 * 1024,
        error: None,
    };
    out.append(format_args!("// Generated native symbol addresses; trusted matching object required.\nuse std::{{ffi::c_void,ptr::NonNull}};\nunsafe extern \"C\" {{\n"))?;
    for (i, name) in symbols.iter().enumerate() {
        out.append(format_args!("#[link_name={name:?}] static S{i}: u8;\n"))?;
    }
    out.append(format_args!(
        "}}\npub fn resolve(_: &(), name: &str) -> Option<NonNull<c_void>> {{ match name {{\n"
    ))?;
    for (i, name) in symbols.iter().enumerate() {
        out.append(format_args!(
            "{name:?} => NonNull::new((&raw const S{i}).cast_mut().cast()),\n"
        ))?;
    }
    out.append(format_args!("_=>None }}}}\n"))?;
    out.append(format_args!("pub const BUILD_ID:&str={build:?};\npub const TARGET:&str={:?};\npub const SEMANTIC:&[u8]=include_bytes!(\"program.lbc\");\npub const NATIVE:&[u8]=include_bytes!(\"program.native\");\n",target.to_string()))?;
    Ok(out.text)
}
fn main_source(profile: &str) -> String {
    let execute = if profile == "owned" {
        r#"
let mut allocator=lang_runtime::allocation::Allocator::new(lang_runtime::allocation::System,Default::default()).map_err(|e|e.to_string())?;
let mut instance=lang_runtime::cleanup::Instance::default();
// The finite preparation budget is separate from bounded semantic decoding.
let config=lang_codegen_clif::owned::runtime::Config{limits:lang_mir::unit::Limits{allocations:512*1024*1024,..Default::default()},..Default::default()};
let value=unsafe{lang_codegen_clif::sdk::runtime::execute_owned(input,&(),bindings::resolve,&mut instance,&mut allocator,config,Default::default())}.map_err(|e|e.to_string())?.value;
"#
    } else {
        r#"
let value=unsafe{lang_codegen_clif::sdk::runtime::execute_copy(input,&(),bindings::resolve,Default::default(),Default::default())}.map_err(|e|e.to_string())?;
"#
    };
    format!(
        r#"// Generated for this exact SDK/object; native build inputs are trusted.
mod bindings;
fn run()->Result<(),String>{{
let input=lang_codegen_clif::sdk::runtime::Input{{target:bindings::TARGET,build_id:bindings::BUILD_ID,semantic:bindings::SEMANTIC,native:bindings::NATIVE}};
// SAFETY: the link stage retains the exact original verified object/SDK;
// all symbol addresses and relocation dependencies live until process exit.
{execute}
println!("{{value}}"); Ok(())
}}
fn main()->std::process::ExitCode{{match run(){{Ok(())=>std::process::ExitCode::SUCCESS,Err(e)=>{{eprintln!("{{e}}");std::process::ExitCode::FAILURE}}}}}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bindings_budget_refuses_before_oversized_reservation() {
        let mut out = SourceWriter {
            text: String::new(),
            limit: 4,
            error: None,
        };
        out.append(format_args!("{}", "éé")).unwrap();
        assert_eq!(out.text.len(), 4);
        assert!(matches!(
            out.append(format_args!("x")),
            Err(Error::Limit {
                bytes: 5,
                limit: 4,
                ..
            })
        ));
        assert_eq!(out.text, "éé");
        let symbols = vec!["\"\\unsafe\n".to_owned()];
        let text = bindings_source(&symbols, "build", &clif::Triple::host()).unwrap();
        assert!(text.contains(&format!("{:?}", symbols[0])));
    }
}
