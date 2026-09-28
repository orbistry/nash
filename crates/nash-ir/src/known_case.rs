//! Known-subject case folding: Boolean cleanup and pre-ANF native constructors.
use crate::{
    build::Builder,
    core::{Binder, Branch, CaseKind, Core, CoreKind, Test},
};
use nash_plutus::{builtin::DefaultFunction, constant::Constant};
use std::collections::{HashMap, HashSet};

/// No evaluation or substitution: the literal subject is already a value.
/// Preserve malformed tables and missing matches for the lowerer/runtime.
/// Keep the enclosing result type view when selecting a branch or default.
pub fn reduce_bool<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::Case {
            kind: CaseKind::Bool,
            scrutinee,
            branches,
            default,
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Lit(Constant::Boolean(value)) = scrutinee.kind else {
            return None;
        };
        let (mut yes, mut no) = (None, None);
        for branch in branches {
            if !branch.binders.is_empty() {
                return None;
            }
            let slot = match branch.test {
                Test::True => &mut yes,
                Test::False => &mut no,
                _ => return None,
            };
            if slot.is_some() {
                return None;
            }
            *slot = Some(branch.body);
        }
        let selected = (if *value { yes } else { no }).or(default)?;
        Some(b.with_type(selected, node.ty))
    })
}

/// Input binders must be globally unique and well scoped, as for beta reduction.
/// Keep every field strict, in source order, even when its binder is unused.
/// Leave invalid tables, absent tags and application arity mismatches untouched.
pub fn reduce_constr<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::Case {
            kind: CaseKind::Tag,
            scrutinee,
            branches,
            default: None,
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Constr { tag, fields } = scrutinee.kind else {
            return None;
        };
        let selected = select_constr(branches, tag, fields.len())?;
        let mut body = selected.body;
        for (&binder, &field) in selected.binders.iter().zip(fields).rev() {
            body = b.let_(binder, field, body);
        }
        Some(b.with_type(body, node.ty))
    })
}

/// Fold matches on let-bound constructors without moving field evaluation.
/// Requires ANF with globally unique, well-scoped binders and the same name supply.
/// Keep constructor bindings for escaping uses; ordinary cleanup removes dead ones.
/// Run binding-splice cleanup afterwards: a selected branch can contain lets.
/// Newly exposed aliases can enable another iteration with that cleanup.
pub fn reduce_bound_constr<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let facts = constructor_bindings(core);
    let mut matched = HashSet::new();
    core.walk(&mut |node| {
        if let Some((root, _, _)) = bound_match(node, &facts) {
            matched.insert(root);
        }
    });
    if matched.is_empty() {
        return core;
    }
    // Give each field a stable reference at the original construction site.
    // In particular, copying an ANF lambda/delay would duplicate its binders.
    // Naming literals also leaves the existing propagation size policy in charge.
    let named = core.map(b, &mut |node| {
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        if !matched.contains(&binder.name.unique) {
            return None;
        }
        let CoreKind::Constr { tag, fields } = value.kind else {
            return None;
        };
        let mut bindings = Vec::new();
        let refs: Vec<_> = fields
            .iter()
            .map(|&field| {
                if matches!(field.kind, CoreKind::Var(_)) {
                    return field;
                }
                let binder = Binder {
                    name: b.fresh("field"),
                    ty: field.ty,
                };
                bindings.push((binder, field));
                b.var(binder.name, binder.ty)
            })
            .collect();
        if bindings.is_empty() {
            return None;
        }
        let mut result = b.let_(binder, b.constr(tag, &refs, value.ty), body);
        for (binder, field) in bindings.into_iter().rev() {
            result = b.let_(binder, field, result);
        }
        Some(b.with_type(result, node.ty))
    });
    let facts = constructor_bindings(named);
    named.map(b, &mut |node| {
        let (_, fields, selected) = bound_match(node, &facts)?;
        let mut body = selected.body;
        for (&binder, &field) in selected.binders.iter().zip(fields).rev() {
            body = b.let_(binder, field, body);
        }
        Some(b.with_type(body, node.ty))
    })
}

/// Fold known bindings and clean up newly exposed aliases to a fixed point.
/// Input has already passed the ordinary ANF cleanup; no normalization is repeated.
pub fn simplify_bound_constr<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let next = crate::small_inline::simplify(b, reduce_bound_constr(b, core));
        if std::ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

