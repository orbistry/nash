use super::*;
use nash_ast::{
    ConstructorName, Ctor, CtorOpts, Kind, ModuleName, PatternCtor, PatternCtorArg, QualifiedName,
    Type as CanType, Union,
};
use nash_plutus::{arena::Arena, constant::Constant};

fn pat<'a>(arena: &'a Arena, p: Pattern<'a>) -> &'a Located<Pattern<'a>> {
    arena.alloc(Located::at_zero(p))
}
fn q(text: &str) -> QualifiedName<'_> {
    QualifiedName {
        home: ModuleName {
            package: None,
            name: "Test",
        },
        name: text,
    }
}
fn bool_union(arena: &Arena) -> &Union<'_> {
    arena.alloc(Union {
        kind: &Kind::Type,
        context: &[],
        name: arena.alloc(Located::at_zero("bool")),
        parameters: &[],
        ctors: &[],
        alternatives: 2,
        options: CtorOpts::Normal,
    })
}
fn boolean<'a>(arena: &'a Arena, b: bool) -> &'a Located<Pattern<'a>> {
    pat(
        arena,
        Pattern::Bool {
            union: bool_union(arena),
            value: b,
        },
    )
}
fn tuple<'a>(
    arena: &'a Arena,
    a: &'a Located<Pattern<'a>>,
    b: &'a Located<Pattern<'a>>,
) -> &'a Located<Pattern<'a>> {
    pat(
        arena,
        Pattern::Tuple {
            first: a,
            second: b,
            rest: &[],
        },
    )
}
fn run<'a>(
    b: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
    value: &'a Core<'a>,
    patterns: &[(&'a Located<Pattern<'a>>, i128)],
    literal_tests: &HashMap<NodeId, &'a Core<'a>>,
) -> crate::harness::Evaluated {
    let records = RecordFields::new();
    let rows = patterns
        .iter()
        .map(|(pattern, result)| MatchBranch {
            pattern,
            bindings: bindings(b, types, ty, pattern, &records).unwrap(),
            body: b.int(*result),
        })
        .collect::<Vec<_>>();
    let core = compile(
        b,
        types,
        ty,
        value,
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests,
        },
        b.int(-1),
    )
    .unwrap();
    crate::harness::eval_core(b.arena, core)
}

#[test]
fn tuple_list_matrix_preserves_row_correlation_and_wildcard_precedence() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    let int = Ty::Const(&ConstTy::Int);
    let list = Ty::Const(arena.alloc(ConstTy::List(int)));
    let ty = Ty::Term(arena.alloc(TermTy::Tuple(
        arena.alloc_slice_copy(&[list, Ty::Const(&ConstTy::Bool)]),
    )));
    let nil = pat(&arena, Pattern::List(&[]));
    let cons = pat(
        &arena,
        Pattern::Cons {
            head: pat(&arena, Pattern::Anything),
            tail: pat(&arena, Pattern::Anything),
        },
    );
    let rows = [
        (tuple(&arena, nil, boolean(&arena, true)), 1),
        (tuple(&arena, cons, boolean(&arena, false)), 2),
        (pat(&arena, Pattern::Anything), 3),
    ];
    let empty = b.lit(Constant::proto_list(
        &arena,
        nash_plutus::typ::Type::integer(&arena),
        &[],
    ));
    let full = b.lit(Constant::proto_list(
        &arena,
        nash_plutus::typ::Type::integer(&arena),
        arena.alloc_slice_copy(&[Constant::integer_from(&arena, 9)]),
    ));
    let literals = HashMap::new();
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[empty, b.lit(Constant::bool(&arena, true))]),
            &rows,
            &literals
        )
        .result,
        "(con integer 1)"
    );
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[full, b.lit(Constant::bool(&arena, false))]),
            &rows,
            &literals
        )
        .result,
        "(con integer 2)"
    );
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[empty, b.lit(Constant::bool(&arena, false))]),
            &rows,
            &literals
        )
        .result,
        "(con integer 3)"
    );
    let rows = [
        (pat(&arena, Pattern::Anything), 7),
        (tuple(&arena, nil, boolean(&arena, true)), 8),
    ];
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[empty, b.lit(Constant::bool(&arena, true))]),
            &rows,
            &literals
        )
        .result,
        "(con integer 7)"
    );
}

