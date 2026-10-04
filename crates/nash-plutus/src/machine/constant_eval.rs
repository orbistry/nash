//! Direct evaluation of pure constant builtin applications for compiler folding.
use bumpalo::collections::Vec as BumpVec;

use crate::{arena::Arena, binder::DeBruijn, builtin::DefaultFunction, constant::Constant};

use super::{
    CostModel, ExBudget, Machine, PlutusVersion,
    cost_model::{builtin_costs::BuiltinCostModel, cost_map::CostMap},
    runtime::Runtime,
    value::Value,
};

#[derive(Default)]
struct NoCosts;

impl BuiltinCostModel for NoCosts {
    fn initialize(_: &CostMap) -> Self {
        Self
    }

    fn get_cost(&self, _: DefaultFunction, _: &[i64]) -> Option<ExBudget> {
        Some(ExBudget::new(0, 0))
    }
}

/// Evaluate one fully applied pure builtin without an execution budget.
/// Arguments must have consistent container metadata and payload types.
/// Errors and effectful calls return None. Ordinary program evaluation retains
/// its ledger cost model and budget.
pub fn eval_constant_builtin<'a>(
    arena: &'a Arena,
    func: DefaultFunction,
    args: &[&'a Constant<'a>],
) -> Option<&'a Constant<'a>> {
    if func == DefaultFunction::Trace || args.len() != func.arity() {
        return None;
    }
    let mut values = BumpVec::new_in(arena.as_bump());
    values.extend(args.iter().map(|c| Value::con(arena, c)));
    let runtime = arena.alloc(Runtime {
        args: values,
        fun: arena.alloc(func),
        forces: func.force_count(),
    });
    let mut machine: Machine<'_, NoCosts, DeBruijn> = Machine::new(
        arena,
        ExBudget::new(0, 0),
        CostModel::default(),
        (&PlutusVersion::V3).into(),
    );
    match machine.call(runtime).ok()? {
        Value::Con(c) => Some(c),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{constant::Integer, typ::Type};

    #[test]
    fn large_pure_evaluation_has_no_cost_budget() {
        let arena = Arena::new();
        let value = Constant::string(&arena, arena.as_bump().alloc_str(&"x".repeat(100_000)));
        let result =
            eval_constant_builtin(&arena, DefaultFunction::AppendString, &[value, value]).unwrap();
        let Constant::String(result) = result else {
            panic!("expected string");
        };
        // Independent semantic check on a result exceeding the former memory and byte caps.
        assert_eq!(*result, "x".repeat(200_000));
    }

    #[test]
    fn huge_drop_uses_builtin_semantics_without_costing_overflow() {
        let arena = Arena::new();
        let count = Constant::integer(&arena, arena.alloc_integer(Integer::from(1) << 200));
        let list = Constant::proto_list(
            &arena,
            &Type::Integer,
            arena.alloc([Constant::integer_from(&arena, 1)]),
        );
        let result =
            eval_constant_builtin(&arena, DefaultFunction::DropList, &[count, list]).unwrap();
        assert_eq!(result, Constant::proto_list(&arena, &Type::Integer, &[]));
    }

    #[test]
    fn effects_and_failed_calls_are_not_folded() {
        let arena = Arena::new();
        let one = Constant::integer_from(&arena, 1);
        let zero = Constant::integer_from(&arena, 0);
        assert!(
            eval_constant_builtin(
                &arena,
                DefaultFunction::Trace,
                &[Constant::string(&arena, "trace"), one]
            )
            .is_none()
        );
        assert!(
            eval_constant_builtin(&arena, DefaultFunction::DivideInteger, &[one, zero]).is_none()
        );
    }
}
