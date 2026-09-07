//! Port of Elm's `Type.Constrain.Expression`: turn canonical expressions
//! into constraints.
//!
//! Deviations from Elm, all because `nash-ast` has no such expressions:
//! no `Float`/`Chr` literals, no `Shader`, no `VarKernel`/`VarDebug`.

use bumpalo::Bump;
use nash_ast::{
    CaseBranch, Def as CanDef, Expr as CanExpr, FieldUpdate, FieldValue, IfBranch, NodeId,
    TypedPattern,
};
use nash_region::{Located, Region};

use crate::error::{Category, Context, Expected, MaybeName, PContext, PExpected, SubContext};
use crate::instantiate;
use crate::pattern;
use crate::type_::{self, Constraint, Definition, Type, exists, mk_flex_var, name_to_rigid};
use crate::union_find::{UnionFind, Variable};

/// Elm's `RTV`: rigid type variables introduced by enclosing type
/// annotations, shared with nested annotations.
pub type Rtv<'a> = instantiate::FreeVars<'a>;

type Exp<'a> = Expected<'a, &'a Type<'a>>;

pub fn constrain<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    expr: &Located<CanExpr<'a>>,
    expected: Exp<'a>,
) -> Constraint<'a> {
    let region = expr.region;
    let node = NodeId::expr(expr);
    match &expr.value {
        CanExpr::VarLocal(name) => Constraint::Local(region, node, name, expected),

        CanExpr::VarTopLevel(reference) => {
            Constraint::Local(region, node, reference.name, expected)
        }

        CanExpr::VarForeign {
            reference,
            annotation,
        } => Constraint::Foreign(region, node, reference.name, annotation, expected),

        CanExpr::VarMethod {
            method, annotation, ..
        } => Constraint::Foreign(region, node, method, annotation, expected),

        CanExpr::VarConstructor {
            reference,
            annotation,
            ..
        } => Constraint::Foreign(region, node, reference.name, annotation, expected),

        CanExpr::VarOperator {
            symbol, annotation, ..
        } => Constraint::Foreign(region, node, symbol, annotation, expected),

        CanExpr::Str(_) => Constraint::Foreign(
            region,
            node,
            "fromString",
            type_::literal_annotation(bump, &[type_::literal_trait("FromString")]),
            expected,
        ),
        CanExpr::Bytes(_) => Constraint::Foreign(
            region,
            node,
            "fromBytes",
            type_::literal_annotation(bump, &[type_::literal_trait("FromBytes")]),
            expected,
        ),
        CanExpr::Int(_) => Constraint::Foreign(
            region,
            node,
            "fromInt",
            type_::literal_annotation(bump, &[type_::literal_trait("FromInt")]),
            expected,
        ),

        CanExpr::List(elements) => constrain_list(bump, uf, rtv, region, elements, expected),

        CanExpr::Binop {
            symbol,
            annotation,
            left,
            right,
            ..
        } => constrain_binop(
            bump, uf, rtv, region, node, symbol, annotation, left, right, expected,
        ),

        CanExpr::Lambda { parameters, body } => {
            constrain_lambda(bump, uf, rtv, region, parameters, body, expected)
        }

        CanExpr::Call {
            function,
            arguments,
        } => constrain_call(bump, uf, rtv, region, function, arguments, expected),

        CanExpr::If {
            branches,
            final_else,
        } => constrain_if(bump, uf, rtv, region, branches, final_else, expected),

        CanExpr::Case {
            scrutinee,
            branches,
        } => constrain_case(bump, uf, rtv, region, scrutinee, branches, expected),

        CanExpr::Let { definition, body } => {
            let body_con = constrain(bump, uf, rtv, body, expected);
            constrain_def(bump, uf, rtv, definition, body_con)
        }

        CanExpr::LetRec { definitions, body } => {
            let body_con = constrain(bump, uf, rtv, body, expected);
            constrain_recursive_defs(bump, uf, rtv, definitions, body_con)
        }

        CanExpr::LetDestruct {
            pattern,
            value,
            body,
        } => {
            let body_con = constrain(bump, uf, rtv, body, expected);
            constrain_destruct(bump, uf, rtv, region, pattern, value, body_con)
        }

        CanExpr::Accessor(field) => {
            let ext_var = mk_flex_var(uf);
            let field_var = mk_flex_var(uf);
            let ext_type: &'a Type<'a> = bump.alloc(Type::VarN(ext_var));
            let field_type: &'a Type<'a> = bump.alloc(Type::VarN(field_var));
            let record_type: &'a Type<'a> = bump.alloc(Type::RecordN {
                fields: bump.alloc_slice_copy(&[(*field, field_type)]),
                ext: ext_type,
            });
            exists(
                bump,
                bump.alloc_slice_copy(&[field_var, ext_var]),
                Constraint::Equal(
                    region,
                    Category::Accessor(field),
                    bump.alloc(Type::FunN(record_type, field_type)),
                    expected,
                ),
            )
        }

        CanExpr::Access { record, field } => {
            let ext_var = mk_flex_var(uf);
            let field_var = mk_flex_var(uf);
            let ext_type: &'a Type<'a> = bump.alloc(Type::VarN(ext_var));
            let field_type: &'a Type<'a> = bump.alloc(Type::VarN(field_var));
            let record_type: &'a Type<'a> = bump.alloc(Type::RecordN {
                fields: bump.alloc_slice_copy(&[(field.value, field_type)]),
                ext: ext_type,
            });

            let context = Context::RecordAccess {
                record_region: record.region,
                maybe_name: get_access_name(record),
                field_region: field.region,
                field: field.value,
            };
            let record_con = constrain(
                bump,
                uf,
                rtv,
                record,
                Expected::FromContext(region, context, record_type),
            );

            exists(
                bump,
                bump.alloc_slice_copy(&[field_var, ext_var]),
                c_and(
                    bump,
                    vec![
                        record_con,
                        Constraint::Equal(
                            region,
                            Category::Access(field.value),
                            field_type,
                            expected,
                        ),
                    ],
                ),
            )
        }

        CanExpr::Update {
            record,
            base,
            fields,
        } => constrain_update(bump, uf, rtv, region, record, base, fields, expected),

        CanExpr::Record(fields) => constrain_record(bump, uf, rtv, region, fields, expected),

        CanExpr::Unit => {
            Constraint::Equal(region, Category::Unit, bump.alloc(Type::UnitN), expected)
        }

        CanExpr::Tuple {
            first,
            second,
            rest,
        } => constrain_tuple(bump, uf, rtv, region, first, second, rest, expected),
    }
}

