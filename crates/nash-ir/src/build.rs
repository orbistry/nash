//! Arena constructors. Use one builder per program to keep names unique.
use crate::{core::*, ty::Ty};
use nash_plutus::{arena::Arena, builtin::DefaultFunction, constant::Constant};
use std::cell::Cell;

pub struct Builder<'a> {
    pub arena: &'a Arena,
    next_unique: Cell<u32>,
}

impl<'a> Builder<'a> {
    pub fn new(arena: &'a Arena) -> Self {
        Self {
            arena,
            next_unique: Cell::new(1),
        }
    }
    pub fn fresh(&self, text: &'a str) -> Name<'a> {
        let unique = self.next_unique.get();
        self.next_unique
            .set(unique.checked_add(1).expect("Core name supply exhausted"));
        Name { text, unique }
    }
    pub fn var(&self, name: Name<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Var(name))
    }
    pub fn lit(&self, value: &'a Constant<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Lit(value))
    }
    pub fn int(&self, value: i128) -> &'a Core<'a> {
        self.lit(Constant::integer_from(self.arena, value))
    }
    pub fn lam(&self, params: &[Binder<'a>], body: &'a Core<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Lam {
            params: self.arena.alloc_slice_copy(params),
            body,
        })
    }
    pub fn app(&self, func: &'a Core<'a>, args: &[&'a Core<'a>]) -> &'a Core<'a> {
        self.arena.alloc(Core::App {
            func,
            args: self.arena.alloc_slice_copy(args),
        })
    }
    pub fn let_(
        &self,
        binder: Binder<'a>,
        value: &'a Core<'a>,
        body: &'a Core<'a>,
    ) -> &'a Core<'a> {
        self.arena.alloc(Core::Let {
            binder,
            value,
            body,
        })
    }
    pub fn let_rec(&self, binders: &[RecBinder<'a>], body: &'a Core<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::LetRec {
            binders: self.arena.alloc_slice_copy(binders),
            body,
        })
    }
    pub fn case(
        &self,
        kind: CaseKind,
        scrutinee: &'a Core<'a>,
        branches: &[Branch<'a>],
        default: Option<&'a Core<'a>>,
    ) -> &'a Core<'a> {
        self.arena.alloc(Core::Case {
            kind,
            scrutinee,
            branches: self.arena.alloc_slice_copy(branches),
            default,
        })
    }
    pub fn if_(
        &self,
        condition: &'a Core<'a>,
        yes: &'a Core<'a>,
        no: &'a Core<'a>,
    ) -> &'a Core<'a> {
        self.case(
            CaseKind::Bool,
            condition,
            &[
                Branch {
                    test: Test::True,
                    binders: &[],
                    body: yes,
                },
                Branch {
                    test: Test::False,
                    binders: &[],
                    body: no,
                },
            ],
            None,
        )
    }
    pub fn constr(&self, tag: u16, fields: &[&'a Core<'a>]) -> &'a Core<'a> {
        self.arena.alloc(Core::Constr {
            tag,
            fields: self.arena.alloc_slice_copy(fields),
        })
    }
    pub fn field(&self, record: &'a Core<'a>, index: u16, arity: u16) -> &'a Core<'a> {
        assert!(
            index < arity,
            "field index must be within constructor arity"
        );
        self.arena.alloc(Core::Field {
            record,
            index,
            arity,
        })
    }
    pub fn builtin(&self, func: DefaultFunction, args: &[&'a Core<'a>]) -> &'a Core<'a> {
        assert!(args.len() <= func.arity(), "builtin arguments exceed arity");
        self.arena.alloc(Core::Builtin {
            func,
            args: self.arena.alloc_slice_copy(args),
        })
    }
    pub fn cast(
        &self,
        kind: CastKind,
        from: Ty<'a>,
        to: Ty<'a>,
        arg: &'a Core<'a>,
    ) -> &'a Core<'a> {
        self.arena.alloc(Core::Cast {
            kind,
            from,
            to,
            arg,
        })
    }
    pub fn trace(&self, message: &'a Core<'a>, body: &'a Core<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Trace { message, body })
    }
    pub fn error(&self) -> &'a Core<'a> {
        self.arena.alloc(Core::Error)
    }
    pub fn delay(&self, body: &'a Core<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Delay(body))
    }
    pub fn force(&self, body: &'a Core<'a>) -> &'a Core<'a> {
        self.arena.alloc(Core::Force(body))
    }
}