type ConstructorBindings<'a> = HashMap<u32, (u32, &'a Core<'a>)>;

fn constructor_bindings<'a>(core: &'a Core<'a>) -> ConstructorBindings<'a> {
    let mut facts = HashMap::new();
    // Preorder sees a lexical binding before its uses. Unique names let this
    // table include captured values without a separate environment per scope.
    core.walk(&mut |node| {
        let CoreKind::Let { binder, value, .. } = node.kind else {
            return;
        };
        let fact = match value.kind {
            CoreKind::Constr { .. } => Some((binder.name.unique, value)),
            CoreKind::Var(name) => facts.get(&name.unique).copied(),
            _ => None,
        };
        if let Some(fact) = fact {
            facts.insert(binder.name.unique, fact);
        }
    });
    facts
}

fn bound_match<'a>(
    node: &'a Core<'a>,
    facts: &ConstructorBindings<'a>,
) -> Option<(u32, &'a [&'a Core<'a>], &'a Branch<'a>)> {
    let CoreKind::Case {
        kind: CaseKind::Tag,
        scrutinee,
        branches,
        default: None,
    } = node.kind
    else {
        return None;
    };
    let CoreKind::Var(name) = scrutinee.kind else {
        return None;
    };
    let &(root, value) = facts.get(&name.unique)?;
    let CoreKind::Constr { tag, fields } = value.kind else {
        return None;
    };
    Some((root, fields, select_constr(branches, tag, fields.len())?))
}

fn select_constr<'a>(branches: &'a [Branch<'a>], tag: u16, arity: usize) -> Option<&'a Branch<'a>> {
    let mut seen = vec![false; branches.len()];
    for branch in branches {
        let Test::Tag(tag) = branch.test else {
            return None;
        };
        let slot = seen.get_mut(usize::from(tag))?;
        if *slot {
            return None;
        }
        *slot = true;
    }
    let selected = branches
        .iter()
        .find(|branch| branch.test == Test::Tag(tag))?;
    (selected.binders.len() == arity).then_some(selected)
}

/// Native-list folding on hygienic ANF. A successful MkCons proves Cons,
/// but its runtime operand checks must still run at the original construction.
/// Branch selection can expose lets; run ordinary cleanup before another pass.
pub fn reduce_list<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let facts = list_bindings(core);
    let mut matched = HashSet::new();
    core.walk(&mut |node| {
        if let Some((Some(root), _)) = list_subject(node, &facts) {
            matched.insert(root);
        }
    });
    // Share derived literal fields as well as MkCons operands. This avoids
    // serializing the same literal tail again for every match on one binding.
    // Never copy operand subtrees: ANF lambdas/delays still contain binders.
    let mut named_fields = HashMap::new();
    let named = core.map(b, &mut |node| {
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        if !matched.contains(&binder.name.unique) {
            return None;
        }
        let fields = list_fields(b, value)?;
        let mut bindings = Vec::new();
        let refs = fields.map(|field| {
            if matches!(field.kind, CoreKind::Var(_)) {
                return field;
            }
            let field_binder = Binder {
                name: b.fresh("list_field"),
                ty: field.ty,
            };
            bindings.push((field_binder, field));
            b.var(field_binder.name, field_binder.ty)
        });
        named_fields.insert(binder.name.unique, refs);
        if bindings.is_empty() {
            return None;
        }
        let value = if matches!(value.kind, CoreKind::Builtin { .. }) {
            b.builtin(DefaultFunction::MkCons, &refs, value.ty)
        } else {
            value
        };
        let mut result = b.let_(binder, value, body);
        for (binder, value) in bindings.into_iter().rev() {
            result = b.let_(binder, value, result);
        }
        Some(b.with_type(result, node.ty))
    });
    let facts = list_bindings(named);
    named.map(b, &mut |node| {
        let (root, value) = list_subject(node, &facts)?;
        let CoreKind::Case {
            branches, default, ..
        } = node.kind
        else {
            return None;
        };
        let (nil, cons) = list_arms(branches)?;
        let fields = root
            .and_then(|id| named_fields.get(&id).copied())
            .or_else(|| list_fields(b, value));
        let selected = if fields.is_some() { cons } else { nil };
        let fields = fields.as_ref().map_or(&[][..], |fields| fields.as_slice());
        let body = match selected {
            Some(branch) => {
                let mut body = branch.body;
                for (&binder, &field) in branch.binders.iter().zip(fields).rev() {
                    body = b.let_(binder, field, body);
                }
                body
            }
            None => default.unwrap_or_else(|| b.error(node.ty)),
        };
        Some(b.with_type(body, node.ty))
    })
}

