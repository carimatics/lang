//! Typed, bounded canonical-frame tooling. Contextual indices carry no authority.
use super::{Error, Limits, Origin, Output, Result, Writer, abi, push};
use lang_mir::unit as m;
use lang_runtime::component::{bindings::MemberKey, idl, wire};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct Function {
    pub member: MemberKey,
    pub name: String,
    pub arguments: String,
    pub result: String,
    pub signature: [u8; 32],
    pub contract_span: m::Span,
}
#[derive(Debug)]
pub struct Header {
    pub header: String,
    pub functions: Vec<Function>,
    pub contract: [u8; 32],
    pub abi: [u8; 32],
    pub usage: m::Usage,
    pub text_bytes: usize,
    pub origins: Vec<Origin>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ref {
    Type(idl::TypeId),
    Definition(usize, usize),
}
struct Plan {
    nodes: Vec<Ref>,
    slots: Vec<Option<usize>>,
    bases: Vec<usize>,
}
impl Plan {
    fn new(w: &mut Writer<'_>) -> Result<Self> {
        let doc = w.document;
        let mut bases = w.budget.vector(doc.interfaces.len())?;
        let mut count = doc.type_nodes.len();
        for i in &doc.interfaces {
            w.budget.charge(1)?;
            bases.push(count);
            count = count
                .checked_add(i.definitions.len())
                .ok_or(Error::OutputLimit)?;
        }
        let mut slots = w.budget.vector(count)?;
        w.budget.charge(count)?;
        slots.resize(count, None);
        Ok(Self {
            nodes: w.budget.vector(0)?,
            slots,
            bases,
        })
    }
    fn canonical(w: &mut Writer<'_>, mut r: Ref, mut depth: usize) -> Result<(Ref, usize)> {
        loop {
            w.budget.charge(1)?;
            if depth > w.limits.depth {
                return Err(Error::Depth);
            }
            let Ref::Type(id) = r else {
                return Ok((r, depth));
            };
            let idl::TypeKind::Named(k) = &w
                .document
                .type_nodes
                .get(id.0)
                .ok_or(Error::Reference)?
                .kind
            else {
                return Ok((r, depth));
            };
            let (i, d) = w.definition(&k.value)?;
            r = match w.document.interfaces[i].definitions[d].kind {
                idl::DefinitionKind::Alias(child) => Ref::Type(child),
                _ => Ref::Definition(i, d),
            };
            depth += 1;
        }
    }
    fn slot(&self, r: Ref) -> usize {
        match r {
            Ref::Type(id) => id.0,
            Ref::Definition(i, d) => self.bases[i] + d,
        }
    }
    fn id(&self, w: &mut Writer<'_>, r: Ref) -> Result<usize> {
        let (r, _) = Self::canonical(w, r, 1)?;
        self.slots[self.slot(r)].ok_or(Error::Reference)
    }
    fn visit(&mut self, w: &mut Writer<'_>, root: Ref) -> Result<()> {
        let mut pending = w.budget.vector(1)?;
        pending.push((root, 1, false));
        while let Some((r, depth, finish)) = pending.pop() {
            let (r, depth) = Self::canonical(w, r, depth)?;
            let slot = self.slot(r);
            if self.slots[slot].is_some() {
                continue;
            }
            if finish {
                // Alias-resolved structural types share C identity across
                // occurrence nodes. Nominal definitions retain their own slot.
                let mut prior = None;
                if let Ref::Type(id) = r {
                    for (index, node) in self.nodes.iter().enumerate() {
                        w.budget.charge(1)?;
                        let Ref::Type(other) = node else { continue };
                        use idl::TypeKind as T;
                        let same = match (
                            &w.document.type_nodes[id.0].kind,
                            &w.document.type_nodes[other.0].kind,
                        ) {
                            (T::Unit, T::Unit)
                            | (T::I64, T::I64)
                            | (T::Bool, T::Bool)
                            | (T::Text, T::Text)
                            | (T::Bytes, T::Bytes) => true,
                            (T::List(a), T::List(b)) | (T::Option(a), T::Option(b)) => {
                                self.id(w, Ref::Type(*a))? == self.id(w, Ref::Type(*b))?
                            }
                            (T::Result(a, b), T::Result(c, d)) => {
                                self.id(w, Ref::Type(*a))? == self.id(w, Ref::Type(*c))?
                                    && self.id(w, Ref::Type(*b))? == self.id(w, Ref::Type(*d))?
                            }
                            (T::Own(a), T::Own(b)) => {
                                charge_resource(w, &a.value, &b.value)?;
                                a.value == b.value
                            }
                            (
                                T::Borrow {
                                    access: a,
                                    resource: ka,
                                },
                                T::Borrow {
                                    access: b,
                                    resource: kb,
                                },
                            ) => {
                                charge_resource(w, &ka.value, &kb.value)?;
                                a == b && ka.value == kb.value
                            }
                            _ => false,
                        };
                        if same {
                            prior = Some(index);
                            break;
                        }
                    }
                }
                let index = if let Some(index) = prior {
                    index
                } else {
                    let index = self.nodes.len();
                    push(&mut w.budget, &mut self.nodes, r)?;
                    index
                };
                self.slots[slot] = Some(index);
                continue;
            }
            push(&mut w.budget, &mut pending, (r, depth, true))?;
            match r {
                Ref::Definition(i, d) => {
                    let fields = match &w.document.interfaces[i].definitions[d].kind {
                        idl::DefinitionKind::Record(f) | idl::DefinitionKind::Variant(f) => f,
                        _ => return Err(Error::Reference),
                    };
                    for field in fields.iter().rev() {
                        push(
                            &mut w.budget,
                            &mut pending,
                            (Ref::Type(field.ty), depth + 1, false),
                        )?;
                    }
                }
                Ref::Type(id) => match w.document.type_nodes[id.0].kind {
                    idl::TypeKind::List(c) | idl::TypeKind::Option(c) => push(
                        &mut w.budget,
                        &mut pending,
                        (Ref::Type(c), depth + 1, false),
                    )?,
                    idl::TypeKind::Result(a, b) => {
                        push(
                            &mut w.budget,
                            &mut pending,
                            (Ref::Type(b), depth + 1, false),
                        )?;
                        push(
                            &mut w.budget,
                            &mut pending,
                            (Ref::Type(a), depth + 1, false),
                        )?;
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }
}
fn charge_resource(
    w: &mut Writer<'_>,
    a: &lang_runtime::component::negotiation::ResourceTypeKey,
    b: &lang_runtime::component::negotiation::ResourceTypeKey,
) -> Result<()> {
    for key in [a, b] {
        w.budget.charge(
            key.interface()
                .package()
                .namespace()
                .len()
                .saturating_add(key.interface().package().package().len())
                .saturating_add(key.interface().interface().len())
                .saturating_add(key.resource().len())
                .saturating_add(1),
        )?;
    }
    Ok(())
}
struct Generator<'a> {
    w: Writer<'a>,
    plan: Plan,
    prefix: String,
    out: String,
}
impl Generator<'_> {
    fn emit(&mut self, parts: &[&str]) -> Result<()> {
        for p in parts {
            self.w.text(&mut self.out, p)?;
        }
        Ok(())
    }
    fn number(&mut self, n: usize) -> Result<()> {
        self.w.number(&mut self.out, n)
    }
    fn name(&mut self, kind: &str, n: usize) -> Result<String> {
        let mut out = String::new();
        self.w.text(&mut out, &self.prefix)?;
        self.w.text(&mut out, kind)?;
        self.w.number(&mut out, n)?;
        Ok(out)
    }
    fn ty(&mut self, id: idl::TypeId) -> Result<String> {
        let index = self.plan.id(&mut self.w, Ref::Type(id))?;
        self.name("t", index)
    }
    fn child(&mut self, ty: idl::TypeId, field: usize, encode: bool) -> Result<()> {
        let id = self.plan.id(&mut self.w, Ref::Type(ty))?;
        let callee = self.name(if encode { "e" } else { "d" }, id)?;
        self.emit(&["if(!", &callee, "(s,&v->f"])?;
        self.number(field)?;
        self.emit(&[",depth+1))return 0;\n"])
    }
    fn representation(&mut self, index: usize, r: Ref) -> Result<()> {
        let name = self.name("t", index)?;
        self.emit(&["typedef "])?;
        match r {
            Ref::Type(id) => match &self.w.document.type_nodes[id.0].kind {
                idl::TypeKind::Unit => self.emit(&["lc_value_unit ", &name, ";\n"]),
                idl::TypeKind::I64 => self.emit(&["int64_t ", &name, ";\n"]),
                idl::TypeKind::Bool => self.emit(&["uint8_t ", &name, ";\n"]),
                idl::TypeKind::Text | idl::TypeKind::Bytes => {
                    self.emit(&["lang_component_v1_bytes ", &name, ";\n"])
                }
                idl::TypeKind::Own(_) | idl::TypeKind::Borrow { .. } => {
                    self.emit(&["struct {uint32_t index;} ", &name, ";\n"])
                }
                idl::TypeKind::List(child) => {
                    let child = self.ty(*child)?;
                    self.emit(&["struct {", &child, " *data;uint64_t count;} ", &name, ";\n"])
                }
                idl::TypeKind::Option(child) => {
                    let child = self.ty(*child)?;
                    self.emit(&["struct {uint32_t tag;", &child, " value;} ", &name, ";\n"])
                }
                idl::TypeKind::Result(a, b) => {
                    let a = self.ty(*a)?;
                    let b = self.ty(*b)?;
                    self.emit(&[
                        "struct {uint32_t tag;union {",
                        &a,
                        " f0;",
                        &b,
                        " f1;} data;} ",
                        &name,
                        ";\n",
                    ])
                }
                idl::TypeKind::Named(_) => Err(Error::Reference),
            },
            Ref::Definition(i, d) => {
                let definition = &self.w.document.interfaces[i].definitions[d];
                let (fields, variant) = match &definition.kind {
                    idl::DefinitionKind::Record(f) => (f, false),
                    idl::DefinitionKind::Variant(f) => (f, true),
                    _ => return Err(Error::Reference),
                };
                self.emit(&["struct {"])?;
                if variant {
                    self.emit(&["uint32_t tag;union {"])?;
                }
                if fields.is_empty() {
                    self.emit(&["uint8_t unused;"])?;
                }
                for (i, field) in fields.iter().enumerate() {
                    let child = self.ty(field.ty)?;
                    self.emit(&[&child, " f"])?;
                    self.number(i)?;
                    self.emit(&[";"])?;
                }
                if variant {
                    self.emit(&["} data;"])?;
                }
                self.emit(&["} ", &name, ";\n"])
            }
        }
    }
    fn codec(&mut self, index: usize, r: Ref, encode: bool) -> Result<()> {
        let name = self.name("t", index)?;
        let method = self.name(if encode { "e" } else { "d" }, index)?;
        self.emit(&[
            "LC_VALUE_INLINE int ",
            &method,
            "(",
            if encode {
                "lc_value_writer *s,const "
            } else {
                "lc_value_reader *s,"
            },
            &name,
            " *v,uint64_t depth){\nif(!lc_value_enter(&s->left,&s->status,depth))return 0;\n",
        ])?;
        match r {
            Ref::Type(id) => {
                use idl::TypeKind as T;
                match &self.w.document.type_nodes[id.0].kind {
                    T::Unit=>self.emit(&[if encode{"(void)v;return 1;\n"}else{"v->unused=0;return 1;\n"}])?,
                    T::I64=>self.emit(&[if encode{"return lc_value_unsigned(s,(uint64_t)*v,8);\n"}else{"uint64_t n;if(!lc_value_number(s,&n,8))return 0;*v=n<=INT64_MAX?(int64_t)n:-INT64_C(1)-(int64_t)(UINT64_MAX-n);return 1;\n"}])?,
                    T::Bool=>self.emit(&[if encode{"if(*v>1)return lc_value_bad(&s->status,2);return lc_value_unsigned(s,*v,1);\n"}else{"uint64_t n;if(!lc_value_number(s,&n,1))return 0;if(n>1)return lc_value_bad(&s->status,2);*v=(uint8_t)n;return 1;\n"}])?,
                    T::Text|T::Bytes=>{let text=matches!(self.w.document.type_nodes[id.0].kind,T::Text);self.emit(&[if encode{"return lc_value_blob_write(s,v,"}else{"return lc_value_blob_read(s,v,"},if text{"1"}else{"0"},");\n"])?;}
                    T::Own(_)|T::Borrow{..}=>self.emit(&[if encode{"if(!s->contextual||!v->index)return lc_value_bad(&s->status,2);return lc_value_unsigned(s,v->index,4);\n"}else{"uint64_t n;if(!s->contextual)return lc_value_bad(&s->status,2);if(!lc_value_number(s,&n,4))return 0;if(!n)return lc_value_bad(&s->status,2);v->index=(uint32_t)n;return 1;\n"}])?,
                    T::List(child)=>{
                        let child=*child;let ty=self.ty(child)?;let id=self.plan.id(&mut self.w,Ref::Type(child))?;let callee=self.name(if encode{"e"}else{"d"},id)?;
                        if encode {self.emit(&["uint64_t i;if(v->count>UINT32_MAX)return lc_value_bad(&s->status,2);if(v->count>s->left.elements)return lc_value_bad(&s->status,3);if(v->count&&!v->data)return lc_value_bad(&s->status,1);if(!lc_value_unsigned(s,v->count,4))return 0;for(i=0;i<v->count;++i)if(!",&callee,"(s,&v->data[i],depth+1))return 0;return 1;\n"])?;}
                        else {self.emit(&["uint64_t i,n;if(!lc_value_number(s,&n,4))return 0;if(n>s->left.elements)return lc_value_bad(&s->status,3);v->count=n;v->data=(",&ty,"*)lc_value_array(s,n,sizeof(",&ty,"),LC_VALUE_ALIGNOF(",&ty,"));if(n&&!v->data)return 0;for(i=0;i<n;++i)if(!",&callee,"(s,&v->data[i],depth+1))return 0;return 1;\n"])?;}
                    }
                    T::Option(child)=>{
                        let child=*child;let id=self.plan.id(&mut self.w,Ref::Type(child))?;let callee=self.name(if encode{"e"}else{"d"},id)?;
                        self.tag(2,1,encode)?;
                        self.emit(&["if(v->tag==0)return 1;return ",&callee,"(s,&v->value,depth+1);\n"])?;
                    }
                    T::Result(a,b)=>{let a=*a;let b=*b;self.tag(2,1,encode)?;self.emit(&["switch(v->tag){\n"])?;self.case(a,0,encode)?;self.case(b,1,encode)?;self.emit(&["default:return lc_value_bad(&s->status,2);}\n"])?;}
                    T::Named(_)=>return Err(Error::Reference),
                }
            }
            Ref::Definition(i, d) => {
                let definition = &self.w.document.interfaces[i].definitions[d];
                let (fields, variant) = match &definition.kind {
                    idl::DefinitionKind::Record(f) => (f, false),
                    idl::DefinitionKind::Variant(f) => (f, true),
                    _ => return Err(Error::Reference),
                };
                if variant {
                    self.tag(fields.len(), 4, encode)?;
                    self.emit(&["switch(v->tag){\n"])?;
                    for (i, f) in fields.iter().enumerate() {
                        self.case(f.ty, i, encode)?;
                    }
                    self.emit(&["default:return lc_value_bad(&s->status,2);}\n"])?;
                } else {
                    for (i, f) in fields.iter().enumerate() {
                        self.child(f.ty, i, encode)?;
                    }
                    self.emit(&["(void)v;return 1;\n"])?;
                }
            }
        }
        self.emit(&["}\n"])
    }
    fn tag(&mut self, count: usize, width: usize, encode: bool) -> Result<()> {
        if !encode {
            self.emit(&["uint64_t n;if(!lc_value_number(s,&n,"])?;
            self.number(width)?;
            self.emit(&["))return 0;v->tag=(uint32_t)n;\n"])?;
        }
        self.emit(&["if(v->tag>="])?;
        self.number(count)?;
        self.emit(&[")return lc_value_bad(&s->status,2);\n"])?;
        if encode {
            self.emit(&["if(!lc_value_unsigned(s,v->tag,"])?;
            self.number(width)?;
            self.emit(&["))return 0;\n"])?;
        }
        Ok(())
    }
    fn case(&mut self, ty: idl::TypeId, index: usize, encode: bool) -> Result<()> {
        let id = self.plan.id(&mut self.w, Ref::Type(ty))?;
        let method = self.name(if encode { "e" } else { "d" }, id)?;
        self.emit(&["case "])?;
        self.number(index)?;
        self.emit(&[":return ", &method, "(s,&v->data.f"])?;
        self.number(index)?;
        self.emit(&[",depth+1);\n"])
    }
    fn function(&mut self, index: usize, i: usize, f: usize, digest: [u8; 32]) -> Result<Function> {
        let function = &self.w.document.interfaces[i].functions[f];
        let name = self.name("f", index)?;
        let arguments = self.name("a", index)?;
        let result = self.ty(function.result)?;
        let key = &self.w.document.interfaces[i].key.value;
        self.emit(&[
            "/* ",
            key.package().namespace(),
            ":",
            key.package().package(),
            "/",
            key.interface(),
            "#",
            &function.name.value,
            "; capability ",
            &function.capability.value,
            "; ",
            if function.effect.value == idl::Effect::Task {
                "task"
            } else {
                "sync"
            },
            ". */\n",
        ])?;
        self.emit(&["typedef struct {"])?;
        if function.params.is_empty() {
            self.emit(&["uint8_t unused;"])?;
        }
        for (n, p) in function.params.iter().enumerate() {
            let ty = self.ty(p.ty)?;
            self.emit(&[&ty, " p"])?;
            self.number(n)?;
            self.emit(&[";"])?;
        }
        self.emit(&[
            "} ",
            &arguments,
            ";\nstatic const uint8_t ",
            &name,
            "_signature[32]={",
        ])?;
        for (n, byte) in digest.iter().enumerate() {
            if n != 0 {
                self.emit(&[","])?;
            }
            self.number(*byte as usize)?;
        }
        self.emit(&["};\n"])?;
        for encode in [true, false] {
            for params in [true, false] {
                self.emit(&[
                    "LC_VALUE_INLINE int ",
                    &name,
                    if encode { "_encode_" } else { "_decode_" },
                    if params { "arguments" } else { "result" },
                    "(",
                    if encode {
                        "lc_value_writer *s,const "
                    } else {
                        "lc_value_reader *s,"
                    },
                    if params { &arguments } else { &result },
                    " *value){\n",
                ])?;
                self.emit(&[
                    if encode {
                        "if(!lc_value_frame_write(s,"
                    } else {
                        "if(!lc_value_frame_read(s,"
                    },
                    &name,
                    "_signature,",
                    if params { "0" } else { "1" },
                    "))return 0;\n",
                ])?;
                if !encode {
                    self.emit(&[
                        if params { &arguments } else { &result },
                        " temporary={0};\n",
                    ])?;
                }
                if params {
                    for (n, p) in function.params.iter().enumerate() {
                        let id = self.plan.id(&mut self.w, Ref::Type(p.ty))?;
                        let method = self.name(if encode { "e" } else { "d" }, id)?;
                        self.emit(&[
                            "if(!",
                            &method,
                            "(s,&",
                            if encode { "value->p" } else { "temporary.p" },
                        ])?;
                        self.number(n)?;
                        self.emit(&[",1))return 0;\n"])?;
                    }
                } else {
                    let id = self.plan.id(&mut self.w, Ref::Type(function.result))?;
                    let method = self.name(if encode { "e" } else { "d" }, id)?;
                    self.emit(&[
                        "if(!",
                        &method,
                        "(s,",
                        if encode { "value" } else { "&temporary" },
                        ",1))return 0;\n",
                    ])?;
                }
                if encode {
                    self.emit(&["(void)value;return lc_value_frame_end(s);\n"])?;
                } else {
                    self.emit(&["if(s->at!=s->length)return lc_value_bad(&s->status,2);*value=temporary;return 1;\n"])?;
                }
                self.emit(&["}\n"])?;
            }
        }
        for context in [false, true] {
            self.emit(&["LC_VALUE_INLINE lc_value_outcome ",&name,if context{"_context_call"}else{"_call"},"(const lang_component_v1_api *api,",if context{"const lang_component_v1_context *context,"}else{"lang_component_v1_control *control,"},"lang_component_v1_binding *binding,const ",&arguments," *arguments,lc_value_writer *scratch,lang_component_v1_error *error){\nlang_component_v1_result *result=NULL;lang_component_v1_bytes bytes;lang_component_v1_status status;\n"])?;
            self.emit(&["scratch->contextual=1;if(!",&name,"_encode_arguments(scratch,arguments)){lc_value_outcome out={LANG_COMPONENT_V1_INVALID_REQUEST,0,0,NULL};return out;}\nbytes.data=scratch->data;bytes.length=scratch->at;\nstatus=api->",if context{"context_call(context,binding"}else{"call(binding,control"},",bytes,&result,error);return lc_value_observe(api,status,result,error);\n}\n"])?;
        }
        let member = self.w.member(
            &self.w.document.interfaces[i].key.value,
            &function.name.value,
        )?;
        Ok(Function {
            member,
            name,
            arguments,
            result,
            signature: digest,
            contract_span: function.span,
        })
    }
}
fn remaining(w: &Writer<'_>) -> wire::Limits {
    let b = w.budget.limits();
    let u = w.usage();
    wire::Limits {
        work: b.work - u.work,
        allocation_bytes: b.allocations - u.allocation_bytes,
        bytes: w.limits.output_bytes,
        serialization_bytes: w.limits.output_bytes,
        depth: w.limits.depth,
        metadata: idl::Limits {
            depth: w.limits.depth,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn absorb(w: &mut Writer<'_>, usage: wire::Usage) -> Result<()> {
    w.budget.charge(usage.work)?;
    w.budget.bytes(usage.allocation_bytes)?;
    w.budget.allocate::<()>(usage.elements)?;
    Ok(())
}

/// One header per complete checked contract. Typed codecs do not bind providers.
pub fn generate(contract: &wire::Contract, target: abi::Target, limits: Limits) -> Result<Header> {
    let document = contract.checked();
    if document.usage().data_depth > limits.depth {
        return Err(Error::Depth);
    }
    let mut w = Writer {
        document: document.document(),
        output: Output {
            limits,
            budget: m::Budget::with_usage(limits.budget, document.usage().budget)?,
            bytes: 0,
        },
    };
    let mut plan = Plan::new(&mut w)?;
    let doc = w.document;
    for (i, interface) in doc.interfaces.iter().enumerate() {
        for (d, def) in interface.definitions.iter().enumerate() {
            if !matches!(def.kind, idl::DefinitionKind::Alias(_)) {
                plan.visit(&mut w, Ref::Definition(i, d))?;
            }
        }
        for function in &interface.functions {
            for p in &function.params {
                plan.visit(&mut w, Ref::Type(p.ty))?;
            }
            plan.visit(&mut w, Ref::Type(function.result))?;
        }
    }
    let count = doc.interfaces.iter().map(|i| i.functions.len()).sum();
    let mut signatures = w.budget.vector(count)?;
    let mut session = wire::Session::new(remaining(&w));
    let canonical = wire::encode_contract(contract, &mut session).map_err(Error::Wire)?;
    for (i, interface) in doc.interfaces.iter().enumerate() {
        for (f, function) in interface.functions.iter().enumerate() {
            let signature = wire::Signature::new(
                contract,
                &interface.key.value,
                &function.name.value,
                &mut session,
            )
            .map_err(Error::Wire)?;
            signatures.push((i, f, signature.digest()));
        }
    }
    absorb(&mut w, session.usage())?;
    w.budget.charge(canonical.len())?;
    let digest: [u8; 32] = Sha256::digest(&canonical).into();
    drop(canonical);
    let mut prefix = String::new();
    w.text(&mut prefix, "component_c_")?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        let pair = [HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]];
        w.text(
            &mut prefix,
            std::str::from_utf8(&pair).map_err(|_| Error::Reference)?,
        )?;
    }
    w.text(&mut prefix, "_")?;
    let b = w.budget.limits();
    let u = w.usage();
    let generated = abi::generate(
        target,
        Limits {
            budget: m::Limits {
                work: b.work - u.work,
                allocations: b.allocations - u.allocation_bytes,
                elements: b.elements - u.elements,
                ..b
            },
            output_bytes: limits.output_bytes - w.bytes,
            ..limits
        },
    )?;
    w.budget.charge(generated.usage.work)?;
    w.budget.bytes(generated.usage.allocation_bytes)?;
    w.budget.allocate::<()>(generated.usage.elements)?;
    w.bytes += generated.text_bytes;
    let abi = generated.fingerprint;
    let mut g = Generator {
        w,
        plan,
        prefix,
        out: generated.header,
    };
    g.emit(&[
        "\n#include <stdint.h>\n",
        include_str!("value-v1.h"),
        "\n#ifndef ",
    ])?;
    let mut guard = String::new();
    g.w.text(&mut guard, &g.prefix)?;
    g.emit(&[&guard, "HEADER\n#define ", &guard, "HEADER\n"])?;
    let mut origins = g.w.budget.vector(g.plan.nodes.len() + count)?;
    for index in 0..g.plan.nodes.len() {
        let r = g.plan.nodes[index];
        let start = g.out.len();
        g.representation(index, r)?;
        g.codec(index, r, true)?;
        g.codec(index, r, false)?;
        let contract = match r {
            Ref::Type(id) => g.w.document.type_nodes[id.0].span,
            Ref::Definition(i, d) => g.w.document.interfaces[i].definitions[d].span,
        };
        origins.push(Origin {
            start,
            end: g.out.len(),
            contract,
        });
    }
    let mut functions = g.w.budget.vector(count)?;
    for (index, (i, f, digest)) in signatures.into_iter().enumerate() {
        let start = g.out.len();
        let function = g.function(index, i, f, digest)?;
        origins.push(Origin {
            start,
            end: g.out.len(),
            contract: function.contract_span,
        });
        functions.push(function);
    }
    g.emit(&["#endif\n"])?;
    let usage = g.w.usage();
    let text_bytes = g.w.bytes;
    Ok(Header {
        header: g.out,
        functions,
        contract: digest,
        abi,
        usage,
        text_bytes,
        origins,
    })
}
