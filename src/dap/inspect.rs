//! Read-only queries execute on the retained VM's original worker.
use lang_bytecode::unit::Program;
use lang_mir::unit as m;
use lang_semantics::program_lowering::DebugInfo;
use lang_vm::unit::{DebugSession, Value};
use std::collections::BTreeMap;

pub(super) enum Query {
    Locals,
    Evaluate(Vec<String>),
    Children(Vec<String>),
}
pub(super) struct Preview {
    pub name: String,
    pub display: String,
    pub type_name: String,
    pub children: usize,
}
#[derive(Default)]
struct Budget {
    work: usize,
    cells: usize,
    text: usize,
}
impl Budget {
    fn charge(&mut self, work: usize, cells: usize, text: usize) -> Result<(), String> {
        self.work = self.work.saturating_add(work);
        self.cells = self.cells.saturating_add(cells);
        self.text = self.text.saturating_add(text);
        if self.work > 10_000_000 || self.cells > 16_384 || self.text > 1024 * 1024 {
            return Err("inspection resource limit exceeded".into());
        }
        Ok(())
    }
}
fn locals(
    session: &DebugSession<'_>,
    debug: &DebugInfo,
    frame: usize,
    budget: &mut Budget,
) -> Result<BTreeMap<String, m::Register>, String> {
    let point = session.frame_point(frame).ok_or("frame is not paused")?;
    let (locals, location) = debug
        .location(point.function, point.block, point.instruction)
        .ok_or("source-local locations unavailable")?;
    budget.charge(location.bindings.len(), 0, 0)?;
    let mut visible = BTreeMap::new();
    for binding in &location.bindings {
        let name = &locals[binding.local].name;
        if name.len() > 4096 {
            return Err("inspection name limit exceeded".into());
        }
        budget.charge(1, 0, name.len())?;
        // Checked local IDs follow binder order. The most recent live binding
        // shadows an earlier binding; unavailable environments never guess.
        visible.insert(name.clone(), binding.register);
    }
    if visible.len() > 1024 {
        return Err("variable response limit exceeded".into());
    }
    Ok(visible)
}
fn read(
    session: &DebugSession<'_>,
    frame: usize,
    register: m::Register,
    budget: &mut Budget,
) -> Result<Value, String> {
    let value = session
        .inspect_register(frame, register, 1024)
        .map_err(|_| "value inspection failed")?
        .ok_or("value unavailable or preview cell limit exceeded")?;
    budget.charge(value.cells().len(), value.cells().len(), 0)?;
    Ok(value)
}
fn children(
    program: &Program,
    value: &Value,
    budget: &mut Budget,
) -> Result<Vec<(String, m::Type, usize)>, String> {
    let unit = program.verified().unit();
    let result = match value.value_type() {
        m::Type::I64 | m::Type::Bool => vec![],
        m::Type::Record(id) => {
            let m::DefinitionKind::Record(fields) = &unit.definitions[id.0 as usize].kind else {
                return Err("invalid record metadata".into());
            };
            budget.charge(fields.len(), 0, 0)?;
            if fields.len() > 1024 {
                return Err("field response limit exceeded".into());
            }
            let offsets = program.verified().facts().layouts()[id.0 as usize].fields();
            let mut result = Vec::new();
            for (field, offset) in fields.iter().zip(offsets) {
                if field.name.len() > 4096 {
                    return Err("field name limit exceeded".into());
                }
                budget.charge(1, 0, field.name.len())?;
                result.push((field.name.clone(), field.value_type, *offset));
            }
            result
        }
        m::Type::Enum(id) => {
            let m::DefinitionKind::Enum(cases) = &unit.definitions[id.0 as usize].kind else {
                return Err("invalid enum metadata".into());
            };
            let tag = usize::try_from(value.cells()[0]).map_err(|_| "invalid enum tag")?;
            let case = cases.get(tag).ok_or("invalid enum tag")?;
            budget.charge(case.payload.len(), 0, 0)?;
            if case.payload.len() > 1024 {
                return Err("payload response limit exceeded".into());
            }
            let offsets = &program.verified().facts().layouts()[id.0 as usize].cases()[tag];
            case.payload
                .iter()
                .zip(offsets)
                .enumerate()
                .map(|(i, (t, offset))| (i.to_string(), *t, *offset))
                .collect()
        }
        _ => return Err("unsupported inspection type".into()),
    };
    Ok(result)
}
fn field(
    program: &Program,
    value: &Value,
    name: &str,
    budget: &mut Budget,
) -> Result<Value, String> {
    let (_, value_type, offset) = children(program, value, budget)?
        .into_iter()
        .find(|(n, _, _)| n == name)
        .ok_or("field is not available")?;
    let length = program
        .verified()
        .facts()
        .type_cells(value_type)
        .ok_or("invalid field layout")?;
    budget.charge(length, length, 0)?;
    let cells = value
        .cells()
        .get(offset..offset.checked_add(length).ok_or("invalid field range")?)
        .ok_or("invalid field range")?
        .to_vec();
    Value::from_cells(program, value_type, cells).map_err(|_| "invalid field value".into())
}
fn preview(
    program: &Program,
    name: String,
    value: &Value,
    budget: &mut Budget,
) -> Result<Preview, String> {
    let (type_name, display) = match value.value_type() {
        m::Type::I64 => ("i64".into(), value.as_i64().unwrap().to_string()),
        m::Type::Bool => ("bool".into(), value.as_bool().unwrap().to_string()),
        m::Type::Record(id) | m::Type::Enum(id) => {
            let definition = &program.verified().unit().definitions[id.0 as usize];
            if definition.name.len() > 4096 {
                return Err("type name limit exceeded".into());
            }
            let display = match &definition.kind {
                m::DefinitionKind::Record(_) => format!("{} {{…}}", definition.name),
                m::DefinitionKind::Enum(cases) => {
                    let case = &cases[value.cells()[0] as usize];
                    if case.name.len() > 4096 {
                        return Err("case name limit exceeded".into());
                    }
                    format!("{}::{}(…)", definition.name, case.name)
                }
            };
            (definition.name.clone(), display)
        }
        _ => return Err("unsupported inspection type".into()),
    };
    let children = children(program, value, budget)?.len();
    budget.charge(1, 0, name.len() + display.len() + type_name.len())?;
    Ok(Preview {
        name,
        display,
        type_name,
        children,
    })
}
pub(super) fn query(
    program: &Program,
    debug: &DebugInfo,
    session: &DebugSession<'_>,
    frame: usize,
    query: Query,
) -> Result<Vec<Preview>, String> {
    let mut budget = Budget::default();
    let visible = locals(session, debug, frame, &mut budget)?;
    if matches!(query, Query::Locals) {
        let mut result = Vec::new();
        for (name, register) in visible {
            let value = read(session, frame, register, &mut budget)?;
            result.push(preview(program, name, &value, &mut budget)?);
        }
        return Ok(result);
    }
    let (path, expand) = match query {
        Query::Evaluate(path) => (path, false),
        Query::Children(path) => (path, true),
        Query::Locals => unreachable!(),
    };
    if path.is_empty() || path.len() > 32 || path.iter().map(String::len).sum::<usize>() > 4096 {
        return Err("evaluation path limit exceeded".into());
    }
    let mut value = if let Some(register) = visible.get(&path[0]) {
        read(session, frame, *register, &mut budget)?
    } else if path.len() == 1 {
        if let Ok(n) = path[0].parse::<i64>() {
            Value::i64(n)
        } else if path[0] == "true" || path[0] == "false" {
            Value::boolean(path[0] == "true")
        } else {
            return Err(
                "expression is not an available identifier, field or scalar literal".into(),
            );
        }
    } else {
        return Err("binding is not available".into());
    };
    for name in path.iter().skip(1) {
        value = field(program, &value, name, &mut budget)?;
    }
    if !expand {
        return Ok(vec![preview(
            program,
            path.last().unwrap().clone(),
            &value,
            &mut budget,
        )?]);
    }
    let mut result = Vec::new();
    for (name, _, _) in children(program, &value, &mut budget)? {
        let child = field(program, &value, &name, &mut budget)?;
        result.push(preview(program, name, &child, &mut budget)?);
    }
    Ok(result)
}