#[test]
fn distinct_overloaded_literals_can_overlap_and_fall_through() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    let one = pat(&arena, Pattern::Int(1));
    let two = pat(&arena, Pattern::Int(2));
    let matcher = b.lam(
        &[Binder {
            name: b.fresh("value"),
            ty: Ty::Const(&ConstTy::Int),
        }],
        b.lit(Constant::bool(&arena, true)),
    );
    let literals = HashMap::from([
        (NodeId::pattern(one), matcher),
        (NodeId::pattern(two), matcher),
    ]);
    let ty = Ty::Term(arena.alloc(TermTy::Tuple(&[
        Ty::Const(&ConstTy::Int),
        Ty::Const(&ConstTy::Bool),
    ])));
    let rows = [
        (tuple(&arena, one, boolean(&arena, true)), 1),
        (tuple(&arena, two, boolean(&arena, false)), 2),
    ];
    let value = b.trace(
        b.lit(Constant::string(&arena, "scrutinee")),
        b.constr(0, &[b.int(0), b.lit(Constant::bool(&arena, false))]),
    );
    let result = run(&b, &mut types, ty, value, &rows, &literals);
    assert_eq!(result.result, "(con integer 2)");
    assert_eq!(result.logs, ["scrutinee"]);
}

fn nominal<'a>(
    arena: &'a Arena,
    text: &'a str,
    field: &'a Located<CanType<'a>>,
) -> (&'a Union<'a>, &'a Located<CanType<'a>>) {
    let some = arena.alloc(Ctor {
        labels: None,
        name: "Some",
        index: 0,
        arity: 1,
        arguments: arena.alloc_slice_copy(&[field]),
    });
    let none = arena.alloc(Ctor {
        labels: None,
        name: "None",
        index: 1,
        arity: 0,
        arguments: &[],
    });
    let union = arena.alloc(Union {
        kind: &Kind::Type,
        context: &[],
        name: arena.alloc(Located::at_zero(text)),
        parameters: &[],
        ctors: arena.alloc_slice_copy(&[&*some, &*none]),
        alternatives: 2,
        options: CtorOpts::Normal,
    });
    let typ = arena.alloc(Located::at_zero(CanType::Named {
        reference: q(text),
        args: &[],
    }));
    (union, typ)
}
fn ctor<'a>(
    arena: &'a Arena,
    union: &'a Union<'a>,
    home: ModuleName<'a>,
    index: u16,
    args: &[&'a Located<Pattern<'a>>],
) -> &'a Located<Pattern<'a>> {
    pat(
        arena,
        Pattern::Constructor(PatternCtor {
            reference: ConstructorName {
                home,
                union: union.name.value,
                name: "Ctor",
            },
            union,
            index,
            arguments: arena.alloc_slice_fill_iter(args.iter().enumerate().map(|(i, pattern)| {
                PatternCtorArg {
                    index: i as u16,
                    typ: arena.alloc(Located::at_zero(CanType::unit())),
                    pattern,
                }
            })),
            options: CtorOpts::Normal,
            alternatives: union.alternatives,
        }),
    )
}
fn cases(core: &Core<'_>, kind: CaseKind) -> usize {
    match core {
        Core::Case {
            kind: k,
            scrutinee,
            branches,
            default,
        } => {
            usize::from(*k == kind)
                + cases(scrutinee, kind)
                + branches.iter().map(|b| cases(b.body, kind)).sum::<usize>()
                + default.map_or(0, |d| cases(d, kind))
        }
        Core::Let { value, body, .. } => cases(value, kind) + cases(body, kind),
        Core::App { func, args } => {
            cases(func, kind) + args.iter().map(|a| cases(a, kind)).sum::<usize>()
        }
        Core::Lam { body, .. } | Core::Delay(body) | Core::Force(body) => cases(body, kind),
        Core::Builtin { args, .. } | Core::Constr { fields: args, .. } => {
            args.iter().map(|a| cases(a, kind)).sum()
        }
        _ => 0,
    }
}
#[test]
fn constructors_share_head_test_before_nested_bool() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let boolean_type = arena.alloc(Located::at_zero(CanType::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "bool",
        },
        args: &[],
    }));
    let (union, typ) = nominal(&arena, "option", boolean_type);
    let unions = HashMap::from([(q("option"), union)]);
    let mut types = TypeEnv::new(&arena, &unions);
    let ty = types.ty(typ, &BTreeMap::new()).unwrap();
    let true_p = ctor(&arena, union, q("option").home, 0, &[boolean(&arena, true)]);
    let false_p = ctor(
        &arena,
        union,
        q("option").home,
        0,
        &[boolean(&arena, false)],
    );
    let any = pat(&arena, Pattern::Anything);
    let records = RecordFields::new();
    let literals = HashMap::new();
    let rows = [(true_p, 1), (false_p, 2), (any, 3)]
        .into_iter()
        .map(|(pattern, result)| MatchBranch {
            pattern,
            bindings: bindings(&b, &mut types, ty, pattern, &records).unwrap(),
            body: b.int(result),
        })
        .collect::<Vec<_>>();
    for (value, result) in [
        (b.constr(0, &[b.lit(Constant::bool(&arena, true))]), 1),
        (b.constr(0, &[b.lit(Constant::bool(&arena, false))]), 2),
        (b.constr(1, &[]), 3),
    ] {
        let core = compile(
            &b,
            &mut types,
            ty,
            value,
            &rows,
            MatchInputs {
                record_fields: &records,
                literal_tests: &literals,
            },
            b.error(),
        )
        .unwrap();
        assert_eq!(cases(core, CaseKind::Tag), 1);
        assert_eq!(cases(core, CaseKind::Bool), 1);
        assert_eq!(
            crate::harness::eval_core(&arena, core).result,
            format!("(con integer {result})")
        );
    }
}

