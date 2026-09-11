//! Canonical types to runtime representations, with lazy nominal layouts.
use std::collections::{BTreeMap, HashMap};

use nash_ast::{
    Alias, AliasArgument, AliasType, QualifiedName, Type as CanType, Union, primitives,
};
use nash_ir::ty::{AdtRef, Adts, BigTy, ConstTy, TermTy, Ty};
use nash_plutus::arena::Arena;
use nash_region::Located;

pub type Substitution<'a> = BTreeMap<&'a str, &'a Located<CanType<'a>>>;
pub type Layout<'a> = &'a [&'a [Ty<'a>]];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TypeError<'a> {
    #[error("unknown canonical type {0:?}")]
    UnknownType(QualifiedName<'a>),
    #[error("type {reference:?} expects at most {expected} arguments, got {actual}")]
    Arity {
        reference: QualifiedName<'a>,
        expected: usize,
        actual: usize,
    },
    #[error("a record type requires its nominal alias")]
    BareRecord,
    #[error("type does not expose a single set of labeled fields")]
    NoFields,
    #[error("type application does not have a type-constructor head")]
    InvalidApplication,
    #[error("invalid declaration order in constructor or record fields")]
    InvalidLayout,
    #[error("no canonical arguments registered for ADT instance {0:?}")]
    UnregisteredInstantiation(AdtRef<'a>),
    #[error("type constructor {0:?} is not fully applied")]
    Unsaturated(AdtRef<'a>),
}

/// Convert solved canonical types. Substitutions are simultaneous, as in the
/// canonicalizer. Missing variables stay erased until an operation needs their
/// representation. Constructor arguments retain their canonical spelling and
/// alias templates so lazy layout substitution can apply higher-kinded heads.
pub struct TypeEnv<'a, 'env> {
    arena: &'a Arena,
    unions: &'env HashMap<QualifiedName<'a>, &'a Union<'a>>,
    aliases: HashMap<QualifiedName<'a>, &'a Alias<'a>>,
    canonical_args: HashMap<AdtRef<'a>, &'a [&'a Located<CanType<'a>>]>,
    adts: Adts<'a>,
}

impl<'a, 'env> TypeEnv<'a, 'env> {
    pub fn new(arena: &'a Arena, unions: &'env HashMap<QualifiedName<'a>, &'a Union<'a>>) -> Self {
        Self {
            arena,
            unions,
            aliases: HashMap::new(),
            canonical_args: HashMap::new(),
            adts: Adts::default(),
        }
    }

    /// Canonical Alias nodes carry their own templates; this table additionally
    /// supports callers holding a Named reference to an alias declaration.
    pub fn insert_alias(&mut self, reference: QualifiedName<'a>, alias: &'a Alias<'a>) {
        self.aliases.insert(reference, alias);
    }

    pub fn adts(&self) -> &Adts<'a> {
        &self.adts
    }

    pub fn ty(
        &mut self,
        typ: &'a Located<CanType<'a>>,
        substitution: &Substitution<'a>,
    ) -> Result<Ty<'a>, TypeError<'a>> {
        self.convert(nash_can::types::substitute_type(
            self.arena.as_bump(),
            substitution,
            typ,
        ))
    }

    /// Labeled fields in declaration (wire) order. Transparent aliases are
    /// opened until a nominal record or single labeled constructor is reached.
    pub fn fields(
        &mut self,
        typ: &'a Located<CanType<'a>>,
        substitution: &Substitution<'a>,
    ) -> Result<Vec<(&'a str, u16, Ty<'a>)>, TypeError<'a>> {
        let typ = nash_can::types::substitute_type(self.arena.as_bump(), substitution, typ);
        self.fields_of(typ)
    }

    fn fields_of(
        &mut self,
        typ: &'a Located<CanType<'a>>,
    ) -> Result<Vec<(&'a str, u16, Ty<'a>)>, TypeError<'a>> {
        match &typ.value {
            CanType::Alias {
                arguments,
                remaining: [],
                target,
                ..
            } => {
                let body = match target {
                    AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
                };
                let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
                let body =
                    nash_can::types::substitute_type(self.arena.as_bump(), &substitution, body);
                if let CanType::Record { fields } = &body.value {
                    let mut fields = fields.to_vec();
                    fields.sort_by_key(|f| f.index);
                    if fields
                        .iter()
                        .enumerate()
                        .any(|(i, f)| i != usize::from(f.index))
                    {
                        return Err(TypeError::InvalidLayout);
                    }
                    fields
                        .into_iter()
                        .map(|f| Ok((f.field, f.index, self.convert(f.typ)?)))
                        .collect()
                } else {
                    self.fields_of(body)
                }
            }
            CanType::Named { reference, args } => {
                if let Some(alias) = self.aliases.get(reference).copied() {
                    if args.len() < alias.parameters.len() {
                        return Err(TypeError::NoFields);
                    }
                    let substitution = alias
                        .parameters
                        .iter()
                        .copied()
                        .zip(args.iter().copied())
                        .collect();
                    let body = nash_can::types::substitute_type(
                        self.arena.as_bump(),
                        &substitution,
                        alias.typ,
                    );
                    if args.len() == alias.parameters.len() {
                        let node = self.arena.alloc(Located::at_zero(CanType::Alias {
                            reference: *reference,
                            arguments: &[],
                            remaining: &[],
                            target: AliasType::Open(body),
                        }));
                        return self.fields_of(node);
                    }
                    return self.fields_applied(body, &args[alias.parameters.len()..]);
                }
                let union = *self
                    .unions
                    .get(reference)
                    .ok_or(TypeError::UnknownType(*reference))?;
                let fields = union.labeled_fields().ok_or(TypeError::NoFields)?;
                let ty = self.convert(typ)?;
                let adt = match ty {
                    Ty::Big(BigTy::Adt(adt)) | Ty::Term(TermTy::Adt(adt)) => *adt,
                    _ => return Err(TypeError::NoFields),
                };
                let layout = self.layout(adt)?;
                Ok(fields
                    .into_iter()
                    .zip(layout[0])
                    .map(|(f, ty)| (f.field, f.index, *ty))
                    .collect())
            }
            CanType::App { head, args } => self.fields_applied(head, args),
            _ => Err(TypeError::NoFields),
        }
    }

    fn fields_applied(
        &mut self,
        head: &'a Located<CanType<'a>>,
        args: &'a [&'a Located<CanType<'a>>],
    ) -> Result<Vec<(&'a str, u16, Ty<'a>)>, TypeError<'a>> {
        if let CanType::Alias {
            arguments,
            remaining: [],
            target,
            ..
        } = &head.value
        {
            let body = match target {
                AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
            };
            let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
            let body = nash_can::types::substitute_type(self.arena.as_bump(), &substitution, body);
            return self.fields_applied(body, args);
        }
        if !matches!(
            &head.value,
            CanType::Named { .. } | CanType::Alias { .. } | CanType::App { .. }
        ) {
            return Err(TypeError::NoFields);
        }
        let app = self
            .arena
            .alloc(Located::at_zero(CanType::App { head, args }));
        let app = nash_can::types::substitute_type(self.arena.as_bump(), &BTreeMap::new(), app);
        self.fields_of(app)
    }

    /// Compute exactly one level. Nested ADTs register only their argument
    /// templates; no nested layout is forced, including polymorphic recursion.
    pub fn layout(&mut self, adt: AdtRef<'a>) -> Result<Layout<'a>, TypeError<'a>> {
        if let Some(layout) = self.adts.layouts.get(&adt) {
            return Ok(layout);
        }
        let union = *self
            .unions
            .get(&adt.name)
            .ok_or(TypeError::UnknownType(adt.name))?;
        if adt.args.len() != union.parameters.len() {
            return Err(TypeError::Unsaturated(adt));
        }
        let args = *self
            .canonical_args
            .get(&adt)
            .ok_or(TypeError::UnregisteredInstantiation(adt))?;
        let substitution = union
            .parameters
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect();
        let mut ctors = union.ctors.to_vec();
        ctors.sort_by_key(|ctor| ctor.index);
        let mut layout = Vec::with_capacity(ctors.len());
        for (index, ctor) in ctors.into_iter().enumerate() {
            if usize::from(ctor.index) != index || usize::from(ctor.arity) != ctor.arguments.len() {
                return Err(TypeError::InvalidLayout);
            }
            let fields = ctor
                .arguments
                .iter()
                .map(|typ| self.ty(typ, &substitution))
                .collect::<Result<Vec<_>, _>>()?;
            layout.push(self.arena.alloc_slice_copy(&fields));
        }
        let layout = self.arena.alloc_slice_copy(&layout);
        self.adts.layouts.insert(adt, layout);
        Ok(layout)
    }

    fn convert(&mut self, typ: &'a Located<CanType<'a>>) -> Result<Ty<'a>, TypeError<'a>> {
        match &typ.value {
            CanType::Var(_) => Ok(Ty::Erased),
            CanType::Lambda { from, to } => {
                let from = self.convert(from)?;
                let to = self.convert(to)?;
                let (args, result) = match to {
                    Ty::Term(TermTy::Fun(args, result)) => {
                        let mut combined = vec![from];
                        combined.extend_from_slice(args);
                        (self.arena.alloc_slice_copy(&combined), *result)
                    }
                    _ => (self.arena.alloc_slice_copy(&[from]), to),
                };
                Ok(Ty::Term(self.arena.alloc(TermTy::Fun(args, result))))
            }
            CanType::Tuple {
                first,
                second,
                rest,
            } => {
                let fields = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|t| self.convert(t))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Ty::Term(self.arena.alloc(TermTy::Tuple(
                    self.arena.alloc_slice_copy(&fields),
                ))))
            }
            CanType::Named { reference, args } => self.named(*reference, args),
            CanType::Alias {
                reference,
                arguments,
                remaining,
                target,
            } => {
                if !remaining.is_empty() {
                    let args = self
                        .arena
                        .alloc_slice_fill_iter(arguments.iter().map(|a| a.typ));
                    return Ok(Ty::Constructor(self.instance(*reference, args)?));
                }
                let body = match target {
                    AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
                };
                let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
                let body =
                    nash_can::types::substitute_type(self.arena.as_bump(), &substitution, body);
                if let CanType::Record { fields } = &body.value {
                    let mut fields = fields.to_vec();
                    fields.sort_by_key(|field| field.index);
                    if fields
                        .iter()
                        .enumerate()
                        .any(|(i, f)| usize::from(f.index) != i)
                    {
                        return Err(TypeError::InvalidLayout);
                    }
                    let fields = fields
                        .iter()
                        .map(|f| self.convert(f.typ))
                        .collect::<Result<Vec<_>, _>>()?;
                    let fields = self.arena.alloc_slice_copy(&fields);
                    Ok(if is_big(reference.name) {
                        Ty::Big(self.arena.alloc(BigTy::Record(fields)))
                    } else {
                        Ty::Term(self.arena.alloc(TermTy::Record(fields)))
                    })
                } else {
                    self.convert(body)
                }
            }
            CanType::Record { .. } => Err(TypeError::BareRecord),
            CanType::App { head, args } => self.application(head, args),
        }
    }

    fn application(
        &mut self,
        head: &'a Located<CanType<'a>>,
        args: &'a [&'a Located<CanType<'a>>],
    ) -> Result<Ty<'a>, TypeError<'a>> {
        if args.is_empty() {
            return self.convert(head);
        }
        if let CanType::Var(_) = head.value {
            return Ok(Ty::Erased);
        }
        // A saturated transparent alias can itself denote a constructor. Open
        // its body before applying the excess suffix; canonical apply_type
        // deliberately leaves this case as App.
        if let CanType::Alias {
            arguments,
            remaining: [],
            target,
            ..
        } = &head.value
        {
            let body = match target {
                AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
            };
            let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
            let body = nash_can::types::substitute_type(self.arena.as_bump(), &substitution, body);
            return self.application(body, args);
        }
        if let CanType::Named {
            reference,
            args: existing,
        } = &head.value
        {
            let combined = self
                .arena
                .alloc_slice_fill_iter(existing.iter().chain(args).copied().collect::<Vec<_>>());
            return self.named(*reference, combined);
        }
        if let CanType::App {
            head,
            args: existing,
        } = &head.value
        {
            let combined = self
                .arena
                .alloc_slice_fill_iter(existing.iter().chain(args).copied().collect::<Vec<_>>());
            return self.application(head, combined);
        }
        if matches!(&head.value, CanType::Alias { .. }) {
            let app = self
                .arena
                .alloc(Located::at(head.region, CanType::App { head, args }));
            let applied =
                nash_can::types::substitute_type(self.arena.as_bump(), &BTreeMap::new(), app);
            return self.convert(applied);
        }
        Err(TypeError::InvalidApplication)
    }

    fn instance(
        &mut self,
        reference: QualifiedName<'a>,
        args: &'a [&'a Located<CanType<'a>>],
    ) -> Result<AdtRef<'a>, TypeError<'a>> {
        let converted = args
            .iter()
            .map(|t| self.convert(t))
            .collect::<Result<Vec<_>, _>>()?;
        let adt = AdtRef {
            name: reference,
            args: self.arena.alloc_slice_copy(&converted),
        };
        self.canonical_args.entry(adt).or_insert(args);
        Ok(adt)
    }

    fn named(
        &mut self,
        reference: QualifiedName<'a>,
        args: &'a [&'a Located<CanType<'a>>],
    ) -> Result<Ty<'a>, TypeError<'a>> {
        let primitive = (reference.home == primitives::builtin_home())
            .then(|| {
                primitives::PRIMITIVES
                    .iter()
                    .find(|p| p.name == reference.name)
            })
            .flatten();
        if primitive.is_none()
            && let Some(alias) = self.aliases.get(&reference).copied()
        {
            let consumed = args.len().min(alias.parameters.len());
            let arguments = self.arena.alloc_slice_fill_iter(
                alias
                    .parameters
                    .iter()
                    .zip(args)
                    .map(|(name, typ)| AliasArgument { name, typ }),
            );
            let typ = self.arena.alloc(Located::at_zero(CanType::Alias {
                reference,
                arguments,
                remaining: &alias.parameters[consumed..],
                target: AliasType::Open(alias.typ),
            }));
            return if consumed == args.len() {
                self.convert(typ)
            } else {
                self.application(typ, &args[consumed..])
            };
        }
        let expected = if let Some(p) = primitive {
            p.kind.arity()
        } else {
            self.unions
                .get(&reference)
                .ok_or(TypeError::UnknownType(reference))?
                .parameters
                .len()
        };
        if args.len() > expected {
            return Err(TypeError::Arity {
                reference,
                expected,
                actual: args.len(),
            });
        }
        let adt = self.instance(reference, args)?;
        if args.len() < expected {
            return Ok(Ty::Constructor(adt));
        }
        let args = adt.args;
        let ty = if primitive.is_some() {
            match reference.name {
                "Int" => Ty::Big(self.arena.alloc(BigTy::Int)),
                "Bytes" => Ty::Big(self.arena.alloc(BigTy::Bytes)),
                "Data" => Ty::Big(self.arena.alloc(BigTy::Data)),
                "List" => Ty::Big(self.arena.alloc(BigTy::List(args[0]))),
                "Map" => Ty::Big(self.arena.alloc(BigTy::Map(args[0], args[1]))),
                "int" => Ty::Const(self.arena.alloc(ConstTy::Int)),
                "bytes" => Ty::Const(self.arena.alloc(ConstTy::Bytes)),
                "string" => Ty::Const(self.arena.alloc(ConstTy::String)),
                "bool" => Ty::Const(self.arena.alloc(ConstTy::Bool)),
                "unit" => Ty::Const(self.arena.alloc(ConstTy::Unit)),
                "list" => Ty::Const(self.arena.alloc(ConstTy::List(args[0]))),
                "pair" => Ty::Const(self.arena.alloc(ConstTy::Pair(args[0], args[1]))),
                "array" => Ty::Const(self.arena.alloc(ConstTy::Array(args[0]))),
                "bls_g1" => Ty::Const(self.arena.alloc(ConstTy::BlsG1)),
                "bls_g2" => Ty::Const(self.arena.alloc(ConstTy::BlsG2)),
                "bls_mlr" => Ty::Const(self.arena.alloc(ConstTy::BlsMlr)),
                "value" => Ty::Const(self.arena.alloc(ConstTy::Value)),
                _ => return Err(TypeError::UnknownType(reference)),
            }
        } else if is_big(reference.name) {
            Ty::Big(self.arena.alloc(BigTy::Adt(adt)))
        } else {
            Ty::Term(self.arena.alloc(TermTy::Adt(adt)))
        };
        Ok(ty)
    }
}