fn list_fields<'a>(b: &Builder<'a>, value: &'a Core<'a>) -> Option<[&'a Core<'a>; 2]> {
    match value.kind {
        CoreKind::Lit(Constant::ProtoList(inner, values)) => {
            let (head, tail) = values.split_first()?;
            Some([
                b.lit(head),
                b.lit(Constant::proto_list(b.arena, inner, tail)),
            ])
        }
        CoreKind::Builtin {
            func: DefaultFunction::MkCons,
            args: [head, tail],
        } => Some([*head, *tail]),
        _ => None,
    }
}

/// Fold known lists and constructors with cleanup; normalize only once upstream.
pub fn simplify_list<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let next = crate::small_inline::simplify(b, reduce_list(b, core));
        let next = simplify_bound_constr(b, next);
        if std::ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

type ListBindings<'a> = HashMap<u32, (u32, &'a Core<'a>)>;
fn list_bindings<'a>(core: &'a Core<'a>) -> ListBindings<'a> {
    let mut facts = HashMap::new();
    core.walk(&mut |node| {
        let CoreKind::Let { binder, value, .. } = node.kind else {
            return;
        };
        let fact = match value.kind {
            CoreKind::Lit(Constant::ProtoList(..))
            | CoreKind::Builtin {
                func: DefaultFunction::MkCons,
                args: [_, _],
            } => Some((binder.name.unique, value)),
            CoreKind::Var(name) => facts.get(&name.unique).copied(),
            _ => None,
        };
        if let Some(fact) = fact {
            facts.insert(binder.name.unique, fact);
        }
    });
    facts
}
fn list_subject<'a>(
    node: &'a Core<'a>,
    facts: &ListBindings<'a>,
) -> Option<(Option<u32>, &'a Core<'a>)> {
    let CoreKind::Case {
        kind: CaseKind::List,
        scrutinee,
        branches,
        ..
    } = node.kind
    else {
        return None;
    };
    list_arms(branches)?;
    match scrutinee.kind {
        CoreKind::Lit(Constant::ProtoList(..)) => Some((None, scrutinee)),
        CoreKind::Var(name) => facts
            .get(&name.unique)
            .map(|&(root, value)| (Some(root), value)),
        _ => None,
    }
}
fn list_arms<'a>(
    branches: &'a [Branch<'a>],
) -> Option<(Option<&'a Branch<'a>>, Option<&'a Branch<'a>>)> {
    let (mut nil, mut cons) = (None, None);
    for branch in branches {
        match branch.test {
            Test::Nil if nil.is_none() && branch.binders.is_empty() => nil = Some(branch),
            Test::Cons if cons.is_none() && branch.binders.len() == 2 => cons = Some(branch),
            _ => return None,
        }
    }
    Some((nil, cons))
}

/// Trial literal Data folding on hygienic ANF. Constructor builtins are not facts.
/// Share unwrapped payloads at their original literal binding, as for list fields.
pub fn reduce_data<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let facts = data_bindings(core);
    let mut matched = HashSet::new();
    core.walk(&mut |node| {
        if let Some((Some(root), _)) = data_subject(node, &facts) {
            matched.insert(root);
        }
    });
    let mut payloads = HashMap::new();
    let named = core.map(b, &mut |node| {
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        if !matched.contains(&binder.name.unique) {
            return None;
        }
        let CoreKind::Lit(Constant::Data(data)) = value.kind else {
            return None;
        };
        let payload = b.lit(data_payload(b, data));
        let field = Binder {
            name: b.fresh("data_payload"),
            ty: payload.ty,
        };
        payloads.insert(binder.name.unique, b.var(field.name, field.ty));
        Some(b.with_type(b.let_(field, payload, b.let_(binder, value, body)), node.ty))
    });
    let facts = data_bindings(named);
    named.map(b, &mut |node| {
        let (root, data) = data_subject(node, &facts)?;
        let CoreKind::Case {
            branches, default, ..
        } = node.kind
        else {
            return None;
        };
        let test = data_test(data);
        let body = match branches.iter().find(|branch| branch.test == test) {
            Some(branch) => {
                let payload = root
                    .and_then(|id| payloads.get(&id).copied())
                    .unwrap_or_else(|| b.lit(data_payload(b, data)));
                b.let_(branch.binders[0], payload, branch.body)
            }
            None => default.unwrap_or_else(|| b.error(node.ty)),
        };
        Some(b.with_type(body, node.ty))
    })
}