#[test]
fn record_names_use_wire_order_and_aliases_bind_whole_subject() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    let record = pat(&arena, Pattern::Record(&["a", "z"]));
    let pattern = pat(
        &arena,
        Pattern::Alias {
            pattern: record,
            name: "whole",
        },
    );
    let records = HashMap::from([(NodeId::pattern(record), &["z", "a"][..])]);
    let literals = HashMap::new();
    let ty = Ty::Term(arena.alloc(TermTy::Record(&[
        Ty::Const(&ConstTy::Int),
        Ty::Const(&ConstTy::Int),
    ])));
    let names = bindings(&b, &mut types, ty, pattern, &records).unwrap();
    let body = b.builtin(
        DefaultFunction::SubtractInteger,
        &[b.var(names["a"].name), b.var(names["z"].name)],
    );
    let rows = [MatchBranch {
        pattern,
        bindings: names,
        body,
    }];
    let core = compile(
        &b,
        &mut types,
        ty,
        b.constr(0, &[b.int(3), b.int(10)]),
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests: &literals,
        },
        b.error(),
    )
    .unwrap();
    assert_eq!(
        crate::harness::eval_core(&arena, core).result,
        "(con integer 7)"
    );
}

#[test]
fn big_constructor_fields_and_data_builtin_shapes_decode() {
    use nash_plutus::data::PlutusData;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let field = arena.alloc(Located::at_zero(CanType::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "Int",
        },
        args: &[],
    }));
    let (union, typ) = nominal(&arena, "Option", field);
    let unions = HashMap::from([(q("Option"), union)]);
    let mut types = TypeEnv::new(&arena, &unions);
    let ty = types.ty(typ, &BTreeMap::new()).unwrap();
    let pattern = ctor(
        &arena,
        union,
        q("Option").home,
        0,
        &[pat(&arena, Pattern::Var("n"))],
    );
    let records = HashMap::new();
    let literals = HashMap::new();
    let names = bindings(&b, &mut types, ty, pattern, &records).unwrap();
    let body = b.builtin(DefaultFunction::UnIData, &[b.var(names["n"].name)]);
    let rows = [MatchBranch {
        pattern,
        bindings: names,
        body,
    }];
    let datum = PlutusData::constr(
        &arena,
        0,
        arena.alloc_slice_copy(&[PlutusData::integer_from(&arena, 17)]),
    );
    let core = compile(
        &b,
        &mut types,
        ty,
        b.lit(Constant::data(&arena, datum)),
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests: &literals,
        },
        b.int(-1),
    )
    .unwrap();
    assert_eq!(
        crate::harness::eval_core(&arena, core).result,
        "(con integer 17)"
    );
    let data_union = arena.alloc(Union {
        kind: &Kind::Type,
        context: &[],
        name: arena.alloc(Located::at_zero("Data")),
        parameters: &[],
        ctors: &[],
        alternatives: 5,
        options: CtorOpts::Normal,
    });
    let patterns = (0..5)
        .map(|i| {
            let args = vec![pat(&arena, Pattern::Anything); if i == 0 { 2 } else { 1 }];
            (
                ctor(
                    &arena,
                    data_union,
                    nash_ast::primitives::builtin_home(),
                    i,
                    &args,
                ),
                i128::from(i),
            )
        })
        .collect::<Vec<_>>();
    let values = [
        PlutusData::constr(&arena, 0, &[]),
        PlutusData::map(&arena, &[]),
        PlutusData::list(&arena, &[]),
        PlutusData::integer_from(&arena, 1),
        PlutusData::byte_string(&arena, b"a"),
    ];
    for (i, data) in values.into_iter().enumerate() {
        let result = run(
            &b,
            &mut types,
            Ty::Big(&BigTy::Data),
            b.lit(Constant::data(&arena, data)),
            &patterns,
            &literals,
        );
        assert_eq!(result.result, format!("(con integer {i})"));
    }
}

