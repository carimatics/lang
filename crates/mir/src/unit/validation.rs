use std::cmp::Ordering;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub sources: usize,
    pub source_bytes: usize,
    pub total_source_bytes: usize,
    pub definitions: usize,
    pub functions: usize,
    pub imports: usize,
    pub fields: usize,
    pub registers: usize,
    pub blocks: usize,
    pub instructions: usize,
    pub depth: usize,
    pub value_cells: usize,
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sources: 256,
            source_bytes: 4 * 1024 * 1024,
            total_source_bytes: 16 * 1024 * 1024,
            definitions: 4096,
            functions: 4096,
            imports: 4096,
            fields: 256,
            registers: 1_000_000,
            blocks: 1_000_000,
            instructions: 1_000_000,
            depth: 64,
            value_cells: 4096,
            work: 10_000_000,
            allocations: 128 * 1024 * 1024,
            elements: 8_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Sources,
    SourceBytes,
    TotalSourceBytes,
    Definitions,
    Functions,
    Imports,
    Fields,
    Registers,
    Blocks,
    Instructions,
    Depth,
    ValueCells,
    Work,
    Allocations,
    Elements,
    FormatCapacity,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Limit {
        kind: LimitKind,
        limit: usize,
        requested: usize,
    },
    AllocationFailed,
    ArithmeticOverflow,
    InvalidName,
    DuplicateName,
    InvalidPath,
    InvalidSourceText,
    InvalidSpan,
    InvalidDefinition(u32),
    InvalidFunction(u32),
    InvalidImport(u32),
    InvalidBlock(u32),
    InvalidRegister(u32),
    ExpectedRecord(Type),
    ExpectedEnum(Type),
    EmptyEnum,
    RecursiveType,
    InvalidCase(u32),
    InvalidField(u32),
    TypeMismatch {
        expected: Type,
        actual: Type,
    },
    Arity {
        expected: usize,
        actual: usize,
    },
    InvalidHostSignature,
    InvalidEntry,
    EmptyFunction,
    EntryParameters,
    EntryPredecessor,
    DuplicateRegister(u32),
    UndefinedRegister(u32),
    UnreachableBlock(u32),
    DoesNotDominate(u32),
    OwnedAggregate,
    OwnedCall,
    OwnedControlFlow,
    OwnedNotConsumed(u32),
    OwnedUsedMoreThanOnce(u32),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub function: Option<u32>,
    pub block: Option<u32>,
    pub instruction: Option<u32>,
}
impl Error {
    pub fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            function: None,
            block: None,
            instruction: None,
        }
    }
    fn location(mut self, function: u32, block: Option<u32>, instruction: Option<u32>) -> Self {
        self.function = Some(function);
        self.block = block;
        self.instruction = instruction;
        self
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid compilation unit: {:?}", self.kind)
    }
}
impl StdError for Error {}