// HELPERS

fn c_and<'a>(bump: &'a Bump, cons: Vec<Constraint<'a>>) -> Constraint<'a> {
    Constraint::And(bump.alloc_slice_fill_iter(cons))
}

fn singleton_header<'a>(
    bump: &'a Bump,
    name: &'a str,
    region: Region,
    tipe: &'a Type<'a>,
) -> &'a [(&'a str, Located<&'a Type<'a>>)] {
    bump.alloc_slice_copy(&[(name, Located::at(region, tipe))])
}

fn header_slice<'a>(
    bump: &'a Bump,
    headers: pattern::Header<'a>,
) -> &'a [(&'a str, Located<&'a Type<'a>>)] {
    bump.alloc_slice_fill_iter(headers)
}

fn reversed_and<'a>(bump: &'a Bump, mut rev_cons: Vec<Constraint<'a>>) -> Constraint<'a> {
    rev_cons.reverse();
    c_and(bump, rev_cons)
}

// CONSTRAIN LAMBDA

fn constrain_lambda<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    args: &[&Located<nash_ast::Pattern<'a>>],
    body: &Located<CanExpr<'a>>,
    expected: Exp<'a>,
) -> Constraint<'a> {
    let Args {
        vars,
        tipe,
        result_type,
        state,
    } = constrain_args(bump, uf, args);

    let body_con = constrain(bump, uf, rtv, body, Expected::NoExpectation(result_type));

    exists(
        bump,
        bump.alloc_slice_fill_iter(vars),
        c_and(
            bump,
            vec![
                Constraint::Let {
                    declarations: &[],
                    given: &[],
                    binder: None,
                    definitions: &[],
                    rigid_vars: &[],
                    flex_vars: bump.alloc_slice_fill_iter(state.vars),
                    header: header_slice(bump, state.headers),
                    header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
                    body_con: bump.alloc(body_con),
                },
                Constraint::Equal(region, Category::Lambda, tipe, expected),
            ],
        ),
    )
}

// CONSTRAIN CALL

fn constrain_call<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    func: &Located<CanExpr<'a>>,
    args: &[&Located<CanExpr<'a>>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let maybe_name = get_name(func);
    let func_region = func.region;

    let func_var = mk_flex_var(uf);
    let result_var = mk_flex_var(uf);
    let func_type: &'a Type<'a> = bump.alloc(Type::VarN(func_var));
    let result_type: &'a Type<'a> = bump.alloc(Type::VarN(result_var));

    let func_con = constrain(bump, uf, rtv, func, Expected::NoExpectation(func_type));

    let mut arg_vars = Vec::with_capacity(args.len());
    let mut arg_types = Vec::with_capacity(args.len());
    let mut arg_cons = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        let arg_var = mk_flex_var(uf);
        let arg_type: &'a Type<'a> = bump.alloc(Type::VarN(arg_var));
        let arg_con = constrain(
            bump,
            uf,
            rtv,
            arg,
            Expected::FromContext(region, Context::CallArg(maybe_name, index), arg_type),
        );
        arg_vars.push(arg_var);
        arg_types.push(arg_type);
        arg_cons.push(arg_con);
    }

    let arity_type = arg_types.iter().rev().fold(result_type, |acc, arg_type| {
        bump.alloc(Type::FunN(arg_type, acc))
    });
    let category = Category::CallResult(maybe_name);

    let mut vars = vec![func_var, result_var];
    vars.extend(arg_vars);

    exists(
        bump,
        bump.alloc_slice_fill_iter(vars),
        c_and(
            bump,
            vec![
                func_con,
                Constraint::Equal(
                    func_region,
                    category,
                    func_type,
                    Expected::FromContext(
                        region,
                        Context::CallArity(maybe_name, args.len()),
                        arity_type,
                    ),
                ),
                c_and(bump, arg_cons),
                Constraint::Equal(region, category, result_type, expected),
            ],
        ),
    )
}

fn get_name<'a>(func: &Located<CanExpr<'a>>) -> MaybeName<'a> {
    match &func.value {
        CanExpr::VarLocal(name) => MaybeName::FuncName(name),
        CanExpr::VarTopLevel(reference) => MaybeName::FuncName(reference.name),
        CanExpr::VarForeign { reference, .. } => MaybeName::FuncName(reference.name),
        CanExpr::VarConstructor { reference, .. } => MaybeName::CtorName(reference.name),
        CanExpr::VarOperator { symbol, .. } => MaybeName::OpName(symbol),
        _ => MaybeName::NoName,
    }
}

