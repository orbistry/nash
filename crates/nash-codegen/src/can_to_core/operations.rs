//! Primitive and Data operations selected from solved representations, never traits.
use super::*;

impl<'a> Engine<'a, '_, '_> {
    pub(super) fn equality(
        &mut self,
        left: &'a Located<Expr<'a>>,
        right: &'a Located<Expr<'a>>,
        negate: bool,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let ty = self.ty(NodeId::expr(left), ctx)?;
        let left = self.expr(left, ctx)?;
        let right = self.expr(right, ctx)?;
        let b = &self.ir;
        let boolean = |value| b.lit(Constant::bool(b.arena, value));
        let primitive = match ty {
            Ty::Const(ConstTy::Int) => Some(F::EqualsInteger),
            Ty::Const(ConstTy::Bytes) => Some(F::EqualsByteString),
            Ty::Const(ConstTy::String) => Some(F::EqualsString),
            Ty::Const(ConstTy::BlsG1) => Some(F::Bls12_381_G1_Equal),
            Ty::Const(ConstTy::BlsG2) => Some(F::Bls12_381_G2_Equal),
            _ => None,
        };
        let equality = if let Some(primitive) = primitive {
            b.builtin(primitive, &[left, right])
        } else {
            match ty {
                Ty::Const(ConstTy::Bool) => {
                    // Keep left-to-right effects, including a failing right
                    // operand, and evaluate each operand exactly once.
                    let lhs = Binder {
                        name: b.fresh("equalLeft"),
                        ty,
                    };
                    let rhs = Binder {
                        name: b.fresh("equalRight"),
                        ty,
                    };
                    b.let_(
                        lhs,
                        left,
                        b.let_(
                            rhs,
                            right,
                            b.if_(
                                b.var(lhs.name),
                                b.var(rhs.name),
                                b.if_(b.var(rhs.name), boolean(false), boolean(true)),
                            ),
                        ),
                    )
                }
                Ty::Const(ConstTy::Unit) => b.builtin(
                    F::ChooseUnit,
                    &[left, b.builtin(F::ChooseUnit, &[right, boolean(true)])],
                ),
                Ty::Const(ConstTy::DataPair(_, _)) => {
                    let wrap = |value| {
                        let pair_type = nash_plutus::typ::Type::pair(
                            b.arena,
                            nash_plutus::typ::Type::data(b.arena),
                            nash_plutus::typ::Type::data(b.arena),
                        );
                        let empty = b.lit(Constant::proto_list(b.arena, pair_type, &[]));
                        b.builtin(F::MapData, &[b.builtin(F::MkCons, &[value, empty])])
                    };
                    b.builtin(F::EqualsData, &[wrap(left), wrap(right)])
                }
                Ty::Const(ConstTy::DataList(element)) => {
                    let wrap = if matches!(element, Ty::Const(ConstTy::DataPair(_, _))) {
                        F::MapData
                    } else {
                        F::ListData
                    };
                    b.builtin(
                        F::EqualsData,
                        &[b.builtin(wrap, &[left]), b.builtin(wrap, &[right])],
                    )
                }
                Ty::Const(ConstTy::DataTuple(_)) => b.builtin(
                    F::EqualsData,
                    &[
                        b.builtin(F::ListData, &[left]),
                        b.builtin(F::ListData, &[right]),
                    ],
                ),
                Ty::Big(_) => b.builtin(F::EqualsData, &[left, right]),
                Ty::Const(ConstTy::BlsMlr)
                | Ty::Term(TermTy::Fun(..))
                | Ty::Erased
                | Ty::Constructor(_) => return Err(Error::RuntimeLayout(ty)),
                _ => b.builtin(
                    F::EqualsData,
                    &[
                        b.cast(CastKind::ToData, ty, DATA, left),
                        b.cast(CastKind::ToData, ty, DATA, right),
                    ],
                ),
            }
        };
        Ok(if negate {
            b.if_(equality, boolean(false), boolean(true))
        } else {
            equality
        })
    }

    pub(super) fn format_value(
        &mut self,
        value: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let ty = self.ty(NodeId::expr(value), ctx)?;
        let value = self.expr(value, ctx)?;
        if ty == Ty::Const(&ConstTy::String) {
            return Ok(value);
        }
        let diagnostic = self.diagnostic_builtin();
        let data = self.ir.cast(CastKind::ToData, ty, DATA, value);
        let suffix = self.ir.lit(Constant::byte_string(self.ir.arena, &[]));
        Ok(self
            .ir
            .builtin(F::DecodeUtf8, &[self.ir.app(diagnostic, &[data, suffix])]))
    }
}