/// Shared by decoding and verification. Counts cumulative requested buffers,
/// including temporary buffers; releasing one does not replenish this budget.
pub struct Budget {
    limits: Limits,
    work: usize,
    allocations: usize,
    elements: usize,
}
impl Budget {
    pub const fn new(limits: Limits) -> Self {
        Self {
            limits,
            work: 0,
            allocations: 0,
            elements: 0,
        }
    }
    pub const fn limits(&self) -> Limits {
        self.limits
    }
    pub const fn work_used(&self) -> usize {
        self.work
    }
    pub const fn allocation_bytes(&self) -> usize {
        self.allocations
    }
    pub const fn elements_used(&self) -> usize {
        self.elements
    }
    pub fn charge(&mut self, steps: usize) -> Result<(), Error> {
        Self::add(&mut self.work, steps, self.limits.work, LimitKind::Work)
    }
    pub fn bytes(&mut self, bytes: usize) -> Result<(), Error> {
        Self::add(
            &mut self.allocations,
            bytes,
            self.limits.allocations,
            LimitKind::Allocations,
        )
    }
    pub fn allocate<T>(&mut self, count: usize) -> Result<(), Error> {
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        let elements = self
            .elements
            .checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        let total = self
            .allocations
            .checked_add(bytes)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        cap(elements, self.limits.elements, LimitKind::Elements)?;
        cap(total, self.limits.allocations, LimitKind::Allocations)?;
        self.elements = elements;
        self.allocations = total;
        Ok(())
    }
    fn add(
        current: &mut usize,
        count: usize,
        maximum: usize,
        kind: LimitKind,
    ) -> Result<(), Error> {
        let total = current
            .checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        cap(total, maximum, kind)?;
        *current = total;
        Ok(())
    }
    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, Error> {
        self.allocate::<T>(count)?;
        let mut vector = Vec::new();
        vector
            .try_reserve_exact(count)
            .map_err(|_| Error::new(ErrorKind::AllocationFailed))?;
        Ok(vector)
    }
    fn filled<T: Clone>(&mut self, count: usize, value: T) -> Result<Vec<T>, Error> {
        self.charge(count)?;
        let mut vector = self.vector(count)?;
        vector.resize(count, value);
        Ok(vector)
    }
}
fn cap(count: usize, maximum: usize, kind: LimitKind) -> Result<(), Error> {
    if count > maximum {
        Err(Error::new(ErrorKind::Limit {
            kind,
            limit: maximum,
            requested: count,
        }))
    } else {
        Ok(())
    }
}
fn count(count: usize, maximum: usize, kind: LimitKind) -> Result<(), Error> {
    cap(count, maximum, kind)?;
    cap(count, u32::MAX as usize, LimitKind::FormatCapacity)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFacts {
    line_starts: Vec<u32>,
}
impl SourceFacts {
    pub fn position(&self, source: &Source, offset: u32) -> Option<(u32, u32)> {
        let offset = offset as usize;
        if offset > source.text.len() || !source.text.is_char_boundary(offset) {
            return None;
        }
        let line = self
            .line_starts
            .partition_point(|start| *start as usize <= offset)
            .checked_sub(1)?;
        let text = source.text.get(self.line_starts[line] as usize..offset)?;
        Some((
            u32::try_from(line + 1).ok()?,
            u32::try_from(text.chars().count() + 1).ok()?,
        ))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    cells: usize,
    depth: usize,
    fields: Vec<usize>,
    cases: Vec<Vec<usize>>,
}
impl Layout {
    pub const fn cells(&self) -> usize {
        self.cells
    }
    pub const fn depth(&self) -> usize {
        self.depth
    }
    pub fn fields(&self) -> &[usize] {
        &self.fields
    }
    pub fn cases(&self) -> &[Vec<usize>] {
        &self.cases
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionFacts {
    cells: usize,
    register_cells: Vec<usize>,
}
impl FunctionFacts {
    pub const fn cells(&self) -> usize {
        self.cells
    }
    pub fn register_cells(&self) -> &[usize] {
        &self.register_cells
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitFacts {
    sources: Vec<SourceFacts>,
    layouts: Vec<Layout>,
    functions: Vec<FunctionFacts>,
}
impl UnitFacts {
    pub fn sources(&self) -> &[SourceFacts] {
        &self.sources
    }
    pub fn layouts(&self) -> &[Layout] {
        &self.layouts
    }
    pub fn functions(&self) -> &[FunctionFacts] {
        &self.functions
    }
    pub fn type_cells(&self, ty: Type) -> Option<usize> {
        match ty {
            Type::I64 | Type::Bool | Type::Own(_) => Some(1),
            Type::Record(id) => self
                .layouts
                .get(id.0 as usize)
                .filter(|l| l.cases.is_empty())
                .map(Layout::cells),
            Type::Enum(id) => self
                .layouts
                .get(id.0 as usize)
                .filter(|l| !l.cases.is_empty())
                .map(Layout::cells),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedUnit {
    pub(super) unit: Unit,
    pub(super) facts: UnitFacts,
}
impl VerifiedUnit {
    pub fn unit(&self) -> &Unit {
        &self.unit
    }
    pub fn facts(&self) -> &UnitFacts {
        &self.facts
    }
    pub fn into_unit(self) -> Unit {
        self.unit
    }
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn name(name: &str, qualified: bool, budget: &mut Budget) -> Result<(), Error> {
    budget.charge(name.len().saturating_add(1))?;
    if if qualified {
        name.split("::").all(identifier)
    } else {
        identifier(name)
    } {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidName))
    }
}
fn compare(left: &str, right: &str, budget: &mut Budget) -> Result<Ordering, Error> {
    for (a, b) in left.bytes().zip(right.bytes()) {
        budget.charge(1)?;
        if a != b {
            return Ok(a.cmp(&b));
        }
    }
    budget.charge(1)?;
    Ok(left.len().cmp(&right.len()))
}
fn sift(names: &mut [&str], mut root: usize, end: usize, budget: &mut Budget) -> Result<(), Error> {
    while let Some(left) = root
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .filter(|n| *n < end)
    {
        let mut child = left;
        if left + 1 < end && compare(names[left], names[left + 1], budget)? == Ordering::Less {
            child += 1;
        }
        if compare(names[root], names[child], budget)? != Ordering::Less {
            break;
        }
        names.swap(root, child);
        root = child;
    }
    Ok(())
}
fn unique(names: &mut [&str], budget: &mut Budget) -> Result<(), Error> {
    for root in (0..names.len() / 2).rev() {
        sift(names, root, names.len(), budget)?;
    }
    for end in (1..names.len()).rev() {
        names.swap(0, end);
        sift(names, 0, end, budget)?;
    }
    for pair in names.windows(2) {
        if compare(pair[0], pair[1], budget)? == Ordering::Equal {
            return Err(Error::new(ErrorKind::DuplicateName));
        }
    }
    Ok(())
}
fn valid_type(unit: &Unit, ty: Type, budget: &mut Budget) -> Result<(), Error> {
    budget.charge(1)?;
    match ty {
        Type::Record(id) => {
            let definition = unit
                .definitions
                .get(id.0 as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidDefinition(id.0)))?;
            if !matches!(definition.kind, DefinitionKind::Record(_)) {
                return Err(Error::new(ErrorKind::ExpectedRecord(Type::Enum(id))));
            }
        }
        Type::Enum(id) => {
            let definition = unit
                .definitions
                .get(id.0 as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidDefinition(id.0)))?;
            if !matches!(definition.kind, DefinitionKind::Enum(_)) {
                return Err(Error::new(ErrorKind::ExpectedEnum(Type::Record(id))));
            }
        }
        _ => {}
    }
    Ok(())
}
fn signature(unit: &Unit, signature: &Signature, budget: &mut Budget) -> Result<(), Error> {
    count(
        signature.parameters.len(),
        budget.limits.fields,
        LimitKind::Fields,
    )?;
    for ty in &signature.parameters {
        valid_type(unit, *ty, budget)?;
    }
    valid_type(unit, signature.result, budget)
}
fn span(unit: &Unit, span: Option<Span>, budget: &mut Budget) -> Result<(), Error> {
    budget.charge(1)?;
    if let Some(span) = span {
        let source = unit
            .sources
            .get(span.source.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidSpan))?;
        if span.start > span.end
            || span.end as usize > source.text.len()
            || !source.text.is_char_boundary(span.start as usize)
            || !source.text.is_char_boundary(span.end as usize)
        {
            return Err(Error::new(ErrorKind::InvalidSpan));
        }
    }
    Ok(())
}

pub(super) fn validate(unit: &Unit, budget: &mut Budget) -> Result<UnitFacts, Error> {
    metadata(unit, budget)?;
    let sources = source_facts(unit, budget)?;
    let layouts = layouts(unit, budget)?;
    let mut functions = budget.vector(unit.functions.len())?;
    for (index, function) in unit.functions.iter().enumerate() {
        functions.push(
            function_facts(unit, function, &layouts, budget).map_err(|error| {
                let block = error.block;
                let instruction = error.instruction;
                error.location(index as u32, block, instruction)
            })?,
        );
    }
    Ok(UnitFacts {
        sources,
        layouts,
        functions,
    })
}

fn metadata(unit: &Unit, budget: &mut Budget) -> Result<(), Error> {
    let limits = budget.limits;
    count(unit.sources.len(), limits.sources, LimitKind::Sources)?;
    count(
        unit.definitions.len(),
        limits.definitions,
        LimitKind::Definitions,
    )?;
    count(unit.imports.len(), limits.imports, LimitKind::Imports)?;
    count(unit.functions.len(), limits.functions, LimitKind::Functions)?;
    let mut paths = budget.vector(unit.sources.len())?;
    let mut bytes = 0_usize;
    for source in &unit.sources {
        budget.charge(source.path.len().saturating_add(1))?;
        if source.path.contains(['\\', '\0'])
            || (source.path.as_bytes().get(1) == Some(&b':')
                && source.path.as_bytes()[0].is_ascii_alphabetic())
            || source.path.split('/').any(|p| matches!(p, "" | "." | ".."))
        {
            return Err(Error::new(ErrorKind::InvalidPath));
        }
        count(
            source.text.len(),
            limits.source_bytes,
            LimitKind::SourceBytes,
        )?;
        bytes = bytes
            .checked_add(source.text.len())
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        cap(
            bytes,
            limits.total_source_bytes,
            LimitKind::TotalSourceBytes,
        )?;
        if source.text.starts_with('\u{feff}') || source.text.contains('\0') {
            return Err(Error::new(ErrorKind::InvalidSourceText));
        }
        paths.push(source.path.as_str());
    }
    unique(&mut paths, budget)?;
    let names_count = unit
        .definitions
        .len()
        .checked_add(unit.imports.len())
        .and_then(|n| n.checked_add(unit.functions.len()))
        .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
    let mut names = budget.vector(names_count)?;
    for definition in &unit.definitions {
        name(&definition.name, true, budget)?;
        names.push(definition.name.as_str());
        let mut members = match &definition.kind {
            DefinitionKind::Record(fields) => {
                count(fields.len(), limits.fields, LimitKind::Fields)?;
                budget.vector(fields.len())?
            }
            DefinitionKind::Enum(cases) => {
                count(cases.len(), limits.fields, LimitKind::Fields)?;
                if cases.is_empty() {
                    return Err(Error::new(ErrorKind::EmptyEnum));
                }
                budget.vector(cases.len())?
            }
        };
        match &definition.kind {
            DefinitionKind::Record(fields) => {
                for field in fields {
                    name(&field.name, false, budget)?;
                    members.push(field.name.as_str());
                    aggregate_type(unit, field.value_type, budget)?;
                }
            }
            DefinitionKind::Enum(cases) => {
                for case in cases {
                    name(&case.name, false, budget)?;
                    members.push(case.name.as_str());
                    count(case.payload.len(), limits.fields, LimitKind::Fields)?;
                    for ty in &case.payload {
                        aggregate_type(unit, *ty, budget)?;
                    }
                }
            }
        }
        unique(&mut members, budget)?;
    }
    for import in &unit.imports {
        name(&import.name, true, budget)?;
        names.push(import.name.as_str());
        signature(unit, &import.signature, budget)?;
        if import.signature.parameters != [Type::I64] || import.signature.result != Type::I64 {
            return Err(Error::new(ErrorKind::InvalidHostSignature));
        }
    }
    let mut registers = 0_usize;
    let mut blocks = 0_usize;
    let mut instructions = 0_usize;
    for (index, function) in unit.functions.iter().enumerate() {
        let result = (|| {
            name(&function.name, true, budget)?;
            names.push(function.name.as_str());
            signature(unit, &function.signature, budget)?;
            span(unit, function.span, budget)?;
            registers = registers
                .checked_add(function.registers.len())
                .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
            count(registers, limits.registers, LimitKind::Registers)?;
            blocks = blocks
                .checked_add(function.blocks.len())
                .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
            count(blocks, limits.blocks, LimitKind::Blocks)?;
            for ty in &function.registers {
                valid_type(unit, *ty, budget)?;
            }
            for block in &function.blocks {
                budget.charge(1)?;
                instructions = instructions
                    .checked_add(block.instructions.len())
                    .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
                count(instructions, limits.instructions, LimitKind::Instructions)?;
                count(
                    block.parameters.len(),
                    limits.registers,
                    LimitKind::Registers,
                )?;
            }
            Ok(())
        })();
        result.map_err(|error: Error| error.location(index as u32, None, None))?;
    }
    unique(&mut names, budget)?;
    if let Some(entry) = unit.entry {
        let function = unit
            .functions
            .get(entry.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidEntry))?;
        if function.name != "main"
            || !function.signature.parameters.is_empty()
            || function.signature.result != Type::I64
        {
            return Err(Error::new(ErrorKind::InvalidEntry));
        }
    }
    Ok(())
}
fn aggregate_type(unit: &Unit, ty: Type, budget: &mut Budget) -> Result<(), Error> {
    valid_type(unit, ty, budget)?;
    if !ty.is_copy() {
        return Err(Error::new(ErrorKind::OwnedAggregate));
    }
    Ok(())
}
fn source_facts(unit: &Unit, budget: &mut Budget) -> Result<Vec<SourceFacts>, Error> {
    let mut result = budget.vector(unit.sources.len())?;
    for source in &unit.sources {
        let count = source
            .text
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        budget.charge(count)?;
        let mut line_starts = budget.vector(count)?;
        line_starts.push(0);
        for (offset, byte) in source.text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(
                    u32::try_from(offset + 1)
                        .map_err(|_| Error::new(ErrorKind::ArithmeticOverflow))?,
                );
            }
        }
        result.push(SourceFacts { line_starts });
    }
    Ok(result)
}

enum Types<'a> {
    Record(std::slice::Iter<'a, Field>),
    Enum {
        cases: std::slice::Iter<'a, Case>,
        current: std::slice::Iter<'a, Type>,
    },
}
impl<'a> Iterator for Types<'a> {
    type Item = Type;
    fn next(&mut self) -> Option<Type> {
        match self {
            Self::Record(fields) => fields.next().map(|f| f.value_type),
            Self::Enum { cases, current } => loop {
                if let Some(ty) = current.next() {
                    return Some(*ty);
                }
                *current = cases.next()?.payload.iter();
            },
        }
    }
}
fn types(definition: &Definition) -> Types<'_> {
    match &definition.kind {
        DefinitionKind::Record(fields) => Types::Record(fields.iter()),
        DefinitionKind::Enum(cases) => Types::Enum {
            cases: cases.iter(),
            current: [].iter(),
        },
    }
}
struct LayoutFrame<'a> {
    id: usize,
    types: Types<'a>,
}
fn layouts(unit: &Unit, budget: &mut Budget) -> Result<Vec<Layout>, Error> {
    let mut state = budget.filled(unit.definitions.len(), 0_u8)?;
    let mut result = budget.filled(unit.definitions.len(), None)?;
    let mut pending = budget.vector(unit.definitions.len())?;
    for root in 0..unit.definitions.len() {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        pending.push(LayoutFrame {
            id: root,
            types: types(&unit.definitions[root]),
        });
        while let Some(frame) = pending.last_mut() {
            budget.charge(1)?;
            if let Some(ty) = frame.types.next() {
                if let Type::Record(id) | Type::Enum(id) = ty {
                    match state[id.0 as usize] {
                        0 => {
                            cap(pending.len() + 1, budget.limits.depth, LimitKind::Depth)?;
                            state[id.0 as usize] = 1;
                            pending.push(LayoutFrame {
                                id: id.0 as usize,
                                types: types(&unit.definitions[id.0 as usize]),
                            });
                        }
                        1 => return Err(Error::new(ErrorKind::RecursiveType)),
                        _ => {}
                    }
                }
            } else {
                let id = frame.id;
                result[id] = Some(compute_layout(&unit.definitions[id], &result, budget)?);
                state[id] = 2;
                pending.pop();
            }
        }
    }
    // Account for both the dependency table and final immutable layout table.
    let mut layouts = budget.vector(result.len())?;
    for layout in result {
        budget.charge(1)?;
        layouts.push(layout.expect("validated acyclic definitions have layouts"));
    }
    Ok(layouts)
}
fn size(ty: Type, layouts: &[Option<Layout>]) -> (usize, usize) {
    match ty {
        Type::I64 | Type::Bool | Type::Own(_) => (1, 1),
        Type::Record(id) | Type::Enum(id) => {
            let l = layouts[id.0 as usize]
                .as_ref()
                .expect("dependency was computed");
            (l.cells, l.depth)
        }
    }
}
fn offsets(
    types: impl Iterator<Item = Type>,
    count: usize,
    layouts: &[Option<Layout>],
    budget: &mut Budget,
) -> Result<(Vec<usize>, usize, usize), Error> {
    let mut offsets = budget.vector(count)?;
    let mut cells = 1_usize;
    let mut depth = 1_usize;
    for ty in types {
        budget.charge(1)?;
        offsets.push(cells);
        let (child_cells, child_depth) = size(ty, layouts);
        cells = cells
            .checked_add(child_cells)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        depth = depth.max(
            child_depth
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?,
        );
        cap(cells, budget.limits.value_cells, LimitKind::ValueCells)?;
        cap(depth, budget.limits.depth, LimitKind::Depth)?;
    }
    cap(cells, budget.limits.value_cells, LimitKind::ValueCells)?;
    cap(depth, budget.limits.depth, LimitKind::Depth)?;
    Ok((offsets, cells, depth))
}
fn compute_layout(
    definition: &Definition,
    layouts: &[Option<Layout>],
    budget: &mut Budget,
) -> Result<Layout, Error> {
    match &definition.kind {
        DefinitionKind::Record(fields) => {
            let (fields, cells, depth) = offsets(
                fields.iter().map(|f| f.value_type),
                fields.len(),
                layouts,
                budget,
            )?;
            Ok(Layout {
                fields,
                cells,
                depth,
                cases: Vec::new(),
            })
        }
        DefinitionKind::Enum(cases) => {
            let mut result = budget.vector(cases.len())?;
            let mut cells = 1;
            let mut depth = 1;
            for case in cases {
                budget.charge(1)?;
                let (offsets, case_cells, case_depth) = offsets(
                    case.payload.iter().copied(),
                    case.payload.len(),
                    layouts,
                    budget,
                )?;
                result.push(offsets);
                cells = cells.max(case_cells);
                depth = depth.max(case_depth);
            }
            Ok(Layout {
                cells,
                depth,
                fields: Vec::new(),
                cases: result,
            })
        }
    }
}

