//! Bounded passive generation. Export signatures need implementation bodies.
use lang::component_bindings::{self as generate, abi::Target};
use lang_mir::unit as m;
use lang_runtime::component::{idl, wire};
use std::{
    error::Error,
    io::{Read, Write},
};

fn interface(key: &lang_runtime::component::negotiation::InterfaceKey) -> String {
    format!(
        "{}:{}/{}",
        key.package().namespace(),
        key.package().package(),
        key.interface()
    )
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: component-bindings <guest|c> <idl-file> [aarch64-apple-darwin|x86_64-pc-windows-msvc]".into());
    }
    let target = match args
        .get(2)
        .map(String::as_str)
        .unwrap_or("aarch64-apple-darwin")
    {
        "aarch64-apple-darwin" => Target::MacosAarch64,
        "x86_64-pc-windows-msvc" => Target::WindowsX86_64Msvc,
        _ => return Err("unsupported target".into()),
    };
    let limits = idl::Limits::default();
    let mut bytes = Vec::new();
    std::fs::File::open(&args[1])?
        .take(limits.source_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    let document = idl::check(m::SourceId(0), &bytes, limits)?;
    match args[0].as_str() {
        "guest" => {
            let guest = generate::guest(&document, Default::default())?;
            let resources=guest.bindings.resources.iter().map(|r|serde_json::json!({"tag":r.tag,"resource":format!("{}#{}",interface(r.resource.interface()),r.resource.resource())})).collect::<Vec<_>>();
            let imports=guest.bindings.imports.iter().map(|i|serde_json::json!({"source":i.source,"interface":interface(&i.member.interface),"member":i.member.member})).collect::<Vec<_>>();
            let exports=guest.bindings.exports.iter().map(|e|serde_json::json!({"source":e.source,"interface":interface(&e.member.interface),"member":e.member.member})).collect::<Vec<_>>();
            let signatures=guest.exports.iter().map(|e|serde_json::json!({"source":e.source,"declaration":e.declaration,"idl_span":{"source":e.contract_span.source.0,"start":e.contract_span.start,"end":e.contract_span.end}})).collect::<Vec<_>>();
            let output = serde_json::json!({"declarations":guest.declarations,"bindings":{"version":guest.bindings.version,"resources":resources,"imports":imports,"exports":exports},"export_signatures":signatures});
            serde_json::to_writer_pretty(std::io::stdout().lock(), &output)?;
        }
        "c" => {
            let contract = wire::Contract::new(
                document,
                vec![],
                &mut wire::Session::new(Default::default()),
            )?;
            let generated = generate::c::generate(&contract, target, Default::default())?;
            std::io::stdout()
                .lock()
                .write_all(generated.header.as_bytes())?;
        }
        _ => return Err("generation mode must be guest or c".into()),
    }
    Ok(())
}
