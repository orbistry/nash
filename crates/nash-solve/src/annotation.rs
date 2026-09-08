//! Port of the second half of Elm's `Type.Type`: reading a solved variable
//! back out as a canonical annotation (`toAnnotation`) or as an error type
//! (`toErrorType`), inventing pretty names for anonymous variables.

use std::collections::{BTreeMap, BTreeSet};

use bumpalo::Bump;
use nash_ast::{AliasArgument, AliasType, Annotation, FieldType, QualifiedName, Type as CanType};
use nash_constrain::error_type::ErrorType;
use nash_constrain::type_::OCCURS_MARK;
use nash_constrain::{Content, FlatType, UnionFind, Variable};
use nash_region::Located;

// TO TYPE ANNOTATION

/// Assign names across a body's schemes and uses before serializing any of
/// them. Earlier roots keep their names; later roots cannot rename a capture
/// that has already appeared in the owning scheme.
pub(crate) fn prepare_scope<'a>(bump: &'a Bump, uf: &mut UnionFind<'a>, roots: &[Variable]) {
    let mut names = BTreeMap::new();
    for root in roots {
        names = get_var_names(bump, uf, &mut BTreeSet::new(), *root, names);
        let mut state = NameState::new(&names);
        variable_to_can_type(bump, uf, &mut state, *root);
        names = get_var_names(bump, uf, &mut BTreeSet::new(), *root, names);
    }
}

pub(crate) fn to_solved_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    var: Variable,
) -> &'a Located<CanType<'a>> {
    // prepare_scope has already assigned names consistently with the owner.
    variable_to_can_type(bump, uf, &mut NameState::new(&BTreeMap::new()), var)
}

pub(crate) fn ordered_quantifiers<'a>(
    uf: &mut UnionFind<'a>,
    quantified: &[Variable],
) -> Vec<Variable> {
    let mut named = BTreeMap::new();
    for var in quantified {
        let name = match uf.get(*var).content {
            Content::FlexVar(Some(name)) | Content::RigidVar(name) => name,
            _ => continue,
        };
        named.insert(name, *var);
    }
    named.into_values().collect()
}

pub fn to_annotation<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    variable: Variable,
) -> &'a Annotation<'a> {
    to_annotation_with_context(bump, uf, variable, &[])
}

/// Render an explicit scheme context in evidence order. Predicate arguments
/// need not be reachable from the result type, so discover their names before
/// assigning names to anonymous variables anywhere in the scheme.
pub fn to_annotation_with_context<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    variable: Variable,
    context: &[crate::preds::Body<'a>],
) -> &'a Annotation<'a> {
    let mut seen = BTreeSet::new();
    let mut user_names = get_var_names(bump, uf, &mut seen, variable, BTreeMap::new());
    for predicate in context {
        for root in predicate.roots() {
            user_names = get_var_names(bump, uf, &mut seen, root, user_names);
        }
    }
    let mut state = NameState::new(&user_names);
    let tipe = variable_to_can_type(bump, uf, &mut state, variable);
    let context = bump.alloc_slice_fill_iter(context.iter().map(|body| {
        let args = bump.alloc_slice_fill_iter(
            body.args()
                .iter()
                .map(|arg| variable_to_can_type(bump, uf, &mut state, *arg)),
        );
        match body {
            crate::preds::Body::Trait {
                trait_,
                hidden: false,
                ..
            } => nash_ast::Pred::Trait {
                trait_: *trait_,
                args,
            },
            crate::preds::Body::Trait {
                trait_,
                hidden: true,
                ..
            } => nash_ast::Pred::Implied {
                trait_: *trait_,
                args,
            },
            crate::preds::Body::Apply { head, .. } => nash_ast::Pred::Apply {
                head: variable_to_can_type(bump, uf, &mut state, *head),
                args,
            },
        }
    }));
    bump.alloc(Annotation {
        context,
        free_vars: bump.alloc_slice_fill_iter(state.taken.keys().copied()),
        typ: tipe,
    })
}

