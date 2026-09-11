//! Reachable, lexical specialization of canonical definitions.
use std::collections::{BTreeMap, HashMap};

use nash_ast::{self as can, *};
use nash_can::environment::Tables;
use nash_ir::{
    build::Builder,
    core::*,
    ty::{BigTy, ConstTy, Ty},
};
use nash_plutus::arena::Arena;
use nash_region::Located;
use nash_solve::{SolvedTypes, solved::Scheme};

use crate::{
    demand::{DemandKind, Demands},
    evidence::{self, ExecutableEvidence},
    ty_of::{Substitution, TypeEnv},
};

mod accessors;
mod emission;
use emission::hoist_strings;

const MAX_SPECIALIZATIONS: usize = 1024;
const MAX_TYPE_DEPTH: usize = 128;

#[derive(Clone, Copy)]
pub struct Input<'a, 's> {
    pub module: &'s can::Module<'a>,
    pub types: &'s SolvedTypes<'a>,
    pub tables: &'s Tables<'a>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceLevel {
    Silent,
    Compact,
    #[default]
    Verbose,
}
#[derive(Clone, Copy, Debug)]
pub struct TraceConfig {
    pub user: TraceLevel,
    pub compiler: bool,
}
impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            user: TraceLevel::Verbose,
            compiler: true,
        }
    }
}

#[derive(Debug)]
pub struct Specialization<'a> {
    pub definition: NodeId,
    pub name: Name<'a>,
    pub layouts: Vec<Ty<'a>>,
    pub evidence: Vec<ExecutableEvidence<'a>>,
}
pub struct Compiled<'a> {
    /// Casts are expanded. Recursion rewriting and final lowering follow this phase.
    pub core: &'a Core<'a>,
    pub root_type: Ty<'a>,
    pub specializations: Vec<Specialization<'a>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("unknown definition {0:?}")]
    UnknownDefinition(QualifiedName<'a>),
    #[error("unknown lexical binding {0}")]
    UnknownLocal(&'a str),
    #[error("missing solved metadata for {0:?}")]
    MissingType(NodeId),
    #[error("root needs {expected} explicit type arguments, got {actual}")]
    RootTypeArguments { expected: usize, actual: usize },
    #[error("invalid type instance for {0:?}")]
    InvalidInstance(NodeId),
    #[error(
        "specialization exceeded the {MAX_SPECIALIZATIONS}-body or {MAX_TYPE_DEPTH}-level type limit"
    )]
    SpecializationLimit,
    #[error("operation needs a concrete runtime layout, got {0}")]
    RuntimeLayout(Ty<'a>),
    #[error("missing method body {method} of {trait_:?}")]
    MissingMethod {
        trait_: QualifiedName<'a>,
        method: &'a str,
    },
    #[error("method type does not match its selected implementation")]
    MethodType,
    #[error("method context has no matching evidence")]
    MethodEvidence,
    #[error("cannot resolve evidence: {0:?}")]
    Resolution(nash_solve::evidence::Failure),
    #[error("recursive values must be functions")]
    RecursiveValue,
    #[error("recursive function initialization exceeded the 4096-step normalization limit")]
    RecursionNormalizationLimit,
    #[error("invalid constructor or record metadata")]
    InvalidConstructor,
    #[error("comptime expression is not closed or could not be assembled: {0}")]
    ComptimeAssembly(String),
    #[error("comptime evaluation failed: {0}")]
    ComptimeEvaluation(String),
    #[error("comptime expression did not evaluate to a constant")]
    ComptimeConstant,
    #[error("{0}")]
    Type(crate::ty_of::TypeError<'a>),
    #[error("{0}")]
    Evidence(evidence::Error<'a>),
    #[error("{0}")]
    Pattern(crate::decision_tree::Error<'a>),
    #[error("{0}")]
    Cast(crate::casts::Error<'a>),
}

impl<'a> From<crate::ty_of::TypeError<'a>> for Error<'a> {
    fn from(e: crate::ty_of::TypeError<'a>) -> Self {
        Self::Type(e)
    }
}
impl<'a> From<evidence::Error<'a>> for Error<'a> {
    fn from(e: evidence::Error<'a>) -> Self {
        Self::Evidence(e)
    }
}
impl<'a> From<crate::decision_tree::Error<'a>> for Error<'a> {
    fn from(e: crate::decision_tree::Error<'a>) -> Self {
        Self::Pattern(e)
    }
}
impl<'a> From<crate::casts::Error<'a>> for Error<'a> {
    fn from(e: crate::casts::Error<'a>) -> Self {
        Self::Cast(e)
    }
}