fn register(function: &Function, register: Register, budget: &mut Budget) -> Result<Type, Error> {
    budget.charge(1)?;
    function
        .registers
        .get(register.0 as usize)
        .copied()
        .ok_or_else(|| Error::new(ErrorKind::InvalidRegister(register.0)))
}
fn expect(
    function: &Function,
    value: Register,
    expected: Type,
    budget: &mut Budget,
) -> Result<(), Error> {
    let actual = register(function, value, budget)?;
    if actual != expected {
        return Err(Error::new(ErrorKind::TypeMismatch { expected, actual }));
    }
    Ok(())
}
fn record(unit: &Unit, id: DefinitionId) -> Result<&[Field], Error> {
    let definition = unit
        .definitions
        .get(id.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidDefinition(id.0)))?;
    match &definition.kind {
        DefinitionKind::Record(fields) => Ok(fields),
        _ => Err(Error::new(ErrorKind::ExpectedRecord(Type::Enum(id)))),
    }
}
fn cases(unit: &Unit, id: DefinitionId) -> Result<&[Case], Error> {
    let definition = unit
        .definitions
        .get(id.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidDefinition(id.0)))?;
    match &definition.kind {
        DefinitionKind::Enum(cases) => Ok(cases),
        _ => Err(Error::new(ErrorKind::ExpectedEnum(Type::Record(id)))),
    }
}
fn case(unit: &Unit, id: DefinitionId, index: u32) -> Result<&Case, Error> {
    cases(unit, id)?
        .get(index as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidCase(index)))
}
fn arity(actual: usize, expected: usize) -> Result<(), Error> {
    if actual != expected {
        Err(Error::new(ErrorKind::Arity { expected, actual }))
    } else {
        Ok(())
    }
}
fn operation(unit: &Unit, function: &Function, op: &Op, budget: &mut Budget) -> Result<(), Error> {
    budget.charge(1)?;
    let result = match op {
        Op::I64Constant { .. } => Type::I64,
        Op::BoolConstant { .. } => Type::Bool,
        Op::Add { left, right, .. }
        | Op::LessThan { left, right, .. }
        | Op::EqualI64 { left, right, .. } => {
            expect(function, *left, Type::I64, budget)?;
            expect(function, *right, Type::I64, budget)?;
            if matches!(op, Op::Add { .. }) {
                Type::I64
            } else {
                Type::Bool
            }
        }
        Op::HostCall {
            import, argument, ..
        } => {
            unit.imports
                .get(import.0 as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidImport(import.0)))?;
            expect(function, *argument, Type::I64, budget)?;
            Type::I64
        }
        Op::Call {
            function: callee,
            arguments,
            ..
        } => {
            let callee = unit
                .functions
                .get(callee.0 as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidFunction(callee.0)))?;
            budget.charge(1)?;
            let mut owned = !callee.signature.result.is_copy();
            for ty in &callee.signature.parameters {
                budget.charge(1)?;
                owned |= !ty.is_copy();
            }
            if owned {
                return Err(Error::new(ErrorKind::OwnedCall));
            }
            arity(arguments.len(), callee.signature.parameters.len())?;
            for (argument, ty) in arguments.iter().zip(&callee.signature.parameters) {
                expect(function, *argument, *ty, budget)?;
            }
            callee.signature.result
        }
        Op::Record {
            definition, fields, ..
        } => {
            let declared = record(unit, *definition)?;
            arity(fields.len(), declared.len())?;
            for (argument, field) in fields.iter().zip(declared) {
                expect(function, *argument, field.value_type, budget)?;
            }
            Type::Record(*definition)
        }
        Op::Field {
            receiver, field, ..
        } => {
            let ty = register(function, *receiver, budget)?;
            let Type::Record(id) = ty else {
                return Err(Error::new(ErrorKind::ExpectedRecord(ty)));
            };
            record(unit, id)?
                .get(*field as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidField(*field)))?
                .value_type
        }
        Op::Enum {
            definition,
            case: index,
            payload,
            ..
        } => {
            let case = case(unit, *definition, *index)?;
            arity(payload.len(), case.payload.len())?;
            for (argument, ty) in payload.iter().zip(&case.payload) {
                expect(function, *argument, *ty, budget)?;
            }
            Type::Enum(*definition)
        }
        Op::IsCase {
            receiver,
            case: index,
            ..
        } => {
            let ty = register(function, *receiver, budget)?;
            let Type::Enum(id) = ty else {
                return Err(Error::new(ErrorKind::ExpectedEnum(ty)));
            };
            case(unit, id, *index)?;
            Type::Bool
        }
        Op::EnumField {
            receiver,
            case: index,
            field,
            ..
        } => {
            let ty = register(function, *receiver, budget)?;
            let Type::Enum(id) = ty else {
                return Err(Error::new(ErrorKind::ExpectedEnum(ty)));
            };
            *case(unit, id, *index)?
                .payload
                .get(*field as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidField(*field)))?
        }
    };
    expect(function, op.destination(), result, budget)
}

