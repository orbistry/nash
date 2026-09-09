use bumpalo::Bump;
use nash_ast::{
    Ctor, CtorOpts, Kind, Pattern as CanPattern, PatternCtor, QualifiedName, Type, Union,
};
use nash_region::{Located, Region};

/// Elm's `Nitpick.PatternMatches.Pattern`.
#[derive(Clone, Copy)]
pub enum Pattern<'a> {
    Anything,
    Literal(Literal<'a>),
    Ctor {
        union: &'a Union<'a>,
        name: &'a str,
        args: &'a [Pattern<'a>],
    },
}

/// Elm's `Literal` without `Chr`, plus `Bytes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Literal<'a> {
    Int(i128),
    Str(&'a str),
    Bytes(&'a [u8]),
}

/// Elm's `Nitpick.PatternMatches.Error`.
#[derive(Debug)]
pub enum Error<'a> {
    Incomplete {
        region: Region,
        context: Context,
        unhandled: &'a [Pattern<'a>],
    },
    Redundant {
        case_region: Region,
        pattern_region: Region,
        /// 1-based position of the redundant branch.
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    BadArg,
    BadDestruct,
    BadCase,
}

// BUILT-IN UNIONS

pub(crate) const UNIT_NAME: &str = "#0";
pub(crate) const PAIR_NAME: &str = "#2";
pub(crate) const TRIPLE_NAME: &str = "#3";
pub(crate) const NIL_NAME: &str = "[]";
pub(crate) const CONS_NAME: &str = "::";

static VAR_A: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("a"));
static VAR_B: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("b"));
static VAR_C: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("c"));
static LIST_A: Located<Type<'static>> = Located::at(
    Region::zero(),
    Type::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "list",
        },
        args: &[&VAR_A],
    },
);

static UNIT_LOCATED: Located<&str> = Located::at(Region::zero(), UNIT_NAME);
static UNIT_CTOR: Ctor<'static> = Ctor {
    labels: None,
    name: UNIT_NAME,
    index: 0,
    arity: 0,
    arguments: &[],
};
pub(crate) static UNIT: Union<'static> = Union {
    kind: &Kind::Type,
    context: &[],
    name: &UNIT_LOCATED,
    parameters: &[],
    ctors: &[&UNIT_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static PAIR_LOCATED: Located<&str> = Located::at(Region::zero(), PAIR_NAME);
static PAIR_CTOR: Ctor<'static> = Ctor {
    labels: None,
    name: PAIR_NAME,
    index: 0,
    arity: 2,
    arguments: &[&VAR_A, &VAR_B],
};
pub(crate) static PAIR: Union<'static> = Union {
    kind: &Kind::Arrow(&Kind::Type, &Kind::Arrow(&Kind::Type, &Kind::Type)),
    context: &[],
    name: &PAIR_LOCATED,
    parameters: &["a", "b"],
    ctors: &[&PAIR_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static TRIPLE_LOCATED: Located<&str> = Located::at(Region::zero(), TRIPLE_NAME);
static TRIPLE_CTOR: Ctor<'static> = Ctor {
    labels: None,
    name: TRIPLE_NAME,
    index: 0,
    arity: 3,
    arguments: &[&VAR_A, &VAR_B, &VAR_C],
};
pub(crate) static TRIPLE: Union<'static> = Union {
    kind: &Kind::Arrow(
        &Kind::Type,
        &Kind::Arrow(&Kind::Type, &Kind::Arrow(&Kind::Type, &Kind::Type)),
    ),
    context: &[],
    name: &TRIPLE_LOCATED,
    parameters: &["a", "b", "c"],
    ctors: &[&TRIPLE_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static LIST_LOCATED: Located<&str> = Located::at(Region::zero(), "List");
static NIL_CTOR: Ctor<'static> = Ctor {
    labels: None,
    name: NIL_NAME,
    index: 0,
    arity: 0,
    arguments: &[],
};
static CONS_CTOR: Ctor<'static> = Ctor {
    labels: None,
    name: CONS_NAME,
    index: 1,
    arity: 2,
    arguments: &[&VAR_A, &LIST_A],
};
pub(crate) static LIST: Union<'static> = Union {
    kind: &Kind::Arrow(&Kind::Type, &Kind::Type),
    context: &[],
    name: &LIST_LOCATED,
    parameters: &["a"],
    ctors: &[&NIL_CTOR, &CONS_CTOR],
    alternatives: 2,
    options: CtorOpts::Normal,
};

const NIL: Pattern<'static> = Pattern::Ctor {
    union: &LIST,
    name: NIL_NAME,
    args: &[],
};

// CREATE SIMPLIFIED PATTERNS

/// Elm's `simplify`.
pub fn simplify<'a>(bump: &'a Bump, pattern: &Located<CanPattern<'a>>) -> Pattern<'a> {
    match &pattern.value {
        CanPattern::Anything | CanPattern::Var(_) | CanPattern::Record(_) => Pattern::Anything,
        CanPattern::Unit => Pattern::Ctor {
            union: &UNIT,
            name: UNIT_NAME,
            args: &[],
        },
        CanPattern::Tuple {
            first,
            second,
            rest,
        } => {
            // Canonicalization rejects tuples above three (`TupleLargerThanThree`),
            // so `rest` is empty or one element; the walk is generic anyway.
            let union: &'a Union<'a> = if rest.is_empty() { &PAIR } else { &TRIPLE };
            Pattern::Ctor {
                union,
                name: union.ctors[0].name,
                args: bump.alloc_slice_fill_with(2 + rest.len(), |i| {
                    simplify(
                        bump,
                        match i {
                            0 => first,
                            1 => second,
                            i => rest[i - 2],
                        },
                    )
                }),
            }
        }
        CanPattern::Constructor(PatternCtor {
            reference,
            union,
            arguments,
            ..
        }) => Pattern::Ctor {
            union,
            name: reference.name,
            args: bump
                .alloc_slice_fill_iter(arguments.iter().map(|arg| simplify(bump, arg.pattern))),
        },
        CanPattern::List(entries) => entries
            .iter()
            .rev()
            .fold(NIL, |tail, head| cons(bump, head, tail)),
        CanPattern::Cons { head, tail } => cons(bump, head, simplify(bump, tail)),
        CanPattern::Alias { pattern, .. } => simplify(bump, pattern),
        CanPattern::Bytes(bytes) => Pattern::Literal(Literal::Bytes(bytes)),
        CanPattern::Int(n) => Pattern::Literal(Literal::Int(*n)),
        CanPattern::Str(s) => Pattern::Literal(Literal::Str(s)),
        CanPattern::Bool { union, value } => Pattern::Ctor {
            union,
            name: if *value { "True" } else { "False" },
            args: &[],
        },
    }
}

/// Elm's `cons`.
fn cons<'a>(bump: &'a Bump, head: &Located<CanPattern<'a>>, tail: Pattern<'a>) -> Pattern<'a> {
    Pattern::Ctor {
        union: &LIST,
        name: CONS_NAME,
        args: bump.alloc_slice_copy(&[simplify(bump, head), tail]),
    }
}

// Keep driver debug output useful without dumping recursive union metadata.
impl std::fmt::Debug for Pattern<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::render::pattern_to_string(
            crate::render::RenderContext::Unambiguous,
            *self,
        ))
    }
}