pub struct Build<'a, 's> {
    pub(crate) inputs: Vec<Input<'a, 's>>,
    pub(crate) unions: HashMap<QualifiedName<'a>, &'a Union<'a>>,
    pub(crate) aliases: HashMap<QualifiedName<'a>, &'a Alias<'a>>,
    pub(crate) tables: Tables<'a>,
    pub(crate) demands: Demands<'a>,
}

impl<'a, 's> Build<'a, 's> {
    pub fn new(inputs: impl IntoIterator<Item = Input<'a, 's>>) -> Self {
        let inputs: Vec<_> = inputs.into_iter().collect();
        let mut unions = HashMap::new();
        let mut aliases = HashMap::new();
        let mut tables = Tables::default();
        for input in &inputs {
            for union in input.module.unions {
                unions.insert(
                    QualifiedName {
                        home: input.module.name,
                        name: union.value.name.value,
                    },
                    &union.value,
                );
            }
            for alias in input.module.aliases {
                aliases.insert(
                    QualifiedName {
                        home: input.module.name,
                        name: alias.value.name.value,
                    },
                    &alias.value,
                );
            }
            tables.traits.extend(input.tables.traits.clone());
            tables.impls.extend(input.tables.impls.clone());
            tables.kinds.types.extend(input.tables.kinds.types.clone());
            tables
                .kinds
                .traits
                .extend(input.tables.kinds.traits.clone());
            tables
                .kinds
                .superclasses
                .extend(input.tables.kinds.superclasses.clone());
            tables.fields.extend(input.tables.fields.clone());
        }
        let pairs: Vec<_> = inputs.iter().map(|i| (i.module, i.types)).collect();
        let demands = crate::demand::analyze(&pairs);
        Self {
            inputs,
            unions,
            aliases,
            tables,
            demands,
        }
    }

    pub fn compile(
        &self,
        arena: &'a Arena,
        root: QualifiedName<'a>,
        root_type_args: Option<&[&'a Located<Type<'a>>]>,
        trace: TraceConfig,
    ) -> Result<Compiled<'a>, Error<'a>> {
        let mut engine = Engine::new(self, arena, trace);
        let template = *engine
            .top
            .get(&root)
            .ok_or(Error::UnknownDefinition(root))?;
        let scheme = engine.scheme(template)?;
        let args = root_type_args.unwrap_or(&[]);
        if args.len() != scheme.annotation.free_vars.len() {
            return Err(Error::RootTypeArguments {
                expected: scheme.annotation.free_vars.len(),
                actual: args.len(),
            });
        }
        let substitution: Substitution<'_> = scheme
            .annotation
            .free_vars
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect();
        let evidence = engine.resolve_context(scheme.annotation.context, &substitution)?;
        let root_type = engine.types.ty(scheme.annotation.typ, &substitution)?;
        let binder = engine.request(template, substitution, evidence)?;
        engine.drain(0)?;
        let core = engine.emit_group(0, engine.ir.var(binder.name), false)?;
        let core =
            crate::casts::expand_with_traces(&engine.ir, &mut engine.types, core, trace.compiler)?;
        let core = accessors::share(&engine.ir, core);
        let core = hoist_strings(&engine.ir, core);
        let specializations = engine
            .specs
            .into_iter()
            .map(|s| Specialization {
                definition: engine.templates[s.template].id(),
                name: s.binder.name,
                layouts: s.key.layouts,
                evidence: s.key.evidence,
            })
            .collect();
        Ok(Compiled {
            core,
            root_type,
            specializations,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Binding<'a> {
    Value(Binder<'a>),
    Template {
        id: usize,
        projection: Option<&'a str>,
    },
}
#[derive(Clone)]
pub(crate) struct Context<'a> {
    pub input: usize,
    pub env: BTreeMap<&'a str, Binding<'a>>,
    pub subst: Substitution<'a>,
    /// Only runtime observations enter binder metadata. Unobserved quantified
    /// variables stay erased even when the first source instance is concrete.
    pub runtime_subst: Substitution<'a>,
    pub givens: HashMap<NodeId, &'a [Evidence<'a>]>,
}
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    Definition(&'a Def<'a>),
    Destruct {
        pattern: &'a Located<Pattern<'a>>,
        value: &'a Located<Expr<'a>>,
    },
}
#[derive(Clone)]
pub(crate) struct Template<'a> {
    pub source: Source<'a>,
    pub captured: Context<'a>,
    pub group: usize,
}
impl Template<'_> {
    pub(crate) fn id(&self) -> NodeId {
        match self.source {
            Source::Definition(d) => NodeId::def(definition(d).0),
            Source::Destruct { pattern, .. } => NodeId::pattern(pattern),
        }
    }
    fn name(&self) -> &str {
        match self.source {
            Source::Definition(d) => definition(d).0.value,
            Source::Destruct { .. } => "destruct",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Key<'a> {
    template: usize,
    evidence: Vec<ExecutableEvidence<'a>>,
    layouts: Vec<Ty<'a>>,
}
struct InstanceBody<'a> {
    template: usize,
    key: Key<'a>,
    binder: Binder<'a>,
    context: Context<'a>,
    value: Option<&'a Core<'a>>,
}
#[derive(Default)]
struct Group {
    specs: Vec<usize>,
    cursor: usize,
}

