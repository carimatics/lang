use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::layout::{Error, Layouts};

#[test]
fn old_layouts_do_not_assign_token_storage_to_task_types() {
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    };
    for constructor in [
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::TaskError),
    ] {
        let table = o::TypeTable {
            sources: vec![m::Source {
                path: "task.lang".into(),
                text: "x".into(),
            }],
            constructed: vec![o::Constructed { constructor, span }],
            definitions: vec![],
        }
        .verify(Default::default())
        .unwrap();
        assert!(
            matches!(Layouts::compile(&table,Default::default()),Err(Error::UnsupportedTaskProfile{span:s}) if s==span)
        );
    }
}