fn get_access_name<'a>(record: &Located<CanExpr<'a>>) -> Option<&'a str> {
    match &record.value {
        CanExpr::VarLocal(name) => Some(name),
        CanExpr::VarTopLevel(reference) => Some(reference.name),
        CanExpr::VarForeign { reference, .. } => Some(reference.name),
        _ => None,
    }
}

// CONSTRAIN BINOP

#[allow(clippy::too_many_arguments)]
fn constrain_binop<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    node: NodeId,
    op: &'a str,
    annotation: &'a nash_ast::Annotation<'a>,
    left_expr: &Located<CanExpr<'a>>,
    right_expr: &Located<CanExpr<'a>>,
    expected: Exp<'a>,
) -> Constraint<'a> {
    let left_var = mk_flex_var(uf);
    let right_var = mk_flex_var(uf);
    let answer_var = mk_flex_var(uf);
    let left_type: &'a Type<'a> = bump.alloc(Type::VarN(left_var));
    let right_type: &'a Type<'a> = bump.alloc(Type::VarN(right_var));
    let answer_type: &'a Type<'a> = bump.alloc(Type::VarN(answer_var));
    let binop_type: &'a Type<'a> = bump.alloc(Type::FunN(
        left_type,
        bump.alloc(Type::FunN(right_type, answer_type)),
    ));

    let op_con = Constraint::Foreign(
        region,
        node,
        op,
        annotation,
        Expected::NoExpectation(binop_type),
    );

    let left_con = constrain(
        bump,
        uf,
        rtv,
        left_expr,
        Expected::FromContext(region, Context::OpLeft(op), left_type),
    );
    let right_con = constrain(
        bump,
        uf,
        rtv,
        right_expr,
        Expected::FromContext(region, Context::OpRight(op), right_type),
    );

    exists(
        bump,
        bump.alloc_slice_copy(&[left_var, right_var, answer_var]),
        c_and(
            bump,
            vec![
                op_con,
                left_con,
                right_con,
                Constraint::Equal(
                    region,
                    Category::CallResult(MaybeName::OpName(op)),
                    answer_type,
                    expected,
                ),
            ],
        ),
    )
}

// CONSTRAIN LISTS

fn constrain_list<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    entries: &[&Located<CanExpr<'a>>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let entry_var = mk_flex_var(uf);
    let entry_type: &'a Type<'a> = bump.alloc(Type::VarN(entry_var));
    let list_type: &'a Type<'a> = bump.alloc(Type::AppN {
        home: type_::list_home(),
        name: "list",
        args: bump.alloc_slice_copy(&[entry_type]),
    });

    let entry_cons = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            constrain(
                bump,
                uf,
                rtv,
                entry,
                Expected::FromContext(region, Context::ListEntry(index), entry_type),
            )
        })
        .collect();

    exists(
        bump,
        bump.alloc_slice_copy(&[entry_var]),
        c_and(
            bump,
            vec![
                c_and(bump, entry_cons),
                Constraint::Equal(region, Category::List, list_type, expected),
            ],
        ),
    )
}

// CONSTRAIN IF EXPRESSIONS

fn constrain_if<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    branches: &[IfBranch<'a>],
    final_else: &Located<CanExpr<'a>>,
    expected: Exp<'a>,
) -> Constraint<'a> {
    let bool_type: &'a Type<'a> = bump.alloc(type_::bool());
    let cond_cons: Vec<Constraint<'a>> = branches
        .iter()
        .map(|branch| {
            constrain(
                bump,
                uf,
                rtv,
                branch.condition,
                Expected::FromContext(region, Context::IfCondition, bool_type),
            )
        })
        .collect();

    let exprs: Vec<&Located<CanExpr<'a>>> = branches
        .iter()
        .map(|branch| branch.then_branch)
        .chain(std::iter::once(final_else))
        .collect();

    match expected {
        Expected::FromAnnotation(name, arity, _, tipe) => {
            let branch_cons = exprs
                .iter()
                .enumerate()
                .map(|(index, branch_expr)| {
                    constrain(
                        bump,
                        uf,
                        rtv,
                        branch_expr,
                        Expected::FromAnnotation(
                            name,
                            arity,
                            SubContext::TypedIfBranch(index),
                            tipe,
                        ),
                    )
                })
                .collect();
            c_and(bump, vec![c_and(bump, cond_cons), c_and(bump, branch_cons)])
        }

        _ => {
            let branch_var = mk_flex_var(uf);
            let branch_type: &'a Type<'a> = bump.alloc(Type::VarN(branch_var));

            let branch_cons = exprs
                .iter()
                .enumerate()
                .map(|(index, branch_expr)| {
                    constrain(
                        bump,
                        uf,
                        rtv,
                        branch_expr,
                        Expected::FromContext(region, Context::IfBranch(index), branch_type),
                    )
                })
                .collect();

            exists(
                bump,
                bump.alloc_slice_copy(&[branch_var]),
                c_and(
                    bump,
                    vec![
                        c_and(bump, cond_cons),
                        c_and(bump, branch_cons),
                        Constraint::Equal(region, Category::If, branch_type, expected),
                    ],
                ),
            )
        }
    }
}

// CONSTRAIN CASE EXPRESSIONS