pub(crate) struct Engine<'a, 'b, 's> {
    pub build: &'b Build<'a, 's>,
    pub ir: Builder<'a>,
    pub types: TypeEnv<'a, 'b>,
    pub trace: TraceConfig,
    pub templates: Vec<Template<'a>>,
    groups: Vec<Group>,
    specs: Vec<InstanceBody<'a>>,
    top: HashMap<QualifiedName<'a>, usize>,
    pub methods: HashMap<(ImplRef<'a>, &'a str), usize>,
    pub defaults: HashMap<(QualifiedName<'a>, &'a str), usize>,
}

impl<'a, 'b, 's> Engine<'a, 'b, 's> {
    fn new(build: &'b Build<'a, 's>, arena: &'a Arena, trace: TraceConfig) -> Self {
        let mut types = TypeEnv::new(arena, &build.unions);
        for (name, alias) in &build.aliases {
            types.insert_alias(*name, alias);
        }
        let mut engine = Self {
            build,
            ir: Builder::new(arena),
            types,
            trace,
            templates: Vec::new(),
            groups: vec![Group::default()],
            specs: Vec::new(),
            top: HashMap::new(),
            methods: HashMap::new(),
            defaults: HashMap::new(),
        };
        for (input, unit) in build.inputs.iter().enumerate() {
            let ctx = Context {
                input,
                env: BTreeMap::new(),
                subst: Substitution::new(),
                runtime_subst: Substitution::new(),
                givens: HashMap::new(),
            };
            for def in declarations(unit.module.decls) {
                let id = engine.add_template(Source::Definition(def), ctx.clone(), 0);
                engine.top.insert(
                    QualifiedName {
                        home: unit.module.name,
                        name: definition(def).0.value,
                    },
                    id,
                );
            }
            for trait_ in unit.module.traits {
                for method in trait_.value.methods {
                    if let Some(def) = method.default {
                        let id = engine.add_template(Source::Definition(def), ctx.clone(), 0);
                        engine.defaults.insert(
                            (
                                QualifiedName {
                                    home: unit.module.name,
                                    name: trait_.value.name.value,
                                },
                                method.name.value,
                            ),
                            id,
                        );
                    }
                }
            }
            for impl_ in unit.module.impls {
                let key = build
                    .tables
                    .impls
                    .iter()
                    .find(|(key, info)| {
                        info.home == unit.module.name
                            && key.trait_ == impl_.value.trait_
                            && key.heads.iter().copied().eq(impl_
                                .value
                                .heads
                                .iter()
                                .map(|h| h.value))
                    })
                    .map(|(key, _)| *key);
                if let Some(key) = key {
                    for def in impl_.value.methods {
                        let id = engine.add_template(Source::Definition(def), ctx.clone(), 0);
                        engine.methods.insert(
                            (
                                ImplRef {
                                    home: unit.module.name,
                                    key,
                                },
                                definition(def).0.value,
                            ),
                            id,
                        );
                    }
                }
            }
        }
        for template in &mut engine.templates {
            let home = build.inputs[template.captured.input].module.name;
            template.captured.env = engine
                .top
                .iter()
                .filter(|(name, _)| name.home == home)
                .map(|(name, id)| {
                    (
                        name.name,
                        Binding::Template {
                            id: *id,
                            projection: None,
                        },
                    )
                })
                .collect();
        }
        engine
    }

