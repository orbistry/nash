//! Canonical expressions to specialized Core. Dictionaries never enter Core.
use std::collections::BTreeMap;

use nash_ast::{self as can, primitives, *};
use nash_ir::{
    core::*,
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};
use nash_region::{Located, Region};

use crate::build::{Binding, Context, Engine, Error, Source, TraceLevel};

mod methods;
mod patterns;

const DATA: Ty<'static> = Ty::Big(&BigTy::Data);

impl<'a> Engine<'a, '_, '_> {
    pub(crate) fn definition(
        &mut self,
        def: &'a Def<'a>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let (parameters, body): (Vec<_>, _) = match def {
            Def::Def { args, body, .. } => (args.to_vec(), *body),
            Def::TypedDef { args, body, .. } => (args.iter().map(|a| a.pattern).collect(), *body),
        };
        self.lambda(&parameters, body, ctx)
    }

    pub(crate) fn expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Unit => self.ir.lit(Constant::unit(self.ir.arena)),
            Expr::Int(n) => {
                let value = self.ir.int(*n);
                self.literal("FromInt", "fromInt", node, value, ctx, 0)?
            }
            Expr::Str(s) => {
                let value = self.ir.lit(Constant::string(self.ir.arena, s));
                self.literal("FromString", "fromString", node, value, ctx, 0)?
            }
            Expr::Bytes(bytes) => {
                let value = self.ir.lit(Constant::byte_string(self.ir.arena, bytes));
                self.literal("FromBytes", "fromBytes", node, value, ctx, 0)?
            }
            Expr::VarLocal(name) => match ctx
                .env
                .get(name)
                .copied()
                .ok_or(Error::UnknownLocal(name))?
            {
                Binding::Value(binder) => self.ir.var(binder.name),
                Binding::Template { id, projection } => {
                    let binder = self.use_template(id, node, ctx)?;
                    if let Some(name) = projection {
                        self.destruct_projection(id, name, binder, node, ctx)?
                    } else {
                        self.ir.var(binder.name)
                    }
                }
            },
            Expr::VarTopLevel(reference)
            | Expr::VarForeign { reference, .. }
            | Expr::VarOperator { reference, .. } => self.reference(*reference, node, ctx)?,
            Expr::VarMethod {
                trait_,
                method,
                annotation,
            } => self.method(*trait_, method, annotation, node, ctx)?,
            Expr::VarConstructor {
                reference, index, ..
            } => self.constructor(*reference, *index, node, ctx)?,
            Expr::Binop {
                reference,
                left,
                right,
                ..
            } => {
                let func = self.reference(*reference, node, ctx)?;
                let left = self.expr(left, ctx)?;
                let right = self.expr(right, ctx)?;
                self.ir.app(func, &[left, right])
            }
            Expr::Call {
                function,
                arguments,
            } => {
                let func = self.expr(function, ctx)?;
                let args = arguments
                    .iter()
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.ir.app(func, &args)
            }
            Expr::Lambda { parameters, body } => self.lambda(parameters, body, ctx)?,
            Expr::If {
                branches,
                final_else,
            } => {
                let mut body = self.expr(final_else, ctx)?;
                for branch in branches.iter().rev() {
                    let condition = self.expr(branch.condition, ctx)?;
                    let yes = self.expr(branch.then_branch, ctx)?;
                    body = self.ir.if_(condition, yes, body);
                }
                body
            }
            Expr::Let { definition, body } => self.let_definitions(&[*definition], body, ctx)?,
            Expr::LetRec { definitions, body } => self.let_definitions(definitions, body, ctx)?,
            Expr::LetDestruct {
                pattern,
                value,
                body,
            } => self.let_destruct(pattern, value, body, ctx)?,
            Expr::Case {
                scrutinee,
                branches,
            } => self.case(scrutinee, branches, ctx)?,
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                let items = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.ir.constr(0, &items)
            }
            Expr::List(items) => {
                let ty = self.ty(node, ctx)?;
                let Ty::Const(ConstTy::List(element)) = ty else {
                    return Err(Error::RuntimeLayout(ty));
                };
                let values = items
                    .iter()
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.list(*element, &values)?
            }
            Expr::Record { fields, .. } => {
                let values = fields
                    .iter()
                    .map(|f| self.expr(f.value, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                let ty = self.ty(node, ctx)?;
                self.product(ty, &values)?
            }
            Expr::Access { record, field } => {
                let typ = self.can_type(NodeId::expr(record), ctx)?;
                let fields = self.types.fields(typ, &ctx.runtime_subst)?;
                let index = fields
                    .iter()
                    .find(|(name, ..)| *name == field.value)
                    .ok_or(Error::InvalidConstructor)?
                    .1;
                let ty = self.ty(NodeId::expr(record), ctx)?;
                let value = self.expr(record, ctx)?;
                self.field(ty, value, index, fields.len())?
            }
            Expr::Accessor(field) => {
                let typ = self.substitute(self.can_type(node, ctx)?, &ctx.runtime_subst)?;
                let Type::Lambda { from, .. } = &typ.value else {
                    return Err(Error::InvalidConstructor);
                };
                let fields = self.types.fields(from, &BTreeMap::new())?;
                let index = fields
                    .iter()
                    .find(|(name, ..)| *name == *field)
                    .ok_or(Error::InvalidConstructor)?
                    .1;
                let ty = self.types.ty(from, &BTreeMap::new())?;
                let binder = Binder {
                    name: self.ir.fresh("record"),
                    ty,
                };
                let body = self.field(ty, self.ir.var(binder.name), index, fields.len())?;
                self.ir.lam(&[binder], body)
            }
            Expr::Update { base, fields, .. } => {
                let typ = self.can_type(NodeId::expr(base), ctx)?;
                let labels = self.types.fields(typ, &ctx.runtime_subst)?;
                let ty = self.ty(NodeId::expr(base), ctx)?;
                let value = self.expr(base, ctx)?;
                let binder = Binder {
                    name: self.ir.fresh("base"),
                    ty,
                };
                let mut values = Vec::new();
                for (name, index, _) in &labels {
                    values.push(
                        if let Some(update) = fields.iter().find(|f| f.field.value == *name) {
                            self.expr(update.value, ctx)?
                        } else {
                            self.field(ty, self.ir.var(binder.name), *index, labels.len())?
                        },
                    );
                }
                let result = match ty {
                    Ty::Big(BigTy::Adt(_)) => self.big_constructor(0, &values)?,
                    _ => self.product(ty, &values)?,
                };
                self.ir.let_(binder, value, result)
            }
            Expr::Trace { message, body } => {
                let body = self.expr(body, ctx)?;
                self.user_trace(Some(message), None, expr.region, body, ctx)?
            }
            Expr::Fail(message) | Expr::Todo(message) => {
                let todo = matches!(expr.value, Expr::Todo(_));
                self.user_trace(
                    *message,
                    todo.then_some("TODO: "),
                    expr.region,
                    self.ir.error(),
                    ctx,
                )?
            }
            Expr::Assert(condition) => {
                let value = self.expr(condition, ctx)?;
                let failed = self.user_trace(
                    None,
                    Some("assertion failed"),
                    expr.region,
                    self.ir.error(),
                    ctx,
                )?;
                self.ir
                    .if_(value, self.ir.lit(Constant::unit(self.ir.arena)), failed)
            }
            Expr::Comptime(value) => {
                let body = self.expr(value, ctx)?;
                let body = self.closed_dependencies(body)?;
                let body = crate::casts::expand_with_traces(
                    &self.ir,
                    &mut self.types,
                    body,
                    self.trace.compiler,
                )?;
                let constant = crate::comptime::eval_closed(self.ir.arena, &[], body)
                    .map_err(|error| Error::ComptimeAssembly(error.to_string()))?;
                self.ir.lit(constant)
            }
        })
    }

    fn let_definitions(
        &mut self,
        defs: &[&'a Def<'a>],
        body: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let group = self.add_group();
        let mut child = ctx.clone();
        let mut templates = Vec::new();
        for def in defs {
            let id = self.add_template(Source::Definition(def), ctx.clone(), group);
            templates.push(id);
            child.env.insert(
                crate::build::definition(def).0.value,
                Binding::Template {
                    id,
                    projection: None,
                },
            );
        }
        for &id in &templates {
            self.templates[id].captured = child.clone();
        }
        // Monomorphic lets are strict even if the result ignores the binding.
        for &id in &templates {
            if self.eager_template(id)? {
                let evidence =
                    self.resolve_context(self.scheme(id)?.annotation.context, &child.subst)?;
                self.request(id, child.subst.clone(), evidence)?;
            }
        }
        let body = self.expr(body, &child)?;
        self.drain(group)?;
        self.emit_group(group, body, false)
    }

    fn constructor(
        &mut self,
        reference: ConstructorName<'a>,
        tag: u16,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let ty = self.ty(node, ctx)?;
        let (args, result) = match ty {
            Ty::Term(TermTy::Fun(args, result)) => (*args, *result),
            _ => (&[][..], ty),
        };
        let params = args
            .iter()
            .map(|ty| Binder {
                name: self.ir.fresh("field"),
                ty: *ty,
            })
            .collect::<Vec<_>>();
        let fields = params
            .iter()
            .map(|p| self.ir.var(p.name))
            .collect::<Vec<_>>();
        let value = if reference.home == primitives::builtin_home() && reference.union == "bool" {
            if !params.is_empty() || tag > 1 {
                return Err(Error::InvalidConstructor);
            }
            self.ir.lit(Constant::bool(self.ir.arena, tag == 1))
        } else if reference.home == primitives::builtin_home() && reference.union == "Data" {
            let func = match tag {
                0 => F::ConstrData,
                1 => F::MapData,
                2 => F::ListData,
                3 => F::IData,
                4 => F::BData,
                _ => return Err(Error::InvalidConstructor),
            };
            self.ir.builtin(func, &fields)
        } else {
            match result {
                Ty::Big(BigTy::Adt(_)) => self.big_constructor(tag, &fields)?,
                Ty::Term(TermTy::Adt(_)) => self.ir.constr(tag, &fields),
                Ty::Big(BigTy::Record(_)) | Ty::Term(TermTy::Record(_)) => {
                    self.product(result, &fields)?
                }
                _ => return Err(Error::RuntimeLayout(result)),
            }
        };
        Ok(if params.is_empty() {
            value
        } else {
            self.ir.lam(&params, value)
        })
    }

    pub(crate) fn list(
        &self,
        element: Ty<'a>,
        values: &[&'a Core<'a>],
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let typ = element
            .plutus_type(self.ir.arena)
            .ok_or(Error::RuntimeLayout(element))?;
        let mut tail = self.ir.lit(Constant::proto_list(self.ir.arena, typ, &[]));
        for &head in values.iter().rev() {
            tail = self.ir.builtin(F::MkCons, &[head, tail]);
        }
        Ok(tail)
    }
    fn big_constructor(
        &self,
        tag: u16,
        fields: &[&'a Core<'a>],
    ) -> Result<&'a Core<'a>, Error<'a>> {
        Ok(self.ir.builtin(
            F::ConstrData,
            &[self.ir.int(i128::from(tag)), self.list(DATA, fields)?],
        ))
    }
    fn product(&self, ty: Ty<'a>, fields: &[&'a Core<'a>]) -> Result<&'a Core<'a>, Error<'a>> {
        Ok(match ty {
            Ty::Big(BigTy::Record(_)) => self.ir.builtin(F::ListData, &[self.list(DATA, fields)?]),
            Ty::Term(TermTy::Record(_) | TermTy::Tuple(_) | TermTy::Adt(_)) => {
                self.ir.constr(0, fields)
            }
            _ => return Err(Error::RuntimeLayout(ty)),
        })
    }
    fn field(
        &self,
        ty: Ty<'a>,
        value: &'a Core<'a>,
        index: u16,
        arity: usize,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let mut list = match ty {
            Ty::Big(BigTy::Record(_)) => self.ir.builtin(F::UnListData, &[value]),
            Ty::Big(BigTy::Adt(_)) => self
                .ir
                .builtin(F::SndPair, &[self.ir.builtin(F::UnConstrData, &[value])]),
            Ty::Term(TermTy::Record(_) | TermTy::Tuple(_) | TermTy::Adt(_)) => {
                return Ok(self.ir.field(
                    value,
                    index,
                    u16::try_from(arity).map_err(|_| Error::InvalidConstructor)?,
                ));
            }
            _ => return Err(Error::RuntimeLayout(ty)),
        };
        for _ in 0..index {
            list = self.ir.builtin(F::TailList, &[list]);
        }
        Ok(self.ir.builtin(F::HeadList, &[list]))
    }

    fn user_trace(
        &mut self,
        message: Option<&'a Located<Expr<'a>>>,
        prefix: Option<&str>,
        region: Region,
        body: &'a Core<'a>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if self.trace.user == TraceLevel::Silent {
            return Ok(body);
        }
        let message = if self.trace.user == TraceLevel::Compact {
            let home = self.build.inputs[ctx.input].module.name.name;
            let text = self.ir.arena.as_bump().alloc_str(&format!(
                "{home}:{}:{}",
                region.start.line, region.start.column
            ));
            self.ir.lit(Constant::string(self.ir.arena, text))
        } else if let Some(message) = message {
            let value = self.expr(message, ctx)?;
            if let Some(prefix) = prefix {
                self.ir.builtin(
                    F::AppendString,
                    &[
                        self.ir.lit(Constant::string(
                            self.ir.arena,
                            self.ir.arena.as_bump().alloc_str(prefix),
                        )),
                        value,
                    ],
                )
            } else {
                value
            }
        } else if let Some(prefix) = prefix {
            self.ir.lit(Constant::string(
                self.ir.arena,
                self.ir.arena.as_bump().alloc_str(prefix),
            ))
        } else {
            return Ok(body);
        };
        Ok(self.ir.trace(message, body))
    }
    pub(crate) fn match_failure(&self) -> &'a Core<'a> {
        if self.trace.compiler {
            self.ir.trace(
                self.ir
                    .lit(Constant::string(self.ir.arena, "incomplete pattern match")),
                self.ir.error(),
            )
        } else {
            self.ir.error()
        }
    }
}
