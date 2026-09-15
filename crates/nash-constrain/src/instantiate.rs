//! Instantiate canonical types directly into union-find variables.

use std::collections::BTreeMap;

#[cfg(test)]
use bumpalo::Bump;
use nash_ast::Type as CanType;
use nash_region::Located;

use crate::{Content, FlatType, UnionFind, Variable};

/// A nominal alias application inspected without allocating inference variables.
pub struct AliasApplication<'a> {
    pub home: nash_ast::ModuleName<'a>,
    pub name: &'a str,
    pub args: Vec<(&'a str, Variable)>,
    pub remaining: Vec<&'a str>,
    pub body: &'a Located<CanType<'a>>,
}

pub fn alias_application<'a>(
    uf: &mut UnionFind<'a>,
    variable: Variable,
) -> Option<AliasApplication<'a>> {
    match uf.get(variable).content.clone() {
        Content::Alias {
            home,
            name,
            args,
            body,
            ..
        } => Some(AliasApplication {
            home,
            name,
            args,
            body,
            remaining: Vec::new(),
        }),
        Content::PartialAlias {
            home,
            name,
            args,
            body,
            remaining,
        } => Some(AliasApplication {
            home,
            name,
            args,
            body,
            remaining,
        }),
        Content::Structure(flat) => {
            let FlatType::AppV1(head, applied) = crate::type_::normalize_application(uf, flat)
            else {
                return None;
            };
            let Content::PartialAlias {
                home,
                name,
                mut args,
                remaining,
                body,
            } = uf.get(head).content.clone()
            else {
                return None;
            };
            if applied.len() > remaining.len() {
                return None;
            }
            let consumed = applied.len();
            args.extend(remaining.iter().copied().zip(applied));
            Some(AliasApplication {
                home,
                name,
                args,
                body,
                remaining: remaining[consumed..].to_vec(),
            })
        }
        _ => None,
    }
}

/// Resolve an application whose constructor is now known, retaining alias identity.
/// Missing formals remain names in the closed body, never live inference variables.
pub fn normalize_variable<'a>(
    uf: &mut UnionFind<'a>,
    variable: Variable,
    variables: &mut Vec<Variable>,
) {
    let descriptor = uf.get(variable).clone();
    let Content::Structure(flat) = descriptor.content else {
        return;
    };
    let flat = crate::type_::normalize_application(uf, flat);
    let content = match flat {
        FlatType::AppV1(head, applied) => {
            let Content::PartialAlias {
                home,
                name,
                mut args,
                remaining,
                body,
            } = uf.get(head).content.clone()
            else {
                uf.modify(variable, |desc| {
                    desc.content = Content::Structure(FlatType::AppV1(head, applied))
                });
                return;
            };
            if applied.len() > remaining.len() {
                return;
            }
            let consumed = applied.len();
            args.extend(remaining.iter().copied().zip(applied));
            if consumed < remaining.len() {
                Content::PartialAlias {
                    home,
                    name,
                    args,
                    remaining: remaining[consumed..].to_vec(),
                    body,
                }
            } else {
                let substitution = args.iter().copied().collect();
                uf.push_instantiation_scope();
                let real =
                    canonical_to_variable(uf, descriptor.rank, variables, &substitution, body);
                uf.pop_instantiation_scope();
                Content::Alias {
                    home,
                    name,
                    args,
                    body,
                    real,
                }
            }
        }
        flat => Content::Structure(flat),
    };
    uf.modify(variable, |desc| desc.content = content);
}

fn register<'a>(
    uf: &mut UnionFind<'a>,
    rank: usize,
    variables: &mut Vec<Variable>,
    content: Content<'a>,
) -> Variable {
    let mut descriptor = crate::type_::make_descriptor(content);
    descriptor.rank = rank;
    let variable = uf.fresh(descriptor);
    variables.push(variable);
    variable
}

