//! Stable, readable Core syntax for snapshots and diagnostics.
use crate::core::*;
use std::fmt::Write;

pub fn pretty(core: &Core<'_>) -> String {
    let mut out = String::new();
    write_core(&mut out, core, 0, 0);
    out
}

fn name(out: &mut String, n: Name<'_>) {
    write!(out, "{}#{}", n.text, n.unique).unwrap();
}
fn binder(out: &mut String, b: Binder<'_>) {
    name(out, b.name);
    write!(out, " : {}", b.ty).unwrap();
}
fn newline(out: &mut String, indent: usize) {
    out.push('\n');
    for _ in 0..indent {
        out.push_str("  ");
    }
}
fn lambda(out: &mut String, params: &[Binder<'_>]) {
    out.push('\\');
    for (index, p) in params.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        // Separate function-valued parameters from the body arrow.
        let parens = matches!(p.ty, crate::ty::Ty::Term(crate::ty::TermTy::Fun(_, _)));
        if parens {
            out.push('(');
        }
        binder(out, *p);
        if parens {
            out.push(')');
        }
    }
    out.push_str(" -> ");
}

enum Task<'a> {
    Core(&'a Core<'a>, usize, u8),
    Text(&'static str),
    Newline(usize),
    Rec(&'a RecBinder<'a>, usize),
    Branch(&'a Branch<'a>, usize),
}

fn arguments<'a>(pending: &mut Vec<Task<'a>>, args: &[&'a Core<'a>], indent: usize) {
    for arg in args.iter().rev() {
        pending.push(Task::Core(arg, indent, 2));
        pending.push(Task::Text(" "));
    }
}

