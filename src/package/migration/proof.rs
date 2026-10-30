//! A scalar resolved/type projection independent of arena numbering and positions.
use super::{Work, invalid};
use crate::package::Diagnostic;
use lang_hir::{program::TypedProgram, resolved as r};
use std::collections::BTreeMap;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Function {
    parameters: Vec<String>,
    result: r::Type,
    locals: BTreeMap<String, (r::BindingKind, r::Type)>,
    body: Vec<Statement>,
}
#[derive(Debug, Eq, PartialEq)]
struct Expression {
    ty: r::Type,
    kind: ExpressionKind,
}
#[derive(Debug, Eq, PartialEq)]
enum ExpressionKind {
    Integer(i64),
    Boolean(bool),
    Local(String),
    Add(Box<Expression>, Box<Expression>),
    LessThan(Box<Expression>, Box<Expression>),
    Call(String, Vec<Expression>),
}
#[derive(Debug, Eq, PartialEq)]
enum Statement {
    Let(String, Expression),
    Assign(String, Expression),
    Return(Expression),
    If(Expression, Vec<Statement>, Option<Vec<Statement>>),
    While(Expression, Vec<Statement>),
}
pub(super) fn project(
    program: &TypedProgram,
    names: &BTreeMap<String, String>,
    depth: usize,
    work: &mut Work,
) -> Result<BTreeMap<String, Function>, Diagnostic> {
    let source = program.source();
    if source.modules.len() != 1
        || !source.definitions.is_empty()
        || !source.types.is_empty()
        || !source.imports.is_empty()
    {
        return Err(invalid(
            "migration proof requires a closed single-file scalar program",
            None,
        ));
    }
    let mut out = BTreeMap::new();
    for (index, f) in source.functions.iter().enumerate() {
        work.take(1)?;
        if f.public
            || f.abi != r::FunctionAbi::Guest
            || f.effect != r::FunctionEffect::Synchronous
            || f.module != r::ModuleId(0)
            || !f.patterns.is_empty()
        {
            return Err(invalid(
                "migration proof refuses non-private scalar functions",
                Some(f.span),
            ));
        }
        let mut context = Context {
            program,
            index,
            names,
            depth,
            work,
        };
        let mut locals = BTreeMap::new();
        for (i, local) in f.locals.iter().enumerate() {
            let ty = scalar(program.functions()[index].locals()[i])?;
            let annotation = local
                .annotation
                .as_ref()
                .ok_or_else(|| invalid("missing historical local type", Some(local.name.span)))?;
            if annotation.value_type != ty
                || !matches!(
                    local.kind,
                    r::BindingKind::Parameter | r::BindingKind::Mutable
                )
            {
                return Err(invalid(
                    "migration local type or binding kind changed",
                    Some(local.name.span),
                ));
            }
            let name = context.name(&local.name.text)?;
            if locals.insert(name, (local.kind, ty)).is_some() {
                return Err(invalid(
                    "migration proof requires unique local names",
                    Some(local.name.span),
                ));
            }
        }
        let parameters = f
            .parameters
            .iter()
            .map(|id| context.local(*id))
            .collect::<Result<_, _>>()?;
        let body = context.block(f.body, 0)?;
        let name = context.name(&f.name.text)?;
        if out
            .insert(
                name,
                Function {
                    parameters,
                    result: scalar(f.result.value_type)?,
                    locals,
                    body,
                },
            )
            .is_some()
        {
            return Err(invalid(
                "migration function identity collision",
                Some(f.name.span),
            ));
        }
    }
    Ok(out)
}
fn scalar(ty: r::Type) -> Result<r::Type, Diagnostic> {
    if matches!(ty, r::Type::I64 | r::Type::Bool) {
        Ok(ty)
    } else {
        Err(invalid("non-scalar type in migration proof", None))
    }
}
struct Context<'a> {
    program: &'a TypedProgram,
    index: usize,
    names: &'a BTreeMap<String, String>,
    depth: usize,
    work: &'a mut Work,
}
impl Context<'_> {
    fn visit(&mut self, depth: usize) -> Result<(), Diagnostic> {
        self.work.take(1)?;
        if depth > self.depth {
            Err(super::capacity())
        } else {
            Ok(())
        }
    }
    fn name(&mut self, name: &str) -> Result<String, Diagnostic> {
        let text = self.names.get(name).map_or(name, String::as_str);
        self.work.take(text.len())?;
        Ok(text.to_string())
    }
    fn local(&mut self, id: r::LocalId) -> Result<String, Diagnostic> {
        self.name(
            &self.program.source().functions[self.index].locals[id.0]
                .name
                .text,
        )
    }
    fn expression(&mut self, id: r::ExpressionId, depth: usize) -> Result<Expression, Diagnostic> {
        self.visit(depth)?;
        let function = &self.program.source().functions[self.index];
        let expression = &function.expressions[id.0];
        let kind = match &expression.kind {
            r::ExpressionKind::Integer(value) => ExpressionKind::Integer(*value),
            r::ExpressionKind::Boolean(value) => ExpressionKind::Boolean(*value),
            r::ExpressionKind::Local(id) => ExpressionKind::Local(self.local(*id)?),
            r::ExpressionKind::Add(a, b) => ExpressionKind::Add(
                Box::new(self.expression(*a, depth + 1)?),
                Box::new(self.expression(*b, depth + 1)?),
            ),
            r::ExpressionKind::LessThan(a, b) => ExpressionKind::LessThan(
                Box::new(self.expression(*a, depth + 1)?),
                Box::new(self.expression(*b, depth + 1)?),
            ),
            r::ExpressionKind::Call {
                callee: r::Callable::Guest(id),
                arguments,
            } => {
                let name = self.name(&self.program.source().functions[id.0].name.text)?;
                let args = arguments
                    .iter()
                    .map(|id| self.expression(*id, depth + 1))
                    .collect::<Result<_, _>>()?;
                ExpressionKind::Call(name, args)
            }
            _ => {
                return Err(invalid(
                    "unsupported expression in migration proof",
                    Some(expression.span),
                ));
            }
        };
        Ok(Expression {
            ty: scalar(self.program.functions()[self.index].expressions()[id.0])?,
            kind,
        })
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<Vec<Statement>, Diagnostic> {
        self.visit(depth)?;
        let block = &self.program.source().functions[self.index].blocks[id.0];
        let mut out = Vec::new();
        for statement in &block.statements {
            self.visit(depth)?;
            out.push(match &statement.kind {
                r::StatementKind::LetMut { local, initializer } => Statement::Let(
                    self.local(*local)?,
                    self.expression(*initializer, depth + 1)?,
                ),
                r::StatementKind::Assign { local, value } => {
                    Statement::Assign(self.local(*local)?, self.expression(*value, depth + 1)?)
                }
                r::StatementKind::Return(value) => {
                    Statement::Return(self.expression(*value, depth + 1)?)
                }
                r::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                } => Statement::If(
                    self.expression(*condition, depth + 1)?,
                    self.block(*then_block, depth + 1)?,
                    else_block.map(|id| self.block(id, depth + 1)).transpose()?,
                ),
                r::StatementKind::While { condition, body } => Statement::While(
                    self.expression(*condition, depth + 1)?,
                    self.block(*body, depth + 1)?,
                ),
                _ => {
                    return Err(invalid(
                        "unsupported statement in migration proof",
                        Some(statement.span),
                    ));
                }
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend;
    use lang_diagnostics::SourceMap;
    use std::path::Path;

    fn shape(source: &str, names: BTreeMap<String, String>) -> BTreeMap<String, Function> {
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("proof.lang"),
            source.as_bytes(),
            None,
            Default::default(),
            None,
        );
        let lang_semantics::analysis::Program::Copy(program) = analysis.result.unwrap() else {
            panic!("scalar fixture")
        };
        project(
            &program,
            &names,
            128,
            &mut Work {
                remaining: 1_000_000,
                analysis: 0,
            },
        )
        .unwrap()
    }

    #[test]
    fn projection_detects_resolved_meaning_changes() {
        let source = "fn first(a:i64,b:i64)->i64{return a;}fn second(a:i64,b:i64)->i64{return b;}fn main()->i64{let mut x:i64=1;let mut y:i64=2;if x<y{x=first(x,y);}else{x=second(x,y);}while x<42{x=x+1;}return x;}";
        let original = shape(source, BTreeMap::new());
        for changed in [
            source.replace("x:i64=1", "x:i64=3"),
            source.replace("first(x,y)", "second(x,y)"),
            source.replace("first(x,y)", "first(y,x)"),
            source.replace("return a;", "return b;"),
            source.replace("if x<y", "if y<x"),
            source.replace("while x<42", "if x<42"),
            source.replace("x=x+1", "y=x+1"),
            source.replace("return x;", "return y;"),
            source.replace(
                "let mut x:i64=1;let mut y:i64=2;",
                "let mut y:i64=2;let mut x:i64=1;",
            ),
        ] {
            assert_ne!(original, shape(&changed, BTreeMap::new()), "{changed}");
        }
        assert_ne!(
            shape(
                "fn main()->i64{let mut x:i64=1;return 42;}",
                BTreeMap::new()
            ),
            shape(
                "fn main()->i64{let mut x:bool=true;return 42;}",
                BTreeMap::new()
            )
        );
    }

    #[test]
    fn projection_ignores_positions_and_arena_order_but_keeps_mapped_identity() {
        let before = "fn helper(value:i64)->i64{return value+1;}fn main()->i64{return helper(41);}";
        let after = "// reordered declarations\nfn main()->i64 { return renamed(41); }\nfn renamed(argument:i64)->i64 { return argument+1; }";
        assert_eq!(
            shape(
                before,
                BTreeMap::from([
                    ("helper".into(), "renamed".into()),
                    ("value".into(), "argument".into())
                ])
            ),
            shape(after, BTreeMap::new())
        );
    }
}
