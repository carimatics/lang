use super::super::negotiation::{InterfaceKey, ResourceTypeKey};
use super::*;

fn interface_bytes(key: &InterfaceKey) -> usize {
    key.package().namespace().len() + key.package().package().len() + key.interface().len() + 2
}
fn value_bytes(key: &ValueKey) -> usize {
    interface_bytes(&key.interface) + key.name.len() + 1
}
fn resource_bytes(key: &ResourceTypeKey) -> usize {
    interface_bytes(key.interface()) + key.resource().len() + 1
}
fn duplicate(c: &Context, code: Code, span: m::Span, first: m::Span) -> Error {
    Error {
        related: Some(first),
        ..c.error(code, Reason::Duplicate, span)
    }
}
fn names(values: &[Field], c: &mut Context, span: m::Span) -> Result<(), Error> {
    let mut names = c.vector(values.len(), span)?;
    names.extend(values.iter().map(|v| &v.name));
    c.sorting(
        names.len(),
        names.iter().map(|v| v.value.len()).max().unwrap_or(0),
        span,
    )?;
    names.sort_unstable_by(|a, b| a.value.cmp(&b.value).then(a.span.start.cmp(&b.span.start)));
    for pair in names.windows(2) {
        if pair[0].value == pair[1].value {
            return Err(duplicate(c, Code::Name, pair[1].span, pair[0].span));
        }
    }
    Ok(())
}

