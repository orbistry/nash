use super::*;
use nash_plutus::typ::Type;

#[derive(Clone, Copy, Debug)]
enum Strategy {
    Builtins,
    DropHead,
    Case,
    DropCase,
}

fn read<'a>(
    b: &Builder<'a>,
    list: &'a Core<'a>,
    position: usize,
    indices: &[usize],
    strategy: Strategy,
) -> &'a Core<'a> {
    let target = indices[0];
    let gap = target - position;
    if matches!(strategy, Strategy::Builtins | Strategy::DropHead) {
        let mut value = list;
        if gap >= 2 && matches!(strategy, Strategy::DropHead) {
            value = b.builtin(DefaultFunction::DropList, &[b.int(gap as i128), value]);
        } else {
            for _ in 0..gap {
                value = b.builtin(DefaultFunction::TailList, &[value]);
            }
        }
        let cursor = Binder {
            name: b.fresh("cursor"),
            ty: Ty::Erased,
        };
        let shared = if matches!(value, Core::Var(_)) {
            value
        } else {
            b.var(cursor.name)
        };
        let head = b.builtin(DefaultFunction::HeadList, &[shared]);
        let body = if indices.len() == 1 {
            head
        } else {
            b.builtin(
                DefaultFunction::AddInteger,
                &[head, read(b, shared, target, &indices[1..], strategy)],
            )
        };
        // Avoid an unused sharing let for an isolated read.
        if indices.len() == 1 {
            b.builtin(DefaultFunction::HeadList, &[value])
        } else if matches!(value, Core::Var(_)) {
            body
        } else {
            b.let_(cursor, value, body)
        }
    } else {
        let (value, position) = if gap >= 2 && matches!(strategy, Strategy::DropCase) {
            (
                b.builtin(DefaultFunction::DropList, &[b.int(gap as i128), list]),
                target,
            )
        } else {
            (list, position)
        };
        let head = Binder {
            name: b.fresh("head"),
            ty: Ty::Erased,
        };
        let tail = Binder {
            name: b.fresh("tail"),
            ty: Ty::Erased,
        };
        let body = if position < target {
            read(b, b.var(tail.name), position + 1, indices, strategy)
        } else if indices.len() == 1 {
            b.var(head.name)
        } else {
            b.builtin(
                DefaultFunction::AddInteger,
                &[
                    b.var(head.name),
                    read(b, b.var(tail.name), position + 1, &indices[1..], strategy),
                ],
            )
        };
        b.case(
            CaseKind::List,
            value,
            &[Branch {
                test: Test::Cons,
                binders: b.arena.alloc_slice_copy(&[head, tail]),
                body,
            }],
            None,
        )
    }
}

#[test]
fn list_extraction_costs() {
    let parameters: serde_json::Value =
        serde_json::from_str(include_str!("mainnet-v3-epoch-656.json")).unwrap();
    let costs: Vec<i64> = serde_json::from_value(parameters["PlutusV3"].clone()).unwrap();
    let mut report = String::from("Mainnet epoch 656, protocol 11.0, Plutus V3 cost parameters\n");
    let mut descriptions = String::new();
    for length in [8, 64] {
        for indices in [
            &[0][..],
            &[1],
            &[2],
            &[3],
            &[4],
            &[5],
            &[0, 1],
            &[0, 1, 2, 3],
            &[0, 3],
            &[2, 5, 6],
        ] {
            let expected = format!("(con integer {})", indices.iter().sum::<usize>());
            for strategy in [
                Strategy::Builtins,
                Strategy::DropHead,
                Strategy::Case,
                Strategy::DropCase,
            ] {
                let arena = Arena::new();
                let b = Builder::new(&arena);
                let items = (0..length)
                    .map(|i| Constant::integer_from(&arena, i))
                    .collect::<Vec<_>>();
                let input = b.lit(Constant::proto_list(
                    &arena,
                    Type::integer(&arena),
                    arena.alloc_slice_copy(&items),
                ));
                let xs = Binder {
                    name: b.fresh("xs"),
                    ty: Ty::Erased,
                };
                let core = b.let_(xs, input, read(&b, b.var(xs.name), 0, indices, strategy));
                let evaluated = crate::harness::eval_core_with_costs(&arena, core, Some(&costs));
                assert_eq!(
                    evaluated.result.split_whitespace().collect::<Vec<_>>(),
                    expected.split_whitespace().collect::<Vec<_>>()
                );
                report.push_str(&format!(
                    "length={length} fields={indices:?} {strategy:?}: cpu={} memory={} result={}\n",
                    evaluated.budget.cpu, evaluated.budget.mem, evaluated.result
                ));
                if length == 8 {
                    descriptions.push_str(&format!(
                        "--- {indices:?} {strategy:?}\n{}\n",
                        nash_ir::pretty::pretty(core)
                    ));
                }
            }
        }
        for use_head in [false, true] {
            let tail = format!("(con (list integer) {:?})", (1..length).collect::<Vec<_>>());
            let expected = if use_head {
                format!("(constr 0 (con integer 0) {tail})")
            } else {
                tail
            };
            for use_case in [false, true] {
                let arena = Arena::new();
                let b = Builder::new(&arena);
                let items = (0..length)
                    .map(|i| Constant::integer_from(&arena, i))
                    .collect::<Vec<_>>();
                let input = b.lit(Constant::proto_list(
                    &arena,
                    Type::integer(&arena),
                    arena.alloc_slice_copy(&items),
                ));
                let xs = Binder {
                    name: b.fresh("xs"),
                    ty: Ty::Erased,
                };
                let value = b.var(xs.name);
                let body = if use_case {
                    let h = Binder {
                        name: b.fresh("h"),
                        ty: Ty::Erased,
                    };
                    let t = Binder {
                        name: b.fresh("t"),
                        ty: Ty::Erased,
                    };
                    let body = if use_head {
                        b.constr(0, &[b.var(h.name), b.var(t.name)])
                    } else {
                        b.var(t.name)
                    };
                    b.case(
                        CaseKind::List,
                        value,
                        &[Branch {
                            test: Test::Cons,
                            binders: arena.alloc_slice_copy(&[h, t]),
                            body,
                        }],
                        None,
                    )
                } else {
                    let tail = b.builtin(DefaultFunction::TailList, &[value]);
                    if use_head {
                        b.constr(0, &[b.builtin(DefaultFunction::HeadList, &[value]), tail])
                    } else {
                        tail
                    }
                };
                let core = b.let_(xs, input, body);
                let evaluated = crate::harness::eval_core_with_costs(&arena, core, Some(&costs));
                assert_eq!(
                    evaluated.result.split_whitespace().collect::<Vec<_>>(),
                    expected.split_whitespace().collect::<Vec<_>>()
                );
                report.push_str(&format!("length={length} head_and_tail={use_head} case={use_case}: cpu={} memory={} result={}\n", evaluated.budget.cpu, evaluated.budget.mem, evaluated.result));
                if length == 8 {
                    descriptions.push_str(&format!(
                        "--- head_and_tail={use_head} case={use_case}\n{}\n",
                        nash_ir::pretty::pretty(core)
                    ));
                }
            }
        }
    }
    insta::with_settings!({description => descriptions, omit_expression => true}, { insta::assert_snapshot!(report); });
}
