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

mod calls;
mod forms;
mod methods;
mod operations;
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
        let function = self.lambda(&parameters, body, ctx)?;
        Ok(self.no_inline_function(function))
    }

    pub(crate) fn expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        match &expr.value {
            Expr::RunnableCheck { function, .. } => self.expr(function, ctx),
            Expr::TypeScope { value } => self.expr(value, ctx),
            Expr::Unit | Expr::Constant(_) | Expr::ModuleConstantCheck { .. } => {
                self.constants_expr(expr, ctx)
            }
            Expr::TupleIndex { .. }
            | Expr::Match { .. }
            | Expr::RecordUpdate { .. }
            | Expr::Equal { .. }
            | Expr::Format { .. }
            | Expr::TraceLabel { .. } => self.operations_expr(expr, ctx),
            Expr::Convert { .. } => self.conversion_expr(expr, ctx),
            Expr::Pair { .. }
            | Expr::DataList { .. }
            | Expr::DataTuple { .. }
            | Expr::Tuple { .. }
            | Expr::List(_) => self.collections_expr(expr, ctx),
            Expr::Int(_)
            | Expr::Str(_)
            | Expr::Bytes(_)
            | Expr::VarLocal(_)
            | Expr::VarTopLevel(_)
            | Expr::VarForeign { .. }
            | Expr::VarOperator { .. }
            | Expr::VarMethod { .. }
            | Expr::VarConstructor { .. }
            | Expr::Binop { .. } => self.values_expr(expr, ctx),
            Expr::Callable { .. }
            | Expr::Function { .. }
            | Expr::SurfaceCall { .. }
            | Expr::Pipe { .. }
            | Expr::Call { .. }
            | Expr::Lambda { .. } => self.calls_expr(expr, ctx),
            Expr::If { .. }
            | Expr::LetValue { .. }
            | Expr::Let { .. }
            | Expr::LetRec { .. }
            | Expr::LetDestruct { .. }
            | Expr::Case { .. } => self.control_expr(expr, ctx),
            Expr::Record { .. }
            | Expr::Access { .. }
            | Expr::FieldOrModule { .. }
            | Expr::Accessor(_)
            | Expr::Update { .. } => self.records_expr(expr, ctx),
            Expr::Trace { .. }
            | Expr::Fail(_)
            | Expr::Todo(_)
            | Expr::Assert(_)
            | Expr::Comptime(_) => self.effects_expr(expr, ctx),
        }
    }

    fn encoded_record_update(
        &mut self,
        base: &'a Located<Expr<'a>>,
        fields: &'a [FieldUpdate<'a>],
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let typ = self.can_type(NodeId::expr(base), ctx)?;
        let labels = self.types.fields(typ, &ctx.runtime_subst)?;
        let ty = self.ty(NodeId::expr(base), ctx)?;
        let Ty::Big(BigTy::Adt(adt)) = ty else {
            return Err(Error::RuntimeLayout(ty));
        };
        let layout = self.types.layout(*adt)?;
        let encoding = layout.data_layout.ok_or(Error::RuntimeLayout(ty))?.encoding;
        let value = self.expr(base, ctx)?;
        let input = Binder {
            name: self.ir.fresh("updated"),
            ty,
        };
        if encoding == DataEncoding::Transparent {
            let body = if let Some(field) = fields.first() {
                let field_ty = self.ty(NodeId::expr(field.value), ctx)?;
                let value = self.expr(field.value, ctx)?;
                self.ir.cast(CastKind::ToData, field_ty, DATA, value)
            } else {
                self.ir.var(input.name)
            };
            return Ok(self.ir.let_(input, value, body));
        }
        let mut updates = BTreeMap::new();
        for field in fields {
            let index = labels
                .iter()
                .find(|(name, ..)| *name == field.field.value)
                .ok_or(Error::InvalidConstructor)?
                .1;
            let field_ty = self.ty(NodeId::expr(field.value), ctx)?;
            let value = self.expr(field.value, ctx)?;
            updates.insert(
                usize::from(index),
                self.ir.cast(CastKind::ToData, field_ty, DATA, value),
            );
        }
        let highest = updates.keys().next_back().copied().unwrap_or(0);
        let list_ty = Ty::Const(self.ir.arena.alloc(ConstTy::List(DATA)));
        let mut tails = Vec::with_capacity(highest + 2);
        for _ in 0..highest + 2 {
            tails.push(Binder {
                name: self.ir.fresh("fields"),
                ty: list_ty,
            });
        }
        let mut body = self.ir.var(tails[highest + 1].name);
        for index in (0..=highest).rev() {
            let field = updates.get(&index).copied().unwrap_or_else(|| {
                self.ir
                    .builtin(F::HeadList, &[self.ir.var(tails[index].name)])
            });
            body = self.ir.builtin(F::MkCons, &[field, body]);
        }
        body = self.ir.builtin(
            if encoding == DataEncoding::List {
                F::ListData
            } else {
                F::ConstrData
            },
            if encoding == DataEncoding::List {
                self.ir.arena.alloc_slice_copy(&[body])
            } else {
                self.ir.arena.alloc_slice_copy(&[self.ir.int(0), body])
            },
        );
        for index in (1..tails.len()).rev() {
            let tail = self
                .ir
                .builtin(F::TailList, &[self.ir.var(tails[index - 1].name)]);
            body = self.ir.let_(tails[index], tail, body);
        }
        let data_fields = if encoding == DataEncoding::List {
            self.ir.builtin(F::UnListData, &[self.ir.var(input.name)])
        } else {
            self.ir.builtin(
                F::SndPair,
                &[self.ir.builtin(F::UnConstrData, &[self.ir.var(input.name)])],
            )
        };
        body = self.ir.let_(tails[0], data_fields, body);
        Ok(self.ir.let_(input, value, body))
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
                Ty::Big(BigTy::Adt(_)) => self.big_constructor(result, tag, &fields)?,
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
        &mut self,
        ty: Ty<'a>,
        index: u16,
        fields: &[&'a Core<'a>],
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let Ty::Big(BigTy::Adt(adt)) = ty else {
            return Err(Error::RuntimeLayout(ty));
        };
        let layout = self.types.layout(*adt)?;
        let types = layout
            .fields
            .get(usize::from(index))
            .ok_or(Error::InvalidConstructor)?;
        if fields.len() != types.len() {
            return Err(Error::InvalidConstructor);
        }
        if layout
            .data_layout
            .is_some_and(|data| data.encoding == DataEncoding::Transparent)
        {
            return Ok(self.ir.cast(CastKind::ToData, types[0], DATA, fields[0]));
        }
        let encoded = if layout.data_layout.is_some() {
            fields
                .iter()
                .zip(*types)
                .map(|(value, typ)| self.ir.cast(CastKind::ToData, *typ, DATA, value))
                .collect::<Vec<_>>()
        } else {
            fields.to_vec()
        };
        let fields = self.list(DATA, &encoded)?;
        Ok(match layout.data_layout {
            Some(data) if data.encoding == DataEncoding::List => {
                self.ir.builtin(F::ListData, &[fields])
            }
            data => self.ir.builtin(
                F::ConstrData,
                &[
                    self.ir.int(data.map_or(i128::from(index), |data| {
                        i128::from(data.tags[usize::from(index)])
                    })),
                    fields,
                ],
            ),
        })
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
        &mut self,
        ty: Ty<'a>,
        value: &'a Core<'a>,
        index: u16,
        arity: usize,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let mut decoded = None;
        let mut list = match ty {
            Ty::Big(BigTy::Record(_)) => self.ir.builtin(F::UnListData, &[value]),
            Ty::Big(BigTy::Adt(adt)) => {
                let layout = self.types.layout(*adt)?;
                if let Some(data) = layout.data_layout {
                    if data.encoding == DataEncoding::Transparent {
                        let field = *layout.fields[0]
                            .get(usize::from(index))
                            .ok_or(Error::InvalidConstructor)?;
                        return Ok(self.ir.cast(CastKind::FromDataShallow, DATA, field, value));
                    }
                    decoded = Some(
                        *layout
                            .fields
                            .first()
                            .and_then(|fields| fields.get(usize::from(index)))
                            .ok_or(Error::InvalidConstructor)?,
                    );
                    if data.encoding == DataEncoding::List {
                        self.ir.builtin(F::UnListData, &[value])
                    } else {
                        self.ir
                            .builtin(F::SndPair, &[self.ir.builtin(F::UnConstrData, &[value])])
                    }
                } else {
                    self.ir
                        .builtin(F::SndPair, &[self.ir.builtin(F::UnConstrData, &[value])])
                }
            }
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
        let value = self.ir.builtin(F::HeadList, &[list]);
        Ok(match decoded {
            Some(ty) if !matches!(ty, Ty::Big(_)) => {
                self.ir.cast(CastKind::FromDataShallow, DATA, ty, value)
            }
            _ => value,
        })
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
