//! Ordered pattern matrices with shared constructor tests and field bindings.
use crate::ty_of::{TypeEnv, TypeError};
use nash_ast::{DataEncoding, NodeId, Pattern};
use nash_ir::{
    build::Builder,
    core::{Binder, Branch, CaseKind, CastKind, Core, Test},
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction, constant::Constant};
use nash_region::Located;
use std::collections::{BTreeMap, HashMap};

pub type RecordFields<'a> = HashMap<NodeId, &'a [&'a str]>;
pub struct MatchInputs<'a, 'i> {
    pub record_fields: &'i RecordFields<'a>,
    pub literal_tests: &'i HashMap<NodeId, &'a Core<'a>>,
}
pub struct MatchBranch<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub bindings: BTreeMap<&'a str, Binder<'a>>,
    pub body: &'a Core<'a>,
}
#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("{0}")]
    Type(TypeError<'a>),
    #[error("pattern does not match its solved runtime type")]
    PatternType,
    #[error("missing record labels for a canonical pattern")]
    RecordLabels,
    #[error("missing literal matcher for a canonical pattern")]
    LiteralMatcher,
    #[error("missing or duplicate pattern binding {0}")]
    Binding(&'a str),
}

impl<'a> From<TypeError<'a>> for Error<'a> {
    fn from(error: TypeError<'a>) -> Self {
        Self::Type(error)
    }
}

#[derive(Clone, Copy)]
enum Pat<'a> {
    Any,
    Bind(&'a str),
    Node(&'a Located<Pattern<'a>>),
    List(&'a [&'a Located<Pattern<'a>>]),
    DataList(
        &'a [&'a Located<Pattern<'a>>],
        Option<&'a Located<Pattern<'a>>>,
    ),
}
#[derive(Clone)]
struct Row<'a> {
    index: usize,
    patterns: Vec<Pat<'a>>,
    bindings: BTreeMap<&'a str, Binder<'a>>,
    assignments: Vec<(Binder<'a>, &'a Core<'a>)>,
    body: &'a Core<'a>,
}
#[derive(Clone, Copy)]
struct Subject<'a> {
    binder: Binder<'a>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Tag(u16),
    False,
    True,
    Nil,
    Cons,
    Data(u16),
    Product,
}
struct Signature<'a> {
    shape: Shape,
    fields: Vec<Ty<'a>>,
}

pub fn bindings<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
    pattern: &'a Located<Pattern<'a>>,
    records: &RecordFields<'a>,
) -> Result<BTreeMap<&'a str, Binder<'a>>, Error<'a>> {
    let mut result = BTreeMap::new();
    collect_bindings(build, types, ty, Pat::Node(pattern), records, &mut result)?;
    Ok(result)
}
fn collect_bindings<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
    pattern: Pat<'a>,
    records: &RecordFields<'a>,
    out: &mut BTreeMap<&'a str, Binder<'a>>,
) -> Result<(), Error<'a>> {
    let pattern = normalize(pattern);
    match pattern {
        Pat::Any => return Ok(()),
        Pat::Bind(name) => {
            if out
                .insert(
                    name,
                    Binder {
                        name: build.fresh(name),
                        ty,
                    },
                )
                .is_some()
            {
                return Err(Error::Binding(name));
            }
            return Ok(());
        }
        Pat::Node(p) => match &p.value {
            Pattern::Anything
            | Pattern::Unit
            | Pattern::Bool { .. }
            | Pattern::Constant(_)
            | Pattern::Int(_)
            | Pattern::Bytes(_)
            | Pattern::Str(_) => return Ok(()),
            Pattern::Var(name) => {
                return collect_bindings(build, types, ty, Pat::Bind(name), records, out);
            }
            Pattern::Alias { pattern, name } => {
                collect_bindings(build, types, ty, Pat::Bind(name), records, out)?;
                return collect_bindings(build, types, ty, Pat::Node(pattern), records, out);
            }
            _ => {}
        },
        Pat::List(_) | Pat::DataList(_, _) => {}
    }
    let shape = shape(pattern).ok_or(Error::PatternType)?;
    let signature = signatures(types, ty)?
        .into_iter()
        .find(|s| s.shape == shape || (shape == Shape::Product && s.shape == Shape::Tag(0)))
        .ok_or(Error::PatternType)?;
    let children = children(pattern, &signature, records)?.ok_or(Error::PatternType)?;
    for (child, ty) in children.into_iter().zip(signature.fields) {
        collect_bindings(build, types, ty, child, records, out)?;
    }
    Ok(())
}

