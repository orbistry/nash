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
fn write_core(out: &mut String, core: &Core<'_>, indent: usize, context: u8) {
    let precedence = match core {
        Core::Var(_) | Core::Lit(_) | Core::Error => 2,
        Core::App { args, .. } | Core::Builtin { args, .. } if args.is_empty() => 2,
        Core::App { .. }
        | Core::Builtin { .. }
        | Core::Constr { .. }
        | Core::Field { .. }
        | Core::Cast { .. }
        | Core::Delay(_)
        | Core::Force(_) => 1,
        _ => 0,
    };
    let parens = precedence < context;
    if parens {
        out.push('(');
    }
    match core {
        Core::Var(n) => name(out, *n),
        Core::Lit(c) => match c {
            nash_plutus::constant::Constant::Integer(i) => write!(out, "{i}").unwrap(),
            nash_plutus::constant::Constant::String(s) => write!(out, "{s:?}").unwrap(),
            nash_plutus::constant::Constant::Boolean(b) => write!(out, "{b}").unwrap(),
            nash_plutus::constant::Constant::Unit => out.push_str("()"),
            _ => out.push_str(&nash_plutus::pretty::constant(c)),
        },
        Core::Lam { params, body } => {
            out.push('\\');
            for (index, p) in params.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                // Parentheses separate function-valued parameters from the body arrow.
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
            write_core(out, body, indent, 0);
        }
        Core::App { func, args } => {
            write_core(out, func, indent, 1);
            arguments(out, args, indent);
        }
        Core::Let {
            binder: b,
            value,
            body,
        } => {
            out.push_str("let ");
            binder(out, *b);
            out.push_str(" = ");
            write_core(out, value, indent, 0);
            out.push_str(" in");
            newline(out, indent);
            write_core(out, body, indent, 0);
        }
        Core::LetRec { binders, body } => {
            out.push_str("letrec");
            for rec in *binders {
                newline(out, indent + 1);
                binder(out, rec.binder);
                write!(out, " [static {:?}] = ", rec.static_params).unwrap();
                write_core(
                    out,
                    &Core::Lam {
                        params: rec.params,
                        body: rec.body,
                    },
                    indent + 1,
                    0,
                );
            }
            newline(out, indent);
            out.push_str("in");
            newline(out, indent);
            write_core(out, body, indent, 0);
        }
        Core::Case {
            kind,
            scrutinee,
            branches,
            default,
        } => {
            write!(out, "case@{kind:?} ").unwrap();
            write_core(out, scrutinee, indent, 1);
            out.push_str(" of");
            for branch in *branches {
                newline(out, indent + 1);
                test(out, branch.test);
                for b in branch.binders {
                    out.push(' ');
                    name(out, b.name);
                }
                out.push_str(" -> ");
                write_core(out, branch.body, indent + 1, 0);
            }
            if let Some(body) = default {
                newline(out, indent + 1);
                out.push_str("_ -> ");
                write_core(out, body, indent + 1, 0);
            }
        }
        Core::Constr { tag, fields } => {
            write!(out, "constr {tag}").unwrap();
            arguments(out, fields, indent);
        }
        Core::Field {
            record,
            index,
            arity,
        } => {
            write!(out, "field@{index}/{arity} ").unwrap();
            write_core(out, record, indent, 2);
        }
        Core::Builtin { func, args } => {
            out.push_str(nash_plutus::pretty::builtin(*func));
            arguments(out, args, indent);
        }
        Core::Cast {
            kind,
            from,
            to,
            arg,
        } => {
            write!(out, "cast@{kind:?}[{from} => {to}] ").unwrap();
            write_core(out, arg, indent, 2);
        }
        Core::Trace { message, body } => {
            out.push_str("trace ");
            write_core(out, message, indent, 2);
            out.push_str(" in ");
            write_core(out, body, indent, 0);
        }
        Core::Error => out.push_str("error"),
        Core::Delay(body) => {
            out.push_str("delay ");
            write_core(out, body, indent, 2);
        }
        Core::Force(body) => {
            out.push_str("force ");
            write_core(out, body, indent, 2);
        }
    }
    if parens {
        out.push(')');
    }
}
fn arguments(out: &mut String, args: &[&Core<'_>], indent: usize) {
    for arg in args {
        out.push(' ');
        write_core(out, arg, indent, 2);
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
            insta::assert_snapshot!(pretty($core));
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
    fn pretty_let_app() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let x = int(&b, "x");
        let y = int(&b, "y");
        let sum = b.builtin(DefaultFunction::AddInteger, &[b.var(x.name), b.var(y.name)]);
        assert_core_snapshot!(b.let_(x, b.int(1), b.app(b.lam(&[y], sum), &[b.int(2)])));
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
            b.var(s),
            &[
                Branch {
                    test: Test::Tag(0),
                    binders: arena.alloc_slice_copy(&[a]),
                    body: b.var(a.name)
                },
                Branch {
                    test: Test::Tag(1),
                    binders: arena.alloc_slice_copy(&[n, other]),
                    body: b.var(other.name)
                },
            ],
            None
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
        assert_core_snapshot!(b.case(
            CaseKind::Data,
            b.var(data),
            &[
                Branch {
                    test: Test::DataConstr,
                    binders: arena.alloc_slice_copy(&[tag, fields]),
                    body: b.int(0)
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
            Some(b.error())
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
            &[b.var(n.name), b.var(step.name)],
        );
        assert_core_snapshot!(b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[step, n]),
                static_params: &[0],
                body: b.app(b.var(f.name), &[b.var(step.name), next])
            }],
            b.app(b.var(f.name), &[b.int(1), b.int(5)])
        ));
    }
}