    pub fn add_group(&mut self) -> usize {
        let id = self.groups.len();
        self.groups.push(Group::default());
        id
    }
    pub fn add_template(
        &mut self,
        source: Source<'a>,
        captured: Context<'a>,
        group: usize,
    ) -> usize {
        let id = self.templates.len();
        self.templates.push(Template {
            source,
            captured,
            group,
        });
        id
    }
    pub fn solved(&self, ctx: &Context<'a>) -> &'s SolvedTypes<'a> {
        self.build.inputs[ctx.input].types
    }
    pub fn instance_context(&self, name: Name<'a>) -> Option<&Context<'a>> {
        self.specs
            .iter()
            .find(|s| s.binder.name == name)
            .map(|s| &s.context)
    }
    pub fn scheme(&self, template: usize) -> Result<&'s Scheme<'a>, Error<'a>> {
        let t = &self.templates[template];
        self.build.inputs[t.captured.input]
            .types
            .schemes
            .get(&t.id())
            .ok_or(Error::MissingType(t.id()))
    }
    pub fn eager_template(&self, template: usize) -> Result<bool, Error<'a>> {
        let annotation = self.scheme(template)?.annotation;
        Ok(annotation.free_vars.is_empty()
            || (annotation.context.iter().all(|p| p.trait_ref().is_none())
                && self
                    .build
                    .demands
                    .get(&self.templates[template].id())
                    .is_none_or(|d| d.is_empty())))
    }
    pub fn can_type(
        &self,
        id: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        self.solved(ctx)
            .exprs
            .get(&id)
            .or_else(|| self.solved(ctx).patterns.get(&id))
            .copied()
            .ok_or(Error::MissingType(id))
    }
    pub fn ty(&mut self, id: NodeId, ctx: &Context<'a>) -> Result<Ty<'a>, Error<'a>> {
        let typ = self.can_type(id, ctx)?;
        check_type(typ, &ctx.runtime_subst)?;
        Ok(self.types.ty(typ, &ctx.runtime_subst)?)
    }
    pub fn substitute(
        &self,
        typ: &'a Located<Type<'a>>,
        subst: &Substitution<'a>,
    ) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        check_type(typ, subst)?;
        Ok(nash_can::types::substitute_type(
            self.ir.arena.as_bump(),
            subst,
            typ,
        ))
    }
    pub fn resolve_context(
        &self,
        predicates: &[Pred<'a>],
        subst: &Substitution<'a>,
    ) -> Result<&'a [Evidence<'a>], Error<'a>> {
        let mut values = Vec::new();
        for predicate in predicates {
            if predicate.trait_ref().is_none() {
                continue;
            }
            let pred = self.predicate(*predicate, subst)?;
            if let Some(value) =
                nash_solve::evidence::resolve(self.ir.arena.as_bump(), &self.build.tables, &pred)
                    .map_err(|e| Error::Resolution(e.reason))?
            {
                values.push(value);
            }
        }
        Ok(self.ir.arena.alloc_slice_fill_iter(values))
    }
    pub fn predicate(
        &self,
        predicate: Pred<'a>,
        subst: &Substitution<'a>,
    ) -> Result<Pred<'a>, Error<'a>> {
        let args = predicate
            .args()
            .iter()
            .map(|t| self.substitute(t, subst))
            .collect::<Result<Vec<_>, _>>()?;
        let args = self.ir.arena.alloc_slice_copy(&args);
        Ok(match predicate {
            Pred::Trait { trait_, .. } => Pred::Trait { trait_, args },
            Pred::Implied { trait_, .. } => Pred::Implied { trait_, args },
            Pred::Apply { head, .. } => Pred::Apply {
                head: self.substitute(head, subst)?,
                args,
            },
        })
    }
    pub fn use_template(
        &mut self,
        template: usize,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<Binder<'a>, Error<'a>> {
        let scheme = self.scheme(template)?;
        let mut subst = self.templates[template].captured.subst.clone();
        let evidence = if let Some(instance) = self.solved(ctx).instances.get(&node) {
            if instance.type_args.len() != scheme.annotation.free_vars.len() {
                return Err(Error::InvalidInstance(node));
            }
            for (name, typ) in scheme.annotation.free_vars.iter().zip(instance.type_args) {
                subst.insert(name, self.substitute(typ, &ctx.subst)?);
            }
            evidence::ground_arguments(
                self.ir.arena,
                &self.build.tables,
                instance.evidence,
                &ctx.subst,
                &ctx.givens,
            )?
        } else {
            if !scheme.annotation.free_vars.is_empty() {
                return Err(Error::InvalidInstance(node));
            }
            self.resolve_context(scheme.annotation.context, &subst)?
        };
        self.request(template, subst, evidence)
    }
    pub fn request(
        &mut self,
        template: usize,
        subst: Substitution<'a>,
        evidence: &'a [Evidence<'a>],
    ) -> Result<Binder<'a>, Error<'a>> {
        let t = &self.templates[template];
        let mut layouts = Vec::new();
        if let Some(demands) = self.build.demands.get(&t.id()) {
            for (name, demand) in demands {
                let Some(typ) = subst.get(name) else {
                    return Err(Error::RuntimeLayout(Ty::Erased));
                };
                check_type(typ, &Substitution::new())?;
                let ty = self.types.ty(typ, &Substitution::new())?;
                layouts.push(match demand {
                    DemandKind::Deep => ty,
                    DemandKind::Native => native(self.ir.arena, ty)?,
                });
            }
        }
        let evidence_key = evidence
            .iter()
            .map(evidence::executable_identity)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|e| *e != ExecutableEvidence::Erased)
            .collect();
        let key = Key {
            template,
            layouts,
            evidence: evidence_key,
        };
        if let Some(old) = self.specs.iter().find(|old| old.key == key) {
            return Ok(old.binder);
        }
        if self.specs.len() >= MAX_SPECIALIZATIONS {
            return Err(Error::SpecializationLimit);
        }
        let scheme = self.scheme(template)?;
        let mut runtime_subst = t.captured.runtime_subst.clone();
        for name in scheme.annotation.free_vars {
            runtime_subst.remove(name);
        }
        if let Some(demands) = self.build.demands.get(&t.id()) {
            for (name, kind) in demands {
                let typ = *subst.get(name).ok_or(Error::RuntimeLayout(Ty::Erased))?;
                let typ = match kind {
                    DemandKind::Deep => typ,
                    DemandKind::Native => {
                        native_type(self.ir.arena, self.types.ty(typ, &Substitution::new())?)?
                    }
                };
                runtime_subst.insert(name, typ);
            }
        }
        check_type(scheme.annotation.typ, &runtime_subst)?;
        let ty = self.types.ty(scheme.annotation.typ, &runtime_subst)?;
        let name = self.ir.fresh(self.ir.arena.as_bump().alloc_str(t.name()));
        let binder = Binder { name, ty };
        let mut context = t.captured.clone();
        context.subst = subst;
        context.runtime_subst = runtime_subst;
        context.givens.insert(scheme.binder, evidence);
        let index = self.specs.len();
        self.groups[t.group].specs.push(index);
        self.specs.push(InstanceBody {
            template,
            key,
            binder,
            context,
            value: None,
        });
        Ok(binder)
    }
    pub fn drain(&mut self, group: usize) -> Result<(), Error<'a>> {
        while self.groups[group].cursor < self.groups[group].specs.len() {
            let index = self.groups[group].specs[self.groups[group].cursor];
            self.groups[group].cursor += 1;
            let context = self.specs[index].context.clone();
            let source = self.templates[self.specs[index].template].source;
            let value = match source {
                Source::Definition(def) => self.definition(def, &context)?,
                Source::Destruct { value, .. } => self.expr(value, &context)?,
            };
            self.specs[index].value = Some(value);
        }
        Ok(())
    }

    pub fn reference(
        &mut self,
        reference: QualifiedName<'a>,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if reference.home == primitives::builtin_home() {
            return self.builtin(reference.name, node, ctx);
        }
        if let Some(&template) = self.top.get(&reference) {
            let binder = self.use_template(template, node, ctx)?;
            return Ok(self.ir.var(binder.name));
        }
        let method = self.build.tables.traits.iter().find_map(|(trait_, info)| {
            (trait_.home == reference.home)
                .then(|| {
                    info.methods
                        .iter()
                        .find(|m| m.name == reference.name)
                        .map(|m| (*trait_, m.annotation))
                })
                .flatten()
        });
        if let Some((trait_, annotation)) = method {
            return self.method(trait_, reference.name, annotation, node, ctx);
        }
        Err(Error::UnknownDefinition(reference))
    }
}