#[test]
fn constructor_argument_indices_restore_named_field_order() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int_type = arena.alloc(Located::at_zero(CanType::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "int",
        },
        args: &[],
    }));
    let c = arena.alloc(Ctor {
        labels: Some(&["z", "a"]),
        name: "Row",
        index: 0,
        arity: 2,
        arguments: arena.alloc_slice_copy(&[&*int_type, &*int_type]),
    });
    let union = arena.alloc(Union {
        kind: &Kind::Type,
        context: &[],
        name: arena.alloc(Located::at_zero("row")),
        parameters: &[],
        ctors: arena.alloc_slice_copy(&[&*c]),
        alternatives: 1,
        options: CtorOpts::Normal,
    });
    let unions = HashMap::from([(q("row"), &*union)]);
    let mut types = TypeEnv::new(&arena, &unions);
    let typ = arena.alloc(Located::at_zero(CanType::Named {
        reference: q("row"),
        args: &[],
    }));
    let ty = types.ty(typ, &BTreeMap::new()).unwrap();
    let pattern = pat(
        &arena,
        Pattern::Constructor(PatternCtor {
            reference: ConstructorName {
                home: q("row").home,
                union: "row",
                name: "Row",
            },
            union,
            index: 0,
            arguments: arena.alloc_slice_fill_iter([
                PatternCtorArg {
                    index: 1,
                    typ: int_type,
                    pattern: pat(&arena, Pattern::Var("a")),
                },
                PatternCtorArg {
                    index: 0,
                    typ: int_type,
                    pattern: pat(&arena, Pattern::Var("z")),
                },
            ]),
            options: CtorOpts::Normal,
            alternatives: 1,
        }),
    );
    let records = HashMap::new();
    let literals = HashMap::new();
    let names = bindings(&b, &mut types, ty, pattern, &records).unwrap();
    let body = b.builtin(
        DefaultFunction::SubtractInteger,
        &[b.var(names["a"].name), b.var(names["z"].name)],
    );
    let rows = [MatchBranch {
        pattern,
        bindings: names,
        body,
    }];
    let core = compile(
        &b,
        &mut types,
        ty,
        b.constr(0, &[b.int(3), b.int(10)]),
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests: &literals,
        },
        b.error(),
    )
    .unwrap();
    assert_eq!(
        crate::harness::eval_core(&arena, core).result,
        "(con integer 7)"
    );
}