#[derive(Clone, Copy)]
struct DefinitionSite {
    block: u32,
    order: usize,
}
const UNDEFINED: u32 = u32::MAX;
fn define(
    sites: &mut [DefinitionSite],
    register: Register,
    block: usize,
    order: usize,
    budget: &mut Budget,
) -> Result<(), Error> {
    budget.charge(1)?;
    let site = sites
        .get_mut(register.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidRegister(register.0)))?;
    if site.block != UNDEFINED {
        return Err(Error::new(ErrorKind::DuplicateRegister(register.0)));
    }
    *site = DefinitionSite {
        block: block as u32,
        order,
    };
    Ok(())
}
fn use_value(
    sites: &[DefinitionSite],
    idom: &[u32],
    value: Register,
    block: usize,
    order: usize,
    budget: &mut Budget,
) -> Result<(), Error> {
    budget.charge(1)?;
    let site = sites
        .get(value.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidRegister(value.0)))?;
    if site.block == UNDEFINED {
        return Err(Error::new(ErrorKind::UndefinedRegister(value.0)));
    }
    if site.block == block as u32 {
        if site.order >= order {
            return Err(Error::new(ErrorKind::DoesNotDominate(value.0)));
        }
        return Ok(());
    }
    let mut ancestor = block as u32;
    loop {
        budget.charge(1)?;
        if ancestor == site.block {
            return Ok(());
        }
        if ancestor == 0 {
            return Err(Error::new(ErrorKind::DoesNotDominate(value.0)));
        }
        ancestor = idom[ancestor as usize];
    }
}

