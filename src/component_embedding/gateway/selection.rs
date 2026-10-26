//! Exact task-root projection; ordinary IDL task imports remain prohibited.
use super::{INVALID, Module, m, wire};
use lang_runtime::component::idl;

pub(super) fn task_root(
    request: &wire::Contract,
    module: &Module,
    session: &mut wire::Session,
) -> Result<m::FunctionId, u32> {
    let doc = request.document();
    let [export] = doc.exports.as_slice() else {
        return Err(INVALID);
    };
    let actual = module.contract().document();
    if !doc.imports.is_empty() || doc.version.value != actual.version.value {
        return Err(INVALID);
    }
    let key = &export.value;
    let key_bytes = key
        .package()
        .namespace()
        .len()
        .saturating_add(key.package().package().len())
        .saturating_add(key.interface().len())
        .saturating_add(3);
    session
        .charge(doc.interfaces.len().saturating_mul(key_bytes), 0)
        .map_err(|_| INVALID)?;
    let interface = doc
        .interfaces
        .iter()
        .find(|i| i.key.value == export.value)
        .ok_or(INVALID)?;
    let [function] = interface.functions.as_slice() else {
        return Err(INVALID);
    };
    if function.effect.value != idl::Effect::Task {
        return Err(INVALID);
    }
    session
        .charge(actual.exports.len().saturating_mul(key_bytes), 0)
        .map_err(|_| INVALID)?;
    if !actual.exports.iter().any(|e| e.value == export.value) {
        return Err(INVALID);
    }
    session
        .charge(actual.interfaces.len().saturating_mul(key_bytes), 0)
        .map_err(|_| INVALID)?;
    let (interface_index, target) = actual
        .interfaces
        .iter()
        .enumerate()
        .find(|(_, i)| i.key.value == export.value)
        .ok_or(INVALID)?;
    session
        .charge(
            target
                .functions
                .len()
                .saturating_mul(function.name.value.len().saturating_add(1)),
            0,
        )
        .map_err(|_| INVALID)?;
    let (member_index, member) = target
        .functions
        .iter()
        .enumerate()
        .find(|(_, f)| f.name.value == function.name.value)
        .ok_or(INVALID)?;
    session
        .charge(function.capability.value.len().saturating_add(1), 0)
        .map_err(|_| INVALID)?;
    if member.effect.value != idl::Effect::Task
        || member.capability.value != function.capability.value
    {
        return Err(INVALID);
    }
    let required = wire::Signature::new(request, &export.value, &function.name.value, session)
        .map_err(|_| INVALID)?;
    let provided = wire::Signature::new(
        module.contract(),
        &export.value,
        &function.name.value,
        session,
    )
    .map_err(|_| INVALID)?;
    // Compare the complete typed preimages, not only their digests.
    let required = required.preimage(session).map_err(|_| INVALID)?;
    let provided = provided.preimage(session).map_err(|_| INVALID)?;
    session
        .charge(required.len().max(provided.len()), 0)
        .map_err(|_| INVALID)?;
    if required != provided {
        return Err(INVALID);
    }
    session
        .charge(module.unit().exports().len(), 0)
        .map_err(|_| INVALID)?;
    module
        .unit()
        .exports()
        .iter()
        .find_map(|(id, binding)| {
            (binding.interface == interface_index && binding.member == member_index).then_some(*id)
        })
        .ok_or(INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_selection_uses_one_cumulative_decode_and_signature_budget() {
        let idl = br#"{"idl":1,"package":"demo:task","version":"1.0.0","interfaces":[{"key":"demo:task/api","resources":[],"types":[],"functions":[{"name":"run","effect":"task","params":[],"result":"i64","capability":"task.run"}]}],"imports":[],"exports":["demo:task/api"]}"#;
        let document = idl::check(m::SourceId(1), idl, Default::default()).unwrap();
        let guest = crate::component_bindings::guest(&document, Default::default()).unwrap();
        let source = format!(
            "{}\n{} {{ return 42; }}",
            guest.declarations, guest.exports[0].declaration
        );
        let module =
            crate::component_embedding::load(0, source.as_bytes(), idl, Default::default())
                .unwrap();
        let bytes = wire::encode_contract(
            module.contract(),
            &mut wire::Session::new(Default::default()),
        )
        .unwrap();
        let select = |limits| {
            let mut session = wire::Session::new(limits);
            let request =
                wire::decode_contract(m::SourceId(7), &bytes, &mut session).map_err(|_| INVALID)?;
            let entry = task_root(&request, &module, &mut session)?;
            Ok::<_, u32>((entry, session.preparation_usage()))
        };
        let (entry, usage) = select(Default::default()).unwrap();
        assert!(usage.work > 0 && usage.allocation_bytes > 0);
        let exact = wire::Limits {
            work: usage.work,
            allocation_bytes: usage.allocation_bytes,
            ..Default::default()
        };
        assert_eq!(select(exact).unwrap(), (entry, usage));
        assert!(
            select(wire::Limits {
                work: exact.work - 1,
                ..exact
            })
            .is_err()
        );
        assert!(
            select(wire::Limits {
                allocation_bytes: exact.allocation_bytes - 1,
                ..exact
            })
            .is_err()
        );
    }
}