fn write_core<'a>(out: &mut String, core: &'a Core<'a>, indent: usize, context: u8) {
    let mut pending = vec![Task::Core(core, indent, context)];
    while let Some(task) = pending.pop() {
        let (core, indent, context) = match task {
            Task::Text(text) => {
                out.push_str(text);
                continue;
            }
            Task::Newline(indent) => {
                newline(out, indent);
                continue;
            }
            Task::Rec(rec, indent) => {
                newline(out, indent);
                binder(out, rec.binder);
                write!(out, " [static {:?}] = ", rec.static_params).unwrap();
                lambda(out, rec.params);
                pending.push(Task::Core(rec.body, indent, 0));
                continue;
            }
            Task::Branch(branch, indent) => {
                newline(out, indent);
                test(out, branch.test);
                for b in branch.binders {
                    out.push(' ');
                    name(out, b.name);
                }
                out.push_str(" -> ");
                pending.push(Task::Core(branch.body, indent, 0));
                continue;
            }
            Task::Core(core, indent, context) => (core, indent, context),
        };
        let precedence = match &core.kind {
            CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error => 2,
            CoreKind::App { args, .. } | CoreKind::Builtin { args, .. } if args.is_empty() => 2,
            CoreKind::App { .. }
            | CoreKind::Builtin { .. }
            | CoreKind::Constr { .. }
            | CoreKind::Field { .. }
            | CoreKind::Delay(_)
            | CoreKind::Force(_) => 1,
            _ => 0,
        };
        if precedence < context {
            out.push('(');
            pending.push(Task::Text(")"));
        }
        match core.kind {
            CoreKind::Var(n) => name(out, n),
            CoreKind::Lit(c) => match c {
                nash_plutus::constant::Constant::Integer(i) => write!(out, "{i}").unwrap(),
                nash_plutus::constant::Constant::String(s) => write!(out, "{s:?}").unwrap(),
                nash_plutus::constant::Constant::Boolean(b) => write!(out, "{b}").unwrap(),
                nash_plutus::constant::Constant::Unit => out.push_str("()"),
                _ => out.push_str(&nash_plutus::pretty::constant(c)),
            },
            CoreKind::Lam { params, body } => {
                lambda(out, params);
                pending.push(Task::Core(body, indent, 0));
            }
            CoreKind::App { func, args } => {
                arguments(&mut pending, args, indent);
                pending.push(Task::Core(func, indent, 1));
            }
            CoreKind::Let {
                binder: b,
                value,
                body,
            } => {
                out.push_str("let ");
                binder(out, b);
                out.push_str(" = ");
                pending.push(Task::Core(body, indent, 0));
                pending.push(Task::Newline(indent));
                pending.push(Task::Text(" in"));
                pending.push(Task::Core(value, indent, 0));
            }
            CoreKind::LetRec { binders, body } => {
                out.push_str("letrec");
                pending.push(Task::Core(body, indent, 0));
                pending.push(Task::Newline(indent));
                pending.push(Task::Text("in"));
                pending.push(Task::Newline(indent));
                pending.extend(binders.iter().rev().map(|rec| Task::Rec(rec, indent + 1)));
            }
            CoreKind::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                write!(out, "case@{kind:?} ").unwrap();
                if let Some(body) = default {
                    pending.push(Task::Core(body, indent + 1, 0));
                    pending.push(Task::Text("_ -> "));
                    pending.push(Task::Newline(indent + 1));
                }
                pending.extend(
                    branches
                        .iter()
                        .rev()
                        .map(|branch| Task::Branch(branch, indent + 1)),
                );
                pending.push(Task::Text(" of"));
                pending.push(Task::Core(scrutinee, indent, 1));
            }
            CoreKind::Constr { tag, fields } => {
                write!(out, "constr {tag}").unwrap();
                arguments(&mut pending, fields, indent);
            }
            CoreKind::Field {
                record,
                index,
                arity,
            } => {
                write!(out, "field@{index}/{arity} ").unwrap();
                pending.push(Task::Core(record, indent, 2));
            }
            CoreKind::Builtin { func, args } => {
                out.push_str(nash_plutus::pretty::builtin(func));
                arguments(&mut pending, args, indent);
            }
            CoreKind::Trace { message, body } => {
                out.push_str("trace ");
                pending.push(Task::Core(body, indent, 0));
                pending.push(Task::Text(" in "));
                pending.push(Task::Core(message, indent, 2));
            }
            CoreKind::Error => out.push_str("error"),
            CoreKind::Delay(body) | CoreKind::Force(body) => {
                out.push_str(if matches!(core.kind, CoreKind::Delay(_)) {
                    "delay "
                } else {
                    "force "
                });
                pending.push(Task::Core(body, indent, 2));
            }
        }
    }
}
fn test(out: &mut String, test: Test<'_>) {
    match test {
        Test::Tag(tag) => write!(out, "{tag}").unwrap(),
        Test::Int(i) => write!(out, "{i}").unwrap(),
        Test::Bytes(bytes) => {
            out.push('#');
            for byte in bytes {
                write!(out, "{byte:02x}").unwrap();
            }
        }
        _ => write!(out, "{test:?}").unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build::Builder, ty::*};
    use nash_plutus::{arena::Arena, builtin::DefaultFunction};

    macro_rules! assert_core_snapshot {
        ($core:expr) => {
            let core = $core;
            insta::with_settings!({description => format!("{core:#?}"), omit_expression => true}, {
                insta::assert_snapshot!(pretty(core));
            });
        };
    }

    fn int(b: &Builder<'_>, text: &'static str) -> Binder<'static> {
        Binder {
            name: Name {
                text,
                unique: b.fresh(text).unique,
            },
            ty: Ty::Const(&ConstTy::Int),
        }
    }

    #[test]
    fn deep_core_prints_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let arena = Arena::new();
                let b = Builder::new(&arena);
                let depth = 20_000;
                let mut core = b.int(1);
                for _ in 0..depth {
                    core = b.alloc(core.ty, CoreKind::Force(core));
                }
                let rendered = pretty(core);
                // Structural depth/parenthesis check; ordinary formatting is covered
                // by the snapshots without storing a huge depth-test snapshot.
                let expected = format!(
                    "{}force 1{}",
                    "force (".repeat(depth - 1),
                    ")".repeat(depth - 1)
                );
                assert_eq!(rendered, expected);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn pretty_let_app() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let x = int(&b, "x");
        let y = int(&b, "y");
        let sum = b.builtin(
            DefaultFunction::AddInteger,
            &[b.var(x.name, x.ty), b.var(y.name, y.ty)],
            Ty::Const(&crate::ty::ConstTy::Int),
        );
        assert_core_snapshot!(b.let_(
            x,
            b.int(1),
            b.app(
                b.lam(&[y], sum),
                &[b.int(2)],
                Ty::Const(&crate::ty::ConstTy::Int)
            )
        ));
    }

    #[test]
    fn pretty_case_tag() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let s = b.fresh("s");
        let a = int(&b, "a");
        let n = int(&b, "n");
        let other = int(&b, "a");
        assert_core_snapshot!(b.case(
            CaseKind::Tag,
            b.var(
                s,
                Ty::Term(&TermTy::Adt(AdtRef {
                    name: nash_ast::QualifiedName {
                        home: nash_ast::ModuleName {
                            package: None,
                            name: "Test"
                        },
                        name: "Choice"
                    },
                    args: &[],
                }))
            ),
            &[
                Branch {
                    test: Test::Tag(0),
                    binders: arena.alloc_slice_copy(&[a]),
                    body: b.var(a.name, a.ty)
                },
                Branch {
                    test: Test::Tag(1),
                    binders: arena.alloc_slice_copy(&[n, other]),
                    body: b.var(other.name, other.ty)
                },
            ],
            None,
            Ty::Const(&crate::ty::ConstTy::Int)
        ));
    }

    #[test]
    fn pretty_case_data() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let data = b.fresh("data");
        let tag = int(&b, "tag");
        let fields = Binder {
            name: b.fresh("fields"),
            ty: Ty::Const(&ConstTy::List(Ty::Big(&BigTy::Data))),
        };
        let map = Binder {
            name: b.fresh("map"),
            ty: Ty::Const(&ConstTy::List(Ty::Const(&ConstTy::Pair(
                Ty::Big(&BigTy::Data),
                Ty::Big(&BigTy::Data),
            )))),
        };
        let list = Binder {
            name: b.fresh("list"),
            ty: fields.ty,
        };
        let integer = int(&b, "integer");
        let bytes = Binder {
            name: b.fresh("bytes"),
            ty: Ty::Const(&ConstTy::Bytes),
        };
        let pair = Binder {
            name: b.fresh("pair"),
            ty: Ty::Const(arena.alloc(ConstTy::Pair(tag.ty, fields.ty))),
        };
        assert_core_snapshot!(b.case(
            CaseKind::Data,
            b.var(data, Ty::Big(&BigTy::Data)),
            &[
                Branch {
                    test: Test::DataConstr,
                    binders: arena.alloc_slice_copy(&[pair]),
                    body: b.case(
                        CaseKind::Pair,
                        b.var(pair.name, pair.ty),
                        &[Branch {
                            test: Test::Pair,
                            binders: arena.alloc_slice_copy(&[tag, fields]),
                            body: b.int(0),
                        }],
                        None,
                        Ty::Const(&crate::ty::ConstTy::Int)
                    )
                },
                Branch {
                    test: Test::DataMap,
                    binders: arena.alloc_slice_copy(&[map]),
                    body: b.int(1)
                },
                Branch {
                    test: Test::DataList,
                    binders: arena.alloc_slice_copy(&[list]),
                    body: b.int(2)
                },
                Branch {
                    test: Test::DataI,
                    binders: arena.alloc_slice_copy(&[integer]),
                    body: b.int(3)
                },
                Branch {
                    test: Test::DataB,
                    binders: arena.alloc_slice_copy(&[bytes]),
                    body: b.int(4)
                },
            ],
            Some(b.error(Ty::Const(&crate::ty::ConstTy::Int))),
            Ty::Const(&crate::ty::ConstTy::Int)
        ));
    }

    #[test]
    fn pretty_letrec_static() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let f = Binder {
            name: b.fresh("loop"),
            ty: Ty::Term(&TermTy::Fun(
                &[Ty::Const(&ConstTy::Int), Ty::Const(&ConstTy::Int)],
                Ty::Const(&ConstTy::Int),
            )),
        };
        let step = int(&b, "step");
        let n = int(&b, "n");
        let next = b.builtin(
            DefaultFunction::SubtractInteger,
            &[b.var(n.name, n.ty), b.var(step.name, step.ty)],
            Ty::Const(&crate::ty::ConstTy::Int),
        );
        assert_core_snapshot!(b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[step, n]),
                static_params: &[0],
                body: b.app(
                    b.var(f.name, f.ty),
                    &[b.var(step.name, step.ty), next],
                    Ty::Const(&crate::ty::ConstTy::Int)
                )
            }],
            b.app(
                b.var(f.name, f.ty),
                &[b.int(1), b.int(5)],
                Ty::Const(&crate::ty::ConstTy::Int)
            )
        ));
    }
}