fn function_facts(
    unit: &Unit,
    function: &Function,
    layouts: &[Layout],
    budget: &mut Budget,
) -> Result<FunctionFacts, Error> {
    if function.blocks.is_empty() {
        return Err(Error::new(ErrorKind::EmptyFunction));
    }
    if !function.blocks[0].parameters.is_empty() {
        return Err(Error::new(ErrorKind::EntryParameters));
    }
    if function.registers.len() < function.signature.parameters.len() {
        return Err(Error::new(ErrorKind::Arity {
            expected: function.signature.parameters.len(),
            actual: function.registers.len(),
        }));
    }
    let mut has_owners = false;
    for ty in function
        .signature
        .parameters
        .iter()
        .chain(std::iter::once(&function.signature.result))
        .chain(&function.registers)
    {
        budget.charge(1)?;
        has_owners |= !ty.is_copy();
    }
    if has_owners && function.blocks.len() != 1 {
        return Err(Error::new(ErrorKind::OwnedControlFlow));
    }
    let mut register_cells = budget.vector(function.registers.len())?;
    let mut cells = 0_usize;
    for ty in &function.registers {
        budget.charge(1)?;
        let count = match ty {
            Type::I64 | Type::Bool | Type::Own(_) => {
                cap(1, budget.limits.value_cells, LimitKind::ValueCells)?;
                cap(1, budget.limits.depth, LimitKind::Depth)?;
                1
            }
            Type::Record(id) | Type::Enum(id) => layouts[id.0 as usize].cells,
        };
        cells = cells
            .checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
        register_cells.push(count);
    }
    let mut sites = budget.filled(
        function.registers.len(),
        DefinitionSite {
            block: UNDEFINED,
            order: 0,
        },
    )?;
    for (index, expected) in function.signature.parameters.iter().enumerate() {
        expect(function, Register(index as u32), *expected, budget)?;
        define(&mut sites, Register(index as u32), 0, 0, budget)?;
    }
    let mut predecessor_starts = budget.filled(function.blocks.len() + 1, 0_usize)?;
    for (index, block) in function.blocks.iter().enumerate() {
        let result = (|| {
            budget.charge(1)?;
            for parameter in &block.parameters {
                register(function, *parameter, budget)?;
                define(&mut sites, *parameter, index, 0, budget)?;
            }
            for (position, instruction) in block.instructions.iter().enumerate() {
                let result = (|| {
                    span(unit, instruction.span, budget)?;
                    operation(unit, function, &instruction.op, budget)?;
                    define(
                        &mut sites,
                        instruction.op.destination(),
                        index,
                        position + 1,
                        budget,
                    )
                })();
                result.map_err(|mut error: Error| {
                    error.instruction = Some(position as u32);
                    error
                })?;
            }
            span(unit, block.terminator.span, budget)?;
            match &block.terminator.kind {
                Term::Return(value) => expect(function, *value, function.signature.result, budget)?,
                Term::Branch { condition, .. } => expect(function, *condition, Type::Bool, budget)?,
                Term::Jump(_) => {}
            }
            for target in block.terminator.kind.targets() {
                budget.charge(1)?;
                if target.block.0 == 0 {
                    return Err(Error::new(ErrorKind::EntryPredecessor));
                }
                let destination = function
                    .blocks
                    .get(target.block.0 as usize)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidBlock(target.block.0)))?;
                arity(target.arguments.len(), destination.parameters.len())?;
                for (argument, parameter) in target.arguments.iter().zip(&destination.parameters) {
                    let ty = register(function, *parameter, budget)?;
                    expect(function, *argument, ty, budget)?;
                }
                predecessor_starts[target.block.0 as usize + 1] = predecessor_starts
                    [target.block.0 as usize + 1]
                    .checked_add(1)
                    .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
            }
            Ok(())
        })();
        result.map_err(|mut error: Error| {
            error.block = Some(index as u32);
            error
        })?;
    }
    for index in 1..predecessor_starts.len() {
        budget.charge(1)?;
        predecessor_starts[index] = predecessor_starts[index]
            .checked_add(predecessor_starts[index - 1])
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow))?;
    }
    let edges = *predecessor_starts.last().unwrap();
    let mut predecessors = budget.filled(edges, 0_u32)?;
    let mut cursors = budget.vector(function.blocks.len())?;
    for start in &predecessor_starts[..function.blocks.len()] {
        budget.charge(1)?;
        cursors.push(*start);
    }
    for (index, block) in function.blocks.iter().enumerate() {
        for target in block.terminator.kind.targets() {
            budget.charge(1)?;
            let cursor = &mut cursors[target.block.0 as usize];
            predecessors[*cursor] = index as u32;
            *cursor += 1;
        }
    }
    let idom = dominators(function, &predecessor_starts, &predecessors, budget)?;
    let mut owned_uses = if has_owners {
        budget.filled(function.registers.len(), 0_u8)?
    } else {
        Vec::new()
    };
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (position, instruction) in block.instructions.iter().enumerate() {
            for value in instruction.op.operands() {
                use_value(&sites, &idom, value, block_index, position + 1, budget).map_err(
                    |mut error| {
                        error.block = Some(block_index as u32);
                        error.instruction = Some(position as u32);
                        error
                    },
                )?;
                count_owner(function, value, &mut owned_uses, budget)?;
            }
        }
        for value in block.terminator.kind.operands() {
            use_value(
                &sites,
                &idom,
                value,
                block_index,
                block.instructions.len() + 1,
                budget,
            )
            .map_err(|mut error| {
                error.block = Some(block_index as u32);
                error
            })?;
            count_owner(function, value, &mut owned_uses, budget)?;
        }
    }
    if has_owners {
        for (index, ty) in function.registers.iter().enumerate() {
            budget.charge(1)?;
            if !ty.is_copy() {
                match owned_uses[index] {
                    0 => return Err(Error::new(ErrorKind::OwnedNotConsumed(index as u32))),
                    1 => {}
                    _ => return Err(Error::new(ErrorKind::OwnedUsedMoreThanOnce(index as u32))),
                }
            }
        }
    }
    Ok(FunctionFacts {
        cells,
        register_cells,
    })
}
fn count_owner(
    function: &Function,
    value: Register,
    counts: &mut [u8],
    budget: &mut Budget,
) -> Result<(), Error> {
    budget.charge(1)?;
    if !function.registers[value.0 as usize].is_copy() {
        counts[value.0 as usize] = counts[value.0 as usize].saturating_add(1);
    }
    Ok(())
}

