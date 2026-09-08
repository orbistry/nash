//! Representation queries on inference types. These queries never select a
//! type for a flexible variable; alias normalization only substitutes known args.
use nash_ast::{QualifiedName, primitives::Repr};
use nash_can::kinds::{KindEnv, TypeInfo};
use nash_constrain::{Content, FlatType, UnionFind, Variable, instantiate};
use std::collections::{BTreeMap, BTreeSet};

/// Substitute saturated transparent aliases without changing a flexible head.
/// Record aliases keep their nominal representation.
pub fn subject<'a>(
    uf: &mut UnionFind<'a>,
    env: &KindEnv<'a>,
    variable: Variable,
    allocated: &mut Vec<Variable>,
) -> Variable {
    let mut current = variable;
    let mut seen = BTreeSet::new();
    while seen.insert(uf.find(current)) {
        current = uf.find(current);
        instantiate::normalize_variable(uf, current, allocated);
        let rank = uf.get(current).rank;
        match uf.get(current).content.clone() {
            Content::Alias {
                home, name, real, ..
            } => {
                if matches!(
                    env.constructor(QualifiedName { home, name }),
                    TypeInfo::Defined { repr: Some(_), .. }
                ) {
                    return current;
                }
                current = real;
            }
            Content::Structure(FlatType::App1(home, name, args)) => {
                let TypeInfo::Defined {
                    parameters,
                    alias: Some(body),
                    repr: None,
                    ..
                } = env.constructor(QualifiedName { home, name })
                else {
                    return current;
                };
                if args.len() != parameters.len() {
                    return current;
                }
                let substitution: BTreeMap<_, _> = parameters.iter().copied().zip(args).collect();
                current =
                    instantiate::canonical_to_variable(uf, rank, allocated, &substitution, body);
            }
            _ => return current,
        }
    }
    current
}

pub fn known<'a>(
    uf: &mut UnionFind<'a>,
    env: &KindEnv<'a>,
    variable: Variable,
    allocated: &mut Vec<Variable>,
) -> Option<Repr> {
    let variable = subject(uf, env, variable, allocated);
    match uf.get(variable).content.clone() {
        Content::Alias { home, name, .. } => match env.constructor(QualifiedName { home, name }) {
            TypeInfo::Defined { repr, .. } => repr,
            _ => None,
        },
        Content::Structure(FlatType::App1(home, name, args)) => {
            let info = env.constructor(QualifiedName { home, name });
            if args.len() != info.parameters().len() {
                return None;
            }
            match info {
                TypeInfo::Builtin(primitive) => Some(primitive.repr),
                TypeInfo::Defined { repr, .. } => repr,
            }
        }
        Content::Structure(FlatType::Fun1(..) | FlatType::Tuple1(..)) => Some(Repr::Term),

        _ => None,
    }
}

pub fn same_predicate<'a>(
    uf: &mut UnionFind<'a>,
    env: &KindEnv<'a>,
    left: &crate::preds::Body<'a>,
    right: &crate::preds::Body<'a>,
    allocated: &mut Vec<Variable>,
) -> bool {
    if let (
        crate::preds::Body::Trait {
            trait_: lt,
            args: la,
            ..
        },
        crate::preds::Body::Trait {
            trait_: rt,
            args: ra,
            ..
        },
    ) = (left, right)
        && lt == rt
        && nash_ast::primitives::ReprTrait::of(*lt).is_some()
        && la.len() == 1
        && ra.len() == 1
    {
        let left = subject(uf, env, la[0], allocated);
        let right = subject(uf, env, ra[0], allocated);
        return crate::preds::same_args(uf, &[left], &[right]);
    }
    left.same(uf, right)
}

/// Flatten supplied arguments without opening an alias or inventing type vars.
pub fn application<'a>(
    uf: &mut UnionFind<'a>,
    head: Variable,
    args: &[Variable],
) -> Option<(QualifiedName<'a>, Vec<Variable>)> {
    match nash_constrain::type_::normalize_application(uf, FlatType::AppV1(head, args.to_vec())) {
        FlatType::App1(home, name, args) => Some((QualifiedName { home, name }, args)),
        FlatType::AppV1(head, supplied) => {
            let alias = instantiate::alias_application(uf, head)?;
            let mut args: Vec<_> = alias.args.into_iter().map(|(_, var)| var).collect();
            args.extend(supplied);
            Some((
                QualifiedName {
                    home: alias.home,
                    name: alias.name,
                },
                args,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use nash_ast::{Kind, ModuleName, Type};
    use nash_constrain::type_::{make_descriptor, mk_flex_var};
    use nash_region::Located;

    #[test]
    fn flexible_representation_queries_do_not_choose_a_type() {
        let mut uf = UnionFind::new();
        let variable = mk_flex_var(&mut uf);
        let mut allocated = Vec::new();
        assert_eq!(
            known(&mut uf, &KindEnv::default(), variable, &mut allocated),
            None
        );
        assert!(matches!(uf.get(variable).content, Content::FlexVar(_)));
        assert!(allocated.is_empty());
    }

    #[test]
    fn saturated_partial_alias_uses_its_arguments_for_representation() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let home = ModuleName {
            package: None,
            name: "Test",
        };
        let body = bump.alloc(Located::at_zero(Type::Var("a")));
        let mut env = KindEnv::default();
        env.types.insert(
            QualifiedName {
                home,
                name: "identity",
            },
            TypeInfo::Defined {
                kind: &Kind::Arrow(&Kind::Type, &Kind::Type),
                parameters: &["a"],
                context: &[],
                repr: None,
                alias: Some(body),
            },
        );
        let partial = uf.fresh(make_descriptor(Content::PartialAlias {
            home,
            name: "identity",
            args: vec![],
            remaining: vec!["a"],
            body,
        }));
        let mut allocated = Vec::new();
        assert_eq!(known(&mut uf, &env, partial, &mut allocated), None);
        let int = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            nash_ast::primitives::builtin_home(),
            "int",
            vec![],
        ))));
        let applied = uf.fresh(make_descriptor(Content::Structure(FlatType::AppV1(
            partial,
            vec![int],
        ))));
        assert_eq!(
            known(&mut uf, &env, applied, &mut allocated),
            Some(Repr::Const)
        );
        assert!(matches!(
            uf.get(partial).content,
            Content::PartialAlias { .. }
        ));
    }
}
