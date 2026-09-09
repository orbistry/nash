use bumpalo::Bump;
use nash_ast::{Pattern as CanPattern, primitives};
use nash_region::Located;

use crate::pattern::{
    CONS_NAME, LIST, NIL_NAME, PAIR, PAIR_NAME, TRIPLE, TRIPLE_NAME, UNIT, UNIT_NAME,
};
use crate::render::{RenderContext, pattern_to_string};
use crate::{Literal, Pattern};

fn text(pattern: Pattern<'_>) -> String {
    pattern_to_string(RenderContext::Unambiguous, pattern)
}

#[test]
fn anything() {
    insta::assert_snapshot!(text(Pattern::Anything), @"_");
}

#[test]
fn unit() {
    insta::assert_snapshot!(text(Pattern::Ctor { union: &UNIT, name: UNIT_NAME, args: &[] }), @"()");
}

#[test]
fn pair() {
    insta::assert_snapshot!(text(Pattern::Ctor { union: &PAIR, name: PAIR_NAME, args: &[Pattern::Anything; 2] }), @"( _, _ )");
}

#[test]
fn triple() {
    insta::assert_snapshot!(text(Pattern::Ctor { union: &TRIPLE, name: TRIPLE_NAME, args: &[Pattern::Anything; 3] }), @"( _, _, _ )");
}

#[test]
fn finite_list_keeps_head_order() {
    let tail = Pattern::Ctor {
        union: &LIST,
        name: NIL_NAME,
        args: &[],
    };
    let tail_args = [Pattern::Literal(Literal::Int(2)), tail];
    let args = [
        Pattern::Literal(Literal::Int(1)),
        Pattern::Ctor {
            union: &LIST,
            name: CONS_NAME,
            args: &tail_args,
        },
    ];
    insta::assert_snapshot!(text(Pattern::Ctor { union: &LIST, name: CONS_NAME, args: &args }), @"[1,2]");
}

#[test]
fn open_list_keeps_head_order() {
    let tail_args = [Pattern::Literal(Literal::Int(2)), Pattern::Anything];
    let args = [
        Pattern::Literal(Literal::Int(1)),
        Pattern::Ctor {
            union: &LIST,
            name: CONS_NAME,
            args: &tail_args,
        },
    ];
    insta::assert_snapshot!(text(Pattern::Ctor { union: &LIST, name: CONS_NAME, args: &args }), @"1 :: 2 :: _");
}

#[test]
fn cons_head_is_parenthesized() {
    let inner = [Pattern::Anything; 2];
    let args = [
        Pattern::Ctor {
            union: &LIST,
            name: CONS_NAME,
            args: &inner,
        },
        Pattern::Anything,
    ];
    insta::assert_snapshot!(text(Pattern::Ctor { union: &LIST, name: CONS_NAME, args: &args }), @"(_ :: _) :: _");
}

#[test]
fn nested_constructor_argument() {
    let inner = [Pattern::Anything];
    let args = [Pattern::Ctor {
        union: &LIST,
        name: "Just",
        args: &inner,
    }];
    insta::assert_snapshot!(text(Pattern::Ctor { union: &LIST, name: "Just", args: &args }), @"Just (Just _)");
}

#[test]
fn data_constructor() {
    let args = [Pattern::Literal(Literal::Int(0)), Pattern::Anything];
    insta::assert_snapshot!(text(Pattern::Ctor { union: &LIST, name: "Constr", args: &args }), @"Constr 0 _");
}

#[test]
fn bytes() {
    insta::assert_snapshot!(text(Pattern::Literal(Literal::Bytes(&[0, 255]))), @r###"#"00ff""###);
}

#[test]
fn simplify_record_is_irrefutable() {
    let bump = Bump::new();
    let pat = Located::at_zero(CanPattern::Record(&["x"]));
    insta::assert_snapshot!(text(crate::pattern::simplify(&bump, &pat)), @"_");
}

#[test]
fn synthetic_list_uses_builtin_type() {
    let nash_ast::Type::Named { reference, args } = LIST.ctors[1].arguments[1].value else {
        panic!("expected builtin list")
    };
    assert_eq!(reference.home, primitives::builtin_home());
    assert_eq!(reference.name, "list");
    assert_eq!(args.len(), 1);
}