/// Isolated Data trial with accepted cleanup; one normalization upstream.
pub fn simplify_data<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let next = simplify_list(b, reduce_data(b, core));
        if std::ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

type DataBindings<'a> = HashMap<u32, (u32, &'a nash_plutus::data::PlutusData<'a>)>;
fn data_bindings<'a>(core: &'a Core<'a>) -> DataBindings<'a> {
    let mut facts = HashMap::new();
    core.walk(&mut |node| {
        let CoreKind::Let { binder, value, .. } = node.kind else {
            return;
        };
        let fact = match value.kind {
            CoreKind::Lit(Constant::Data(data)) => Some((binder.name.unique, *data)),
            CoreKind::Var(name) => facts.get(&name.unique).copied(),
            _ => None,
        };
        if let Some(fact) = fact {
            facts.insert(binder.name.unique, fact);
        }
    });
    facts
}
fn data_subject<'a>(
    node: &'a Core<'a>,
    facts: &DataBindings<'a>,
) -> Option<(Option<u32>, &'a nash_plutus::data::PlutusData<'a>)> {
    let CoreKind::Case {
        kind: CaseKind::Data,
        scrutinee,
        branches,
        ..
    } = node.kind
    else {
        return None;
    };
    let mut seen = [false; 5];
    for branch in branches {
        let index = match branch.test {
            Test::DataConstr => 0,
            Test::DataMap => 1,
            Test::DataList => 2,
            Test::DataI => 3,
            Test::DataB => 4,
            _ => return None,
        };
        if seen[index] || branch.binders.len() != 1 {
            return None;
        }
        seen[index] = true;
    }
    match scrutinee.kind {
        CoreKind::Lit(Constant::Data(data)) => Some((None, data)),
        CoreKind::Var(name) => facts
            .get(&name.unique)
            .map(|&(root, data)| (Some(root), data)),
        _ => None,
    }
}
fn data_test(data: &nash_plutus::data::PlutusData<'_>) -> Test<'static> {
    use nash_plutus::data::PlutusData;
    match data {
        PlutusData::Constr { .. } => Test::DataConstr,
        PlutusData::Map(_) => Test::DataMap,
        PlutusData::List(_) => Test::DataList,
        PlutusData::Integer(_) => Test::DataI,
        PlutusData::ByteString(_) => Test::DataB,
    }
}
fn data_payload<'a>(
    b: &Builder<'a>,
    data: &'a nash_plutus::data::PlutusData<'a>,
) -> &'a Constant<'a> {
    use nash_plutus::{data::PlutusData, typ::Type};
    let a = b.arena;
    let list = |items: &'a [&'a PlutusData<'a>]| {
        let values = a.alloc_slice_fill_iter(items.iter().map(|item| Constant::data(a, item)));
        Constant::proto_list(a, &Type::Data, values)
    };
    match data {
        PlutusData::Integer(i) => Constant::integer(a, i),
        PlutusData::ByteString(bytes) => Constant::byte_string(a, bytes),
        PlutusData::List(items) => list(items),
        PlutusData::Map(entries) => {
            let values = a.alloc_slice_fill_iter(entries.iter().map(|(key, value)| {
                Constant::proto_pair(
                    a,
                    &Type::Data,
                    &Type::Data,
                    Constant::data(a, key),
                    Constant::data(a, value),
                )
            }));
            Constant::proto_list(a, Type::pair(a, &Type::Data, &Type::Data), values)
        }
        PlutusData::Constr { tag, fields } => Constant::proto_pair(
            a,
            &Type::Integer,
            Type::list(a, &Type::Data),
            Constant::integer_from(a, i128::from(*tag)),
            list(fields),
        ),
    }
}
