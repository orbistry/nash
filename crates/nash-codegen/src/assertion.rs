//! Test-only power assertions, lowered without replacing canonical nodes.
//!
//! Solved metadata is keyed by node identity. The temporary substitutions below
//! replace only emitted Core and leave canonical nodes and evidence intact.
use nash_ast::{Expr, ModuleName, NodeId, Pred, QualifiedName, primitives};
use nash_ir::{core::*, ty::Ty};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};
use nash_region::Located;
use nash_test::{AssertSite, Capture};

use crate::{
    build::{Context, Engine, Error, TraceLevel},
    ty_of::Substitution,
};

struct Captured<'a> {
    metadata: Capture,
    value: Option<&'a Core<'a>>,
}

impl<'a> Engine<'a, '_, '_> {
    pub(crate) fn string(&self, value: &str) -> &'a Core<'a> {
        self.ir.lit(Constant::string(
            self.ir.arena,
            self.ir.arena.as_bump().alloc_str(value),
        ))
    }

    /// Resolve and compile the real Show implementation. Missing Show is an
    /// intentional fallback; a selected but invalid implementation is an error.
    pub(crate) fn show_value(
        &mut self,
        node: NodeId,
        value: &'a Core<'a>,
        ctx: &Context<'a>,
    ) -> Result<Option<&'a Core<'a>>, Error<'a>> {
        let trait_ = QualifiedName {
            home: ModuleName {
                package: Some(primitives::CORE),
                name: "Show",
            },
            name: "Show",
        };
        let Some(info) = self.build.tables.traits.get(&trait_).copied() else {
            return Ok(None);
        };
        let typ = self.substitute(self.can_type(node, ctx)?, &ctx.subst)?;
        let pred = Pred::Trait {
            trait_,
            args: self.ir.arena.alloc_slice_copy(&[typ]),
        };
        let Ok(Some(evidence)) =
            nash_solve::evidence::resolve(self.ir.arena.as_bump(), &self.build.tables, &pred)
        else {
            return Ok(None);
        };
        let annotation = self.method_annotation(trait_, "show")?;
        let [parameter] = info.parameters else {
            return Err(Error::MethodType);
        };
        let subst = Substitution::from([(*parameter, typ)]);
        let evidence = self.ir.arena.alloc_slice_fill_iter([evidence]);
        // Keep even a Show method's function-valued initialization inside the
        // failure branch. Ordinary root specialization hoists strict top-level
        // definitions, which could otherwise trace or fail on passing asserts.
        // Share the name supply so this closed function cannot capture the
        // surrounding test's binders when its Core is inserted below.
        let mut show = Engine::new(self.build, self.ir.arena, self.trace);
        std::mem::swap(&mut self.ir, &mut show.ir);
        let function = (|| {
            let function = show.selected_method(trait_, "show", annotation, &subst, evidence)?;
            show.finish_root(function)
        })();
        std::mem::swap(&mut self.ir, &mut show.ir);
        let function = function?;
        Ok(Some(self.ir.app(function, &[value])))
    }

    pub(crate) fn power_assert(
        &mut self,
        condition: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let id = self.asserts.len() as u32;
        self.asserts.push(AssertSite {
            id,
            region: condition.region,
            captures: Vec::new(),
        });
        let saved = std::mem::take(&mut self.replacements);
        let result = (|| {
            let mut bindings = Vec::new();
            let mut captures = Vec::new();
            let condition =
                self.assert_expression(condition, ctx, false, &mut bindings, &mut captures)?;
            let mut failed = self.ir.error();
            for captured in captures.iter().rev() {
                if let Some(shown) = captured.value {
                    let prefix =
                        self.string(&format!("\0assert\0{id}\0{}\0", captured.metadata.index));
                    failed = self
                        .ir
                        .trace(self.ir.builtin(F::AppendString, &[prefix, shown]), failed);
                }
            }
            // A site marker also identifies assertions with no printable captures.
            failed = self
                .ir
                .trace(self.string(&format!("\0assert\0{id}")), failed);
            let mut body = self.ir.if_(
                condition,
                self.ir.lit(Constant::unit(self.ir.arena)),
                failed,
            );
            for (binder, value) in bindings.into_iter().rev() {
                body = self.ir.let_(binder, value, body);
            }
            self.asserts[id as usize].captures = captures.into_iter().map(|c| c.metadata).collect();
            Ok(body)
        })();
        self.replacements = saved;
        result
    }

    /// Bind every strict operand, even uncaptured literals, to preserve ordering
    /// around overloaded literal conversions and partially applied functions.
    fn assert_expression(
        &mut self,
        expression: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
        operand: bool,
        bindings: &mut Vec<(Binder<'a>, &'a Core<'a>)>,
        captures: &mut Vec<Captured<'a>>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let capture = operand
            && matches!(
                expression.value,
                Expr::VarLocal(_)
                    | Expr::VarTopLevel(_)
                    | Expr::VarForeign { .. }
                    | Expr::VarOperator { .. }
                    | Expr::VarMethod { .. }
                    | Expr::Call { .. }
                    | Expr::Binop { .. }
                    | Expr::Access { .. }
                    | Expr::If { .. }
                    | Expr::Case { .. }
                    | Expr::Let { .. }
                    | Expr::LetRec { .. }
                    | Expr::LetDestruct { .. }
            );
        let index = captures.len();
        if capture {
            captures.push(Captured {
                metadata: Capture {
                    index: index as u32,
                    region: expression.region,
                    shown: false,
                },
                value: None,
            });
        }
        let value = match &expression.value {
            Expr::Trace { message, body } => {
                let home = self.build.inputs[ctx.input].module.name;
                let reserved = home.package == Some(primitives::CORE) && home.name == "Test";
                if reserved || self.trace.user == TraceLevel::Verbose {
                    self.assert_expression(message, ctx, true, bindings, captures)?;
                }
                // Trace before evaluating/capturing its body, preserving the
                // effect boundary while retaining the body's strict captures.
                let unit = self.ir.lit(Constant::unit(self.ir.arena));
                let traced = if reserved {
                    let message = self.expr(message, ctx)?;
                    self.ir.trace(message, unit)
                } else {
                    self.user_trace(Some(message), None, expression.region, unit, ctx)?
                };
                let binder = Binder {
                    name: self.ir.fresh("assert_trace"),
                    ty: Ty::Const(&nash_ir::ty::ConstTy::Unit),
                };
                bindings.push((binder, traced));
                self.assert_expression(body, ctx, true, bindings, captures)?
            }
            Expr::Call {
                function,
                arguments,
            } => {
                let mut function =
                    self.assert_expression(function, ctx, true, bindings, captures)?;
                for (position, arg) in arguments.iter().enumerate() {
                    let arg = self.assert_expression(arg, ctx, true, bindings, captures)?;
                    function = self.ir.app(function, &[arg]);
                    if position + 1 < arguments.len() {
                        let binder = Binder {
                            name: self.ir.fresh("apply"),
                            ty: Ty::Erased,
                        };
                        bindings.push((binder, function));
                        function = self.ir.var(binder.name);
                    }
                }
                function
            }
            Expr::Binop {
                reference,
                left,
                right,
                ..
            } => {
                if let Some(conjunction) = crate::can_to_core::short_circuit(*reference) {
                    let left = self.assert_expression(left, ctx, true, bindings, captures)?;
                    let right = self.expr(right, ctx)?;
                    let constant = self.ir.lit(Constant::bool(self.ir.arena, !conjunction));
                    let value = if conjunction {
                        self.ir.if_(left, right, constant)
                    } else {
                        self.ir.if_(left, constant, right)
                    };
                    // No operand from the RHS may escape its lazy branch.
                    return self.bind_assert_value(
                        expression,
                        ctx,
                        operand,
                        capture.then_some(index),
                        value,
                        bindings,
                        captures,
                    );
                }
                let function = self.reference(*reference, NodeId::expr(expression), ctx)?;
                let binder = Binder {
                    name: self.ir.fresh("operator"),
                    ty: Ty::Erased,
                };
                bindings.push((binder, function));
                let left = self.assert_expression(left, ctx, true, bindings, captures)?;
                let partial = Binder {
                    name: self.ir.fresh("apply"),
                    ty: Ty::Erased,
                };
                bindings.push((partial, self.ir.app(self.ir.var(binder.name), &[left])));
                let right = self.assert_expression(right, ctx, true, bindings, captures)?;
                self.ir.app(self.ir.var(partial.name), &[right])
            }
            _ => {
                let mut children = Vec::new();
                match &expression.value {
                    Expr::Access { record, .. } => children.push(*record),
                    // Only the first condition is unconditional. Later conditions
                    // and all branches remain inside their original lazy boundary.
                    Expr::If { branches, .. } => {
                        if let Some(first) = branches.first() {
                            children.push(first.condition);
                        }
                    }
                    Expr::Case { scrutinee, .. } => children.push(*scrutinee),
                    Expr::Tuple {
                        first,
                        second,
                        rest,
                    } => {
                        children.extend([*first, *second]);
                        children.extend_from_slice(rest);
                    }
                    Expr::List(items) => children.extend_from_slice(items),
                    Expr::Record { fields, .. } => children.extend(fields.iter().map(|f| f.value)),
                    Expr::Update { base, fields, .. } => {
                        children.push(*base);
                        let typ = self.can_type(NodeId::expr(base), ctx)?;
                        for (name, ..) in self.types.fields(typ, &ctx.runtime_subst)? {
                            if let Some(field) =
                                fields.iter().find(|field| field.field.value == name)
                            {
                                children.push(field.value);
                            }
                        }
                    }
                    // Let bindings and lambda bodies are lowered intact.
                    // Capturing a whole let observes its value without moving
                    // anything across its lexical scope.
                    _ => {}
                }
                for child in children {
                    self.assert_expression(child, ctx, true, bindings, captures)?;
                }
                self.expr(expression, ctx)?
            }
        };
        self.bind_assert_value(
            expression,
            ctx,
            operand,
            capture.then_some(index),
            value,
            bindings,
            captures,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_assert_value(
        &mut self,
        expression: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
        operand: bool,
        capture: Option<usize>,
        value: &'a Core<'a>,
        bindings: &mut Vec<(Binder<'a>, &'a Core<'a>)>,
        captures: &mut [Captured<'a>],
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if !operand {
            return Ok(value);
        }
        let binder = Binder {
            name: self.ir.fresh("assert_value"),
            ty: self.ty(NodeId::expr(expression), ctx)?,
        };
        bindings.push((binder, value));
        let value = self.ir.var(binder.name);
        self.replacements.insert(NodeId::expr(expression), value);
        if let Some(index) = capture {
            let shown = self.show_value(NodeId::expr(expression), value, ctx)?;
            captures[index].metadata.shown = shown.is_some();
            captures[index].value = shown;
        }
        Ok(value)
    }
}