/// Unlike a type's free variables, a scheme's quantifiers exclude captures.
/// Their ownership was fixed at the definition's generalization boundary.
pub(crate) fn to_scheme_annotation<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    variable: Variable,
    context: &[crate::preds::Body<'a>],
    quantified: &[Variable],
) -> &'a Annotation<'a> {
    let annotation = to_annotation_with_context(bump, uf, variable, context);
    let names: BTreeSet<_> = quantified
        .iter()
        .filter_map(|var| match uf.get(*var).content {
            Content::FlexVar(name) => name,
            Content::RigidVar(name) => Some(name),
            _ => None,
        })
        .collect();
    bump.alloc(Annotation {
        free_vars: bump.alloc_slice_fill_iter(
            annotation
                .free_vars
                .iter()
                .copied()
                .filter(|name| names.contains(name))
                .collect::<Vec<_>>(),
        ),
        context: annotation.context,
        typ: annotation.typ,
    })
}

fn variable_to_can_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    state: &mut NameState<'a>,
    variable: Variable,
) -> &'a Located<CanType<'a>> {
    if matches!(uf.get(variable).content, Content::Structure(_))
        && let Some(alias) = nash_constrain::instantiate::alias_application(uf, variable)
    {
        return bump.alloc(Located::at_zero(CanType::Alias {
            reference: QualifiedName {
                home: alias.home,
                name: alias.name,
            },
            arguments: bump.alloc_slice_fill_iter(alias.args.iter().map(|(name, var)| {
                AliasArgument {
                    name,
                    typ: variable_to_can_type(bump, uf, state, *var),
                }
            })),
            remaining: bump.alloc_slice_fill_iter(alias.remaining),
            target: AliasType::Open(alias.body),
        }));
    }
    let content = uf.get(variable).content.clone();
    match content {
        Content::PartialAlias {
            home,
            name,
            args,
            remaining,
            body,
        } => bump.alloc(Located::at_zero(CanType::Alias {
            reference: QualifiedName { home, name },
            arguments: bump.alloc_slice_fill_iter(args.iter().map(|(name, var)| AliasArgument {
                name,
                typ: variable_to_can_type(bump, uf, state, *var),
            })),
            remaining: bump.alloc_slice_fill_iter(remaining),
            target: AliasType::Open(body),
        })),
        Content::Structure(term) => term_to_can_type(bump, uf, state, term),

        Content::FlexVar(maybe_name) => {
            let name = match maybe_name {
                Some(name) => name,
                None => {
                    let name = state.fresh_var_name(bump);
                    uf.modify(variable, |desc| desc.content = Content::FlexVar(Some(name)));
                    name
                }
            };
            bump.alloc(Located::at_zero(CanType::Var(name)))
        }

        Content::RigidVar(name) => bump.alloc(Located::at_zero(CanType::Var(name))),

        Content::Alias {
            home,
            name,
            args,
            real,
            body,
        } => {
            let can_args =
                bump.alloc_slice_fill_iter(args.iter().map(|(arg_name, arg_var)| AliasArgument {
                    name: arg_name,
                    typ: variable_to_can_type(bump, uf, state, *arg_var),
                }));
            let can_type = variable_to_can_type(bump, uf, state, real);
            bump.alloc(Located::at_zero(CanType::Alias {
                reference: QualifiedName { home, name },
                arguments: can_args,
                remaining: &[],
                target: AliasType::Filled {
                    body,
                    typ: can_type,
                },
            }))
        }

        Content::Error => panic!("cannot handle Error types in variable_to_can_type"),
    }
}

fn term_to_can_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    state: &mut NameState<'a>,
    term: FlatType<'a>,
) -> &'a Located<CanType<'a>> {
    match nash_constrain::type_::normalize_application(uf, term) {
        FlatType::AppV1(head, args) => bump.alloc(Located::at_zero(CanType::App {
            head: variable_to_can_type(bump, uf, state, head),
            args: bump.alloc_slice_fill_iter(
                args.iter()
                    .map(|arg| variable_to_can_type(bump, uf, state, *arg)),
            ),
        })),
        FlatType::App1(home, name, args) => bump.alloc(Located::at_zero(CanType::Named {
            reference: QualifiedName { home, name },
            args: bump.alloc_slice_fill_iter(
                args.iter()
                    .map(|arg| variable_to_can_type(bump, uf, state, *arg)),
            ),
        })),

        FlatType::Fun1(a, b) => bump.alloc(Located::at_zero(CanType::Lambda {
            from: variable_to_can_type(bump, uf, state, a),
            to: variable_to_can_type(bump, uf, state, b),
        })),

        FlatType::Record1(fields) => bump.alloc(Located::at_zero(CanType::Record {
            fields: bump.alloc_slice_fill_iter(fields.iter().enumerate().map(
                |(index, (field, var))| FieldType {
                    index: index as u16,
                    field,
                    typ: variable_to_can_type(bump, uf, state, *var),
                },
            )),
        })),

        FlatType::Tuple1(a, b, rest) => {
            let first = variable_to_can_type(bump, uf, state, a);
            let second = variable_to_can_type(bump, uf, state, b);
            let rest = bump.alloc_slice_fill_iter(
                rest.into_iter()
                    .map(|c| variable_to_can_type(bump, uf, state, c)),
            );
            bump.alloc(Located::at_zero(CanType::Tuple {
                first,
                second,
                rest,
            }))
        }
    }
}