fn is_big(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_ast::{AliasArgument, AliasType, Ctor, CtorOpts, FieldType, Kind, ModuleName};

    fn located<'a>(arena: &'a Arena, value: CanType<'a>) -> &'a Located<CanType<'a>> {
        arena.alloc(Located::at_zero(value))
    }
    fn name(text: &str) -> QualifiedName<'_> {
        QualifiedName {
            home: ModuleName {
                package: None,
                name: "Example",
            },
            name: text,
        }
    }
    fn named<'a>(
        arena: &'a Arena,
        reference: QualifiedName<'a>,
        args: &[&'a Located<CanType<'a>>],
    ) -> &'a Located<CanType<'a>> {
        located(
            arena,
            CanType::Named {
                reference,
                args: arena.alloc_slice_copy(args),
            },
        )
    }
    fn primitive<'a>(
        arena: &'a Arena,
        text: &'a str,
        args: &[&'a Located<CanType<'a>>],
    ) -> &'a Located<CanType<'a>> {
        named(
            arena,
            QualifiedName {
                home: primitives::builtin_home(),
                name: text,
            },
            args,
        )
    }
    fn union<'a>(
        arena: &'a Arena,
        text: &'a str,
        parameters: &'a [&'a str],
        kind: &'a Kind<'a>,
        arguments: &[&'a Located<CanType<'a>>],
    ) -> &'a Union<'a> {
        let ctor = arena.alloc(Ctor {
            labels: None,
            name: text,
            index: 0,
            arity: arguments.len() as u16,
            arguments: arena.alloc_slice_copy(arguments),
        });
        arena.alloc(Union {
            kind,
            context: &[],
            name: arena.alloc(Located::at_zero(text)),
            parameters,
            ctors: arena.alloc_slice_copy(&[&*ctor]),
            alternatives: 1,
            options: CtorOpts::Normal,
        })
    }
    fn adt(ty: Ty<'_>) -> AdtRef<'_> {
        match ty {
            Ty::Big(BigTy::Adt(a)) | Ty::Term(TermTy::Adt(a)) => *a,
            _ => panic!("expected ADT, got {ty:?}"),
        }
    }

    #[test]
    fn exact_primitive_identity_and_complete_inventory() {
        let arena = Arena::new();
        let user = name("int");
        let fake_builtin = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Builtin",
            },
            name: "Int",
        };
        let unions = HashMap::from([
            (user, union(&arena, "int", &[], &Kind::Type, &[])),
            (fake_builtin, union(&arena, "Int", &[], &Kind::Type, &[])),
        ]);
        let mut env = TypeEnv::new(&arena, &unions);
        let subst = BTreeMap::new();
        assert_eq!(
            env.ty(primitive(&arena, "int", &[]), &subst).unwrap(),
            Ty::Const(&ConstTy::Int)
        );
        assert!(matches!(
            env.ty(named(&arena, user, &[]), &subst).unwrap(),
            Ty::Term(TermTy::Adt(_))
        ));
        assert!(matches!(
            env.ty(named(&arena, fake_builtin, &[]), &subst).unwrap(),
            Ty::Big(BigTy::Adt(_))
        ));
        for p in primitives::PRIMITIVES {
            let args = vec![primitive(&arena, "Int", &[]); p.kind.arity()];
            let converted = env.ty(primitive(&arena, p.name, &args), &subst).unwrap();
            assert_eq!(converted.repr(), Some(p.repr), "{}", p.name);
        }
    }

    #[test]
    fn polymorphically_recursive_layout_is_one_level_only() {
        let arena = Arena::new();
        let a = located(&arena, CanType::Var("a"));
        let nested = named(&arena, name("Nest"), &[primitive(&arena, "List", &[a])]);
        let unions = HashMap::from([(
            name("Nest"),
            union(
                &arena,
                "Nest",
                &["a"],
                &Kind::Arrow(&Kind::Type, &Kind::Type),
                &[nested],
            ),
        )]);
        let mut env = TypeEnv::new(&arena, &unions);
        let root = adt(env
            .ty(
                named(&arena, name("Nest"), &[primitive(&arena, "Int", &[])]),
                &BTreeMap::new(),
            )
            .unwrap());
        assert!(env.adts().layouts.is_empty());
        let next = adt(env.layout(root).unwrap()[0][0]);
        assert_eq!(env.adts().layouts.len(), 1);
        assert_eq!(next.args, &[Ty::Big(&BigTy::List(Ty::Big(&BigTy::Int)))]);
        let third = adt(env.layout(next).unwrap()[0][0]);
        assert_ne!(third, next);
        assert_eq!(env.adts().layouts.len(), 2);
        assert_eq!(env.layout(root).unwrap()[0][0], Ty::Big(&BigTy::Adt(next)));
    }

    #[test]
    fn higher_kinded_adt_arguments_keep_constructor_identity() {
        let arena = Arena::new();
        let body = located(
            &arena,
            CanType::App {
                head: located(&arena, CanType::Var("f")),
                args: arena.alloc_slice_copy(&[located(&arena, CanType::Var("a"))]),
            },
        );
        let kind = Kind::Arrow(
            &Kind::Arrow(&Kind::Type, &Kind::Type),
            &Kind::Arrow(&Kind::Type, &Kind::Type),
        );
        let unions = HashMap::from([(
            name("wrap"),
            union(&arena, "wrap", &["f", "a"], &kind, &[body]),
        )]);
        let mut env = TypeEnv::new(&arena, &unions);
        let int = primitive(&arena, "int", &[]);
        let list = adt(env
            .ty(
                named(&arena, name("wrap"), &[primitive(&arena, "list", &[]), int]),
                &BTreeMap::new(),
            )
            .unwrap());
        let array = adt(env
            .ty(
                named(
                    &arena,
                    name("wrap"),
                    &[primitive(&arena, "array", &[]), int],
                ),
                &BTreeMap::new(),
            )
            .unwrap());
        assert_ne!(list, array);
        assert_eq!(
            env.layout(list).unwrap()[0],
            &[Ty::Const(&ConstTy::List(Ty::Const(&ConstTy::Int)))]
        );
        assert_eq!(
            env.layout(array).unwrap()[0],
            &[Ty::Const(&ConstTy::Array(Ty::Const(&ConstTy::Int)))]
        );
        assert_eq!(list.args[0].repr(), None);
        assert_eq!(list.args[0].plutus_type(&arena), None);
    }

    #[test]
    fn partial_alias_applies_after_higher_kinded_substitution() {
        let arena = Arena::new();
        let int = primitive(&arena, "int", &[]);
        let bytes = primitive(&arena, "bytes", &[]);
        let body = primitive(
            &arena,
            "pair",
            &[
                located(&arena, CanType::Var("left")),
                located(&arena, CanType::Var("right")),
            ],
        );
        let alias = located(
            &arena,
            CanType::Alias {
                reference: name("PairWith"),
                arguments: arena.alloc_slice_fill_iter([AliasArgument {
                    name: "left",
                    typ: int,
                }]),
                remaining: &["right"],
                target: AliasType::Filled { body, typ: body },
            },
        );
        let applied = located(
            &arena,
            CanType::App {
                head: located(&arena, CanType::Var("f")),
                args: arena.alloc_slice_copy(&[located(&arena, CanType::Var("a"))]),
            },
        );
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        let subst = BTreeMap::from([("f", alias), ("a", bytes)]);
        assert_eq!(
            env.ty(applied, &subst).unwrap(),
            Ty::Const(&ConstTy::Pair(
                Ty::Const(&ConstTy::Int),
                Ty::Const(&ConstTy::Bytes)
            ))
        );
        assert!(matches!(
            env.ty(alias, &BTreeMap::new()).unwrap(),
            Ty::Constructor(_)
        ));
    }

    #[test]
    fn transparent_aliases_preserve_nominal_record_representation_and_wire_order() {
        let arena = Arena::new();
        let body = located(
            &arena,
            CanType::Record {
                fields: arena.alloc_slice_copy(&[
                    FieldType {
                        index: 1,
                        field: "a",
                        typ: primitive(&arena, "bytes", &[]),
                    },
                    FieldType {
                        index: 0,
                        field: "z",
                        typ: primitive(&arena, "int", &[]),
                    },
                ]),
            },
        );
        let record = located(
            &arena,
            CanType::Alias {
                reference: name("Record"),
                arguments: &[],
                remaining: &[],
                target: AliasType::Open(body),
            },
        );
        let transparent = located(
            &arena,
            CanType::Alias {
                reference: name("small"),
                arguments: &[],
                remaining: &[],
                target: AliasType::Open(record),
            },
        );
        let little = located(
            &arena,
            CanType::Alias {
                reference: name("record"),
                arguments: &[],
                remaining: &[],
                target: AliasType::Open(body),
            },
        );
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        let fields = &[Ty::Const(&ConstTy::Int), Ty::Const(&ConstTy::Bytes)];
        assert_eq!(
            env.ty(transparent, &BTreeMap::new()).unwrap(),
            Ty::Big(&BigTy::Record(fields))
        );
        assert_eq!(
            env.ty(little, &BTreeMap::new()).unwrap(),
            Ty::Term(&TermTy::Record(fields))
        );
        assert!(env.ty(body, &BTreeMap::new()).is_err());
    }

    #[test]
    fn free_pass_through_variables_are_erased() {
        let arena = Arena::new();
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        let ty = env
            .ty(located(&arena, CanType::Var("a")), &BTreeMap::new())
            .unwrap();
        assert_eq!(ty, Ty::Erased);
        assert_eq!(ty.repr(), None);
        assert_eq!(ty.plutus_type(&arena), None);
    }
    #[test]
    fn lazy_layout_keeps_partial_alias_templates() {
        let arena = Arena::new();
        let int = primitive(&arena, "int", &[]);
        let bytes = primitive(&arena, "bytes", &[]);
        let alias_body = primitive(
            &arena,
            "pair",
            &[
                located(&arena, CanType::Var("left")),
                located(&arena, CanType::Var("right")),
            ],
        );
        let alias = located(
            &arena,
            CanType::Alias {
                reference: name("withInt"),
                arguments: arena.alloc_slice_fill_iter([AliasArgument {
                    name: "left",
                    typ: int,
                }]),
                remaining: &["right"],
                target: AliasType::Open(alias_body),
            },
        );
        let field = located(
            &arena,
            CanType::App {
                head: located(&arena, CanType::Var("f")),
                args: arena.alloc_slice_copy(&[bytes]),
            },
        );
        let kind = Kind::Arrow(&Kind::Arrow(&Kind::Type, &Kind::Type), &Kind::Type);
        let unions =
            HashMap::from([(name("wrap"), union(&arena, "wrap", &["f"], &kind, &[field]))]);
        let mut env = TypeEnv::new(&arena, &unions);
        let instance = adt(env
            .ty(named(&arena, name("wrap"), &[alias]), &BTreeMap::new())
            .unwrap());
        assert!(env.adts().layouts.is_empty());
        assert_eq!(
            env.layout(instance).unwrap()[0],
            &[Ty::Const(&ConstTy::Pair(
                Ty::Const(&ConstTy::Int),
                Ty::Const(&ConstTy::Bytes)
            ))]
        );
    }

    #[test]
    fn named_alias_can_return_a_partially_applied_constructor() {
        let arena = Arena::new();
        let int = primitive(&arena, "int", &[]);
        let bytes = primitive(&arena, "bytes", &[]);
        let body = primitive(&arena, "pair", &[located(&arena, CanType::Var("left"))]);
        let alias = arena.alloc(Alias {
            name: arena.alloc(Located::at_zero("with")),
            kind: &Kind::Arrow(&Kind::Type, &Kind::Arrow(&Kind::Type, &Kind::Type)),
            context: &[],
            parameters: &["left"],
            typ: body,
        });
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        env.insert_alias(name("with"), alias);
        let ty = env
            .ty(named(&arena, name("with"), &[int, bytes]), &BTreeMap::new())
            .unwrap();
        assert_eq!(
            ty,
            Ty::Const(&ConstTy::Pair(
                Ty::Const(&ConstTy::Int),
                Ty::Const(&ConstTy::Bytes)
            ))
        );
    }

    #[test]
    fn application_does_not_capture_partial_alias_binders() {
        let arena = Arena::new();
        let right = located(&arena, CanType::Var("right"));
        let body = primitive(
            &arena,
            "pair",
            &[located(&arena, CanType::Var("left")), right],
        );
        let partial = located(
            &arena,
            CanType::Alias {
                reference: name("with"),
                arguments: arena.alloc_slice_fill_iter([AliasArgument {
                    name: "left",
                    typ: right,
                }]),
                remaining: &["right"],
                target: AliasType::Open(body),
            },
        );
        let app = located(
            &arena,
            CanType::App {
                head: partial,
                args: arena.alloc_slice_copy(&[primitive(&arena, "bytes", &[])]),
            },
        );
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        let ty = env
            .ty(
                app,
                &BTreeMap::from([("right", primitive(&arena, "int", &[]))]),
            )
            .unwrap();
        assert_eq!(
            ty,
            Ty::Const(&ConstTy::Pair(
                Ty::Const(&ConstTy::Int),
                Ty::Const(&ConstTy::Bytes)
            ))
        );
    }

    #[test]
    fn constructor_and_record_layouts_follow_indices() {
        let arena = Arena::new();
        let first = arena.alloc(Ctor {
            labels: None,
            name: "First",
            index: 0,
            arity: 1,
            arguments: arena.alloc_slice_copy(&[primitive(&arena, "int", &[])]),
        });
        let second = arena.alloc(Ctor {
            labels: None,
            name: "Second",
            index: 1,
            arity: 1,
            arguments: arena.alloc_slice_copy(&[primitive(&arena, "bytes", &[])]),
        });
        let union = arena.alloc(Union {
            kind: &Kind::Type,
            context: &[],
            name: arena.alloc(Located::at_zero("Choice")),
            parameters: &[],
            ctors: arena.alloc_slice_copy(&[&*second, &*first]),
            alternatives: 2,
            options: CtorOpts::Normal,
        });
        let unions = HashMap::from([(name("Choice"), &*union)]);
        let mut env = TypeEnv::new(&arena, &unions);
        let instance = adt(env
            .ty(named(&arena, name("Choice"), &[]), &BTreeMap::new())
            .unwrap());
        assert_eq!(
            env.layout(instance).unwrap(),
            &[
                &[Ty::Const(&ConstTy::Int)][..],
                &[Ty::Const(&ConstTy::Bytes)][..]
            ]
        );
        assert!(
            env.ty(
                primitive(&arena, "int", &[primitive(&arena, "int", &[])]),
                &BTreeMap::new()
            )
            .is_err()
        );
    }
    #[test]
    fn public_fields_query_resolves_alias_parameters_in_wire_order() {
        let arena = Arena::new();
        let body = located(
            &arena,
            CanType::Record {
                fields: arena.alloc_slice_copy(&[
                    FieldType {
                        index: 1,
                        field: "a",
                        typ: located(&arena, CanType::Var("v")),
                    },
                    FieldType {
                        index: 0,
                        field: "z",
                        typ: primitive(&arena, "bytes", &[]),
                    },
                ]),
            },
        );
        let alias = located(
            &arena,
            CanType::Alias {
                reference: name("row"),
                arguments: arena.alloc_slice_fill_iter([AliasArgument {
                    name: "v",
                    typ: located(&arena, CanType::Var("caller")),
                }]),
                remaining: &[],
                target: AliasType::Open(body),
            },
        );
        let outer = located(
            &arena,
            CanType::Alias {
                reference: name("Outer"),
                arguments: arena.alloc_slice_fill_iter([AliasArgument {
                    name: "caller",
                    typ: located(&arena, CanType::Var("site")),
                }]),
                remaining: &[],
                target: AliasType::Open(alias),
            },
        );
        let unions = HashMap::new();
        let mut env = TypeEnv::new(&arena, &unions);
        assert_eq!(
            env.fields(
                outer,
                &BTreeMap::from([("site", primitive(&arena, "int", &[]))])
            )
            .unwrap(),
            vec![
                ("z", 0, Ty::Const(&ConstTy::Bytes)),
                ("a", 1, Ty::Const(&ConstTy::Int))
            ]
        );
    }

    #[test]
    fn public_fields_query_handles_single_labeled_constructor() {
        let arena = Arena::new();
        let ctor = arena.alloc(Ctor {
            labels: Some(&["z", "a"]),
            name: "Row",
            index: 0,
            arity: 2,
            arguments: arena.alloc_slice_copy(&[
                primitive(&arena, "int", &[]),
                primitive(&arena, "bytes", &[]),
            ]),
        });
        let union = arena.alloc(Union {
            kind: &Kind::Type,
            context: &[],
            name: arena.alloc(Located::at_zero("row")),
            parameters: &[],
            ctors: arena.alloc_slice_copy(&[&*ctor]),
            alternatives: 1,
            options: CtorOpts::Normal,
        });
        let unions = HashMap::from([(name("row"), &*union)]);
        let mut env = TypeEnv::new(&arena, &unions);
        assert_eq!(
            env.fields(named(&arena, name("row"), &[]), &BTreeMap::new())
                .unwrap(),
            vec![
                ("z", 0, Ty::Const(&ConstTy::Int)),
                ("a", 1, Ty::Const(&ConstTy::Bytes))
            ]
        );
    }
}