pub fn compile<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
    scrutinee: &'a Core<'a>,
    branches: &[MatchBranch<'a>],
    inputs: MatchInputs<'a, '_>,
    fallback: &'a Core<'a>,
) -> Result<&'a Core<'a>, Error<'a>> {
    let root = Binder {
        name: build.fresh("match"),
        ty,
    };
    let mut rows: Vec<_> = branches
        .iter()
        .enumerate()
        .map(|(index, b)| Row {
            index,
            patterns: vec![Pat::Node(b.pattern)],
            bindings: b.bindings.clone(),
            assignments: Vec::new(),
            body: b.body,
        })
        .collect();
    let mut matrix = Matrix {
        build,
        types,
        inputs,
        fallback,
        leaf_counts: vec![0; branches.len()],
    };
    let mut body = matrix.compile(vec![Subject { binder: root }], rows.clone())?;
    let mut joins = Vec::new();
    for (row, count) in rows.iter_mut().zip(&matrix.leaf_counts) {
        if *count < 2 {
            continue;
        }
        let params = row.bindings.values().copied().collect::<Vec<_>>();
        // Join results are passed through without inspecting their representation.
        let binder = Binder {
            name: build.fresh("leaf"),
            ty: Ty::Erased,
        };
        let value = if params.is_empty() {
            let value = build.delay(row.body);
            row.body = build.force(build.var(binder.name));
            value
        } else {
            let value = build.lam(&params, row.body);
            let args = params.iter().map(|p| build.var(p.name)).collect::<Vec<_>>();
            row.body = build.app(build.var(binder.name), &args);
            value
        };
        joins.push((binder, value));
    }
    if !joins.is_empty() {
        body = matrix.compile(vec![Subject { binder: root }], rows)?;
        for (binder, value) in joins.into_iter().rev() {
            body = build.let_(binder, value, body);
        }
    }
    Ok(build.let_(root, scrutinee, body))
}
struct Matrix<'a, 'b, 'env, 'i> {
    build: &'b Builder<'a>,
    types: &'b mut TypeEnv<'a, 'env>,
    inputs: MatchInputs<'a, 'i>,
    fallback: &'a Core<'a>,
    leaf_counts: Vec<usize>,
}
impl<'a> Matrix<'a, '_, '_, '_> {
    fn compile(
        &mut self,
        subjects: Vec<Subject<'a>>,
        mut rows: Vec<Row<'a>>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if rows.is_empty() {
            return Ok(self.fallback);
        }
        for row in &mut rows {
            for (index, subject) in subjects.iter().enumerate() {
                loop {
                    row.patterns[index] = normalize(row.patterns[index]);
                    let (name, next) = match row.patterns[index] {
                        Pat::Bind(name) => (Some(name), Pat::Any),
                        Pat::Node(p) => match &p.value {
                            Pattern::Anything => (None, Pat::Any),
                            Pattern::Unit if subject.binder.ty == Ty::Const(&ConstTy::Unit) => {
                                (None, Pat::Any)
                            }
                            Pattern::Var(name) => (Some(*name), Pat::Any),
                            Pattern::Alias { pattern, name } => (Some(*name), Pat::Node(pattern)),
                            _ => break,
                        },
                        _ => break,
                    };
                    if let Some(name) = name {
                        let binder = *row.bindings.get(name).ok_or(Error::Binding(name))?;
                        row.assignments
                            .push((binder, self.build.var(subject.binder.name)));
                    }
                    row.patterns[index] = next;
                }
            }
        }
        let Some(column) = rows[0].patterns.iter().position(|p| !matches!(p, Pat::Any)) else {
            let row = &rows[0];
            self.leaf_counts[row.index] += 1;
            return Ok(row
                .assignments
                .iter()
                .rev()
                .fold(row.body, |body, (binder, value)| {
                    self.build.let_(*binder, value, body)
                }));
        };
        if let Pat::Node(pattern) = rows[0].patterns[column]
            && is_literal(rows[0].patterns[column])
        {
            let subject = subjects[column].binder;
            let value = self.build.var(subject.name);
            let condition = if let Pattern::Constant(constant) = pattern.value {
                let (eq, literal) = match (constant, subject.ty) {
                    (nash_ast::Constant::Int(n), Ty::Const(ConstTy::Int)) => {
                        (DefaultFunction::EqualsInteger, self.build.int(n))
                    }
                    (nash_ast::Constant::BigInt(digits), Ty::Const(ConstTy::Int)) => {
                        let integer = digits.parse().map_err(|_| Error::PatternType)?;
                        let integer = self.build.arena.alloc_integer(integer);
                        (
                            DefaultFunction::EqualsInteger,
                            self.build.lit(Constant::integer(self.build.arena, integer)),
                        )
                    }
                    (nash_ast::Constant::Bytes(bytes), Ty::Const(ConstTy::Bytes)) => (
                        DefaultFunction::EqualsByteString,
                        self.build
                            .lit(Constant::byte_string(self.build.arena, bytes)),
                    ),
                    (nash_ast::Constant::Str(s), Ty::Const(ConstTy::String)) => (
                        DefaultFunction::EqualsString,
                        self.build.lit(Constant::string(self.build.arena, s)),
                    ),
                    (nash_ast::Constant::BlsG1(bytes), Ty::Const(ConstTy::BlsG1)) => {
                        let point =
                            nash_plutus::bls::Compressable::uncompress(self.build.arena, bytes)
                                .map_err(|_| Error::PatternType)?;
                        (
                            DefaultFunction::Bls12_381_G1_Equal,
                            self.build.lit(Constant::g1(self.build.arena, point)),
                        )
                    }
                    (nash_ast::Constant::BlsG2(bytes), Ty::Const(ConstTy::BlsG2)) => {
                        let point =
                            nash_plutus::bls::Compressable::uncompress(self.build.arena, bytes)
                                .map_err(|_| Error::PatternType)?;
                        (
                            DefaultFunction::Bls12_381_G2_Equal,
                            self.build.lit(Constant::g2(self.build.arena, point)),
                        )
                    }
                    _ => return Err(Error::PatternType),
                };
                self.build.builtin(eq, &[value, literal])
            } else {
                let matcher = *self
                    .inputs
                    .literal_tests
                    .get(&NodeId::pattern(pattern))
                    .ok_or(Error::LiteralMatcher)?;
                self.build.app(matcher, &[value])
            };
            let mut yes = rows.clone();
            yes[0].patterns[column] = Pat::Any;
            let yes = self.compile(subjects.clone(), yes)?;
            rows.remove(0);
            let no = self.compile(subjects, rows)?;
            return Ok(self.build.if_(condition, yes, no));
        }
        let signatures = signatures(self.types, subjects[column].binder.ty)?;
        let mut branches = Vec::new();
        for signature in &signatures {
            let fields = signature
                .fields
                .iter()
                .map(|ty| Binder {
                    name: self.build.fresh("field"),
                    ty: *ty,
                })
                .collect::<Vec<_>>();
            let mut specialized = Vec::new();
            for row in &rows {
                let mut row = row.clone();
                let child = if matches!(row.patterns[column], Pat::Any)
                    || is_literal(row.patterns[column])
                {
                    Some(vec![Pat::Any; fields.len()])
                } else {
                    children(row.patterns[column], signature, self.inputs.record_fields)?
                };
                if let Some(child) = child {
                    if !is_literal(row.patterns[column]) {
                        row.patterns[column] = Pat::Any;
                    }
                    row.patterns.splice(column + 1..column + 1, child);
                    specialized.push(row);
                }
            }
            let mut child_subjects = subjects.clone();
            child_subjects.splice(
                column + 1..column + 1,
                fields.iter().map(|binder| Subject { binder: *binder }),
            );
            let body = self.compile(child_subjects, specialized)?;
            branches.push((signature.shape, fields, body));
        }
        let external = match subjects[column].binder.ty {
            Ty::Big(BigTy::Adt(adt)) => self.types.layout(*adt)?.data_layout.is_some(),
            _ => false,
        };
        let mut default = None;
        let mut default_shape = None;
        if external {
            let defaults = rows
                .iter()
                .filter(|row| matches!(row.patterns[column], Pat::Any))
                .cloned()
                .collect::<Vec<_>>();
            if defaults.is_empty() {
                let mut seen = Vec::new();
                for row in &rows {
                    if let Some(shape) = shape(row.patterns[column])
                        && !seen.contains(&shape)
                    {
                        seen.push(shape);
                    }
                }
                default_shape = seen.last().copied();
            } else {
                default = Some(self.compile(subjects.clone(), defaults)?);
            }
        }
        self.emit(subjects[column], branches, default, default_shape)
    }

    fn emit(
        &mut self,
        subject: Subject<'a>,
        branches: Vec<(Shape, Vec<Binder<'a>>, &'a Core<'a>)>,
        default: Option<&'a Core<'a>>,
        default_shape: Option<Shape>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let value = self.build.var(subject.binder.name);
        let ty = subject.binder.ty;
        if let Ty::Const(ConstTy::DataPair(_, _)) = ty {
            let [(Shape::Product, fields, body)] = branches.as_slice() else {
                return Err(Error::PatternType);
            };
            if fields.len() != 2 {
                return Err(Error::PatternType);
            }
            let mut body = *body;
            for (field, func) in fields
                .iter()
                .zip([DefaultFunction::FstPair, DefaultFunction::SndPair])
                .rev()
            {
                let raw = self.build.builtin(func, &[value]);
                body = self.bind_decoded(*field, raw, body);
            }
            return Ok(body);
        }
        if let Ty::Const(ConstTy::DataTuple(_)) = ty {
            let [(Shape::Product, fields, body)] = branches.as_slice() else {
                return Err(Error::PatternType);
            };
            return Ok(self.list_fields(value, fields, body, true));
        }
        if let Ty::Const(ConstTy::DataList(element)) = ty {
            let map = matches!(element, Ty::Const(ConstTy::DataPair(_, _)));
            let mut arms = Vec::new();
            for (shape, fields, body) in branches {
                match shape {
                    Shape::Nil => arms.push(Branch {
                        test: Test::Nil,
                        binders: &[],
                        body,
                    }),
                    Shape::Cons => {
                        let [head, tail] = fields.as_slice() else {
                            return Err(Error::PatternType);
                        };
                        let raw = Binder {
                            name: self.build.fresh("encodedHead"),
                            ty: if map { *element } else { Ty::Big(&BigTy::Data) },
                        };
                        let decoded = if map {
                            self.build.var(raw.name)
                        } else {
                            self.decode_field(head.ty, self.build.var(raw.name))
                        };
                        arms.push(Branch {
                            test: Test::Cons,
                            binders: self.build.arena.alloc_slice_copy(&[raw, *tail]),
                            body: if uses(body, head.name.unique) {
                                self.build.let_(*head, decoded, body)
                            } else {
                                body
                            },
                        });
                    }
                    _ => return Err(Error::PatternType),
                }
            }
            return Ok(self.build.case(CaseKind::List, value, &arms, None));
        }
        if let Ty::Big(BigTy::List(element)) = ty {
            let mut arms = Vec::new();
            for (shape, fields, body) in branches {
                match shape {
                    Shape::Nil => arms.push(Branch {
                        test: Test::Nil,
                        binders: &[],
                        body,
                    }),
                    Shape::Cons => {
                        let [head, tail] = fields.as_slice() else {
                            return Err(Error::PatternType);
                        };
                        let raw_tail = Binder {
                            name: self.build.fresh("listTail"),
                            ty: Ty::Const(self.build.arena.alloc(ConstTy::List(*element))),
                        };
                        let body = self.build.let_(
                            *tail,
                            self.build.builtin(
                                DefaultFunction::ListData,
                                &[self.build.var(raw_tail.name)],
                            ),
                            body,
                        );
                        arms.push(Branch {
                            test: Test::Cons,
                            binders: self.build.arena.alloc_slice_copy(&[*head, raw_tail]),
                            body,
                        });
                    }
                    _ => return Err(Error::PatternType),
                }
            }
            return Ok(self.build.case(
                CaseKind::List,
                self.build.builtin(DefaultFunction::UnListData, &[value]),
                &arms,
                None,
            ));
        }
        if let Ty::Big(BigTy::Adt(adt)) = ty {
            let layout = self.types.layout(*adt)?;
            let external = layout.data_layout.is_some();
            if layout
                .data_layout
                .is_some_and(|layout| layout.encoding == DataEncoding::Transparent)
            {
                let [(Shape::Tag(0), fields, body)] = branches.as_slice() else {
                    return Err(Error::PatternType);
                };
                let [field] = fields.as_slice() else {
                    return Err(Error::PatternType);
                };
                return Ok(self.bind_decoded(*field, value, body));
            }
            if external && layout.fields.len() == 1 {
                let [(Shape::Tag(0), fields, body)] = branches.as_slice() else {
                    return Err(Error::PatternType);
                };
                let fields_value = if layout
                    .data_layout
                    .is_some_and(|layout| layout.encoding == DataEncoding::List)
                {
                    self.build.builtin(DefaultFunction::UnListData, &[value])
                } else {
                    self.build.builtin(
                        DefaultFunction::SndPair,
                        &[self.build.builtin(DefaultFunction::UnConstrData, &[value])],
                    )
                };
                return Ok(self.list_fields(fields_value, fields, body, true));
            }
            let tag = Binder {
                name: self.build.fresh("tag"),
                ty: Ty::Const(&ConstTy::Int),
            };
            let list = Binder {
                name: self.build.fresh("fields"),
                ty: data_list(self.build),
            };
            let arms = branches
                .iter()
                .find(|(shape, _, _)| Some(*shape) == default_shape)
                .map(|(_, fields, body)| {
                    self.list_fields(self.build.var(list.name), fields, body, external)
                });
            let fallback = default.or(arms).unwrap_or(self.fallback);
            let arms = branches
                .into_iter()
                .map(|(shape, fields, body)| {
                    let Shape::Tag(index) = shape else {
                        return Err(Error::PatternType);
                    };
                    Ok(Branch {
                        test: Test::Int(nash_plutus::constant::integer_from(
                            self.build.arena,
                            layout.data_layout.map_or(i128::from(index), |data| {
                                i128::from(data.tags[usize::from(index)])
                            }),
                        )),
                        binders: &[],
                        body: self.list_fields(self.build.var(list.name), &fields, body, external),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let body = self.build.case(
                CaseKind::Int,
                self.build.var(tag.name),
                &arms,
                Some(fallback),
            );
            return Ok(self.build.case(
                CaseKind::Data,
                value,
                &[Branch {
                    test: Test::DataConstr,
                    binders: self.build.arena.alloc_slice_copy(&[tag, list]),
                    body,
                }],
                Some(self.fallback),
            ));
        }
        if matches!(ty, Ty::Big(BigTy::Record(_))) {
            let [(Shape::Product, fields, body)] = branches.as_slice() else {
                return Err(Error::PatternType);
            };
            return Ok(self.list_fields(
                self.build.builtin(DefaultFunction::UnListData, &[value]),
                fields,
                body,
                false,
            ));
        }
        let mut kind = None;
        let arms = branches
            .into_iter()
            .map(|(shape, fields, body)| {
                let (k, test) = match shape {
                    Shape::Tag(index) => (CaseKind::Tag, Test::Tag(index)),
                    Shape::Product => (CaseKind::Tag, Test::Tag(0)),
                    Shape::False => (CaseKind::Bool, Test::False),
                    Shape::True => (CaseKind::Bool, Test::True),
                    Shape::Nil => (CaseKind::List, Test::Nil),
                    Shape::Cons => (CaseKind::List, Test::Cons),
                    Shape::Data(0) => (CaseKind::Data, Test::DataConstr),
                    Shape::Data(1) => (CaseKind::Data, Test::DataMap),
                    Shape::Data(2) => (CaseKind::Data, Test::DataList),
                    Shape::Data(3) => (CaseKind::Data, Test::DataI),
                    Shape::Data(4) => (CaseKind::Data, Test::DataB),
                    _ => return Err(Error::PatternType),
                };
                kind = Some(k);
                Ok(Branch {
                    test,
                    binders: self.build.arena.alloc_slice_copy(&fields),
                    body,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(self
            .build
            .case(kind.ok_or(Error::PatternType)?, value, &arms, None))
    }
    fn list_fields(
        &self,
        value: &'a Core<'a>,
        fields: &[Binder<'a>],
        body: &'a Core<'a>,
        decode: bool,
    ) -> &'a Core<'a> {
        if fields.is_empty()
            || (decode && !fields.iter().any(|field| uses(body, field.name.unique)))
        {
            return body;
        }
        let list = Binder {
            name: self.build.fresh("fieldList"),
            ty: data_list(self.build),
        };
        let tail = self
            .build
            .builtin(DefaultFunction::TailList, &[self.build.var(list.name)]);
        let body = self.list_fields(tail, &fields[1..], body, decode);
        let head = self
            .build
            .builtin(DefaultFunction::HeadList, &[self.build.var(list.name)]);
        let body = if decode {
            self.bind_decoded(fields[0], head, body)
        } else {
            self.build.let_(fields[0], head, body)
        };
        self.build.let_(list, value, body)
    }
    fn decode_field(&self, ty: Ty<'a>, value: &'a Core<'a>) -> &'a Core<'a> {
        if matches!(ty, Ty::Big(_)) {
            value
        } else {
            self.build
                .cast(CastKind::FromDataShallow, Ty::Big(&BigTy::Data), ty, value)
        }
    }
    fn bind_decoded(
        &self,
        field: Binder<'a>,
        value: &'a Core<'a>,
        body: &'a Core<'a>,
    ) -> &'a Core<'a> {
        if uses(body, field.name.unique) {
            self.build
                .let_(field, self.decode_field(field.ty, value), body)
        } else {
            body
        }
    }
}
fn data_list<'a>(build: &Builder<'a>) -> Ty<'a> {
    Ty::Const(build.arena.alloc(ConstTy::List(Ty::Big(&BigTy::Data))))
}
fn normalize<'a>(mut pattern: Pat<'a>) -> Pat<'a> {
    loop {
        pattern = match pattern {
            Pat::Node(p) => match &p.value {
                Pattern::DataList { elements, tail } => Pat::DataList(elements, *tail),
                _ => return pattern,
            },
            Pat::DataList([], Some(tail)) => Pat::Node(tail),
            _ => return pattern,
        };
    }
}
fn uses(body: &Core<'_>, name: u32) -> bool {
    let mut found = false;
    body.walk(&mut |node| {
        found |= matches!(node, Core::Var(n) if n.unique == name);
    });
    found
}
fn is_literal(pattern: Pat<'_>) -> bool {
    matches!(pattern, Pat::Node(p) if matches!(&p.value,
        Pattern::Constant(_) | Pattern::Int(_) | Pattern::Str(_) | Pattern::Bytes(_)))
}
fn shape(pattern: Pat<'_>) -> Option<Shape> {
    match normalize(pattern) {
        Pat::List(xs) | Pat::DataList(xs, _) => Some(if xs.is_empty() {
            Shape::Nil
        } else {
            Shape::Cons
        }),
        Pat::Node(p) => match &p.value {
            Pattern::Bool { value, .. } => Some(if *value { Shape::True } else { Shape::False }),
            Pattern::Tuple { .. }
            | Pattern::DataTuple { .. }
            | Pattern::Pair { .. }
            | Pattern::Record(_) => Some(Shape::Product),
            Pattern::List(xs) => Some(if xs.is_empty() {
                Shape::Nil
            } else {
                Shape::Cons
            }),
            Pattern::Cons { .. } => Some(Shape::Cons),
            Pattern::Constructor(c) => Some(
                if c.reference.home == nash_ast::primitives::builtin_home()
                    && c.reference.union == "Data"
                {
                    Shape::Data(c.index)
                } else if c.reference.home == nash_ast::primitives::builtin_home()
                    && c.reference.union == "bool"
                {
                    if c.index == 0 {
                        Shape::False
                    } else {
                        Shape::True
                    }
                } else {
                    Shape::Tag(c.index)
                },
            ),
            _ => None,
        },
        _ => None,
    }
}
fn signatures<'a>(
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
) -> Result<Vec<Signature<'a>>, Error<'a>> {
    let result = match ty {
        Ty::Term(TermTy::Adt(adt)) | Ty::Big(BigTy::Adt(adt)) => types
            .layout(*adt)?
            .fields
            .iter()
            .enumerate()
            .map(|(i, fields)| Signature {
                shape: Shape::Tag(i as u16),
                fields: fields.to_vec(),
            })
            .collect(),
        Ty::Const(ConstTy::DataTuple(fields))
        | Ty::Term(TermTy::Tuple(fields))
        | Ty::Term(TermTy::Record(fields))
        | Ty::Big(BigTy::Record(fields)) => vec![Signature {
            shape: Shape::Product,
            fields: fields.to_vec(),
        }],
        Ty::Const(ConstTy::DataPair(first, second)) => vec![Signature {
            shape: Shape::Product,
            fields: vec![*first, *second],
        }],
        Ty::Const(ConstTy::Bool) => vec![
            Signature {
                shape: Shape::False,
                fields: vec![],
            },
            Signature {
                shape: Shape::True,
                fields: vec![],
            },
        ],
        Ty::Const(ConstTy::DataList(element))
        | Ty::Const(ConstTy::List(element))
        | Ty::Big(BigTy::List(element)) => vec![
            Signature {
                shape: Shape::Nil,
                fields: vec![],
            },
            Signature {
                shape: Shape::Cons,
                fields: vec![*element, ty],
            },
        ],
        Ty::Big(BigTy::Data) => vec![
            Signature {
                shape: Shape::Data(0),
                fields: vec![
                    Ty::Const(&ConstTy::Int),
                    Ty::Const(&ConstTy::List(Ty::Big(&BigTy::Data))),
                ],
            },
            Signature {
                shape: Shape::Data(1),
                fields: vec![Ty::Const(&ConstTy::List(Ty::Const(&ConstTy::Pair(
                    Ty::Big(&BigTy::Data),
                    Ty::Big(&BigTy::Data),
                ))))],
            },
            Signature {
                shape: Shape::Data(2),
                fields: vec![Ty::Const(&ConstTy::List(Ty::Big(&BigTy::Data)))],
            },
            Signature {
                shape: Shape::Data(3),
                fields: vec![Ty::Const(&ConstTy::Int)],
            },
            Signature {
                shape: Shape::Data(4),
                fields: vec![Ty::Const(&ConstTy::Bytes)],
            },
        ],
        _ => return Err(Error::PatternType),
    };
    Ok(result)
}
fn children<'a>(
    pattern: Pat<'a>,
    signature: &Signature<'a>,
    records: &RecordFields<'a>,
) -> Result<Option<Vec<Pat<'a>>>, Error<'a>> {
    let pattern = normalize(pattern);
    if shape(pattern) != Some(signature.shape)
        && !(matches!(pattern,Pat::Node(p) if matches!(p.value,Pattern::Record(_)))
            && signature.shape == Shape::Tag(0))
    {
        return Ok(None);
    }
    let children = match pattern {
        Pat::List(xs) => {
            if xs.is_empty() {
                vec![]
            } else {
                vec![Pat::Node(xs[0]), Pat::List(&xs[1..])]
            }
        }
        Pat::DataList(xs, tail) => {
            if xs.is_empty() {
                vec![]
            } else {
                vec![Pat::Node(xs[0]), Pat::DataList(&xs[1..], tail)]
            }
        }
        Pat::Node(p) => match &p.value {
            Pattern::List(xs) => {
                if xs.is_empty() {
                    vec![]
                } else {
                    vec![Pat::Node(xs[0]), Pat::List(&xs[1..])]
                }
            }
            Pattern::Cons { head, tail } => vec![Pat::Node(head), Pat::Node(tail)],
            Pattern::Bool { .. } => vec![],
            Pattern::Pair { first, second } => vec![Pat::Node(first), Pat::Node(second)],
            Pattern::DataTuple {
                first,
                second,
                rest,
            }
            | Pattern::Tuple {
                first,
                second,
                rest,
            } => [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .map(Pat::Node)
                .collect(),
            Pattern::Constructor(c) => {
                let mut fields = vec![Pat::Any; signature.fields.len()];
                if c.arguments.len() != fields.len() {
                    return Err(Error::PatternType);
                }
                let mut seen = vec![false; fields.len()];
                for argument in c.arguments {
                    let index = usize::from(argument.index);
                    if index >= fields.len() || seen[index] {
                        return Err(Error::PatternType);
                    }
                    seen[index] = true;
                    fields[index] = Pat::Node(argument.pattern);
                }
                fields
            }
            Pattern::Record(names) => {
                let labels = records
                    .get(&NodeId::pattern(p))
                    .ok_or(Error::RecordLabels)?;
                if labels.len() != signature.fields.len() {
                    return Err(Error::RecordLabels);
                }
                let mut fields = vec![Pat::Any; labels.len()];
                for name in *names {
                    let index = labels
                        .iter()
                        .position(|label| label == name)
                        .ok_or(Error::RecordLabels)?;
                    fields[index] = Pat::Bind(name);
                }
                fields
            }
            _ => return Err(Error::PatternType),
        },
        _ => return Err(Error::PatternType),
    };
    if children.len() != signature.fields.len() {
        return Err(Error::PatternType);
    }
    Ok(Some(children))
}

#[cfg(test)]
#[path = "decision_tree_tests.rs"]
mod tests;