// TO ERROR TYPE

pub fn to_error_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    variable: Variable,
) -> &'a ErrorType<'a> {
    let mut seen = BTreeSet::new();
    let user_names = get_var_names(bump, uf, &mut seen, variable, BTreeMap::new());
    let mut state = NameState::new(&user_names);
    variable_to_error_type(bump, uf, &mut state, variable)
}

fn variable_to_error_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    state: &mut NameState<'a>,
    variable: Variable,
) -> &'a ErrorType<'a> {
    let mark = uf.get(variable).mark;
    if mark == OCCURS_MARK {
        bump.alloc(ErrorType::Infinite)
    } else {
        uf.modify(variable, |desc| desc.mark = OCCURS_MARK);
        let content = match nash_constrain::instantiate::alias_application(uf, variable) {
            Some(alias) if matches!(uf.get(variable).content, Content::Structure(_)) => {
                Content::PartialAlias {
                    home: alias.home,
                    name: alias.name,
                    args: alias.args,
                    remaining: alias.remaining,
                    body: alias.body,
                }
            }
            _ => uf.get(variable).content.clone(),
        };
        let err_type = content_to_error_type(bump, uf, state, variable, content);
        uf.modify(variable, |desc| desc.mark = mark);
        err_type
    }
}

fn content_to_error_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    state: &mut NameState<'a>,
    variable: Variable,
    content: Content<'a>,
) -> &'a ErrorType<'a> {
    match content {
        Content::PartialAlias {
            home, name, args, ..
        } => bump.alloc(ErrorType::Type {
            home,
            name,
            args: bump.alloc_slice_fill_iter(
                args.iter()
                    .map(|(_, var)| variable_to_error_type(bump, uf, state, *var)),
            ),
        }),
        Content::Structure(term) => term_to_error_type(bump, uf, state, term),

        Content::FlexVar(maybe_name) => {
            let name = match maybe_name {
                Some(name) => name,
                None => {
                    let name = state.fresh_var_name(bump);
                    uf.modify(variable, |desc| desc.content = Content::FlexVar(Some(name)));
                    name
                }
            };
            bump.alloc(ErrorType::FlexVar(name))
        }

        Content::RigidVar(name) => bump.alloc(ErrorType::RigidVar(name)),

        Content::Alias {
            home,
            name,
            args,
            real,
            ..
        } => {
            let err_args = bump.alloc_slice_fill_iter(args.iter().map(|(arg_name, arg_var)| {
                (*arg_name, variable_to_error_type(bump, uf, state, *arg_var))
            }));
            let err_type = variable_to_error_type(bump, uf, state, real);
            bump.alloc(ErrorType::Alias {
                home,
                name,
                args: err_args,
                real: err_type,
            })
        }

        Content::Error => bump.alloc(ErrorType::Error),
    }
}