/// Lower a canonical type with one lexical substitution, recording every allocation.
pub fn canonical_to_variable<'a>(
    uf: &mut UnionFind<'a>,
    rank: usize,
    variables: &mut Vec<Variable>,
    flex_vars: &BTreeMap<&'a str, Variable>,
    src_type: &Located<CanType<'a>>,
) -> Variable {
    let owned = uf.begin_instantiation();
    let variable = canonical_to_variable_inner(uf, rank, variables, flex_vars, src_type);
    uf.end_instantiation(owned);
    variable
}

fn canonical_to_variable_inner<'a>(
    uf: &mut UnionFind<'a>,
    rank: usize,
    variables: &mut Vec<Variable>,
    flex_vars: &BTreeMap<&'a str, Variable>,
    src_type: &Located<CanType<'a>>,
) -> Variable {
    match &src_type.value {
        CanType::Hole => register(uf, rank, variables, Content::FlexVar(None)),
        CanType::DeclaredHole(hole) => {
            if hole.kind == nash_ast::DeclaredHoleKind::Generic {
                if let Some(variable) = uf.generic_variable(hole.id) {
                    return variable;
                }
                let variable = register(uf, rank, variables, Content::FlexVar(None));
                uf.bind_generic_variable(hole.id, variable);
                return variable;
            }
            if let Some(variable) = uf.declared_variable(hole) {
                return variable;
            }
            if let Some(solution) = uf.resolve_declared(hole) {
                // An imported resolved graph is instantiated, not captured at
                // module rank. Its Generic descriptors bind by ID, never by an
                // unrelated annotation variable with the same printed name.
                return canonical_to_variable(uf, rank, variables, &BTreeMap::new(), solution);
            }
            let mut descriptor = crate::type_::make_descriptor(Content::FlexVar(None));
            descriptor.rank = crate::type_::OUTERMOST_RANK;
            let variable = uf.fresh(descriptor);
            uf.bind_declared_hole(hole, variable);
            variable
        }
        CanType::App { head, args } => {
            let head = canonical_to_variable(uf, rank, variables, flex_vars, head);
            let args = args
                .iter()
                .map(|arg| canonical_to_variable(uf, rank, variables, flex_vars, arg))
                .collect();
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::AppV1(head, args)),
            )
        }
        CanType::Function { arguments, result } => {
            let args = arguments
                .iter()
                .map(|arg| canonical_to_variable(uf, rank, variables, flex_vars, arg))
                .collect();
            let result = canonical_to_variable(uf, rank, variables, flex_vars, result);
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::Function1(args, result)),
            )
        }
        CanType::Lambda { from, to } => {
            let arg_var = canonical_to_variable(uf, rank, variables, flex_vars, from);
            let result_var = canonical_to_variable(uf, rank, variables, flex_vars, to);
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::Fun1(arg_var, result_var)),
            )
        }

        CanType::Var(name) => *flex_vars
            .get(name)
            .expect("annotations only mention their free variables"),

        CanType::Named { reference, args } => {
            let arg_vars: Vec<Variable> = args
                .iter()
                .map(|arg| canonical_to_variable(uf, rank, variables, flex_vars, arg))
                .collect();
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::App1(reference.home, reference.name, arg_vars)),
            )
        }

        CanType::Record { fields } => {
            let field_vars: BTreeMap<&'a str, Variable> = fields
                .iter()
                .map(|field| {
                    (
                        field.field,
                        canonical_to_variable(uf, rank, variables, flex_vars, field.typ),
                    )
                })
                .collect();
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::Record1(field_vars)),
            )
        }

        CanType::Tuple {
            first,
            second,
            rest,
        } => {
            let a_var = canonical_to_variable(uf, rank, variables, flex_vars, first);
            let b_var = canonical_to_variable(uf, rank, variables, flex_vars, second);
            let c_var = rest
                .iter()
                .map(|item| canonical_to_variable(uf, rank, variables, flex_vars, item))
                .collect();
            register(
                uf,
                rank,
                variables,
                Content::Structure(FlatType::Tuple1(a_var, b_var, c_var)),
            )
        }

        CanType::Alias {
            reference,
            arguments,
            remaining,
            target,
        } => {
            let arg_vars: Vec<(&'a str, Variable)> = arguments
                .iter()
                .map(|arg| {
                    (
                        arg.name,
                        canonical_to_variable(uf, rank, variables, flex_vars, arg.typ),
                    )
                })
                .collect();
            if !remaining.is_empty() {
                let nash_ast::AliasType::Open(body) = target else {
                    panic!("partial alias body must be closed")
                };
                return register(
                    uf,
                    rank,
                    variables,
                    Content::PartialAlias {
                        home: reference.home,
                        name: reference.name,
                        args: arg_vars,
                        remaining: remaining.to_vec(),
                        body,
                    },
                );
            }
            uf.push_instantiation_scope();
            let alias_var = match target {
                nash_ast::AliasType::Open(real_type) => {
                    let arg_dict: BTreeMap<&'a str, Variable> = arg_vars.iter().copied().collect();
                    canonical_to_variable(uf, rank, variables, &arg_dict, real_type)
                }
                nash_ast::AliasType::Filled { typ: real_type, .. } => {
                    canonical_to_variable(uf, rank, variables, flex_vars, real_type)
                }
            };
            uf.pop_instantiation_scope();
            register(
                uf,
                rank,
                variables,
                Content::Alias {
                    home: reference.home,
                    name: reference.name,
                    args: arg_vars,
                    real: alias_var,
                    body: match target {
                        nash_ast::AliasType::Open(body)
                        | nash_ast::AliasType::Filled { body, .. } => body,
                    },
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_ast::{AliasArgument, AliasType, ModuleName, QualifiedName};

    #[test]
    fn partial_alias_saturation_does_not_capture_caller_variables() {
        let bump = Bump::new();
        let var = |name| &*bump.alloc(Located::at_zero(CanType::Var(name)));
        let home = ModuleName {
            package: None,
            name: "Main",
        };
        let nested = bump.alloc(Located::at_zero(CanType::Alias {
            reference: QualifiedName {
                home,
                name: "Identity",
            },
            arguments: bump.alloc_slice_fill_iter([AliasArgument {
                name: "x",
                typ: var("right"),
            }]),
            remaining: &[],
            target: AliasType::Filled {
                body: var("x"),
                typ: var("right"),
            },
        }));
        let body = bump.alloc(Located::at_zero(CanType::Lambda {
            from: var("left"),
            to: nested,
        }));
        let partial = Located::at_zero(CanType::Alias {
            reference: QualifiedName {
                home,
                name: "Arrow",
            },
            arguments: bump.alloc_slice_fill_iter([AliasArgument {
                name: "left",
                typ: var("right"),
            }]),
            remaining: &["right"],
            target: AliasType::Open(body),
        });
        let mut uf = UnionFind::new();
        let caller = crate::type_::mk_flex_var(&mut uf);
        let mut variables = Vec::new();
        let head = canonical_to_variable(
            &mut uf,
            2,
            &mut variables,
            &BTreeMap::from([("right", caller)]),
            &partial,
        );
        assert_eq!(
            variables,
            [head],
            "missing formals must not allocate variables"
        );
        let unit = register(
            &mut uf,
            2,
            &mut variables,
            Content::Structure(FlatType::App1(
                nash_ast::primitives::builtin_home(),
                "unit",
                Vec::new(),
            )),
        );
        let applied = register(
            &mut uf,
            2,
            &mut variables,
            Content::Structure(FlatType::AppV1(head, vec![unit])),
        );
        normalize_variable(&mut uf, applied, &mut variables);
        let Content::Alias { args, real, .. } = uf.get(applied).content.clone() else {
            panic!("saturated alias")
        };
        assert_eq!(args, [("left", caller), ("right", unit)]);
        let Content::Structure(FlatType::Fun1(from, to)) = uf.get(real).content else {
            panic!("arrow body")
        };
        assert_eq!(from, caller);
        let Content::Alias { real, .. } = uf.get(to).content else {
            panic!("nested filled alias")
        };
        assert_eq!(real, unit);
        assert!(matches!(uf.get(caller).content, Content::FlexVar(_)));
    }
}