fn constrain_case<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    expr: &Located<CanExpr<'a>>,
    branches: &[CaseBranch<'a>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let ptrn_var = mk_flex_var(uf);
    let ptrn_type: &'a Type<'a> = bump.alloc(Type::VarN(ptrn_var));
    let expr_con = constrain(bump, uf, rtv, expr, Expected::NoExpectation(ptrn_type));

    match expected {
        Expected::FromAnnotation(name, arity, _, tipe) => {
            let mut cons = vec![expr_con];
            for (index, branch) in branches.iter().enumerate() {
                cons.push(constrain_case_branch(
                    bump,
                    uf,
                    rtv,
                    branch,
                    PExpected::FromContext(region, PContext::CaseMatch(index), ptrn_type),
                    Expected::FromAnnotation(name, arity, SubContext::TypedCaseBranch(index), tipe),
                ));
            }
            exists(bump, bump.alloc_slice_copy(&[ptrn_var]), c_and(bump, cons))
        }

        _ => {
            let branch_var = mk_flex_var(uf);
            let branch_type: &'a Type<'a> = bump.alloc(Type::VarN(branch_var));

            let mut branch_cons = Vec::with_capacity(branches.len());
            for (index, branch) in branches.iter().enumerate() {
                branch_cons.push(constrain_case_branch(
                    bump,
                    uf,
                    rtv,
                    branch,
                    PExpected::FromContext(region, PContext::CaseMatch(index), ptrn_type),
                    Expected::FromContext(region, Context::CaseBranch(index), branch_type),
                ));
            }

            exists(
                bump,
                bump.alloc_slice_copy(&[ptrn_var, branch_var]),
                c_and(
                    bump,
                    vec![
                        expr_con,
                        c_and(bump, branch_cons),
                        Constraint::Equal(region, Category::Case, branch_type, expected),
                    ],
                ),
            )
        }
    }
}

fn constrain_case_branch<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    branch: &CaseBranch<'a>,
    p_expect: PExpected<'a, &'a Type<'a>>,
    b_expect: Exp<'a>,
) -> Constraint<'a> {
    let state = pattern::add(bump, uf, branch.pattern, p_expect, pattern::empty_state());

    let body_con = constrain(bump, uf, rtv, branch.body, b_expect);

    Constraint::Let {
        declarations: &[],
        given: &[],
        binder: None,
        definitions: &[],
        rigid_vars: &[],
        flex_vars: bump.alloc_slice_fill_iter(state.vars),
        header: header_slice(bump, state.headers),
        header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
        body_con: bump.alloc(body_con),
    }
}

// CONSTRAIN RECORD

fn constrain_record<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    fields: &[FieldValue<'a>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    // Canonical record fields are name-sorted, matching Elm's `Map` order.
    let dict: Vec<(&'a str, (Variable, &'a Type<'a>, Constraint<'a>))> = fields
        .iter()
        .map(|field| {
            let var = mk_flex_var(uf);
            let tipe: &'a Type<'a> = bump.alloc(Type::VarN(var));
            let con = constrain(bump, uf, rtv, field.value, Expected::NoExpectation(tipe));
            (field.field.value, (var, tipe, con))
        })
        .collect();

    let record_type: &'a Type<'a> = bump.alloc(Type::RecordN {
        fields: bump.alloc_slice_fill_iter(dict.iter().map(|(name, (_, tipe, _))| (*name, *tipe))),
        ext: bump.alloc(Type::EmptyRecordN),
    });
    let record_con = Constraint::Equal(region, Category::Record, record_type, expected);

    let vars: Vec<Variable> = dict.iter().map(|(_, (var, _, _))| *var).collect();
    let mut cons: Vec<Constraint<'a>> = dict.into_iter().map(|(_, (_, _, con))| con).collect();
    cons.push(record_con);

    exists(bump, bump.alloc_slice_fill_iter(vars), c_and(bump, cons))
}

// CONSTRAIN RECORD UPDATE

#[allow(clippy::too_many_arguments)]
fn constrain_update<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    name: &'a str,
    expr: &Located<CanExpr<'a>>,
    fields: &'a [FieldUpdate<'a>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let ext_var = mk_flex_var(uf);

    // Canonical update fields are name-sorted, matching Elm's `Map` order.
    let field_dict: Vec<(&'a str, (Variable, &'a Type<'a>, Constraint<'a>))> = fields
        .iter()
        .map(|field| {
            let var = mk_flex_var(uf);
            let tipe: &'a Type<'a> = bump.alloc(Type::VarN(var));
            let con = constrain(
                bump,
                uf,
                rtv,
                field.value,
                Expected::FromContext(region, Context::RecordUpdateValue(field.field.value), tipe),
            );
            (field.field.value, (var, tipe, con))
        })
        .collect();

    let record_var = mk_flex_var(uf);
    let record_type: &'a Type<'a> = bump.alloc(Type::VarN(record_var));
    let fields_type: &'a Type<'a> = bump.alloc(Type::RecordN {
        fields: bump
            .alloc_slice_fill_iter(field_dict.iter().map(|(name, (_, tipe, _))| (*name, *tipe))),
        ext: bump.alloc(Type::VarN(ext_var)),
    });

    // NOTE: fieldsType is separate so that Error propagates better
    let fields_con = Constraint::Equal(
        region,
        Category::Record,
        record_type,
        Expected::NoExpectation(fields_type),
    );
    let record_con = Constraint::Equal(region, Category::Record, record_type, expected);

    let mut vars: Vec<Variable> = field_dict.iter().map(|(_, (var, _, _))| *var).collect();
    vars.push(record_var);
    vars.push(ext_var);

    let con = constrain(
        bump,
        uf,
        rtv,
        expr,
        Expected::FromContext(region, Context::RecordUpdateKeys(name, fields), record_type),
    );

    let mut cons = vec![fields_con, con];
    cons.extend(field_dict.into_iter().map(|(_, (_, _, con))| con));
    cons.push(record_con);

    exists(bump, bump.alloc_slice_fill_iter(vars), c_and(bump, cons))
}

