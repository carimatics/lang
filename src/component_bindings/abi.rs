//! The RFC 0027 C surface. Descriptors are compiler inputs, never host layouts.
use super::{Error, Limits, Output as Writer, Result};
use lang_mir::unit as m;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    MacosAarch64,
    WindowsX86_64Msvc,
}
impl Target {
    pub fn triple(self) -> &'static str {
        match self {
            Self::MacosAarch64 => "aarch64-apple-darwin",
            Self::WindowsX86_64Msvc => "x86_64-pc-windows-msvc",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: &'static str,
    pub declaration: &'static str,
    pub offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    pub name: &'static str,
    pub size: usize,
    pub alignment: usize,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub name: &'static str,
    pub declaration: &'static str,
    pub fingerprint: [u8; 32],
}
#[derive(Debug)]
pub struct Abi {
    pub header: String,
    pub target: Target,
    pub layouts: Vec<Layout>,
    pub signatures: Vec<Signature>,
    pub fingerprint: [u8; 32],
    pub usage: m::Usage,
    pub text_bytes: usize,
}

const PREFIX: &str = "lang_component_v1_";
// `@` marks the declaration's name. This small closed table is the ABI, not an
// IDL parser or a mechanism for accepting native layouts from metadata.
type Spec = (&'static str, &'static [(&'static str, &'static str)]);
const RECORDS: &[Spec] = &[
    (
        "bytes",
        &[("data", "const uint8_t *@"), ("length", "uint64_t @")],
    ),
    (
        "mut_bytes",
        &[("data", "uint8_t *@"), ("length", "uint64_t @")],
    ),
    (
        "header",
        &[("size", "uint32_t @"), ("version", "uint32_t @")],
    ),
    (
        "context",
        &[
            ("header", "lang_component_v1_header @"),
            ("engine", "lang_component_v1_engine *@"),
            ("invocation", "uint64_t @"),
            ("scope", "uint64_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "error",
        &[
            ("header", "lang_component_v1_header @"),
            ("diagnostic", "uint32_t @"),
            ("limit_kind", "uint32_t @"),
            ("limit", "uint64_t @"),
            ("attempted", "uint64_t @"),
            ("input_digest", "uint8_t @[32]"),
            ("source_id", "uint32_t @"),
            ("start", "uint32_t @"),
            ("end", "uint32_t @"),
            ("detail_code", "uint32_t @"),
            ("flags", "uint32_t @"),
            ("reserved0", "uint32_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "limits",
        &[
            ("header", "lang_component_v1_header @"),
            ("fuel", "uint64_t @"),
            ("service_work", "uint64_t @"),
            ("host_calls", "uint64_t @"),
            ("heap_bytes", "uint64_t @"),
            ("live_allocations", "uint64_t @"),
            ("frames", "uint64_t @"),
            ("live_cells", "uint64_t @"),
            ("live_tasks", "uint64_t @"),
            ("canonical_indices", "uint64_t @"),
            ("preparation_work", "uint64_t @"),
            ("metadata_bytes", "uint64_t @"),
            ("value_elements", "uint64_t @"),
            ("serialization_bytes", "uint64_t @"),
            ("depth", "uint64_t @"),
            ("decoder_allocation_bytes", "uint64_t @"),
            ("suppressed_records", "uint64_t @"),
            ("trace_frames", "uint64_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "allocator",
        &[
            ("header", "lang_component_v1_header @"),
            ("user", "void *@"),
            ("allocate", "lang_component_v1_allocate_fn @"),
            ("deallocate", "lang_component_v1_deallocate_fn @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "config",
        &[
            ("header", "lang_component_v1_header @"),
            ("mode", "uint32_t @"),
            ("flags", "uint32_t @"),
            ("allocator", "lang_component_v1_allocator @"),
            ("ceiling", "lang_component_v1_limits @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "resource_type",
        &[
            ("header", "lang_component_v1_header @"),
            ("resource_key", "lang_component_v1_bytes @"),
            ("destroy", "lang_component_v1_resource_destroy_fn @"),
            ("user", "void *@"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "import",
        &[
            ("header", "lang_component_v1_header @"),
            ("function_key", "lang_component_v1_bytes @"),
            ("signature", "uint8_t @[32]"),
            ("invoke", "lang_component_v1_invoke_fn @"),
            ("user", "void *@"),
            ("flags", "uint32_t @"),
            ("reserved0", "uint32_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "provider_config",
        &[
            ("header", "lang_component_v1_header @"),
            ("contract", "lang_component_v1_bytes @"),
            ("imports", "const lang_component_v1_import *@"),
            ("import_count", "uint64_t @"),
            ("resource_types", "const lang_component_v1_resource_type *@"),
            ("resource_type_count", "uint64_t @"),
            ("owner", "void *@"),
            ("release_owner", "lang_component_v1_release_fn @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "native_image",
        &[
            ("header", "lang_component_v1_header @"),
            ("metadata", "lang_component_v1_bytes @"),
            ("program", "const void *@"),
            ("owner", "void *@"),
            ("release_owner", "lang_component_v1_release_fn @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "instance_config",
        &[
            ("header", "lang_component_v1_header @"),
            ("grants", "const lang_component_v1_bytes *@"),
            ("grant_count", "uint64_t @"),
            ("ceiling", "lang_component_v1_limits @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "result_info",
        &[
            ("header", "lang_component_v1_header @"),
            ("kind", "uint32_t @"),
            ("flags", "uint32_t @"),
            ("omitted_records", "uint64_t @"),
            ("suppressed_count", "uint64_t @"),
            ("frame_count", "uint64_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
    (
        "usage",
        &[
            ("header", "lang_component_v1_header @"),
            ("fuel", "uint64_t @"),
            ("service_work", "uint64_t @"),
            ("host_calls", "uint64_t @"),
            ("peak_heap_bytes", "uint64_t @"),
            ("peak_live_allocations", "uint64_t @"),
            ("peak_frames", "uint64_t @"),
            ("peak_live_cells", "uint64_t @"),
            ("peak_live_tasks", "uint64_t @"),
            ("canonical_indices", "uint64_t @"),
            ("reserved", "uint64_t @[2]"),
        ],
    ),
];
const CALLBACKS: &[(&str, &str)] = &[
    (
        "allocate_fn",
        "uint8_t *(LANG_COMPONENT_V1_CALL *@)(void *,uint64_t,uint64_t)",
    ),
    (
        "deallocate_fn",
        "void (LANG_COMPONENT_V1_CALL *@)(void *,uint8_t *,uint64_t,uint64_t)",
    ),
    ("release_fn", "void (LANG_COMPONENT_V1_CALL *@)(void *)"),
    (
        "invoke_fn",
        "lang_component_v1_status (LANG_COMPONENT_V1_CALL *@)(void *,const lang_component_v1_context *,lang_component_v1_bytes,lang_component_v1_buffer **)",
    ),
    (
        "resource_destroy_fn",
        "lang_component_v1_status (LANG_COMPONENT_V1_CALL *@)(void *,const lang_component_v1_context *,lang_component_v1_mut_bytes,lang_component_v1_buffer **)",
    ),
];
const API: &[(&str, &str)] = &[
    (
        "engine_create",
        "const lang_component_v1_config *,lang_component_v1_engine **,lang_component_v1_error *",
    ),
    (
        "engine_destroy",
        "lang_component_v1_engine *,lang_component_v1_error *",
    ),
    (
        "module_load",
        "lang_component_v1_engine *,uint32_t,lang_component_v1_bytes,lang_component_v1_bytes,lang_component_v1_module **,lang_component_v1_error *",
    ),
    (
        "module_drop",
        "lang_component_v1_module *,lang_component_v1_error *",
    ),
    (
        "module_bind_native",
        "lang_component_v1_engine *,const lang_component_v1_native_image *,lang_component_v1_module **,lang_component_v1_error *",
    ),
    (
        "provider_register",
        "lang_component_v1_engine *,const lang_component_v1_provider_config *,lang_component_v1_provider **,lang_component_v1_error *",
    ),
    (
        "provider_drop",
        "lang_component_v1_provider *,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "instance_create",
        "lang_component_v1_module *,const lang_component_v1_instance_config *,lang_component_v1_instance **,lang_component_v1_error *",
    ),
    (
        "instance_close",
        "lang_component_v1_instance *,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "bind_export",
        "lang_component_v1_instance *,lang_component_v1_bytes,lang_component_v1_binding **,lang_component_v1_error *",
    ),
    (
        "binding_drop",
        "lang_component_v1_binding *,lang_component_v1_error *",
    ),
    (
        "control_create",
        "lang_component_v1_engine *,const lang_component_v1_limits *,lang_component_v1_control **,lang_component_v1_error *",
    ),
    ("interrupt", "lang_component_v1_control *"),
    (
        "control_drop",
        "lang_component_v1_control *,lang_component_v1_error *",
    ),
    (
        "call",
        "lang_component_v1_binding *,lang_component_v1_control *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "context_call",
        "const lang_component_v1_context *,lang_component_v1_binding *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "context_check",
        "const lang_component_v1_context *,lang_component_v1_error *",
    ),
    (
        "context_buffer_create",
        "const lang_component_v1_context *,uint64_t,uint64_t,lang_component_v1_buffer **,lang_component_v1_error *",
    ),
    (
        "context_buffer_drop",
        "const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_error *",
    ),
    (
        "buffer_mut_bytes",
        "const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_mut_bytes *,lang_component_v1_error *",
    ),
    (
        "result_info",
        "const lang_component_v1_result *,lang_component_v1_result_info *,lang_component_v1_error *",
    ),
    (
        "result_usage",
        "const lang_component_v1_result *,lang_component_v1_usage *,lang_component_v1_error *",
    ),
    (
        "result_bytes",
        "const lang_component_v1_result *,lang_component_v1_bytes *,lang_component_v1_error *",
    ),
    (
        "result_drop",
        "lang_component_v1_result *,lang_component_v1_error *",
    ),
    (
        "resource_drop",
        "lang_component_v1_instance *,uint32_t,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "resource_publish",
        "lang_component_v1_instance *,lang_component_v1_provider *,lang_component_v1_bytes,lang_component_v1_bytes,uint64_t,uint32_t *,lang_component_v1_error *",
    ),
    (
        "context_resource_publish",
        "const lang_component_v1_context *,lang_component_v1_bytes,lang_component_v1_buffer *,uint32_t *,lang_component_v1_error *",
    ),
    (
        "context_resource_drop",
        "const lang_component_v1_context *,uint32_t,lang_component_v1_result **,lang_component_v1_error *",
    ),
    (
        "context_resource_read",
        "const lang_component_v1_context *,uint32_t,lang_component_v1_bytes *,lang_component_v1_error *",
    ),
    (
        "context_resource_write",
        "const lang_component_v1_context *,uint32_t,lang_component_v1_mut_bytes *,lang_component_v1_error *",
    ),
    (
        "context_resource_keep",
        "const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *",
    ),
    (
        "context_resource_take",
        "const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *",
    ),
];
const QUERY: &str = "lang_component_v1_status LANG_COMPONENT_V1_CALL @(uint32_t,uint32_t,const lang_component_v1_api **,lang_component_v1_error *)";

fn declaration(w: &mut Writer, out: &mut String, template: &str, name: &str) -> Result<()> {
    let (before, after) = template.split_once('@').ok_or(Error::Reference)?;
    w.text(out, before)?;
    w.text(out, name)?;
    w.text(out, after)
}
fn hash(w: &mut Writer, parts: &[&[u8]]) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for part in parts {
        w.budget.charge(part.len().saturating_add(8))?;
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    Ok(hash.finalize().into())
}
fn shape(template: &str, layouts: &[Layout]) -> Result<(usize, usize)> {
    if template.contains('*') || template.contains("_fn ") {
        return Ok((8, 8));
    }
    let (ty, array) = template.split_once(" @").ok_or(Error::Reference)?;
    let (size, align) = match ty {
        "uint8_t" => (1, 1),
        "uint32_t" => (4, 4),
        "uint64_t" => (8, 8),
        _ => {
            let name = ty.strip_prefix(PREFIX).ok_or(Error::Reference)?;
            let l = layouts
                .iter()
                .find(|l| l.name == name)
                .ok_or(Error::Reference)?;
            (l.size, l.alignment)
        }
    };
    let count = if array.is_empty() {
        1
    } else {
        array
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or(Error::Reference)?
    };
    Ok((size * count, align))
}
fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

/// Generate the same version-1 surface for either specified 64-bit target.
/// Fingerprints describe C declarations and fixed target layout, not Rust data.
pub fn generate(target: Target, limits: Limits) -> Result<Abi> {
    let mut w = Writer::new(limits);
    let mut out = String::new();
    w.text(&mut out,"#ifndef LANG_COMPONENT_ABI_V1_H\n#define LANG_COMPONENT_ABI_V1_H\n/* Generated from RFC 0027. Handles, scopes and buffers obey its lifetimes. */\n#include <stdint.h>\n#if defined(_WIN32)\n#define LANG_COMPONENT_V1_CALL __cdecl\n#else\n#define LANG_COMPONENT_V1_CALL\n#endif\n#ifdef __cplusplus\nextern \"C\" {\n#endif\n")?;
    for (name, value) in [
        ("VERSION", 1),
        ("OK", 0),
        ("CALL_FAILED", 1),
        ("INVALID_REQUEST", 2),
        ("UNSUPPORTED_VERSION", 3),
        ("BUSY", 4),
        ("STALE", 5),
        ("OUT_OF_MEMORY", 6),
        ("SOURCE", 0),
        ("LANGBC", 1),
    ] {
        w.text(&mut out, "#define LANG_COMPONENT_V1_")?;
        w.text(&mut out, name)?;
        w.text(&mut out, " UINT32_C(")?;
        w.number(&mut out, value)?;
        w.text(&mut out, ")\n")?;
    }
    w.text(&mut out, "typedef uint32_t lang_component_v1_status;\n")?;
    for name in [
        "engine", "module", "provider", "instance", "binding", "control", "buffer", "result",
    ] {
        w.text(&mut out, "typedef struct ")?;
        w.text(&mut out, PREFIX)?;
        w.text(&mut out, name)?;
        w.text(&mut out, " ")?;
        w.text(&mut out, PREFIX)?;
        w.text(&mut out, name)?;
        w.text(&mut out, ";\n")?;
    }
    let mut layouts = w.budget.vector(RECORDS.len() + 1)?;
    // Forward declarations allow callbacks to refer to context before its body.
    w.text(
        &mut out,
        "typedef struct lang_component_v1_context lang_component_v1_context;\n",
    )?;
    // Invoke callbacks take byte views by value, so emit those three leaves first.
    for (index, (name, fields)) in RECORDS.iter().enumerate() {
        if index == 3 {
            for (name, template) in CALLBACKS {
                w.text(&mut out, "typedef ")?;
                let mut full = String::new();
                w.text(&mut full, PREFIX)?;
                w.text(&mut full, name)?;
                declaration(&mut w, &mut out, template, &full)?;
                w.text(&mut out, ";\n")?;
            }
        }
        w.text(&mut out, "typedef struct ")?;
        w.text(&mut out, PREFIX)?;
        w.text(&mut out, name)?;
        w.text(&mut out, " {\n")?;
        let mut output = w.budget.vector(fields.len())?;
        let mut offset = 0;
        let mut alignment = 1;
        for (field, template) in *fields {
            w.budget
                .charge(layouts.len().saturating_add(template.len()))?;
            let (size, a) = shape(template, &layouts)?;
            offset = align(offset, a);
            alignment = alignment.max(a);
            output.push(Field {
                name: field,
                declaration: template,
                offset,
            });
            offset += size;
            declaration(&mut w, &mut out, template, field)?;
            w.text(&mut out, ";\n")?;
        }
        layouts.push(Layout {
            name,
            size: align(offset, alignment),
            alignment,
            fields: output,
        });
        w.text(&mut out, "} ")?;
        w.text(&mut out, PREFIX)?;
        w.text(&mut out, name)?;
        w.text(&mut out, ";\n")?;
    }
    let mut signatures = w.budget.vector(CALLBACKS.len() + API.len() + 1)?;
    for (name, template) in CALLBACKS {
        let fingerprint = hash(
            &mut w,
            &[
                b"LCAB1-signature",
                target.triple().as_bytes(),
                name.as_bytes(),
                template.as_bytes(),
            ],
        )?;
        signatures.push(Signature {
            name,
            declaration: template,
            fingerprint,
        });
    }
    w.text(
        &mut out,
        "typedef struct lang_component_v1_api {\nlang_component_v1_header header;\n",
    )?;
    let mut fields = w.budget.vector(API.len() + 2)?;
    fields.push(Field {
        name: "header",
        declaration: "lang_component_v1_header @",
        offset: 0,
    });
    for (i, (name, args)) in API.iter().enumerate() {
        let result = if *name == "interrupt" {
            "void"
        } else {
            "lang_component_v1_status"
        };
        let fingerprint = hash(
            &mut w,
            &[
                b"LCAB1-signature",
                target.triple().as_bytes(),
                name.as_bytes(),
                result.as_bytes(),
                b"LANG_COMPONENT_V1_CALL",
                args.as_bytes(),
            ],
        )?;
        signatures.push(Signature {
            name,
            declaration: args,
            fingerprint,
        });
        fields.push(Field {
            name,
            declaration: args,
            offset: 8 + i * 8,
        });
        w.text(&mut out, result)?;
        w.text(&mut out, " (LANG_COMPONENT_V1_CALL *")?;
        w.text(&mut out, name)?;
        w.text(&mut out, ")(")?;
        w.text(&mut out, args)?;
        w.text(&mut out, ");\n")?;
    }
    fields.push(Field {
        name: "reserved",
        declaration: "uint64_t @[2]",
        offset: 264,
    });
    layouts.push(Layout {
        name: "api",
        size: 280,
        alignment: 8,
        fields,
    });
    w.text(
        &mut out,
        "uint64_t reserved[2];\n} lang_component_v1_api;\n",
    )?;
    declaration(&mut w, &mut out, QUERY, "lang_component_v1_get_api")?;
    w.text(&mut out, ";\n#ifdef __cplusplus\n}\n#endif\n#endif\n")?;
    let fingerprint = hash(
        &mut w,
        &[
            b"LCAB1-signature",
            target.triple().as_bytes(),
            b"get_api",
            QUERY.as_bytes(),
        ],
    )?;
    signatures.push(Signature {
        name: "get_api",
        declaration: QUERY,
        fingerprint,
    });
    let mut digest = Sha256::new();
    digest.update(hash(
        &mut w,
        &[b"LCAB1-layout", target.triple().as_bytes()],
    )?);
    for l in &layouts {
        digest.update(hash(
            &mut w,
            &[
                l.name.as_bytes(),
                &(l.size as u64).to_le_bytes(),
                &(l.alignment as u64).to_le_bytes(),
            ],
        )?);
        for f in &l.fields {
            digest.update(hash(
                &mut w,
                &[
                    f.name.as_bytes(),
                    f.declaration.as_bytes(),
                    &(f.offset as u64).to_le_bytes(),
                ],
            )?);
        }
    }
    for s in &signatures {
        w.budget.charge(32)?;
        digest.update(s.fingerprint);
    }
    let fingerprint = digest.finalize().into();
    let usage = w.usage();
    Ok(Abi {
        header: out,
        target,
        layouts,
        signatures,
        fingerprint,
        usage,
        text_bytes: w.bytes,
    })
}