#[test]
fn big_record_projects_data_list_fields_once() {
    use nash_plutus::data::PlutusData;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    let pattern = pat(&arena, Pattern::Record(&["a", "z"]));
    let records = HashMap::from([(NodeId::pattern(pattern), &["z", "a"][..])]);
    let literals = HashMap::new();
    let ty = Ty::Big(arena.alloc(BigTy::Record(&[Ty::Big(&BigTy::Int), Ty::Big(&BigTy::Int)])));
    let names = bindings(&b, &mut types, ty, pattern, &records).unwrap();
    let a = b.builtin(DefaultFunction::UnIData, &[b.var(names["a"].name)]);
    let z = b.builtin(DefaultFunction::UnIData, &[b.var(names["z"].name)]);
    let body = b.builtin(DefaultFunction::SubtractInteger, &[a, z]);
    let rows = [MatchBranch {
        pattern,
        bindings: names,
        body,
    }];
    let data = PlutusData::list(
        &arena,
        arena.alloc_slice_copy(&[
            PlutusData::integer_from(&arena, 3),
            PlutusData::integer_from(&arena, 10),
        ]),
    );
    let value = b.trace(
        b.lit(Constant::string(&arena, "once")),
        b.lit(Constant::data(&arena, data)),
    );
    let core = compile(
        &b,
        &mut types,
        ty,
        value,
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests: &literals,
        },
        b.error(),
    )
    .unwrap();
    let result = crate::harness::eval_core(&arena, core);
    assert_eq!(result.result, "(con integer 7)");
    assert_eq!(result.logs, ["once"]);
}

#[test]
fn strings_and_bytes_use_supplied_literal_matchers() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    for (pattern, ty, literal, other, eq) in [
        (
            pat(&arena, Pattern::Str("yes")),
            Ty::Const(&ConstTy::String),
            Constant::string(&arena, "yes"),
            Constant::string(&arena, "no"),
            DefaultFunction::EqualsString,
        ),
        (
            pat(&arena, Pattern::Bytes(b"yes")),
            Ty::Const(&ConstTy::Bytes),
            Constant::byte_string(&arena, b"yes"),
            Constant::byte_string(&arena, b"no"),
            DefaultFunction::EqualsByteString,
        ),
    ] {
        let arg = Binder {
            name: b.fresh("literal"),
            ty,
        };
        let matcher = b.lam(&[arg], b.builtin(eq, &[b.var(arg.name), b.lit(literal)]));
        let literals = HashMap::from([(NodeId::pattern(pattern), matcher)]);
        let rows = [(pattern, 1), (pat(&arena, Pattern::Anything), 2)];
        assert_eq!(
            run(&b, &mut types, ty, b.lit(literal), &rows, &literals).result,
            "(con integer 1)"
        );
        assert_eq!(
            run(&b, &mut types, ty, b.lit(other), &rows, &literals).result,
            "(con integer 2)"
        );
    }
}

#[test]
fn nested_constructors_share_tests_and_preserve_fallback() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let bool_type = arena.alloc(Located::at_zero(CanType::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "bool",
        },
        args: &[],
    }));
    let (inner, inner_ty) = nominal(&arena, "inner", bool_type);
    let (outer, outer_ty) = nominal(&arena, "outer", inner_ty);
    let unions = HashMap::from([(q("inner"), inner), (q("outer"), outer)]);
    let mut types = TypeEnv::new(&arena, &unions);
    let ty = types.ty(outer_ty, &BTreeMap::new()).unwrap();
    let some_true = ctor(&arena, inner, q("inner").home, 0, &[boolean(&arena, true)]);
    let none = ctor(&arena, inner, q("inner").home, 1, &[]);
    let rows = [
        (ctor(&arena, outer, q("outer").home, 0, &[some_true]), 1),
        (ctor(&arena, outer, q("outer").home, 0, &[none]), 2),
        (pat(&arena, Pattern::Anything), 3),
    ];
    let literals = HashMap::new();
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[b.constr(0, &[b.lit(Constant::bool(&arena, true))])]),
            &rows,
            &literals
        )
        .result,
        "(con integer 1)"
    );
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[b.constr(1, &[])]),
            &rows,
            &literals
        )
        .result,
        "(con integer 2)"
    );
    assert_eq!(
        run(
            &b,
            &mut types,
            ty,
            b.constr(0, &[b.constr(0, &[b.lit(Constant::bool(&arena, false))])]),
            &rows,
            &literals
        )
        .result,
        "(con integer 3)"
    );
    assert_eq!(
        run(&b, &mut types, ty, b.constr(1, &[]), &rows, &literals).result,
        "(con integer 3)"
    );
}