fn dominators(
    function: &Function,
    starts: &[usize],
    predecessors: &[u32],
    budget: &mut Budget,
) -> Result<Vec<u32>, Error> {
    let count = function.blocks.len();
    let mut seen = budget.filled(count, false)?;
    let mut pending = budget.vector(count)?;
    let mut order = budget.vector(count)?;
    seen[0] = true;
    pending.push((0_u32, 0_usize));
    while let Some((block, next)) = pending.last_mut() {
        budget.charge(1)?;
        if let Some(target) = function.blocks[*block as usize]
            .terminator
            .kind
            .targets()
            .nth(*next)
        {
            *next += 1;
            if !seen[target.block.0 as usize] {
                seen[target.block.0 as usize] = true;
                pending.push((target.block.0, 0));
            }
        } else {
            let block = *block;
            pending.pop();
            order.push(block);
        }
    }
    if order.len() != count {
        return Err(Error::new(ErrorKind::UnreachableBlock(
            seen.iter().position(|v| !v).unwrap() as u32,
        )));
    }
    budget.charge(order.len())?;
    order.reverse();
    let mut rank = budget.filled(count, 0_u32)?;
    for (index, block) in order.iter().enumerate() {
        budget.charge(1)?;
        rank[*block as usize] = index as u32;
    }
    let mut idom = budget.filled(count, UNDEFINED)?;
    idom[0] = 0;
    loop {
        let mut changed = false;
        for block in order.iter().skip(1).copied() {
            budget.charge(1)?;
            let mut candidate = UNDEFINED;
            for predecessor in &predecessors[starts[block as usize]..starts[block as usize + 1]] {
                budget.charge(1)?;
                if idom[*predecessor as usize] == UNDEFINED {
                    continue;
                }
                candidate = if candidate == UNDEFINED {
                    *predecessor
                } else {
                    intersect(candidate, *predecessor, &idom, &rank, budget)?
                };
            }
            if candidate != UNDEFINED && idom[block as usize] != candidate {
                idom[block as usize] = candidate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(idom)
}
fn intersect(
    mut left: u32,
    mut right: u32,
    idom: &[u32],
    rank: &[u32],
    budget: &mut Budget,
) -> Result<u32, Error> {
    while left != right {
        budget.charge(1)?;
        if rank[left as usize] > rank[right as usize] {
            left = idom[left as usize];
        } else {
            right = idom[right as usize];
        }
    }
    Ok(left)
}