pub(crate) fn definition<'a>(def: &'a Def<'a>) -> (&'a Located<&'a str>, &'a Located<Expr<'a>>) {
    match def {
        Def::Def { name, body, .. } | Def::TypedDef { name, body, .. } => (name, body),
    }
}
fn declarations<'a>(mut decls: &'a Decls<'a>) -> Vec<&'a Def<'a>> {
    let mut result = Vec::new();
    loop {
        match decls {
            Decls::Empty => return result,
            Decls::Declare { definition, next } => {
                result.push(*definition);
                decls = next;
            }
            Decls::DeclareRec {
                definition,
                following,
                next,
            } => {
                result.push(*definition);
                result.extend_from_slice(following);
                decls = next;
            }
        }
    }
}
fn native<'a>(arena: &'a Arena, ty: Ty<'a>) -> Result<Ty<'a>, Error<'a>> {
    Ok(match ty {
        Ty::Big(_) => Ty::Big(&BigTy::Data),
        Ty::Const(ConstTy::List(t)) => Ty::Const(arena.alloc(ConstTy::List(native(arena, *t)?))),
        Ty::Const(ConstTy::Array(t)) => Ty::Const(arena.alloc(ConstTy::Array(native(arena, *t)?))),
        Ty::Const(ConstTy::Pair(a, b)) => {
            Ty::Const(arena.alloc(ConstTy::Pair(native(arena, *a)?, native(arena, *b)?)))
        }
        Ty::Const(_) => ty,
        _ => return Err(Error::RuntimeLayout(ty)),
    })
}

