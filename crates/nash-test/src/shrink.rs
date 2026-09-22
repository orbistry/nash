//! Internal reduction ordered by the shortlex order of consumed primitive choices.
//! Groups guide edits and isolate replay; they are not an additional size metric.
use crate::prng::{Choice, Trace};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Status<T> {
    Keep(T, Vec<Trace>),
    Ignore,
    Invalid,
}
type Oracle<'a, T> = Box<dyn FnMut(&[Trace]) -> Status<T> + 'a>;
pub struct Cache<'a, T> {
    db: BTreeMap<Vec<Trace>, Status<T>>,
    run: Oracle<'a, T>,
}
impl<'a, T: Clone + PartialEq> Cache<'a, T> {
    pub fn new(run: impl FnMut(&[Trace]) -> Status<T> + 'a) -> Self {
        Self {
            db: BTreeMap::new(),
            run: Box::new(run),
        }
    }
    pub fn size(&self) -> usize {
        self.db.len()
    }
    pub fn get(&mut self, choices: &[Trace]) -> Status<T> {
        if let Some(status) = self.db.get(choices) {
            return status.clone();
        }
        let status = (self.run)(choices);
        self.db.insert(choices.to_vec(), status.clone());
        status
    }
}
pub struct Counterexample<'a, T> {
    pub value: T,
    pub choices: Vec<Trace>,
    pub cache: Cache<'a, T>,
    pub steps: usize,
}

fn smaller(a: &[Trace], b: &[Trace]) -> bool {
    let (a, b) = (Trace::flatten(a), Trace::flatten(b));
    (a.len(), &a) < (b.len(), &b)
}

fn groups(nodes: &[Trace]) -> Vec<Vec<usize>> {
    fn visit(nodes: &[Trace], path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        out.push(path.clone());
        for (i, node) in nodes.iter().enumerate() {
            if let Trace::Group(children) = node {
                path.push(i);
                visit(children, path, out);
                path.pop();
            }
        }
    }
    let mut out = Vec::new();
    visit(nodes, &mut vec![], &mut out);
    out
}
fn at<'a>(mut nodes: &'a [Trace], path: &[usize]) -> &'a [Trace] {
    for &i in path {
        let Trace::Group(children) = &nodes[i] else {
            unreachable!()
        };
        nodes = children;
    }
    nodes
}
fn at_mut<'a>(mut nodes: &'a mut Vec<Trace>, path: &[usize]) -> &'a mut Vec<Trace> {
    for &i in path {
        let Trace::Group(children) = &mut nodes[i] else {
            unreachable!()
        };
        nodes = children;
    }
    nodes
}
fn replace_number(nodes: &mut [Trace], mut index: usize, value: Choice) {
    fn visit(nodes: &mut [Trace], index: &mut usize, value: Choice) -> bool {
        for node in nodes {
            match node {
                Trace::Choice(n) if *index == 0 => {
                    *n = value;
                    return true;
                }
                Trace::Choice(_) => *index -= 1,
                Trace::Group(children) => {
                    if visit(children, index, value) {
                        return true;
                    }
                }
            }
        }
        false
    }
    visit(nodes, &mut index, value);
}
fn widths(n: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut k = n;
    while k > 0 {
        result.push(k);
        k /= 2;
    }
    result
}
fn zero(nodes: &mut [Trace]) {
    for node in nodes {
        match node {
            Trace::Choice(n) => *n = 0,
            Trace::Group(c) => zero(c),
        }
    }
}