pub(super) fn check(d: &mut Document, c: &mut Context) -> Result<(), Error> {
    check_inner(d, c).map(|_| ())
}
/// A serialized dictionary can contain otherwise unreachable type nodes.
pub(super) fn check_complete(d: &mut Document, c: &mut Context) -> Result<(), Error> {
    let facts = check_inner(d, c)?;
    for (id, node) in d.type_nodes.iter().enumerate() {
        c.charge(1, node.span)?;
        if !matches!(node.kind, TypeKind::Borrow { .. }) {
            no_borrow(TypeId(id), d, &facts, c)?;
        }
    }
    Ok(())
}
fn check_inner(d: &mut Document, c: &mut Context) -> Result<Vec<Facts>, Error> {
    let root = c.span(0, d.source_bytes as usize);
    c.sorting(
        d.interfaces.len(),
        d.interfaces
            .iter()
            .map(|v| interface_bytes(&v.key.value))
            .max()
            .unwrap_or(0),
        root,
    )?;
    d.interfaces.sort_unstable_by(|a, b| {
        a.key
            .value
            .cmp(&b.key.value)
            .then(a.key.span.start.cmp(&b.key.span.start))
    });
    for pair in d.interfaces.windows(2) {
        if pair[0].key.value == pair[1].key.value {
            return Err(duplicate(c, Code::Name, pair[1].key.span, pair[0].key.span));
        }
    }
    for i in &mut d.interfaces {
        c.sorting(
            i.resources.len(),
            i.resources
                .iter()
                .map(|v| resource_bytes(&v.value))
                .max()
                .unwrap_or(0),
            i.span,
        )?;
        i.resources
            .sort_unstable_by(|a, b| a.value.cmp(&b.value).then(a.span.start.cmp(&b.span.start)));
        for pair in i.resources.windows(2) {
            if pair[0].value == pair[1].value {
                return Err(duplicate(c, Code::Name, pair[1].span, pair[0].span));
            }
        }
        c.sorting(
            i.definitions.len(),
            i.definitions
                .iter()
                .map(|v| value_bytes(&v.key.value))
                .max()
                .unwrap_or(0),
            i.span,
        )?;
        i.definitions.sort_unstable_by(|a, b| {
            a.key
                .value
                .cmp(&b.key.value)
                .then(a.key.span.start.cmp(&b.key.span.start))
        });
        for pair in i.definitions.windows(2) {
            if pair[0].key.value == pair[1].key.value {
                return Err(duplicate(c, Code::Name, pair[1].key.span, pair[0].key.span));
            }
        }
        for def in &i.definitions {
            if let DefinitionKind::Record(fields) | DefinitionKind::Variant(fields) = &def.kind {
                names(fields, c, def.span)?;
            }
        }
        c.sorting(
            i.functions.len(),
            i.functions
                .iter()
                .map(|v| v.name.value.len())
                .max()
                .unwrap_or(0),
            i.span,
        )?;
        i.functions.sort_unstable_by(|a, b| {
            a.name
                .value
                .cmp(&b.name.value)
                .then(a.name.span.start.cmp(&b.name.span.start))
        });
        for pair in i.functions.windows(2) {
            if pair[0].name.value == pair[1].name.value {
                return Err(duplicate(
                    c,
                    Code::Name,
                    pair[1].name.span,
                    pair[0].name.span,
                ));
            }
        }
        for function in &i.functions {
            names(&function.params, c, function.span)?;
        }
    }
    let definitions = d.interfaces.iter().map(|i| i.definitions.len()).sum();
    let resources = d.interfaces.iter().map(|i| i.resources.len()).sum();
    let mut defs = c.vector(definitions, root)?;
    let mut res = c.vector(resources, root)?;
    for i in &d.interfaces {
        defs.extend(i.definitions.iter());
        res.extend(i.resources.iter());
    }
    let graph = Graph::new(d, &defs, &res, c, root);
    let facts = graph.verify()?;
    for def in &defs {
        match &def.kind {
            DefinitionKind::Record(fields) | DefinitionKind::Variant(fields) => {
                for field in fields {
                    no_borrow(field.ty, d, &facts, c)?;
                }
            }
            DefinitionKind::Alias(ty) => no_borrow(*ty, d, &facts, c)?,
        }
    }
    for i in &d.interfaces {
        for function in &i.functions {
            for field in &function.params {
                if !matches!(d.type_nodes[field.ty.0].kind, TypeKind::Borrow { .. }) {
                    no_borrow(field.ty, d, &facts, c)?;
                }
                if function.effect.value == Effect::Task && facts[field.ty.0].resource {
                    return Err(Error {
                        related: Some(function.effect.span),
                        ..c.error(
                            Code::Type,
                            Reason::InvalidEffect,
                            d.type_nodes[field.ty.0].span,
                        )
                    });
                }
            }
            no_borrow(function.result, d, &facts, c)?;
            if function.effect.value == Effect::Task && facts[function.result.0].resource {
                return Err(Error {
                    related: Some(function.effect.span),
                    ..c.error(
                        Code::Type,
                        Reason::InvalidEffect,
                        d.type_nodes[function.result.0].span,
                    )
                });
            }
        }
    }
    c.sorting(
        d.imports.len(),
        d.imports
            .iter()
            .map(|v| interface_bytes(&v.interface.value))
            .max()
            .unwrap_or(0),
        root,
    )?;
    d.imports.sort_unstable_by(|a, b| {
        a.interface
            .value
            .cmp(&b.interface.value)
            .then(a.interface.span.start.cmp(&b.interface.span.start))
    });
    for pair in d.imports.windows(2) {
        if pair[0].interface.value == pair[1].interface.value {
            return Err(duplicate(
                c,
                Code::Projection,
                pair[1].interface.span,
                pair[0].interface.span,
            ));
        }
    }
    for import in &mut d.imports {
        c.lookup(
            d.interfaces.len(),
            interface_bytes(&import.interface.value),
            import.interface.span,
        )?;
        let index = d
            .interfaces
            .binary_search_by(|i| i.key.value.cmp(&import.interface.value))
            .map_err(|_| c.error(Code::Projection, Reason::Unresolved, import.interface.span))?;
        let target = &d.interfaces[index];
        c.sorting(
            import.members.len(),
            import
                .members
                .iter()
                .map(|v| v.value.len())
                .max()
                .unwrap_or(0),
            import.span,
        )?;
        import
            .members
            .sort_unstable_by(|a, b| a.value.cmp(&b.value).then(a.span.start.cmp(&b.span.start)));
        for pair in import.members.windows(2) {
            if pair[0].value == pair[1].value {
                return Err(duplicate(c, Code::Projection, pair[1].span, pair[0].span));
            }
        }
        for member in &import.members {
            c.lookup(target.functions.len(), member.value.len(), member.span)?;
            let index = target
                .functions
                .binary_search_by(|f| f.name.value.cmp(&member.value))
                .map_err(|_| c.error(Code::Projection, Reason::Unresolved, member.span))?;
            if target.functions[index].effect.value != Effect::Sync {
                return Err(Error {
                    related: Some(target.functions[index].effect.span),
                    ..c.error(Code::Type, Reason::InvalidEffect, member.span)
                });
            }
        }
    }
    c.sorting(
        d.exports.len(),
        d.exports
            .iter()
            .map(|v| interface_bytes(&v.value))
            .max()
            .unwrap_or(0),
        root,
    )?;
    d.exports
        .sort_unstable_by(|a, b| a.value.cmp(&b.value).then(a.span.start.cmp(&b.span.start)));
    for pair in d.exports.windows(2) {
        if pair[0].value == pair[1].value {
            return Err(duplicate(c, Code::Projection, pair[1].span, pair[0].span));
        }
    }
    for export in &d.exports {
        c.lookup(
            d.interfaces.len(),
            interface_bytes(&export.value),
            export.span,
        )?;
        if export.value.package() != &d.package.value
            || d.interfaces
                .binary_search_by(|i| i.key.value.cmp(&export.value))
                .is_err()
        {
            return Err(c.error(Code::Projection, Reason::InvalidProjection, export.span));
        }
    }
    Ok(facts)
}
fn no_borrow(id: TypeId, d: &Document, facts: &[Facts], c: &Context) -> Result<(), Error> {
    if facts[id.0].borrow {
        Err(c.error(Code::Type, Reason::InvalidBorrow, d.type_nodes[id.0].span))
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Default)]
struct Facts {
    borrow: bool,
    resource: bool,
    depth: usize,
}
struct Edges<'a> {
    small: [Option<usize>; 2],
    fields: std::slice::Iter<'a, Field>,
}
impl Iterator for Edges<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        self.small[0]
            .take()
            .or_else(|| self.small[1].take())
            .or_else(|| self.fields.next().map(|f| f.ty.0))
    }
}
struct Graph<'a, 'c> {
    d: &'a Document,
    defs: &'a [&'a Definition],
    resources: &'a [&'a Spanned<ResourceTypeKey>],
    c: &'c mut Context,
    root: m::Span,
}
impl<'a, 'c> Graph<'a, 'c> {
    fn new(
        d: &'a Document,
        defs: &'a [&'a Definition],
        resources: &'a [&'a Spanned<ResourceTypeKey>],
        c: &'c mut Context,
        root: m::Span,
    ) -> Self {
        Self {
            d,
            defs,
            resources,
            c,
            root,
        }
    }
    fn span(&self, id: usize) -> m::Span {
        if id < self.d.type_nodes.len() {
            self.d.type_nodes[id].span
        } else {
            self.defs[id - self.d.type_nodes.len()].key.span
        }
    }
    fn edges(&mut self, id: usize) -> Result<Edges<'a>, Error> {
        let mut small = [None, None];
        let mut fields: &[Field] = &[];
        if let Some(node) = self.d.type_nodes.get(id) {
            match &node.kind {
                TypeKind::List(child) | TypeKind::Option(child) => small[0] = Some(child.0),
                TypeKind::Result(a, b) => small = [Some(a.0), Some(b.0)],
                TypeKind::Named(key) => {
                    self.c
                        .lookup(self.defs.len(), value_bytes(&key.value), key.span)?;
                    let target = self
                        .defs
                        .binary_search_by(|def| def.key.value.cmp(&key.value))
                        .map_err(|_| self.c.error(Code::Name, Reason::Unresolved, key.span))?;
                    small[0] = Some(self.d.type_nodes.len() + target);
                }
                TypeKind::Own(key) | TypeKind::Borrow { resource: key, .. } => {
                    self.c
                        .lookup(self.resources.len(), resource_bytes(&key.value), key.span)?;
                    if self
                        .resources
                        .binary_search_by(|resource| resource.value.cmp(&key.value))
                        .is_err()
                    {
                        return Err(self.c.error(Code::Name, Reason::Unresolved, key.span));
                    }
                }
                _ => {}
            }
        } else {
            match &self.defs[id - self.d.type_nodes.len()].kind {
                DefinitionKind::Alias(ty) => small[0] = Some(ty.0),
                DefinitionKind::Record(v) | DefinitionKind::Variant(v) => fields = v,
            }
        }
        Ok(Edges {
            small,
            fields: fields.iter(),
        })
    }
    fn verify(mut self) -> Result<Vec<Facts>, Error> {
        let total = self
            .d
            .type_nodes
            .len()
            .checked_add(self.defs.len())
            .ok_or_else(|| {
                self.c.error(
                    Code::Resource,
                    Reason::Budget(m::ErrorKind::ArithmeticOverflow),
                    self.root,
                )
            })?;
        let mut states = self.c.vector(total, self.root)?;
        states.resize(total, 0u8);
        let mut facts = self.c.vector(total, self.root)?;
        facts.resize(total, Facts::default());
        let mut stack = Vec::new();
        for root in 0..total {
            if states[root] == 2 {
                continue;
            }
            self.c.push(&mut stack, (root, false), self.span(root))?;
            while let Some((id, finish)) = stack.pop() {
                let span = self.span(id);
                self.c.charge(1, span)?;
                if finish {
                    let mut value = Facts::default();
                    for child in self.edges(id)? {
                        let f = facts[child];
                        value.borrow |= f.borrow;
                        value.resource |= f.resource;
                        value.depth = value.depth.max(f.depth);
                    }
                    let structural = if let Some(node) = self.d.type_nodes.get(id) {
                        match node.kind {
                            TypeKind::Own(_) => {
                                value.resource = true;
                                true
                            }
                            TypeKind::Borrow { .. } => {
                                value.resource = true;
                                value.borrow = true;
                                true
                            }
                            TypeKind::Named(_) => false,
                            _ => true,
                        }
                    } else {
                        !matches!(
                            self.defs[id - self.d.type_nodes.len()].kind,
                            DefinitionKind::Alias(_)
                        )
                    };
                    if structural {
                        value.depth = value.depth.checked_add(1).ok_or_else(|| {
                            self.c.error(
                                Code::Resource,
                                Reason::Budget(m::ErrorKind::ArithmeticOverflow),
                                span,
                            )
                        })?;
                    }
                    self.c
                        .cap(value.depth, self.c.limits.depth, LimitKind::Depth, span)?;
                    self.c.data_depth = self.c.data_depth.max(value.depth);
                    facts[id] = value;
                    states[id] = 2;
                    continue;
                }
                if states[id] == 2 {
                    continue;
                }
                states[id] = 1;
                self.c.push(&mut stack, (id, true), span)?;
                for child in self.edges(id)? {
                    if child >= total {
                        return Err(self.c.error(Code::Type, Reason::Unresolved, span));
                    }
                    if states[child] == 1 {
                        return Err(Error {
                            related: Some(self.span(child)),
                            ..self.c.error(Code::Type, Reason::CyclicType, span)
                        });
                    }
                    if states[child] == 0 {
                        self.c.push(&mut stack, (child, false), span)?;
                    }
                }
            }
        }
        Ok(facts)
    }
}
