//! Structural differences and hint evidence for solved error types.
use crate::{
    doc::Doc,
    localizer::Localizer,
    render_type::{self as rt, Ctx},
};
use nash_ast::{ModuleName, primitives};
use nash_constrain::error_type::{ErrorType, iterated_dealias};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Have,
    Need,
}
#[derive(Clone, Debug)]
pub enum Problem<'a> {
    AnythingToBool,
    AnythingFromOption,
    ArityMismatch(usize, usize),
    BadRigidVar(&'a str, &'a ErrorType<'a>),
    FieldTypo(&'a str, Vec<&'a str>),
    FieldsMissing(Vec<&'a str>),
    BigLittle {
        big: &'a str,
        little: &'a str,
        direction: Direction,
    },
}
#[derive(Clone, Debug)]
pub enum Status<'a> {
    Similar,
    Different(Vec<Problem<'a>>),
}
#[derive(Clone, Debug)]
pub struct Diff<'a, T> {
    pub left: T,
    pub right: T,
    pub status: Status<'a>,
}
pub fn merge<'a>(a: Status<'a>, b: Status<'a>) -> Status<'a> {
    match (a, b) {
        (Status::Similar, s) | (s, Status::Similar) => s,
        (Status::Different(mut a), Status::Different(b)) => {
            a.extend(b);
            Status::Different(a)
        }
    }
}
pub fn to_doc(l: &Localizer, ctx: Ctx, t: &ErrorType<'_>) -> Doc {
    match t {
        ErrorType::VarApp(head, args) => rt::apply(
            ctx,
            to_doc(l, Ctx::App, head),
            args.iter().map(|t| to_doc(l, Ctx::App, t)).collect(),
        ),
        ErrorType::Lambda(a, b, cs) => rt::lambda(
            ctx,
            to_doc(l, Ctx::Func, a),
            to_doc(l, Ctx::Func, b),
            cs.iter().map(|t| to_doc(l, Ctx::Func, t)).collect(),
        ),
        ErrorType::Infinite => Doc::text("∞"),
        ErrorType::Error => Doc::text("?"),
        ErrorType::FlexVar(n) | ErrorType::RigidVar(n) => rt::variable(n),
        ErrorType::Type { home, name, args } => rt::apply(
            ctx,
            l.to_doc(*home, name),
            args.iter().map(|t| to_doc(l, Ctx::App, t)).collect(),
        ),
        ErrorType::Record { fields } => rt::record(fields_to_docs(l, fields), None),
        ErrorType::Tuple(a, b, cs) => rt::tuple(
            to_doc(l, Ctx::None, a),
            to_doc(l, Ctx::None, b),
            cs.iter().map(|t| to_doc(l, Ctx::None, t)).collect(),
        ),
        ErrorType::Alias {
            home, name, args, ..
        } => alias_to_doc(l, ctx, *home, name, args),
    }
}
pub fn alias_to_doc(
    l: &Localizer,
    ctx: Ctx,
    home: ModuleName<'_>,
    name: &str,
    args: &[(&str, &ErrorType<'_>)],
) -> Doc {
    rt::apply(
        ctx,
        l.to_doc(home, name),
        args.iter().map(|(_, t)| to_doc(l, Ctx::App, t)).collect(),
    )
}
pub fn fields_to_docs(l: &Localizer, fields: &[(&str, &ErrorType<'_>)]) -> Vec<(Doc, Doc)> {
    let sorted: BTreeMap<_, _> = fields.iter().copied().collect();
    sorted
        .into_iter()
        .map(|(n, t)| (Doc::text(n), to_doc(l, Ctx::None, t)))
        .collect()
}
pub fn to_comparison<'a>(
    l: &Localizer,
    a: &'a ErrorType<'a>,
    b: &'a ErrorType<'a>,
) -> (Doc, Doc, Vec<Problem<'a>>) {
    let d = to_diff(l, Ctx::None, a, b);
    (
        d.left,
        d.right,
        match d.status {
            Status::Similar => vec![],
            Status::Different(p) => p,
        },
    )
}
fn different<'a>(left: Doc, right: Doc, problems: Vec<Problem<'a>>) -> Diff<'a, Doc> {
    Diff {
        left,
        right,
        status: Status::Different(problems),
    }
}
pub fn is_similar<T>(d: &Diff<'_, T>) -> bool {
    matches!(d.status, Status::Similar)
}
fn similar<'a>(l: &Localizer, c: Ctx, a: &ErrorType<'_>, b: &ErrorType<'_>) -> Diff<'a, Doc> {
    Diff {
        left: to_doc(l, c, a),
        right: to_doc(l, c, b),
        status: Status::Similar,
    }
}
fn sequence<'a>(diffs: impl IntoIterator<Item = Diff<'a, Doc>>) -> Diff<'a, Vec<Doc>> {
    let mut left = vec![];
    let mut right = vec![];
    let mut status = Status::Similar;
    for d in diffs {
        left.push(d.left);
        right.push(d.right);
        status = merge(status, d.status);
    }
    Diff {
        left,
        right,
        status,
    }
}
fn apply_diff<'a>(
    l: &Localizer,
    c: Ctx,
    head: Doc,
    a: &[&'a ErrorType<'a>],
    b: &[&'a ErrorType<'a>],
) -> Diff<'a, Doc> {
    let d = sequence(a.iter().zip(b).map(|(a, b)| to_diff(l, Ctx::App, a, b)));
    Diff {
        left: rt::apply(c, head.clone(), d.left),
        right: rt::apply(c, head, d.right),
        status: d.status,
    }
}
fn builtin(home: ModuleName<'_>, name: &str, want: &str) -> bool {
    home == primitives::builtin_home() && name == want
}
pub fn is_bool(h: ModuleName<'_>, n: &str) -> bool {
    builtin(h, n, "bool")
}
pub fn is_int(h: ModuleName<'_>, n: &str) -> bool {
    builtin(h, n, "int")
}
pub fn is_string(h: ModuleName<'_>, n: &str) -> bool {
    builtin(h, n, "string")
}
pub fn is_list(h: ModuleName<'_>, n: &str) -> bool {
    builtin(h, n, "list")
}
/// `option` is a library type, not a compiler primitive.
pub fn is_option(h: ModuleName<'_>, n: &str) -> bool {
    h.package == Some(primitives::CORE) && h.name == "Option" && n == "option"
}
fn named<'a>(t: &'a ErrorType<'a>) -> Option<(ModuleName<'a>, &'a str, Vec<&'a ErrorType<'a>>)> {
    match t {
        ErrorType::Type { home, name, args } => Some((*home, name, args.to_vec())),
        ErrorType::Alias {
            home, name, args, ..
        } => Some((*home, name, args.iter().map(|(_, t)| *t).collect())),
        _ => None,
    }
}
fn name_clash(l: &Localizer, c: Ctx, h: ModuleName<'_>, n: &str, args: &[&ErrorType<'_>]) -> Doc {
    let module = if let Some(p) = h.package {
        format!("{}/{}.{}", p.author, p.project, h.name)
    } else {
        h.name.into()
    };
    rt::apply(
        c,
        Doc::cat([
            Doc::text(module).yellow(),
            Doc::text(format!(".{n}")).dullyellow(),
        ]),
        args.iter().map(|t| to_doc(l, Ctx::App, t)).collect(),
    )
}
fn is_map(t: &ErrorType<'_>) -> bool {
    matches!(t,ErrorType::Type{home,name:"Map",args} if *home==primitives::builtin_home()&&args.len()==2)
}
fn is_map_little(t: &ErrorType<'_>) -> bool {
    matches!(t,ErrorType::Type{home,name:"list",args:[ErrorType::Type{home:pair_home,name:"pair",args}]} if *home==primitives::builtin_home()&&*pair_home==primitives::builtin_home()&&args.len()==2)
}
/// The producer retains transparent alias chains. The alias directly around
/// the record owns its nominal identity and representation (kinds::record_repr).
fn nominal_record<'a>(mut typ: &'a ErrorType<'a>) -> Option<(ModuleName<'a>, &'a str)> {
    while let ErrorType::Alias {
        home, name, real, ..
    } = typ
    {
        if matches!(real, ErrorType::Record { .. }) {
            return Some((*home, *name));
        }
        typ = real;
    }
    None
}
pub fn to_diff<'a>(
    l: &Localizer,
    c: Ctx,
    a: &'a ErrorType<'a>,
    b: &'a ErrorType<'a>,
) -> Diff<'a, Doc> {
    use ErrorType::*;
    match (a, b) {
        (Error, Error) | (Infinite, Infinite) => return similar(l, c, a, b),
        (RigidVar(x), RigidVar(y)) if x == y => return similar(l, c, a, b),
        (FlexVar(_), _) | (_, FlexVar(_)) => return similar(l, c, a, b),
        (Lambda(x, y, z), Lambda(u, v, w)) | (Tuple(x, y, z), Tuple(u, v, w))
            if z.len() == w.len() =>
        {
            let func = matches!(a, Lambda(..));
            let cx = if func { Ctx::Func } else { Ctx::None };
            let d = sequence(
                std::iter::once((*x, *u))
                    .chain([(*y, *v)])
                    .chain(z.iter().copied().zip(w.iter().copied()))
                    .map(|(a, b)| to_diff(l, cx, a, b)),
            );
            let render = |mut p: Vec<Doc>| {
                let a = p.remove(0);
                let b = p.remove(0);
                if func {
                    rt::lambda(c, a, b, p)
                } else {
                    rt::tuple(a, b, p)
                }
            };
            return Diff {
                left: render(d.left),
                right: render(d.right),
                status: d.status,
            };
        }
        (Lambda(_, _, z), Lambda(_, _, w)) => {
            return different(
                to_doc(l, c, a).dullyellow(),
                to_doc(l, c, b).dullyellow(),
                vec![Problem::ArityMismatch(1 + z.len(), 1 + w.len())],
            );
        }
        (Record { fields: x }, Record { fields: y }) => return diff_record(l, x, y),
        (VarApp(x, xs), VarApp(y, ys)) if xs.len() == ys.len() => {
            let head = to_diff(l, Ctx::App, x, y);
            let args = sequence(xs.iter().zip(*ys).map(|(a, b)| to_diff(l, Ctx::App, a, b)));
            return Diff {
                left: rt::apply(c, head.left, args.left),
                right: rt::apply(c, head.right, args.right),
                status: merge(head.status, args.status),
            };
        }
        _ => {}
    }
    if let (Some((h, n, x)), Some((j, m, y))) = (named(a), named(b)) {
        if h == j && n == m && x.len() == y.len() {
            return apply_diff(l, c, l.to_doc(h, n), &x, &y);
        }
        if l.to_string(h, n) == l.to_string(j, m) && (h != j || n != m) {
            return different(
                name_clash(l, c, h, n, &x),
                name_clash(l, c, j, m, &y),
                vec![],
            );
        }
        if h == primitives::builtin_home()
            && j == h
            && matches!(
                (n, m),
                ("Int", "int")
                    | ("int", "Int")
                    | ("Bytes", "bytes")
                    | ("bytes", "Bytes")
                    | ("List", "list")
                    | ("list", "List")
            )
        {
            let (big, little, direction) = if n.as_bytes()[0].is_ascii_uppercase() {
                (n, m, Direction::Have)
            } else {
                (m, n, Direction::Need)
            };
            let problem = Problem::BigLittle {
                big,
                little,
                direction,
            };
            if x.len() == y.len() {
                let args = sequence(x.iter().zip(&y).map(|(a, b)| to_diff(l, Ctx::App, a, b)));
                return Diff {
                    left: rt::apply(c, l.to_doc(h, n).dullyellow(), args.left),
                    right: rt::apply(c, l.to_doc(j, m).dullyellow(), args.right),
                    status: merge(Status::Different(vec![problem]), args.status),
                };
            }
            return different(
                to_doc(l, c, a).dullyellow(),
                to_doc(l, c, b).dullyellow(),
                vec![problem],
            );
        }
    }
    if (is_map(a) && is_map_little(b)) || (is_map(b) && is_map_little(a)) {
        return different(
            to_doc(l, c, a).dullyellow(),
            to_doc(l, c, b).dullyellow(),
            vec![Problem::BigLittle {
                big: "Map",
                little: "list (pair 'k 'v)",
                direction: if is_map(a) {
                    Direction::Have
                } else {
                    Direction::Need
                },
            }],
        );
    }
    if let Type {
        home,
        name,
        args: [inner],
    } = a
        && is_option(*home, name)
        && is_similar(&to_diff(l, c, inner, b))
    {
        return different(
            rt::apply(
                c,
                l.to_doc(*home, name).dullyellow(),
                vec![to_doc(l, Ctx::App, inner)],
            ),
            to_doc(l, c, b),
            vec![Problem::AnythingFromOption],
        );
    }
    if let Type {
        home,
        name,
        args: [inner],
    } = b
        && is_list(*home, name)
        && is_similar(&to_diff(l, c, a, inner))
    {
        return different(
            to_doc(l, c, a),
            rt::apply(
                c,
                l.to_doc(*home, name).dullyellow(),
                vec![to_doc(l, Ctx::App, inner)],
            ),
            vec![],
        );
    }
    if (matches!(a, Alias { .. }) || matches!(b, Alias { .. }))
        && let (Record { fields: x }, Record { fields: y }) =
            (iterated_dealias(a), iterated_dealias(b))
    {
        let mut d = diff_record(l, x, y);
        let left_identity = nominal_record(a);
        let right_identity = nominal_record(b);
        if left_identity != right_identity {
            let mut problems = vec![];
            if let (Some((_, left)), Some((_, right))) = (left_identity, right_identity) {
                let left_big = left.chars().next().is_some_and(char::is_uppercase);
                let right_big = right.chars().next().is_some_and(char::is_uppercase);
                let left_fields: std::collections::BTreeSet<_> =
                    x.iter().map(|(name, _)| *name).collect();
                let right_fields: std::collections::BTreeSet<_> =
                    y.iter().map(|(name, _)| *name).collect();
                if left_big != right_big && left_fields == right_fields {
                    let (big, little, direction) = if left_big {
                        (left, right, Direction::Have)
                    } else {
                        (right, left, Direction::Need)
                    };
                    problems.push(Problem::BigLittle {
                        big,
                        little,
                        direction,
                    });
                }
            }
            d.status = merge(Status::Different(problems), d.status);
        }
        if matches!(a, Alias { .. }) {
            d.left = to_doc(l, c, a).dullyellow()
        }
        if matches!(b, Alias { .. }) {
            d.right = to_doc(l, c, b).dullyellow()
        }
        return d;
    }
    let problems = match (a, b) {
        (RigidVar(n), t) | (t, RigidVar(n)) => vec![Problem::BadRigidVar(n, t)],
        (_, Type { home, name, args }) if args.is_empty() && is_bool(*home, name) => {
            vec![Problem::AnythingToBool]
        }
        _ => vec![],
    };
    different(
        to_doc(l, c, a).dullyellow(),
        to_doc(l, c, b).dullyellow(),
        problems,
    )
}
fn diff_record<'a>(
    l: &Localizer,
    a: &[(&'a str, &'a ErrorType<'a>)],
    b: &[(&'a str, &'a ErrorType<'a>)],
) -> Diff<'a, Doc> {
    let a: BTreeMap<_, _> = a.iter().copied().collect();
    let b: BTreeMap<_, _> = b.iter().copied().collect();
    let mut left = BTreeMap::new();
    let mut right = BTreeMap::new();
    let mut status = Status::Similar;
    for (n, t) in &a {
        if let Some(u) = b.get(n) {
            let d = to_diff(l, Ctx::None, t, u);
            left.insert(*n, (Doc::text(*n), d.left));
            right.insert(*n, (Doc::text(*n), d.right));
            status = merge(status, d.status);
        } else {
            left.insert(*n, (Doc::text(*n).dullyellow(), to_doc(l, Ctx::None, t)));
        }
    }
    for (n, t) in &b {
        if !a.contains_key(n) {
            right.insert(*n, (Doc::text(*n).dullyellow(), to_doc(l, Ctx::None, t)));
        }
    }
    if let Some(n) = a.keys().find(|n| !b.contains_key(*n)) {
        status = merge(
            status,
            Status::Different(vec![Problem::FieldTypo(n, b.keys().copied().collect())]),
        )
    } else {
        let missing: Vec<_> = b.keys().filter(|n| !a.contains_key(*n)).copied().collect();
        if !missing.is_empty() {
            status = merge(
                status,
                Status::Different(vec![Problem::FieldsMissing(missing)]),
            )
        }
    }
    Diff {
        left: rt::record(left.into_values().collect(), None),
        right: rt::record(right.into_values().collect(), None),
        status,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn typ(n: &str) -> ErrorType<'_> {
        ErrorType::Type {
            home: primitives::builtin_home(),
            name: n,
            args: &[],
        }
    }
    #[test]
    fn nested_big_little() {
        let a = typ("Int");
        let b = typ("int");
        let aa = [&a];
        let bb = [&b];
        let x = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "list",
            args: &aa,
        };
        let y = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "list",
            args: &bb,
        };
        let (a, b, p) = to_comparison(&Localizer::from_names(["Builtin"]), &x, &y);
        insta::assert_snapshot!(a.render(80,true), @"list \u{1b}[33mInt\u{1b}[0m");
        insta::assert_snapshot!(b.render(80,false), @"list int");
        assert!(matches!(
            p.as_slice(),
            [Problem::BigLittle {
                direction: Direction::Have,
                ..
            }]
        ));
    }
    #[test]
    fn missing_field() {
        let t = typ("int");
        let a = ErrorType::Record { fields: &[] };
        let fs = [("x", &t)];
        let b = ErrorType::Record { fields: &fs };
        let (_, b, p) = to_comparison(&Localizer::from_names(["Builtin"]), &a, &b);
        insta::assert_snapshot!(b.render(80,false), @"{ x : int }");
        assert!(matches!(p.as_slice(),[Problem::FieldsMissing(f)] if f==&["x"]));
    }
    #[test]
    fn flexible_is_similar() {
        let a = ErrorType::FlexVar("a");
        let b = typ("int");
        assert!(is_similar(&to_diff(
            &Localizer::default(),
            Ctx::None,
            &a,
            &b
        )));
    }
    #[test]
    fn application_arity_not_truncated() {
        let t = typ("int");
        let xs = [&t];
        let a = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "list",
            args: &xs,
        };
        let b = typ("list");
        let d = to_diff(&Localizer::from_names(["Builtin"]), Ctx::None, &a, &b);
        assert!(!is_similar(&d));
        insta::assert_snapshot!(d.left.render(80,false), @"list int");
    }
    #[test]
    fn rigid_variable_hint() {
        let a = ErrorType::RigidVar("a");
        let b = typ("int");
        let (doc, _, p) = to_comparison(&Localizer::default(), &a, &b);
        insta::assert_snapshot!(doc.render(80,false), @"'a");
        assert!(matches!(p.as_slice(), [Problem::BadRigidVar("a", _)]));
    }
    #[test]
    fn bool_requires_exact_builtin_identity() {
        let a = typ("int");
        let b = typ("bool");
        assert!(matches!(
            to_comparison(&Localizer::default(), &a, &b).2.as_slice(),
            [Problem::AnythingToBool]
        ));
        let b = ErrorType::Type {
            home: ModuleName {
                package: None,
                name: "User",
            },
            name: "bool",
            args: &[],
        };
        assert!(to_comparison(&Localizer::default(), &a, &b).2.is_empty());
    }
    #[test]
    fn function_argument_counts() {
        let t = typ("int");
        let rest = [&t];
        let a = ErrorType::Lambda(&t, &t, &[]);
        let b = ErrorType::Lambda(&t, &t, &rest);
        assert!(matches!(
            to_comparison(&Localizer::default(), &a, &b).2.as_slice(),
            [Problem::ArityMismatch(1, 2)]
        ));
    }
    #[test]
    fn tuple_arity_is_preserved() {
        let t = typ("int");
        let rest = [&t, &t];
        let a = ErrorType::Tuple(&t, &t, &rest);
        let b = ErrorType::Tuple(&t, &t, &[]);
        let d = to_diff(&Localizer::from_names(["Builtin"]), Ctx::None, &a, &b);
        insta::assert_snapshot!(d.left.render(80,false), @"( int, int, int, int )");
        assert!(!is_similar(&d));
    }
    #[test]
    fn field_typo_and_overlapping_type_diff() {
        let a = typ("Int");
        let b = typ("int");
        let xs = [("z", &a), ("naem", &a)];
        let ys = [("name", &b), ("z", &b)];
        let x = ErrorType::Record { fields: &xs };
        let y = ErrorType::Record { fields: &ys };
        let (a, b, p) = to_comparison(&Localizer::from_names(["Builtin"]), &x, &y);
        insta::assert_snapshot!(a.render(80,false), @"{ naem : Int, z : Int }");
        insta::assert_snapshot!(b.render(80,false), @"{ name : int, z : int }");
        assert!(matches!(
            p.as_slice(),
            [Problem::BigLittle { .. }, Problem::FieldTypo("naem", _)]
        ));
    }
    #[test]
    fn name_clash_qualifies_modules() {
        let a = ErrorType::Type {
            home: ModuleName {
                package: None,
                name: "A",
            },
            name: "Thing",
            args: &[],
        };
        let b = ErrorType::Type {
            home: ModuleName {
                package: None,
                name: "B",
            },
            name: "Thing",
            args: &[],
        };
        let (a, b, _) = to_comparison(&Localizer::from_names(["A", "B"]), &a, &b);
        insta::assert_snapshot!(a.render(80,false), @"A.Thing");
        insta::assert_snapshot!(b.render(80,false), @"B.Thing");
    }
    #[test]
    fn alias_record_keeps_alias_and_field_hint() {
        let t = typ("int");
        let fs = [("x", &t)];
        let a = ErrorType::Record { fields: &[] };
        let b = ErrorType::Record { fields: &fs };
        let alias = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "Empty",
            args: &[],
            real: &a,
        };
        let (a, _, p) = to_comparison(&Localizer::from_names(["Builtin"]), &alias, &b);
        insta::assert_snapshot!(a.render(80,false), @"Empty");
        assert!(matches!(p.as_slice(), [Problem::FieldsMissing(_)]));
    }
    #[test]
    fn higher_kinded_application_diff() {
        let f = ErrorType::RigidVar("f");
        let a = typ("Int");
        let b = typ("int");
        let xs = [&a];
        let ys = [&b];
        let x = ErrorType::VarApp(&f, &xs);
        let y = ErrorType::VarApp(&f, &ys);
        let (a, _, p) = to_comparison(&Localizer::from_names(["Builtin"]), &x, &y);
        insta::assert_snapshot!(a.render(80,false), @"'f Int");
        assert!(matches!(p.as_slice(), [Problem::BigLittle { .. }]));
    }
    #[test]
    fn sentinels_render_explicitly() {
        let l = Localizer::default();
        insta::assert_snapshot!(to_doc(&l,Ctx::None,&ErrorType::Infinite).render(80,false), @"∞");
        insta::assert_snapshot!(to_doc(&l,Ctx::None,&ErrorType::Error).render(80,false), @"?");
    }
    #[test]
    fn map_representation_hint_requires_list_of_pairs() {
        let t = typ("Data");
        let args = [&t, &t];
        let map = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "Map",
            args: &args,
        };
        let pair = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "pair",
            args: &args,
        };
        let list_args = [&pair];
        let list = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "list",
            args: &list_args,
        };
        assert!(matches!(
            to_comparison(&Localizer::default(), &map, &list)
                .2
                .as_slice(),
            [Problem::BigLittle {
                big: "Map",
                direction: Direction::Have,
                ..
            }]
        ));
        assert!(
            to_comparison(&Localizer::default(), &map, &pair)
                .2
                .is_empty()
        );
    }
    #[test]
    fn option_hint_preserves_inner_type() {
        let t = typ("int");
        let args = [&t];
        let option = ErrorType::Type {
            home: ModuleName {
                package: Some(primitives::CORE),
                name: "Option",
            },
            name: "option",
            args: &args,
        };
        let (a, b, p) = to_comparison(&Localizer::from_names(["Builtin", "Option"]), &option, &t);
        insta::assert_snapshot!(a.render(80,false), @"option int");
        insta::assert_snapshot!(b.render(80,false), @"int");
        assert!(matches!(p.as_slice(), [Problem::AnythingFromOption]));
    }
    #[test]
    fn different_alias_names_do_not_unfold_nonrecords() {
        let t = typ("int");
        let alias = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "Count",
            args: &[],
            real: &t,
        };
        let d = to_diff(&Localizer::from_names(["Builtin"]), Ctx::None, &alias, &t);
        insta::assert_snapshot!(d.left.render(80,false), @"Count");
        assert!(!is_similar(&d));
    }
    #[test]
    fn big_little_container_highlights_only_changed_head() {
        let t = typ("Data");
        let args = [&t];
        let a = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "List",
            args: &args,
        };
        let b = ErrorType::Type {
            home: primitives::builtin_home(),
            name: "list",
            args: &args,
        };
        let (a, _, _) = to_comparison(&Localizer::from_names(["Builtin"]), &a, &b);
        insta::assert_snapshot!(a.render(80,true), @"\u{1b}[33mList\u{1b}[0m Data");
    }
    #[test]
    fn distinct_nominal_records_are_not_similar() {
        let real = ErrorType::Record { fields: &[] };
        let a = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "One",
            args: &[],
            real: &real,
        };
        let b = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "Two",
            args: &[],
            real: &real,
        };
        assert!(!is_similar(&to_diff(
            &Localizer::default(),
            Ctx::None,
            &a,
            &b
        )));
    }
    #[test]
    fn record_representation_uses_defining_alias_not_transparent_name() {
        let real = ErrorType::Record { fields: &[] };
        let big = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "BigRecord",
            args: &[],
            real: &real,
        };
        let little = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "littleRecord",
            args: &[],
            real: &real,
        };
        let outer = ErrorType::Alias {
            home: primitives::builtin_home(),
            name: "MisleadingUppercase",
            args: &[],
            real: &little,
        };
        let (a, b, p) = to_comparison(&Localizer::from_names(["Builtin"]), &big, &outer);
        insta::assert_snapshot!(a.render(80,false), @"BigRecord");
        insta::assert_snapshot!(b.render(80,false), @"MisleadingUppercase");
        assert!(matches!(
            p.as_slice(),
            [Problem::BigLittle {
                big: "BigRecord",
                little: "littleRecord",
                direction: Direction::Have
            }]
        ));
        assert!(is_similar(&to_diff(
            &Localizer::default(),
            Ctx::None,
            &outer,
            &little
        )));
    }
    #[test]
    fn solver_error_producer_retains_record_identity() {
        use nash_constrain::type_::make_descriptor;
        use nash_constrain::{Content, FlatType, UnionFind};
        let arena = bumpalo::Bump::new();
        let mut uf = UnionFind::new();
        let body = nash_region::Located::at_zero(nash_ast::Type::Record { fields: &[] });
        let real = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            BTreeMap::new(),
        ))));
        let little = uf.fresh(make_descriptor(Content::Alias {
            home: primitives::builtin_home(),
            name: "littleRecord",
            args: vec![],
            real,
            body: &body,
        }));
        let produced = nash_solve::to_error_type(&arena, &mut uf, little);
        assert_eq!(
            nominal_record(produced),
            Some((primitives::builtin_home(), "littleRecord"))
        );
    }
}