fn term_to_error_type<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    state: &mut NameState<'a>,
    term: FlatType<'a>,
) -> &'a ErrorType<'a> {
    match nash_constrain::type_::normalize_application(uf, term) {
        FlatType::AppV1(head, args) => bump.alloc(ErrorType::VarApp(
            variable_to_error_type(bump, uf, state, head),
            bump.alloc_slice_fill_iter(
                args.iter()
                    .map(|arg| variable_to_error_type(bump, uf, state, *arg)),
            ),
        )),
        FlatType::App1(home, name, args) => bump.alloc(ErrorType::Type {
            home,
            name,
            args: bump.alloc_slice_fill_iter(
                args.iter()
                    .map(|arg| variable_to_error_type(bump, uf, state, *arg)),
            ),
        }),

        FlatType::Fun1(a, b) => {
            let arg = variable_to_error_type(bump, uf, state, a);
            let result = variable_to_error_type(bump, uf, state, b);
            match result {
                ErrorType::Lambda(arg1, arg2, others) => {
                    let mut rest = vec![*arg2];
                    rest.extend(others.iter().copied());
                    bump.alloc(ErrorType::Lambda(
                        arg,
                        arg1,
                        bump.alloc_slice_fill_iter(rest),
                    ))
                }
                _ => bump.alloc(ErrorType::Lambda(arg, result, &[])),
            }
        }

        FlatType::Record1(fields) => bump.alloc(ErrorType::Record {
            fields: bump.alloc_slice_fill_iter(
                fields
                    .iter()
                    .map(|(field, var)| (*field, variable_to_error_type(bump, uf, state, *var))),
            ),
        }),

        FlatType::Tuple1(a, b, rest) => {
            let first = variable_to_error_type(bump, uf, state, a);
            let second = variable_to_error_type(bump, uf, state, b);
            let third = bump.alloc_slice_fill_iter(
                rest.into_iter()
                    .map(|c| variable_to_error_type(bump, uf, state, c)),
            );
            bump.alloc(ErrorType::Tuple(first, second, third))
        }
    }
}

// MANAGE FRESH VARIABLE NAMES

struct NameState<'a> {
    taken: BTreeMap<&'a str, ()>,
    normals: usize,
}

impl<'a> NameState<'a> {
    fn new(taken: &BTreeMap<&'a str, Variable>) -> NameState<'a> {
        NameState {
            taken: taken.keys().map(|name| (*name, ())).collect(),
            normals: 0,
        }
    }

    fn fresh_var_name(&mut self, bump: &'a Bump) -> &'a str {
        let mut index = self.normals;
        loop {
            let name = from_type_variable_scheme(bump, index);
            if !self.taken.contains_key(name) {
                self.taken.insert(name, ());
                self.normals = index + 1;
                return name;
            }
            index += 1;
        }
    }
}

// FRESH VAR NAMES

/// Elm's `Name.fromTypeVariableScheme`: `a`..`z`, then `a1`, `b1`, ...
fn from_type_variable_scheme(bump: &Bump, scheme: usize) -> &str {
    let letter = (b'a' + (scheme % 26) as u8) as char;
    if scheme < 26 {
        bump.alloc_str(&letter.to_string())
    } else {
        let extra = scheme / 26;
        bump.alloc_str(&format!("{letter}{extra}"))
    }
}

/// Elm's `Name.fromTypeVariable`: append the index, separated by `_` when
/// the name already ends in a digit.
fn from_type_variable<'a>(bump: &'a Bump, name: &'a str, index: usize) -> &'a str {
    if index == 0 {
        name
    } else if name.ends_with(|c: char| c.is_ascii_digit()) {
        bump.alloc_str(&format!("{name}_{index}"))
    } else {
        bump.alloc_str(&format!("{name}{index}"))
    }
}

// GET ALL VARIABLE NAMES

// DEVIATION: Elm tracks visited variables by stamping `getVarNamesMark`
// into their descriptors, and the stamps persist across the `toAnnotation`
// calls of one solver run. Top-level values that share generalized
// variables (e.g. unannotated mutually recursive functions) then get
// annotations whose `Forall` is missing the shared variables, and Elm
// 0.19.1 crashes with "Map.!: given key is not an element in the map" when
// another module instantiates such an export. Nash tracks visits with a
// per-call `seen` set (keyed by representative) instead, so every
// annotation lists its actual free variables.
fn get_var_names<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    seen: &mut BTreeSet<Variable>,
    var: Variable,
    taken_names: BTreeMap<&'a str, Variable>,
) -> BTreeMap<&'a str, Variable> {
    if !seen.insert(uf.find(var)) {
        return taken_names;
    }
    let content = uf.get(var).content.clone();

