//! Solver-owned predicate bodies. Descriptor IDs refer into this store;
//! source provenance stays attached when a predicate is retained in a scheme.

use nash_ast::{NodeId, QualifiedName};
use nash_constrain::type_::PredId;
use nash_constrain::{UnionFind, Variable};
use nash_region::Region;

#[derive(Clone, Copy, Debug)]
pub struct UseSite<'a> {
    pub node: NodeId,
    pub region: Region,
    pub name: &'a str,
}

#[derive(Clone, Debug)]
pub struct Predicate<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: Vec<Variable>,
    pub site: UseSite<'a>,
    /// Context position at the originating use, in evidence order.
    pub index: usize,
}

#[derive(Default)]
pub struct Store<'a> {
    predicates: Vec<Predicate<'a>>,
}

impl<'a> Store<'a> {
    pub fn get(&self, id: PredId) -> &Predicate<'a> {
        &self.predicates[id.0 as usize]
    }

    pub fn push(&mut self, uf: &mut UnionFind<'a>, predicate: Predicate<'a>) -> PredId {
        let id = PredId(u32::try_from(self.predicates.len()).expect("predicate store exhausted"));
        for arg in &predicate.args {
            uf.modify(*arg, |desc| {
                if !desc.preds.contains(&id) {
                    desc.preds.push(id);
                }
            });
        }
        self.predicates.push(predicate);
        id
    }
}
