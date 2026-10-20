use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HeaderOptions {
    /// Optional artifact-local readable aliases. Ambiguous aliases are errors.
    pub aliases: bool,
}
impl Artifact {
    pub fn header(&self, options: HeaderOptions, limits: Limits) -> Result<String> {
        let mut out = Header {
            bytes: String::new(),
            budget: m::Budget::new(limits.budget),
            limit: limits.header_bytes,
        };
        let guard = format!("LANG_CABI_ARTIFACT_{}", self.digest).to_ascii_uppercase();
        out.add(&format!(
            "#ifndef {guard}\n#define {guard}\n#include <stdint.h>\n#include <stddef.h>\n"
        ))?;
        let condition = match self.target {
            Target::MacosAarch64 => "defined(__APPLE__) && defined(__aarch64__)",
            Target::WindowsX86_64 => "defined(_WIN32) && (defined(_M_X64) || defined(__x86_64__))",
        };
        out.add(&format!(
            "#if !({condition})\n#error incompatible_C_ABI_target\n#endif\n"
        ))?;
        out.add(SUPPORT)?;
        for (index, ty) in self.types.iter().enumerate() {
            out.budget.charge(1)?;
            if matches!(ty.kind, Kind::I64 | Kind::Bool) {
                continue;
            }
            let name = self.c_type(TypeId(index));
            let guard = format!("LANG_CTYPE_{}", ty.digest).to_ascii_uppercase();
            out.add(&format!("#ifndef {guard}\n#define {guard}\n"))?;
            match &ty.kind {
                Kind::Unit => {
                    out.add(&format!("typedef struct {{ uint8_t reserved; }} {name};\n"))?
                }
                Kind::Record { fields, .. } => {
                    out.add("typedef struct {\n")?;
                    if fields.is_empty() {
                        out.add("  uint8_t reserved;\n")?;
                    }
                    for field in fields {
                        if !identifier(&field.name) {
                            return Err(Error::InvalidName);
                        }
                        out.add(&format!("  {} f_{};\n", self.c_type(field.ty), field.name))?;
                    }
                    out.add(&format!("}} {name};\n"))?;
                    for field in fields {
                        out.offset(&name, &format!("f_{}", field.name), field.offset)?;
                    }
                }
                Kind::Enum {
                    cases,
                    payload_offset,
                    payload_size,
                    ..
                } => {
                    for (index, case) in cases.iter().enumerate() {
                        let payload_name = format!("{name}_case_{index}");
                        out.add("typedef struct {\n")?;
                        if case.payload.is_empty() {
                            out.add("  uint8_t reserved;\n")?;
                        }
                        for (i, ty) in case.payload.iter().enumerate() {
                            out.add(&format!("  {} p{i};\n", self.c_type(*ty)))?;
                        }
                        out.add(&format!("}} {payload_name};\n"))?;
                        out.layout(&payload_name, case.size, case.align)?;
                        for (i, offset) in case.offsets.iter().enumerate() {
                            out.offset(&payload_name, &format!("p{i}"), *offset)?;
                        }
                        if !identifier(&case.name) {
                            return Err(Error::InvalidName);
                        }
                        out.add(&format!(
                            "#define {name}_tag_{} UINT32_C({index})\n",
                            case.name
                        ))?;
                    }
                    out.add("typedef struct {\n  uint32_t tag;\n  union {\n")?;
                    for (index, _) in cases.iter().enumerate() {
                        out.add(&format!("    {name}_case_{index} c{index};\n"))?;
                    }
                    out.add(&format!("  }} payload;\n}} {name};\n"))?;
                    out.offset(&name, "tag", 0)?;
                    out.offset(&name, "payload", *payload_offset)?;
                    out.add(&format!("_Static_assert(sizeof((({name} *)0)->payload) == UINT64_C({payload_size}), \"{name} union extent\");\n"))?;
                }
                Kind::I64 | Kind::Bool => unreachable!(),
            }
            out.layout(&name, ty.size, ty.align)?;
            if matches!(ty.kind, Kind::Unit)
                || matches!(&ty.kind, Kind::Record { fields, .. } if fields.is_empty())
            {
                out.offset(&name, "reserved", 0)?;
            }
            out.add("#endif\n")?;
        }
        let mut symbols: Vec<String> = out.budget.vector(self.functions.len() + 3)?;
        for name in ["create", "destroy", "last_error"] {
            symbols.push(text(
                &mut out.budget,
                &format!("lang_context_{name}_{}", self.digest),
            )?);
        }
        let mut aliases: Vec<String> = out.budget.vector(self.functions.len())?;
        for function in &self.functions {
            out.budget.charge(symbols.len())?;
            if symbols.contains(&function.symbol) {
                return Err(Error::Collision(CollisionKind::Symbol));
            }
            symbols.push(text(&mut out.budget, &function.symbol)?);
            let callback = format!("lang_callback_{}", function.digest);
            match function.direction {
                Direction::Import => out.add(&format!(
                    "typedef uint32_t (*{callback})(lang_context *context, void *user"
                ))?,
                Direction::Export => out.add(&format!(
                    "uint32_t {}(lang_context *context",
                    function.symbol
                ))?,
            }
            for (index, id) in function.parameters.iter().enumerate() {
                let ty = &self.types[id.0];
                let name = self.c_type(*id);
                if matches!(ty.kind, Kind::I64 | Kind::Bool) {
                    out.add(&format!(", {name} p{index}"))?;
                } else {
                    out.add(&format!(", const {name} *p{index}"))?;
                }
            }
            if !matches!(self.types[function.result.0].kind, Kind::Unit) {
                out.add(&format!(", {} *out", self.c_type(function.result)))?;
            }
            out.add(");\n")?;
            if options.aliases {
                let root = function
                    .name
                    .rsplit("::")
                    .next()
                    .ok_or(Error::InvalidName)?;
                let alias = if function.direction == Direction::Import {
                    format!("lang_{root}_callback")
                } else {
                    format!("lang_{root}")
                };
                out.budget.charge(aliases.len() + symbols.len())?;
                if aliases.contains(&alias) || symbols.contains(&alias) {
                    return Err(Error::Collision(CollisionKind::Alias));
                }
                aliases.push(text(&mut out.budget, &alias)?);
                out.add(&format!(
                    "#define {alias} {}\n",
                    if function.direction == Direction::Import {
                        &callback
                    } else {
                        &function.symbol
                    }
                ))?;
            }
        }
        out.add(&format!(
            "uint32_t {}(const lang_program_config *config, lang_context **out);\n",
            symbols[0]
        ))?;
        out.add(&format!(
            "uint32_t {}(lang_context *context);\n",
            symbols[1]
        ))?;
        out.add(&format!(
            "uint32_t {}(lang_context *context, lang_error_summary *out);\n",
            symbols[2]
        ))?;
        out.add("#endif\n")?;
        Ok(out.bytes)
    }
    fn c_type(&self, id: TypeId) -> String {
        match self.types[id.0].kind {
            Kind::I64 => "int64_t".into(),
            Kind::Bool => "uint8_t".into(),
            _ => format!("lang_type_{}", self.types[id.0].digest),
        }
    }
}
fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
struct Header {
    bytes: String,
    budget: m::Budget,
    limit: usize,
}
impl Header {
    fn add(&mut self, value: &str) -> Result<()> {
        let count = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(Error::Overflow)?;
        cap(LimitKind::HeaderBytes, count, self.limit)?;
        self.budget.charge(value.len())?;
        self.budget.allocate::<u8>(value.len())?;
        self.bytes
            .try_reserve(value.len())
            .map_err(|_| Error::Allocation)?;
        self.bytes.push_str(value);
        Ok(())
    }
    fn layout(&mut self, name: &str, size: u64, align: u64) -> Result<()> {
        self.add(&format!("_Static_assert(sizeof({name}) == UINT64_C({size}), \"{name} size\");\n_Static_assert(_Alignof({name}) == UINT64_C({align}), \"{name} alignment\");\n"))
    }
    fn offset(&mut self, name: &str, field: &str, offset: u64) -> Result<()> {
        self.add(&format!(
            "_Static_assert(offsetof({name}, {field}) == UINT64_C({offset}), \"{name}.{field}\");\n"
        ))
    }
}
const SUPPORT: &str = include_str!("support.h");