fn native_type<'a>(arena: &'a Arena, ty: Ty<'a>) -> Result<&'a Located<Type<'a>>, Error<'a>> {
    let (name, children) = match ty {
        Ty::Big(_) => ("Data", Vec::new()),
        Ty::Const(c) => match c {
            ConstTy::Int => ("int", Vec::new()),
            ConstTy::Bytes => ("bytes", Vec::new()),
            ConstTy::String => ("string", Vec::new()),
            ConstTy::Bool => ("bool", Vec::new()),
            ConstTy::Unit => ("unit", Vec::new()),
            ConstTy::BlsG1 => ("bls_g1", Vec::new()),
            ConstTy::BlsG2 => ("bls_g2", Vec::new()),
            ConstTy::BlsMlr => ("bls_mlr", Vec::new()),
            ConstTy::Value => ("value", Vec::new()),
            ConstTy::List(t) => ("list", vec![native_type(arena, *t)?]),
            ConstTy::Array(t) => ("array", vec![native_type(arena, *t)?]),
            ConstTy::Pair(a, b) => (
                "pair",
                vec![native_type(arena, *a)?, native_type(arena, *b)?],
            ),
        },
        _ => return Err(Error::RuntimeLayout(ty)),
    };
    Ok(arena.alloc(Located::at_zero(Type::Named {
        reference: QualifiedName {
            home: primitives::builtin_home(),
            name,
        },
        args: arena.alloc_slice_copy(&children),
    })))
}
fn check_type<'a>(typ: &'a Located<Type<'a>>, subst: &Substitution<'a>) -> Result<(), Error<'a>> {
    let mut pending = vec![(typ, 0, true)];
    while let Some((t, depth, replace)) = pending.pop() {
        if depth > MAX_TYPE_DEPTH {
            return Err(Error::SpecializationLimit);
        }
        let mut push = |t| pending.push((t, depth + 1, replace));
        match &t.value {
            Type::Var(n) => {
                if replace && let Some(t) = subst.get(n) {
                    pending.push((*t, depth + 1, false));
                }
            }
            Type::Named { args, .. } => {
                for t in *args {
                    push(*t);
                }
            }
            Type::App { head, args } => {
                push(*head);
                for t in *args {
                    push(*t);
                }
            }
            Type::Lambda { from, to } => {
                push(*from);
                push(*to);
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                push(*first);
                push(*second);
                for t in *rest {
                    push(*t);
                }
            }
            Type::Record { fields } => {
                for f in *fields {
                    push(f.typ);
                }
            }
            Type::Alias {
                arguments, target, ..
            } => {
                for a in *arguments {
                    push(a.typ);
                }
                if let AliasType::Filled { typ, .. } = target {
                    push(*typ);
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
