use super::{column, row, source_text};
use crate::*;
use serde_json::{Value, json};
pub fn render(seed: u32, max_success: usize, outcomes: &[Outcome]) -> String {
    let tests = outcomes.iter().map(|o| {
        let assertion = o.assert.as_ref().map(|a| {
            let values = a.site.captures.iter().filter_map(|cap| {
                let value = a.values.iter().find(|(i, _)| *i == cap.index).map(|(_, v)| v.as_str()).or(if cap.shown { None } else { Some("?") })?;
                Some(json!({"row": row(a.site.region, cap.region), "column": column(&o.test.source, a.site.region, cap.region), "value": value}))
            }).collect::<Vec<_>>();
            json!({"file": o.test.source_path, "line": a.site.region.start.line, "column": a.site.region.start.column, "source": source_text(&o.test.source, a.site.region), "values": values})
        });
        let failure = match &o.status {
            Status::Pass => Value::Null,
            Status::Fail(Failure::Body) => json!({"kind": "body"}),
            Status::Fail(Failure::NoCounterexample) => json!({"kind":"noCounterexample"}),
            Status::Fail(Failure::Generator { message }) => json!({"kind":"generator", "message":message}),
            Status::Fail(Failure::InvalidProgram { message }) => json!({"kind":"invalidProgram", "message":message}),
            Status::Fail(Failure::BudgetExceeded { limit, used }) => json!({"kind":"budgetExceeded", "limit": budget_limit(*limit), "used":{"cpu":used.cpu,"mem":used.mem}}),
        };
        json!({"module":o.test.module,"name":o.test.name,"kind":if matches!(o.test.programs, Programs::Unit { .. }) { "test" } else { "prop" }, "status": if o.status == Status::Pass { "pass" } else { "fail" }, "iterations":o.iterations,"budget":{"cpu":o.budget.cpu,"mem":o.budget.mem},"counterexample":o.counterexample.as_ref().map(|c| c.iter().map(|(name,value)| json!({"name":name,"value":value})).collect::<Vec<_>>()),"replay":o.replay.as_ref().map(|nodes| nodes.iter().map(trace).collect::<Vec<_>>()),"assert":assertion,"labels":o.labels,"traces":o.traces,"expectedFailure":o.expected_failure,"failure":failure})
    }).collect::<Vec<_>>();
    serde_json::to_string_pretty(&json!({"seed":seed,"maxSuccess":max_success,"tests":tests}))
        .expect("JSON report serializes")
}
fn budget_limit(limit: Budget) -> Value {
    // Limits can exceed JSON's integer range, so preserve their decimal value.
    let number = |n: i128| {
        u64::try_from(n)
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(n.to_string()))
    };
    match limit {
        Budget::Cpu(cpu) => json!({"cpu":number(cpu)}),
        Budget::Mem(mem) => json!({"mem":number(mem)}),
        Budget::Both { cpu, mem } => json!({"cpu":number(cpu),"mem":number(mem)}),
    }
}

fn trace(node: &crate::prng::Trace) -> Value {
    match node {
        crate::prng::Trace::Choice(value) => json!({"choice": value.to_string()}),
        crate::prng::Trace::Group(children) => {
            json!({"group": children.iter().map(trace).collect::<Vec<_>>()})
        }
    }
}