// CONSTRAIN TUPLE

#[allow(clippy::too_many_arguments)]
fn constrain_tuple<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    a: &Located<CanExpr<'a>>,
    b: &Located<CanExpr<'a>>,
    rest: &[&Located<CanExpr<'a>>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let a_var = mk_flex_var(uf);
    let b_var = mk_flex_var(uf);
    let a_type: &'a Type<'a> = bump.alloc(Type::VarN(a_var));
    let b_type: &'a Type<'a> = bump.alloc(Type::VarN(b_var));

    let a_con = constrain(bump, uf, rtv, a, Expected::NoExpectation(a_type));
    let b_con = constrain(bump, uf, rtv, b, Expected::NoExpectation(b_type));

    let mut vars = vec![a_var, b_var];
    let mut cons = vec![a_con, b_con];
    let mut types = Vec::with_capacity(rest.len());
    for item in rest {
        let var = mk_flex_var(uf);
        let tipe: &'a Type<'a> = bump.alloc(Type::VarN(var));
        vars.push(var);
        types.push(tipe);
        cons.push(constrain(
            bump,
            uf,
            rtv,
            item,
            Expected::NoExpectation(tipe),
        ));
    }
    let tuple_type = bump.alloc(Type::TupleN(a_type, b_type, bump.alloc_slice_copy(&types)));
    cons.push(Constraint::Equal(
        region,
        Category::Tuple,
        tuple_type,
        expected,
    ));
    exists(bump, bump.alloc_slice_copy(&vars), c_and(bump, cons))
}

// CONSTRAIN DESTRUCTURES

fn constrain_destruct<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    pattern_ast: &Located<nash_ast::Pattern<'a>>,
    expr: &Located<CanExpr<'a>>,
    body_con: Constraint<'a>,
) -> Constraint<'a> {
    let pattern_var = mk_flex_var(uf);
    let pattern_type: &'a Type<'a> = bump.alloc(Type::VarN(pattern_var));

    let mut state = pattern::add(
        bump,
        uf,
        pattern_ast,
        PExpected::NoExpectation(pattern_type),
        pattern::empty_state(),
    );

    let expr_con = constrain(
        bump,
        uf,
        rtv,
        expr,
        Expected::FromContext(region, Context::Destructure, pattern_type),
    );

    let mut flex_vars = vec![pattern_var];
    flex_vars.append(&mut state.vars);

    // Elm: `CAnd (reverse (exprCon:revCons))` — exprCon runs last.
    let mut cons = state.rev_cons;
    cons.reverse();
    cons.push(expr_con);

    let binder = type_::Binder::Pattern {
        node: NodeId::pattern(pattern_ast),
        name: bump.alloc(Located::at(region, "<destructure>")),
    };
    Constraint::Let {
        declarations: &[],
        given: &[],
        binder: Some(binder),
        definitions: bump.alloc_slice_copy(&[Definition {
            site: binder,
            typ: pattern_type,
            context: None,
        }]),
        rigid_vars: &[],
        flex_vars: bump.alloc_slice_fill_iter(flex_vars),
        header: header_slice(bump, state.headers),
        header_con: bump.alloc(c_and(bump, cons)),
        body_con: bump.alloc(body_con),
    }
}

// CONSTRAIN DEF

pub fn constrain_def<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    def: &CanDef<'a>,
    body_con: Constraint<'a>,
) -> Constraint<'a> {
    constrain_definition(bump, uf, rtv, def, body_con, true)
}

/// Check a method body in the module environment without introducing its
/// name as a top-level value. Its annotation is already specialized for
/// the enclosing trait or impl by canonicalization.
pub fn constrain_method<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    def: &CanDef<'a>,
) -> Constraint<'a> {
    assert!(matches!(def, CanDef::TypedDef { .. }));
    constrain_definition(bump, uf, &Rtv::new(), def, Constraint::True, false)
}