impl<T: Clone + PartialEq> Counterexample<'_, T> {
    fn consider(&mut self, candidate: &[Trace]) -> bool {
        if candidate == self.choices {
            return false;
        }
        let before = self.cache.size();
        let result = self.cache.get(candidate);
        self.steps += self.cache.size() - before;
        let Status::Keep(mut value, used) = result else {
            return false;
        };
        if !smaller(&used, &self.choices) {
            return false;
        }
        if used != candidate {
            // Public state functions can inspect unused input. Confirm the
            // normalized trace itself reproduces the counterexample.
            let before = self.cache.size();
            let normalized = self.cache.get(&used);
            self.steps += self.cache.size() - before;
            match normalized {
                Status::Keep(replayed, consumed) if consumed == used => value = replayed,
                _ => return false,
            }
        }
        self.value = value;
        self.choices = used;
        true
    }

    // A shape change can need an accompanying numeric change. In particular,
    // a length/branch change and a new strict-group partition must be tried together.
    fn coordinated(&mut self, candidate: &[Trace]) -> bool {
        if self.consider(candidate) {
            return true;
        }
        let numbers = Trace::flatten(candidate);
        for (i, &n) in numbers.iter().enumerate() {
            if n == 0 {
                continue;
            }
            for value in [0, n / 2, n - 1] {
                let mut changed = candidate.to_vec();
                replace_number(&mut changed, i, value);
                if self.consider(&changed) {
                    return true;
                }
            }
        }
        false
    }

    fn regions(&mut self) -> bool {
        let original = self.choices.clone();
        for path in groups(&original) {
            let children = at(&original, &path);
            for k in widths(children.len()) {
                for start in (0..=children.len() - k).rev() {
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path).drain(start..start + k);
                    if self.coordinated(&candidate) {
                        return true;
                    }
                    let mut candidate = original.clone();
                    zero(&mut at_mut(&mut candidate, &path)[start..start + k]);
                    if self.consider(&candidate) {
                        return true;
                    }
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path).splice(start..start + k, [Trace::Choice(0)]);
                    if self.consider(&candidate) {
                        return true;
                    }
                }
            }
            // Replace a draw's contents with a descendant draw's contents.
            for descendant in groups(children).into_iter().skip(1) {
                let mut candidate = original.clone();
                *at_mut(&mut candidate, &path) = at(children, &descendant).to_vec();
                if self.coordinated(&candidate) {
                    return true;
                }
            }
        }
        false
    }

    fn numbers(&mut self) -> bool {
        let original = self.choices.clone();
        let numbers = Trace::flatten(&original);
        for (i, &n) in numbers.iter().enumerate().rev() {
            if n == 0 {
                continue;
            }
            let mut candidate = original.clone();
            replace_number(&mut candidate, i, 0);
            if self.consider(&candidate) {
                return true;
            }
            // Probe the boundary between rejected and accepted numeric candidates.
            let (mut lo, mut hi) = (0, n);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                replace_number(&mut candidate, i, mid);
                if self.consider(&candidate) {
                    return true;
                }
                lo = mid;
            }
            hi = n - 1;
            replace_number(&mut candidate, i, hi);
            if self.consider(&candidate) {
                return true;
            }
        }
        for k in widths(numbers.len()).into_iter().filter(|k| *k > 1) {
            for start in 0..=numbers.len() - k {
                let mut sorted = numbers[start..start + k].to_vec();
                sorted.sort_unstable();
                let mut candidate = original.clone();
                for (i, value) in sorted.into_iter().enumerate() {
                    replace_number(&mut candidate, start + i, value);
                }
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        for distance in [1, 2] {
            for j in distance..numbers.len() {
                let i = j - distance;
                let (a, b) = (numbers[i], numbers[j]);
                if a > b {
                    let mut candidate = original.clone();
                    replace_number(&mut candidate, i, b);
                    replace_number(&mut candidate, j, a);
                    if self.consider(&candidate) {
                        return true;
                    }
                }
                if a > 0 && b <= Choice::MAX - a {
                    let mut lo = 0;
                    let mut value = 0;
                    loop {
                        let mut candidate = original.clone();
                        replace_number(&mut candidate, i, value);
                        replace_number(&mut candidate, j, b + (a - value));
                        if self.consider(&candidate) {
                            return true;
                        }
                        lo = lo.max(value);
                        if a - lo <= 1 {
                            break;
                        }
                        value = lo + (a - lo) / 2;
                    }
                }
            }
        }
        false
    }

    fn restructure(&mut self) -> bool {
        let original = self.choices.clone();
        for path in groups(&original) {
            let children = at(&original, &path);
            for i in 0..children.len() {
                if let Trace::Group(inner) = &children[i] {
                    // Remove a boundary, or split one group into two groups.
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path).splice(i..=i, inner.clone());
                    if self.coordinated(&candidate) {
                        return true;
                    }
                    for split in 0..=inner.len() {
                        let mut candidate = original.clone();
                        at_mut(&mut candidate, &path).splice(
                            i..=i,
                            [
                                Trace::Group(inner[..split].to_vec()),
                                Trace::Group(inner[split..].to_vec()),
                            ],
                        );
                        if self.coordinated(&candidate) {
                            return true;
                        }
                    }
                    if let Some(Trace::Group(right)) = children.get(i + 1) {
                        let joined = [inner.as_slice(), right].concat();
                        let mut candidate = original.clone();
                        at_mut(&mut candidate, &path)
                            .splice(i..i + 2, [Trace::Group(joined.clone())]);
                        if self.coordinated(&candidate) {
                            return true;
                        }
                        for split in 0..=joined.len() {
                            let mut candidate = original.clone();
                            at_mut(&mut candidate, &path).splice(
                                i..i + 2,
                                [
                                    Trace::Group(joined[..split].to_vec()),
                                    Trace::Group(joined[split..].to_vec()),
                                ],
                            );
                            if self.coordinated(&candidate) {
                                return true;
                            }
                        }
                    }
                }
                // Introduce a boundary around a contiguous region.
                for end in i + 1..=children.len() {
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path)
                        .splice(i..end, [Trace::Group(children[i..end].to_vec())]);
                    if self.coordinated(&candidate) {
                        return true;
                    }
                }
            }
            // A smaller branch may require a new draw; proposals may grow locally
            // provided successful replay consumes a globally smaller choice sequence.
            for i in 0..=children.len() {
                for node in [Trace::Choice(0), Trace::Group(vec![])] {
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path).insert(i, node);
                    if self.coordinated(&candidate) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn simplify(&mut self) {
        if Trace::flatten(&self.choices).is_empty() {
            return;
        }
        while self.regions() || self.numbers() || self.restructure() {}
    }
}