#[test]
fn big_list_tail_binding_remains_data_encoded() {
    use nash_plutus::data::PlutusData;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unions = HashMap::new();
    let mut types = TypeEnv::new(&arena, &unions);
    let ty = Ty::Big(&BigTy::List(Ty::Big(&BigTy::Int)));
    let pattern = pat(
        &arena,
        Pattern::Cons {
            head: pat(&arena, Pattern::Anything),
            tail: pat(&arena, Pattern::Var("tail")),
        },
    );
    let records = HashMap::new();
    let literals = HashMap::new();
    let names = bindings(&b, &mut types, ty, pattern, &records).unwrap();
    let rows = [MatchBranch {
        pattern,
        body: b.var(names["tail"].name),
        bindings: names,
    }];
    let data = PlutusData::list(
        &arena,
        arena.alloc_slice_copy(&[
            PlutusData::integer_from(&arena, 1),
            PlutusData::integer_from(&arena, 2),
        ]),
    );
    let core = compile(
        &b,
        &mut types,
        ty,
        b.lit(Constant::data(&arena, data)),
        &rows,
        MatchInputs {
            record_fields: &records,
            literal_tests: &literals,
        },
        b.error(),
    )
    .unwrap();
    assert_eq!(
        crate::harness::eval_core(&arena, core).result,
        "(con data (List [I 2]))"
    );
}

#[test]
fn duplicated_leaf_joins_preserve_bindings_and_lazy_effects() {
    for bind_default in [false, true] {
        for first in [false, true] {
            for second in [false, true] {
                let arena = Arena::new();
                let b = Builder::new(&arena);
                let unions = HashMap::new();
                let mut types = TypeEnv::new(&arena, &unions);
                let ty = Ty::Term(arena.alloc(TermTy::Tuple(&[
                    Ty::Const(&ConstTy::Bool),
                    Ty::Const(&ConstTy::Bool),
                ])));
                let records = HashMap::new();
                let literals = HashMap::new();
                let default = if bind_default {
                    pat(&arena, Pattern::Var("whole"))
                } else {
                    pat(&arena, Pattern::Anything)
                };
                let names = bindings(&b, &mut types, ty, default, &records).unwrap();
                let result = if bind_default {
                    b.field(b.var(names["whole"].name), 0, 2)
                } else {
                    b.lit(Constant::bool(&arena, false))
                };
                let rows = [
                    MatchBranch {
                        pattern: tuple(&arena, boolean(&arena, true), boolean(&arena, true)),
                        bindings: BTreeMap::new(),
                        body: b.lit(Constant::bool(&arena, true)),
                    },
                    MatchBranch {
                        pattern: default,
                        bindings: names,
                        body: b.trace(b.lit(Constant::string(&arena, "default")), result),
                    },
                ];
                let core = compile(
                    &b,
                    &mut types,
                    ty,
                    b.constr(
                        0,
                        &[
                            b.lit(Constant::bool(&arena, first)),
                            b.lit(Constant::bool(&arena, second)),
                        ],
                    ),
                    &rows,
                    MatchInputs {
                        record_fields: &records,
                        literal_tests: &literals,
                    },
                    b.error(),
                )
                .unwrap();
                let pretty = nash_ir::pretty::pretty(core);
                assert_eq!(pretty.matches("trace").count(), 1);
                let evaluated = crate::harness::eval_core(&arena, core);
                let expected = first && (second || bind_default);
                assert_eq!(
                    evaluated.result,
                    if expected {
                        "(con bool True)"
                    } else {
                        "(con bool False)"
                    }
                );
                assert_eq!(
                    evaluated.logs,
                    if first && second {
                        Vec::<String>::new()
                    } else {
                        vec!["default".to_string()]
                    }
                );
            }
        }
    }
}