fn constrain_definition<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    def: &CanDef<'a>,
    body_con: Constraint<'a>,
    bind_name: bool,
) -> Constraint<'a> {
    match def {
        CanDef::Def { name, args, body } => {
            let Args {
                vars,
                tipe,
                result_type,
                state,
            } = constrain_args(bump, uf, args);

            let expr_con = constrain(bump, uf, rtv, body, Expected::NoExpectation(result_type));

            Constraint::Let {
                declarations: &[],
                given: &[],
                binder: Some(type_::Binder::Named(name)),
                definitions: bump.alloc_slice_copy(&[Definition {
                    site: type_::Binder::Named(name),
                    typ: tipe,
                    context: None,
                }]),
                rigid_vars: &[],
                flex_vars: bump.alloc_slice_fill_iter(vars),
                header: if bind_name {
                    singleton_header(bump, name.value, name.region, tipe)
                } else {
                    &[]
                },
                header_con: bump.alloc(Constraint::Let {
                    declarations: &[],
                    given: &[],
                    binder: None,
                    definitions: &[],
                    rigid_vars: &[],
                    flex_vars: bump.alloc_slice_fill_iter(state.vars),
                    header: header_slice(bump, state.headers),
                    header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
                    body_con: bump.alloc(expr_con),
                }),
                body_con: bump.alloc(body_con),
            }
        }

        CanDef::TypedDef {
            name,
            free_vars,
            context,
            args,
            body,
            typ: src_result_type,
            ..
        } => {
            let (new_rigids, new_rtv) = make_rigids(bump, uf, rtv, free_vars);

            let TypedArgs {
                tipe,
                result_type,
                state,
            } = constrain_typed_args(bump, uf, &new_rtv, name.value, args, src_result_type);

            let expected = Expected::FromAnnotation(
                name.value,
                args.len(),
                SubContext::TypedBody,
                result_type,
            );
            let expr_con = constrain(bump, uf, &new_rtv, body, expected);
            let given = instantiate::from_src_context(bump, &new_rtv, context);

            Constraint::Let {
                declarations: &[],
                given,
                binder: Some(type_::Binder::Named(name)),
                definitions: bump.alloc_slice_copy(&[Definition {
                    site: type_::Binder::Named(name),
                    typ: tipe,
                    context: Some(given),
                }]),
                rigid_vars: bump.alloc_slice_fill_iter(new_rigids.iter().map(|(_, var)| *var)),
                flex_vars: &[],
                header: if bind_name {
                    singleton_header(bump, name.value, name.region, tipe)
                } else {
                    &[]
                },
                header_con: bump.alloc(Constraint::Let {
                    declarations: &[],
                    given: &[],
                    binder: None,
                    definitions: &[],
                    rigid_vars: &[],
                    flex_vars: bump.alloc_slice_fill_iter(state.vars),
                    header: header_slice(bump, state.headers),
                    header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
                    body_con: bump.alloc(expr_con),
                }),
                body_con: bump.alloc(body_con),
            }
        }
    }
}

/// Elm: `newNames = Map.difference freeVars rtv` then `nameToRigid` per
/// name in `Map` (name-sorted) order.
fn make_rigids<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    free_vars: &[&'a str],
) -> (Vec<(&'a str, Variable)>, Rtv<'a>) {
    let mut new_names: Vec<&'a str> = free_vars
        .iter()
        .filter(|name| !rtv.contains_key(*name))
        .copied()
        .collect();
    new_names.sort_unstable();

    let new_rigids: Vec<(&'a str, Variable)> = new_names
        .into_iter()
        .map(|name| (name, name_to_rigid(uf, name)))
        .collect();

    let mut new_rtv = rtv.clone();
    for (name, var) in &new_rigids {
        new_rtv.insert(name, bump.alloc(Type::VarN(*var)));
    }

    (new_rigids, new_rtv)
}

// CONSTRAIN RECURSIVE DEFS

struct Info<'a> {
    definitions: Vec<Definition<'a>>,
    vars: Vec<Variable>,
    cons: Vec<Constraint<'a>>,
    headers: pattern::Header<'a>,
}

impl<'a> Info<'a> {
    fn empty() -> Info<'a> {
        Info {
            definitions: Vec::new(),
            vars: Vec::new(),
            cons: Vec::new(),
            headers: pattern::Header::new(),
        }
    }
}

pub fn constrain_recursive_defs<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    defs: &[&CanDef<'a>],
    body_con: Constraint<'a>,
) -> Constraint<'a> {
    let mut rigid_info = Info::empty();
    let mut flex_info = Info::empty();

    for def in defs {
        match def {
            CanDef::Def { name, args, body } => {
                let Args {
                    vars: new_flex_vars,
                    tipe,
                    result_type,
                    state,
                } = constrain_args(bump, uf, args);

                let expr_con = constrain(bump, uf, rtv, body, Expected::NoExpectation(result_type));

                let def_con = Constraint::Let {
                    declarations: &[],
                    given: &[],
                    binder: None,
                    definitions: &[],
                    rigid_vars: &[],
                    flex_vars: bump.alloc_slice_fill_iter(state.vars),
                    header: header_slice(bump, state.headers),
                    header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
                    body_con: bump.alloc(expr_con),
                };

                // All recursive headers share one rank until every body has
                // been checked. Introducing an earlier header's variables in a
                // later definition's pattern scope can generalize them early.
                flex_info.vars.extend(new_flex_vars);
                flex_info.cons.push(def_con);
                flex_info.definitions.push(Definition {
                    site: type_::Binder::Named(name),
                    typ: tipe,
                    context: None,
                });
                flex_info
                    .headers
                    .insert(name.value, Located::at(name.region, tipe));
            }

            CanDef::TypedDef {
                name,
                free_vars,
                context,
                args,
                body,
                typ: src_result_type,
                ..
            } => {
                let (new_rigids, new_rtv) = make_rigids(bump, uf, rtv, free_vars);

                let TypedArgs {
                    tipe,
                    result_type,
                    state,
                } = constrain_typed_args(bump, uf, &new_rtv, name.value, args, src_result_type);

                let expr_con = constrain(
                    bump,
                    uf,
                    &new_rtv,
                    body,
                    Expected::FromAnnotation(
                        name.value,
                        args.len(),
                        SubContext::TypedBody,
                        result_type,
                    ),
                );

                let def_con = Constraint::Let {
                    declarations: &[],
                    given: &[],
                    binder: None,
                    definitions: &[],
                    rigid_vars: &[],
                    flex_vars: bump.alloc_slice_fill_iter(state.vars),
                    header: header_slice(bump, state.headers),
                    header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
                    body_con: bump.alloc(expr_con),
                };

                let given = instantiate::from_src_context(bump, &new_rtv, context);

                // Elm prepends each def's rigids: latest def first, names
                // sorted within a def.
                let mut vars: Vec<Variable> = new_rigids.iter().map(|(_, var)| *var).collect();
                vars.append(&mut rigid_info.vars);
                rigid_info.vars = vars;
                rigid_info.definitions.push(Definition {
                    site: type_::Binder::Named(name),
                    typ: tipe,
                    context: Some(given),
                });
                rigid_info.cons.push(Constraint::Let {
                    declarations: &[],
                    given,
                    binder: Some(type_::Binder::Named(name)),
                    definitions: bump.alloc_slice_copy(&[Definition {
                        site: type_::Binder::Named(name),
                        typ: tipe,
                        context: Some(given),
                    }]),
                    rigid_vars: bump.alloc_slice_fill_iter(new_rigids.iter().map(|(_, var)| *var)),
                    flex_vars: &[],
                    header: &[],
                    header_con: bump.alloc(def_con),
                    body_con: bump.alloc(Constraint::True),
                });
                rigid_info
                    .headers
                    .insert(name.value, Located::at(name.region, tipe));
            }
        }
    }

    // Elm builds the cons lists by prepending, so they end up latest-first.
    rigid_info.cons.reverse();
    flex_info.cons.reverse();

    let flex_headers = header_slice(bump, flex_info.headers);
    let flex_definitions = bump.alloc_slice_fill_iter(flex_info.definitions);
    Constraint::Let {
        declarations: bump.alloc_slice_fill_iter(rigid_info.definitions),
        given: &[],
        binder: None,
        definitions: &[],
        rigid_vars: bump.alloc_slice_fill_iter(rigid_info.vars),
        flex_vars: &[],
        header: header_slice(bump, rigid_info.headers),
        header_con: bump.alloc(Constraint::True),
        body_con: bump.alloc(Constraint::Let {
            declarations: &[],
            given: &[],
            binder: flex_definitions.first().map(|def| def.site),
            definitions: flex_definitions,
            rigid_vars: &[],
            flex_vars: bump.alloc_slice_fill_iter(flex_info.vars),
            header: flex_headers,
            header_con: bump.alloc(Constraint::Let {
                declarations: flex_definitions,
                given: &[],
                binder: None,
                definitions: &[],
                rigid_vars: &[],
                flex_vars: &[],
                header: flex_headers,
                header_con: bump.alloc(Constraint::True),
                body_con: bump.alloc(c_and(bump, flex_info.cons)),
            }),
            body_con: bump.alloc(c_and(bump, vec![c_and(bump, rigid_info.cons), body_con])),
        }),
    }
}