    match content {
        Content::Error => taken_names,

        Content::FlexVar(maybe_name) => match maybe_name {
            None => taken_names,
            Some(name) => add_name(
                bump,
                uf,
                name,
                var,
                |n| Content::FlexVar(Some(n)),
                taken_names,
            ),
        },

        Content::RigidVar(name) => add_name(bump, uf, name, var, Content::RigidVar, taken_names),

        // Elm folds with `foldrM`, so children are visited right-to-left.
        Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
            args.iter().rev().fold(taken_names, |taken, (_, arg)| {
                get_var_names(bump, uf, seen, *arg, taken)
            })
        }

        Content::Structure(flat_type) => match flat_type {
            FlatType::AppV1(head, args) => {
                let taken = args.iter().rev().fold(taken_names, |taken, arg| {
                    get_var_names(bump, uf, seen, *arg, taken)
                });
                get_var_names(bump, uf, seen, head, taken)
            }
            FlatType::App1(_, _, args) => args.iter().rev().fold(taken_names, |taken, arg| {
                get_var_names(bump, uf, seen, *arg, taken)
            }),

            FlatType::Fun1(arg, body) => {
                let taken = get_var_names(bump, uf, seen, body, taken_names);
                get_var_names(bump, uf, seen, arg, taken)
            }

            FlatType::Record1(fields) => fields.values().rev().fold(taken_names, |taken, field| {
                get_var_names(bump, uf, seen, *field, taken)
            }),

            FlatType::Tuple1(a, b, rest) => {
                let taken = rest.into_iter().rev().fold(taken_names, |taken, c| {
                    get_var_names(bump, uf, seen, c, taken)
                });
                let taken = get_var_names(bump, uf, seen, b, taken);
                get_var_names(bump, uf, seen, a, taken)
            }
        },
    }
}

// REGISTER NAME / RENAME DUPLICATES

fn add_name<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    given_name: &'a str,
    var: Variable,
    make_content: impl Fn(&'a str) -> Content<'a>,
    mut taken_names: BTreeMap<&'a str, Variable>,
) -> BTreeMap<&'a str, Variable> {
    let mut index = 0;
    loop {
        let indexed_name = from_type_variable(bump, given_name, index);
        match taken_names.get(indexed_name) {
            None => {
                if indexed_name != given_name {
                    let content = make_content(indexed_name);
                    uf.modify(var, |desc| desc.content = content);
                }
                taken_names.insert(indexed_name, var);
                return taken_names;
            }
            Some(other_var) => {
                if uf.equivalent(var, *other_var) {
                    return taken_names;
                }
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use crate::preds::Body;
    use nash_constrain::type_::mk_flex_var;

    #[test]
    fn hidden_apply_roots_are_named_and_retained_in_the_scheme() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let result = mk_flex_var(&mut uf);
        let head = mk_flex_var(&mut uf);
        let arg = mk_flex_var(&mut uf);
        let context = [
            Body::Apply {
                head,
                args: vec![arg],
            },
            Body::Trait {
                trait_: nash_ast::primitives::ReprTrait::Big.qualified(),
                args: vec![arg],
                hidden: true,
            },
        ];
        let annotation = to_annotation_with_context(&bump, &mut uf, result, &context);
        assert_eq!(annotation.free_vars.len(), 3);
        assert!(annotation.context.iter().all(|pred| pred.hidden()));
        let nash_ast::Pred::Apply { head, args } = annotation.context[0] else {
            panic!("Apply survives rendering")
        };
        assert_ne!(head.value, args[0].value);
        assert_eq!(args[0].value, annotation.context[1].args()[0].value);
    }

    #[test]
    fn scheme_quantifiers_exclude_captured_predicate_roots() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let local = mk_flex_var(&mut uf);
        let captured = mk_flex_var(&mut uf);
        let context = [Body::Apply {
            head: captured,
            args: vec![local],
        }];
        let annotation = to_scheme_annotation(&bump, &mut uf, local, &context, &[local]);
        assert_eq!(annotation.free_vars.len(), 1);
        let nash_ast::Pred::Apply { head, args } = annotation.context[0] else {
            panic!("Apply survives")
        };
        let CanType::Var(local_name) = args[0].value else {
            panic!("local variable")
        };
        let CanType::Var(captured_name) = head.value else {
            panic!("captured variable")
        };
        assert_eq!(annotation.free_vars, &[local_name]);
        assert_ne!(local_name, captured_name);
    }
}
