//! Preserve the complete resolved program, including bindings absent from navigation.
use super::*;
use lang_hir::{resolved as r, source::Name};

pub(super) fn verify(
    view: &View,
    next: &Snapshot,
    plan: &Plan,
    work: &mut usize,
) -> Result<(), Diagnostic> {
    let old = view
        .snapshot
        .analysis()
        .result
        .as_ref()
        .map_err(|_| invalid("rename requires valid source"))?;
    let new = next
        .analysis()
        .result
        .as_ref()
        .map_err(|_| invalid("rename would invalidate the program"))?;
    let mut expected = old.source().clone();
    let mut destinations = BTreeMap::new();
    for (id, file) in next.analysis().sources.iter() {
        spend(work)?;
        if destinations.insert(file.name(), id).is_some() {
            return Err(invalid("ambiguous rename source identity"));
        }
    }
    let mut source_ids = BTreeMap::new();
    for (id, file) in view.snapshot.analysis().sources.iter() {
        spend(work)?;
        let next_id = destinations
            .get(file.name())
            .ok_or_else(|| invalid("rename changed the source set"))?;
        source_ids.insert(id, *next_id);
    }
    if source_ids.len() != destinations.len() {
        return Err(invalid("rename changed the source set"));
    }
    let mut mapper = Mapper {
        old: &view.snapshot.analysis().sources,
        new: &next.analysis().sources,
        plan,
        work,
        source_ids,
    };
    mapper.program(&mut expected)?;
    if expected != *new.source() || old.functions() != new.functions() {
        return Err(invalid(
            "rename would change binding, type, ABI, effect or control-flow facts",
        ));
    }
    Ok(())
}
struct Mapper<'a> {
    old: &'a SourceMap,
    new: &'a SourceMap,
    plan: &'a Plan,
    work: &'a mut usize,
    source_ids: BTreeMap<lang_diagnostics::SourceId, lang_diagnostics::SourceId>,
}
impl Mapper<'_> {
    fn span(&mut self, span: &mut Span) -> Result<(), Diagnostic> {
        spend(self.work)?;
        let file = self
            .old
            .file(span.source())
            .map_err(|_| invalid("missing old rename source"))?;
        let id = *self
            .source_ids
            .get(&span.source())
            .ok_or_else(|| invalid("rename changed the source set"))?;
        let path = Path::new(file.name());
        let range = span.range();
        *span = self
            .new
            .span(
                id,
                self.plan.boundary(path, range.start)?..self.plan.boundary(path, range.end)?,
            )
            .map_err(|_| invalid("rename span correspondence failed"))?;
        Ok(())
    }
    fn name(&mut self, name: &mut Name) -> Result<(), Diagnostic> {
        spend_bytes(self.work, name.text.len())?;
        let file = self
            .old
            .file(name.span.source())
            .map_err(|_| invalid("missing rename name source"))?;
        let range = name.span.range();
        if self
            .plan
            .files
            .get(Path::new(file.name()))
            .is_some_and(|edits| {
                edits
                    .binary_search_by_key(&(range.start, range.end), |e| (e.start, e.end))
                    .is_ok()
            })
        {
            spend_bytes(self.work, self.plan.replacement.len())?;
            name.text.clone_from(&self.plan.replacement);
        }
        self.span(&mut name.span)
    }
    fn ty(&mut self, value: &mut r::TypeUse) -> Result<(), Diagnostic> {
        self.span(&mut value.span)
    }
    fn qualified(&mut self, value: &mut String, name: &str) -> Result<(), Diagnostic> {
        spend_bytes(self.work, value.len())?;
        let start = value.rfind("::").map_or(0, |i| i + 2);
        spend_bytes(self.work, name.len())?;
        value.truncate(start);
        value.push_str(name);
        Ok(())
    }
    fn program(&mut self, p: &mut r::Program) -> Result<(), Diagnostic> {
        for module in &mut p.modules {
            self.span(&mut module.span)?;
        }
        for ty in &mut p.types {
            self.span(&mut ty.span)?;
        }
        for d in &mut p.definitions {
            self.name(&mut d.name)?;
            self.qualified(&mut d.qualified_name, &d.name.text)?;
            self.span(&mut d.span)?;
            match &mut d.kind {
                r::DefinitionKind::Record(fields) => {
                    for f in fields {
                        self.name(&mut f.name)?;
                        self.ty(&mut f.value_type)?;
                    }
                }
                r::DefinitionKind::Enum(cases) => {
                    for c in cases {
                        self.name(&mut c.name)?;
                        for ty in &mut c.payload {
                            self.ty(ty)?;
                        }
                    }
                }
            }
        }
        for import in &mut p.imports {
            self.name(&mut import.name)?;
            self.qualified(&mut import.qualified_name, &import.name.text)?;
            self.ty(&mut import.result)?;
            self.span(&mut import.span)?;
            for parameter in &mut import.parameters {
                self.name(&mut parameter.name)?;
                self.ty(&mut parameter.ty)?;
            }
        }
        for f in &mut p.functions {
            self.name(&mut f.name)?;
            self.qualified(&mut f.qualified_name, &f.name.text)?;
            self.ty(&mut f.result)?;
            self.span(&mut f.span)?;
            self.span(&mut f.signature_span)?;
            if let r::FunctionEffect::Asynchronous { span } = &mut f.effect {
                self.span(span)?;
            }
            for local in &mut f.locals {
                self.name(&mut local.name)?;
                if let Some(ty) = &mut local.annotation {
                    self.ty(ty)?;
                }
            }
            for e in &mut f.expressions {
                self.span(&mut e.span)?;
                match &mut e.kind {
                    r::ExpressionKind::CoreCase { case_span, .. } => self.span(case_span)?,
                    r::ExpressionKind::CoreCall {
                        type_arguments,
                        callee_span,
                        ..
                    } => {
                        self.span(callee_span)?;
                        for ty in type_arguments {
                            self.ty(ty)?;
                        }
                    }
                    r::ExpressionKind::Record { fields, .. } => {
                        for field in fields {
                            self.name(&mut field.name)?;
                        }
                    }
                    r::ExpressionKind::Field { name, .. } => self.name(name)?,
                    r::ExpressionKind::CoreUnit
                    | r::ExpressionKind::Integer(_)
                    | r::ExpressionKind::Boolean(_)
                    | r::ExpressionKind::Local(_)
                    | r::ExpressionKind::Borrow { .. }
                    | r::ExpressionKind::Dereference(_)
                    | r::ExpressionKind::Spawn(_)
                    | r::ExpressionKind::Await(_)
                    | r::ExpressionKind::Add(..)
                    | r::ExpressionKind::LessThan(..)
                    | r::ExpressionKind::Call { .. }
                    | r::ExpressionKind::Case { .. } => {}
                }
            }
            for pattern in &mut f.patterns {
                self.span(&mut pattern.span)?;
                match &mut pattern.kind {
                    r::PatternKind::CoreCase { case_span, .. } => self.span(case_span)?,
                    r::PatternKind::Record { fields, .. } => {
                        for field in fields {
                            self.name(&mut field.name)?;
                        }
                    }
                    r::PatternKind::CoreUnit
                    | r::PatternKind::Wildcard
                    | r::PatternKind::Binding(_)
                    | r::PatternKind::Integer(_)
                    | r::PatternKind::Boolean(_)
                    | r::PatternKind::Case { .. } => {}
                }
            }
            for block in &mut f.blocks {
                self.span(&mut block.span)?;
                self.span(&mut block.closing_brace)?;
                for statement in &mut block.statements {
                    self.span(&mut statement.span)?;
                    match &mut statement.kind {
                        r::StatementKind::Match { arms, .. } => {
                            for arm in arms {
                                self.span(&mut arm.span)?;
                            }
                        }
                        r::StatementKind::TaskScope { .. }
                        | r::StatementKind::Yield
                        | r::StatementKind::Cancel(_)
                        | r::StatementKind::CancelCheck
                        | r::StatementKind::Unsafe { .. }
                        | r::StatementKind::LetMut { .. }
                        | r::StatementKind::Assign { .. }
                        | r::StatementKind::PlaceAssign { .. }
                        | r::StatementKind::Arena { .. }
                        | r::StatementKind::If { .. }
                        | r::StatementKind::While { .. }
                        | r::StatementKind::Return(_) => {}
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::editor::{
        Limits,
        workspace::{Config, OpenDocument},
    };
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn valid_types_do_not_prove_binding_correspondence() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Files(PathBuf);
        impl Drop for Files {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let dir = std::env::temp_dir().join(format!(
            "lang rename proof {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let files = Files(fs::canonicalize(dir).unwrap());
        fs::create_dir(files.0.join("src")).unwrap();
        fs::write(files.0.join("Lang.json"),r#"{"schema":1,"package":{"name":"proof","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#).unwrap();
        let old = "fn main()->i64{let mut first:i64=1;let mut second:i64=2;return first;}";
        fs::write(files.0.join("src/main.lang"), old).unwrap();
        let config = Config {
            root: files.0.clone(),
            manifest: files.0.join("Lang.json"),
            selected: None,
            target: lang_codegen_clif::Triple::host().to_string(),
        };
        let view = config.capture(1, &[], Limits::default()).unwrap();
        let spans = old
            .match_indices("first")
            .map(|(start, _)| ByteSpan {
                start,
                end: start + 5,
            })
            .collect::<Vec<_>>();
        let mut end = 0;
        let mut mapped = 0;
        let mut offsets = Vec::new();
        for span in &spans {
            mapped += span.start - end + 6;
            end = span.end;
            offsets.push((end, mapped));
        }
        let plan = Plan {
            replacement: "amount".into(),
            files: BTreeMap::from([("src/main.lang".into(), spans)]),
            offsets: BTreeMap::from([("src/main.lang".into(), offsets)]),
        };
        for (source, accepted) in [
            (old.replace("first", "amount"), true),
            (
                old.replace("first", "amount")
                    .replace("return amount;", "return second;"),
                false,
            ),
        ] {
            let document = OpenDocument {
                path: files.0.join("src/main.lang"),
                version: 1,
                bytes: Arc::from(source.as_bytes()),
            };
            let next = config.capture(1, &[document], Limits::default()).unwrap();
            assert!(next.snapshot.analysis().result.is_ok());
            assert_eq!(
                verify(&view, &next.snapshot, &plan, &mut 10_000_000).is_ok(),
                accepted,
                "{source}"
            );
        }
    }
}