// CONSTRAIN ARGS

struct Args<'a> {
    vars: Vec<Variable>,
    tipe: &'a Type<'a>,
    result_type: &'a Type<'a>,
    state: pattern::State<'a>,
}

fn constrain_args<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    args: &[&Located<nash_ast::Pattern<'a>>],
) -> Args<'a> {
    args_help(bump, uf, args, pattern::empty_state())
}

fn args_help<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    args: &[&Located<nash_ast::Pattern<'a>>],
    state: pattern::State<'a>,
) -> Args<'a> {
    let mut arg_vars = Vec::with_capacity(args.len());
    let mut arg_types: Vec<&'a Type<'a>> = Vec::with_capacity(args.len());

    let mut state = state;
    for arg_pattern in args {
        let arg_var = mk_flex_var(uf);
        let arg_type: &'a Type<'a> = bump.alloc(Type::VarN(arg_var));
        state = pattern::add(
            bump,
            uf,
            arg_pattern,
            PExpected::NoExpectation(arg_type),
            state,
        );
        arg_vars.push(arg_var);
        arg_types.push(arg_type);
    }

    let result_var = mk_flex_var(uf);
    let result_type: &'a Type<'a> = bump.alloc(Type::VarN(result_var));

    let tipe = arg_types.iter().rev().fold(result_type, |acc, arg_type| {
        bump.alloc(Type::FunN(arg_type, acc))
    });

    let mut vars = arg_vars;
    vars.push(result_var);

    Args {
        vars,
        tipe,
        result_type,
        state,
    }
}

// CONSTRAIN TYPED ARGS

struct TypedArgs<'a> {
    tipe: &'a Type<'a>,
    result_type: &'a Type<'a>,
    state: pattern::State<'a>,
}

fn constrain_typed_args<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    name: &'a str,
    args: &[TypedPattern<'a>],
    src_result_type: &Located<nash_ast::Type<'a>>,
) -> TypedArgs<'a> {
    let mut state = pattern::empty_state();
    let mut arg_types: Vec<&'a Type<'a>> = Vec::with_capacity(args.len());

    for (index, arg) in args.iter().enumerate() {
        let arg_type = instantiate::from_src_type(bump, rtv, arg.typ);
        let expected = PExpected::FromContext(
            arg.pattern.region,
            PContext::TypedArg(name, index),
            arg_type,
        );
        state = pattern::add(bump, uf, arg.pattern, expected, state);
        arg_types.push(arg_type);
    }

    let result_type = instantiate::from_src_type(bump, rtv, src_result_type);

    let tipe = arg_types.iter().rev().fold(result_type, |acc, arg_type| {
        bump.alloc(Type::FunN(arg_type, acc))
    });

    TypedArgs {
        tipe,
        result_type,
        state,
    }
}

#[cfg(test)]
mod node_tests {
    use super::*;
    use nash_ast::{Annotation, ModuleName, QualifiedName};

    #[test]
    fn predicate_instantiation_preserves_order_and_captured_variables() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let captured = name_to_rigid(&mut uf, "captured");
        let rtv = Rtv::from([("captured", &*bump.alloc(Type::VarN(captured)))]);
        let names = ["z", "captured", "a"];
        let (rigids, scope) = make_rigids(&bump, &mut uf, &rtv, &names);
        assert_eq!(
            rigids.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["a", "z"]
        );
        let variables = names.map(|name| &*bump.alloc(Located::at_zero(nash_ast::Type::Var(name))));
        let context = [nash_ast::Pred::Apply {
            head: variables[0],
            args: bump.alloc_slice_copy(&variables[1..]),
        }];
        let extract = |scope: &Rtv<'_>| {
            instantiate::from_src_context(&bump, scope, &context)[0]
                .types()
                .map(|typ| {
                    let Type::VarN(variable) = typ else {
                        panic!("predicate variable")
                    };
                    *variable
                })
                .collect::<Vec<_>>()
        };
        let signature = extract(&scope);
        assert_eq!(signature, [rigids[1].1, captured, rigids[0].1]);
        let (_, other_scope) = make_rigids(&bump, &mut uf, &rtv, &names);
        let other = extract(&other_scope);
        assert_eq!(other[1], captured);
        assert_ne!(other[0], signature[0]);
        assert_ne!(other[2], signature[2]);
    }

    #[test]
    fn literals_and_patterns_keep_original_nodes_and_ordered_predicates() {
        let bump = Bump::new();
        for (expr, pattern, trait_name) in [
            (CanExpr::Int(7), nash_ast::Pattern::Int(7), "FromInt"),
            (
                CanExpr::Bytes(&[0, 255]),
                nash_ast::Pattern::Bytes(&[0, 255]),
                "FromBytes",
            ),
            (
                CanExpr::Str("nash"),
                nash_ast::Pattern::Str("nash"),
                "FromString",
            ),
        ] {
            let expr = bump.alloc(Located::at_zero(expr));
            let pattern = bump.alloc(Located::at_zero(pattern));
            let mut uf = UnionFind::new();
            let expected = bump.alloc(Type::VarN(mk_flex_var(&mut uf)));
            let constraint = constrain(
                &bump,
                &mut uf,
                &Rtv::new(),
                expr,
                Expected::NoExpectation(expected),
            );
            let Constraint::Foreign(_, node, _, annotation, _) = constraint else {
                panic!("literal scheme")
            };
            assert_eq!(node, NodeId::expr(expr));
            assert_eq!(annotation.context.len(), 1);
            assert_eq!(annotation.context[0].trait_ref().unwrap().name, trait_name);
            assert_eq!(
                annotation.context[0].trait_ref().unwrap().home.package,
                Some(nash_ast::primitives::CORE)
            );
            let state = pattern::add(
                &bump,
                &mut uf,
                pattern,
                PExpected::NoExpectation(expected),
                pattern::empty_state(),
            );
            let annotations: Vec<_> = state
                .rev_cons
                .iter()
                .filter_map(|c| match c {
                    Constraint::Foreign(_, node, _, annotation, _) => {
                        assert_eq!(*node, NodeId::pattern(pattern));
                        Some(annotation)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(annotations.len(), 1, "one evidence instance per pattern");
            assert_eq!(
                annotations[0]
                    .context
                    .iter()
                    .map(|p| p.trait_ref().unwrap().name)
                    .collect::<Vec<_>>(),
                [trait_name, "Eq"]
            );
            assert!(std::ptr::eq(
                annotations[0].context[0].args()[0],
                annotations[0].context[1].args()[0]
            ));
        }
    }

    fn uses(constraint: &Constraint<'_>, nodes: &mut Vec<NodeId>) {
        match constraint {
            Constraint::Local(_, node, ..) | Constraint::Foreign(_, node, ..) => nodes.push(*node),
            Constraint::And(constraints) => {
                for constraint in *constraints {
                    uses(constraint, nodes);
                }
            }
            Constraint::Let {
                header_con,
                body_con,
                ..
            } => {
                uses(header_con, nodes);
                uses(body_con, nodes);
            }
            _ => {}
        }
    }

    #[test]
    fn operator_and_method_uses_keep_distinct_node_identity_at_the_same_region() {
        let bump = Bump::new();
        let region = Region::zero();
        let reference = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Main",
            },
            name: "Combine",
        };
        let annotation = bump.alloc(Annotation {
            context: &[],
            free_vars: &[],
            typ: bump.alloc(Located::at(region, nash_ast::Type::Unit)),
        });
        let local = bump.alloc(Located::at(region, CanExpr::VarLocal("x")));
        let method = bump.alloc(Located::at(
            region,
            CanExpr::VarMethod {
                trait_: reference,
                method: "combine",
                annotation,
            },
        ));
        let operator = bump.alloc(Located::at(
            region,
            CanExpr::Binop {
                symbol: "+",
                operator_home: reference.home,
                reference,
                annotation,
                left: local,
                right: method,
            },
        ));
        let mut uf = UnionFind::new();
        let constraint = constrain(
            &bump,
            &mut uf,
            &Rtv::new(),
            operator,
            Expected::NoExpectation(bump.alloc(Type::UnitN)),
        );
        let mut nodes = Vec::new();
        uses(&constraint, &mut nodes);
        assert_eq!(nodes.len(), 3);
        for expression in [operator as &Located<CanExpr<'_>>, local, method] {
            assert_eq!(
                nodes
                    .iter()
                    .filter(|node| **node == NodeId::expr(expression))
                    .count(),
                1
            );
        }
    }
}
