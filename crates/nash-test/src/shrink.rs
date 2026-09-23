//! Internal reduction ordered by the shortlex order of consumed primitive choices.
//! Groups guide edits and isolate replay; they are not an additional size metric.
use crate::prng::{Choice, Trace};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Status<T> {
    Keep(T, Vec<Trace>),
    Ignore(Vec<Trace>),
    Invalid,
}
type Rebuild<'a, T> = Box<dyn FnMut(&[Choice]) -> Status<T> + 'a>;
type Oracle<'a, T> = Box<dyn FnMut(&[Trace]) -> Status<T> + 'a>;
pub struct Cache<'a, T> {
    db: BTreeMap<Vec<Trace>, Status<T>>,
    run: Oracle<'a, T>,
    rebuild: Option<Rebuild<'a, T>>,
    rebuilt: BTreeMap<Vec<Choice>, Status<T>>,
    changed: BTreeSet<usize>,
}
impl<'a, T: Clone + PartialEq> Cache<'a, T> {
    pub fn new(run: impl FnMut(&[Trace]) -> Status<T> + 'a) -> Self {
        Self {
            db: BTreeMap::new(),
            run: Box::new(run),
            rebuild: None,
            rebuilt: BTreeMap::new(),
            changed: BTreeSet::new(),
        }
    }
    pub fn with_rebuild(mut self, rebuild: impl FnMut(&[Choice]) -> Status<T> + 'a) -> Self {
        self.rebuild = Some(Box::new(rebuild));
        self
    }
    pub fn size(&self) -> usize {
        self.db.len() + self.rebuilt.len()
    }
    fn build(&mut self, choices: &[Choice]) -> Status<T> {
        let Some(rebuild) = self.rebuild.as_mut() else {
            return Status::Invalid;
        };
        if let Some(result) = self.rebuilt.get(choices) {
            return result.clone();
        }
        let result = rebuild(choices);
        self.rebuilt.insert(choices.to_vec(), result.clone());
        result
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
    result.extend(1..=n.min(5));
    result.sort_unstable_by(|a, b| b.cmp(a));
    result.dedup();
    result
}
fn reductions(n: Choice) -> Vec<Choice> {
    if n == 0 {
        return vec![];
    }
    let mut result = vec![0];
    let mut low = 0;
    while n - low > 1 {
        low += (n - low) / 2;
        result.push(low);
    }
    if *result.last().unwrap() != n - 1 {
        result.push(n - 1);
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

struct Draw {
    start: usize,
    end: usize,
    children: Vec<(usize, usize)>,
}
fn draws(nodes: &[Trace]) -> BTreeMap<Vec<usize>, Draw> {
    fn visit(
        nodes: &[Trace],
        path: &mut Vec<usize>,
        offset: &mut usize,
        out: &mut BTreeMap<Vec<usize>, Draw>,
    ) {
        let start = *offset;
        let mut children = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            let first = *offset;
            match node {
                Trace::Choice(_) => *offset += 1,
                Trace::Group(inner) => {
                    path.push(index);
                    visit(inner, path, offset, out);
                    path.pop();
                }
            }
            children.push((first, *offset));
        }
        out.insert(
            path.clone(),
            Draw {
                start,
                end: *offset,
                children,
            },
        );
    }
    let mut out = BTreeMap::new();
    visit(nodes, &mut Vec::new(), &mut 0, &mut out);
    out
}
fn remove_choices(nodes: &mut Vec<Trace>, start: usize, end: usize) {
    fn visit(nodes: &mut Vec<Trace>, offset: &mut usize, start: usize, end: usize) {
        nodes.retain_mut(|node| match node {
            Trace::Choice(_) => {
                let keep = *offset < start || *offset >= end;
                *offset += 1;
                keep
            }
            Trace::Group(children) => {
                visit(children, offset, start, end);
                true
            }
        });
    }
    visit(nodes, &mut 0, start, end);
}

impl<T: Clone + PartialEq> Counterexample<'_, T> {
    fn accept(&mut self, candidate: &[Trace]) -> bool {
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
        self.accept_result(Status::Keep(value, used))
    }

    fn accept_result(&mut self, result: Status<T>) -> bool {
        let Status::Keep(value, used) = result else {
            return false;
        };
        if !smaller(&used, &self.choices) {
            return false;
        }
        let previous = Trace::flatten(&self.choices);
        let next = Trace::flatten(&used);
        if previous.len() != next.len() {
            self.cache.changed.clear();
        } else {
            self.cache.changed.extend(
                previous
                    .iter()
                    .zip(&next)
                    .enumerate()
                    .filter_map(|(i, (a, b))| (a != b).then_some(i)),
            );
        }
        self.value = value;
        self.choices = used;
        true
    }

    fn rebuild(&mut self, numbers: &[Choice]) -> Status<T> {
        let before = self.cache.size();
        let result = self.cache.build(numbers);
        self.steps += self.cache.size() - before;
        result
    }

    fn flat_attempt(&mut self, numbers: &[Choice]) -> bool {
        let result = self.rebuild(numbers);
        self.accept_result(result)
    }

    fn feedback(&mut self, candidate: &[Trace], used: &[Trace]) -> bool {
        let numbers = Trace::flatten(candidate);
        let original = Trace::flatten(&self.choices);
        let consumed = Trace::flatten(used);
        let end = original
            .iter()
            .zip(&numbers)
            .rposition(|(a, b)| a != b)
            .map_or(0, |i| i + 1);
        let mut deletions = BTreeSet::new();
        if let Some(lost) = numbers.len().checked_sub(consumed.len()).filter(|n| *n > 0)
            && end + lost <= numbers.len()
        {
            deletions.insert((end, end + lost));
        }
        let old_draws = draws(&self.choices);
        let new_draws = draws(used);
        for (path, old) in &old_draws {
            let Some(new) = new_draws.get(path) else {
                continue;
            };
            if old.start == new.start && new.end < old.end {
                deletions.insert((new.end, old.end));
            }
            // If the shorter draw retained fewer children, try retaining its
            // rightmost children instead of the leftmost ones observed in the trial.
            if old.start <= end && old.end > end {
                let old_children = old
                    .children
                    .iter()
                    .filter(|(start, _)| *start >= end)
                    .collect::<Vec<_>>();
                let count = new
                    .children
                    .iter()
                    .filter(|(start, _)| *start >= end)
                    .count();
                if count > 0 && count < old_children.len() {
                    deletions.insert((
                        old_children[0].0,
                        old_children[old_children.len() - count].0,
                    ));
                }
            }
        }
        for (start, finish) in deletions.into_iter().rev() {
            if start >= finish || finish > numbers.len() {
                continue;
            }
            let mut shortened = candidate.to_vec();
            remove_choices(&mut shortened, start, finish);
            if self.accept(&shortened) {
                return true;
            }
            let mut flat = numbers.clone();
            flat.drain(start..finish);
            if self.flat_attempt(&flat) {
                return true;
            }
        }
        false
    }

    fn consider(&mut self, candidate: &[Trace]) -> bool {
        if self.accept(candidate) {
            return true;
        }
        if let Some(Status::Keep(_, used) | Status::Ignore(used)) =
            self.cache.db.get(candidate).cloned()
            && self.feedback(candidate, &used)
        {
            return true;
        }
        // Rebuilding already evaluated the property on its generated value.
        let result = self.rebuild(&Trace::flatten(candidate));
        if self.accept_result(result.clone()) {
            return true;
        }
        if let Status::Keep(_, used) | Status::Ignore(used) = result
            && self.feedback(candidate, &used)
        {
            return true;
        }
        false
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

    fn adaptive_delete(
        &mut self,
        original: &[Trace],
        path: &[usize],
        start: usize,
        available: usize,
    ) -> bool {
        let mut accepted = 0;
        let mut width = 1;
        let mut rejected = available + 1;
        while width <= available {
            let mut candidate = original.to_vec();
            at_mut(&mut candidate, path).drain(start..start + width);
            if !self.coordinated(&candidate) {
                rejected = width;
                break;
            }
            accepted = width;
            if width == available {
                return true;
            }
            width = width.saturating_mul(2).min(available);
        }
        if accepted == 0 {
            return false;
        }
        // Refine the successful batch size without accepting equal-sized trees.
        while rejected - accepted > 1 {
            let middle = accepted + (rejected - accepted) / 2;
            let mut candidate = original.to_vec();
            at_mut(&mut candidate, path).drain(start..start + middle);
            if self.coordinated(&candidate) {
                accepted = middle;
            } else {
                rejected = middle;
            }
        }
        true
    }

    fn regions(&mut self) -> bool {
        let original = self.choices.clone();
        for path in groups(&original) {
            let children = at(&original, &path);
            if !children.is_empty() {
                let mut candidate = original.clone();
                at_mut(&mut candidate, &path).clear();
                if self.coordinated(&candidate) {
                    return true;
                }
                for start in 0..children.len() {
                    if self.adaptive_delete(&original, &path, start, children.len() - start) {
                        return true;
                    }
                }
                // Target actual draw scopes, not every possible zeroed interval.
                let mut candidate = original.clone();
                zero(at_mut(&mut candidate, &path));
                if self.consider(&candidate) {
                    return true;
                }
                let mut candidate = original.clone();
                *at_mut(&mut candidate, &path) = vec![Trace::Choice(0)];
                if self.consider(&candidate) {
                    return true;
                }
            }
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

    fn joint_numbers(&mut self) -> bool {
        let original = self.choices.clone();
        let values = Trace::flatten(&original);
        for value in values.iter().copied().collect::<BTreeSet<_>>() {
            if value == 0 {
                continue;
            }
            let positions = values
                .iter()
                .enumerate()
                .filter_map(|(i, n)| (*n == value).then_some(i))
                .collect::<Vec<_>>();
            if positions.len() > 1 {
                for replacement in reductions(value) {
                    let mut candidate = original.clone();
                    for &i in &positions {
                        replace_number(&mut candidate, i, replacement);
                    }
                    if self.consider(&candidate) {
                        return true;
                    }
                }
            }
            // Collapse a range of the choice alphabet together, rather than
            // getting trapped when equal or related values must change together.
            for low in values
                .iter()
                .copied()
                .filter(|n| *n > 0 && *n <= value)
                .collect::<BTreeSet<_>>()
            {
                for replacement in [0, low - 1] {
                    let mut candidate = original.clone();
                    for (i, &n) in values.iter().enumerate() {
                        if low <= n && n <= value {
                            replace_number(&mut candidate, i, replacement);
                        }
                    }
                    if self.consider(&candidate) {
                        return true;
                    }
                }
            }
        }
        // Preserve differences across changed blocks and whole draws, as well
        // as pairs. Reset changed indices whenever the primitive length changes.
        let mut subsets = BTreeSet::new();
        subsets.insert(self.cache.changed.iter().copied().collect::<Vec<_>>());
        for draw in draws(&original).values() {
            subsets.insert((draw.start..draw.end).filter(|&i| values[i] > 0).collect());
        }
        for i in 0..values.len() {
            for j in i + 1..values.len() {
                subsets.insert(vec![i, j]);
            }
        }
        for indices in subsets.into_iter().filter(|indices| indices.len() > 1) {
            let common = indices.iter().map(|&i| values[i]).min().unwrap();
            for remaining in reductions(common) {
                let delta = common - remaining;
                let mut candidate = original.clone();
                for &i in &indices {
                    replace_number(&mut candidate, i, values[i] - delta);
                }
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        false
    }

    fn reorder_draws(&mut self) -> bool {
        let original = self.choices.clone();
        for path in groups(&original) {
            let children = at(&original, &path);
            let indices = children
                .iter()
                .enumerate()
                .filter_map(|(i, node)| matches!(node, Trace::Group(_)).then_some(i))
                .collect::<Vec<_>>();
            let mut sorted = indices
                .iter()
                .map(|&i| children[i].clone())
                .collect::<Vec<_>>();
            sorted.sort_by_key(|node| {
                let n = Trace::flatten(std::slice::from_ref(node));
                (n.len(), n)
            });
            let mut candidate = original.clone();
            for (&i, node) in indices.iter().zip(sorted) {
                at_mut(&mut candidate, &path)[i] = node;
            }
            if self.consider(&candidate) {
                return true;
            }
            for (position, &i) in indices.iter().enumerate() {
                for &j in &indices[position + 1..] {
                    let mut candidate = original.clone();
                    at_mut(&mut candidate, &path).swap(i, j);
                    if self.consider(&candidate) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn block_programs(&mut self) -> bool {
        let original = self.choices.clone();
        let values = Trace::flatten(&original);
        // The paper's X through XXXXX programs operate on primitive choices,
        // including regions that span more than one draw boundary.
        for width in 1..=values.len().min(5) {
            for start in (0..=values.len() - width).rev() {
                let mut candidate = original.clone();
                remove_choices(&mut candidate, start, start + width);
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        // -XX and --X: decrement one/two choices and delete the next two/one.
        for decrements in [1, 2] {
            for start in 0..values.len().saturating_sub(2) {
                if values[start..start + decrements].contains(&0) {
                    continue;
                }
                let mut candidate = original.clone();
                for (index, &value) in values.iter().enumerate().skip(start).take(decrements) {
                    replace_number(&mut candidate, index, value - 1);
                }
                remove_choices(&mut candidate, start + decrements, start + 3);
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        false
    }

    pub fn simplify(&mut self) {
        if Trace::flatten(&self.choices).is_empty() {
            return;
        }
        while self.regions()
            || self.joint_numbers()
            || self.numbers()
            || self.reorder_draws()
            || self.block_programs()
        {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Trace::{Choice as C, Group as G};

    #[test]
    fn unsuccessful_trial_repairs_the_consumed_length() {
        let initial = vec![C(2), C(10), C(42)];
        let mut ce = Counterexample {
            value: "initial",
            choices: initial,
            steps: 0,
            cache: Cache::new(|nodes| match nodes {
                [C(1), C(10), C(42)] => Status::Ignore(vec![C(1), C(10)]),
                [C(1), C(42)] => Status::Keep("repaired", nodes.to_vec()),
                _ => Status::Invalid,
            }),
        };
        assert!(ce.consider(&[C(1), C(10), C(42)]));
        assert_eq!(ce.steps, 3);
        insta::with_settings!({omit_expression => true}, {
            insta::assert_snapshot!(format!("result: {}\nchoices: {:?}\nevaluations: {}", ce.value, ce.choices, ce.steps));
        });
    }

    #[test]
    fn coordinated_and_whole_draw_passes() {
        let cases: Vec<(&str, Vec<Trace>, Oracle<'_, ()>)> = vec![
            (
                "equal choices",
                vec![C(15), C(77), C(15)],
                Box::new(|nodes| match nodes {
                    [C(a), C(77), C(b)] if a == b && *a >= 7 => Status::Keep((), nodes.to_vec()),
                    _ => Status::Invalid,
                }),
            ),
            (
                "common offset",
                vec![C(10), C(13)],
                Box::new(|nodes| match nodes {
                    [C(a), C(b)] if b.checked_sub(*a) == Some(3) => {
                        Status::Keep((), nodes.to_vec())
                    }
                    _ => Status::Invalid,
                }),
            ),
            (
                "three related choices",
                vec![C(10), C(13), C(17)],
                Box::new(|nodes| match nodes {
                    [C(a), C(b), C(c)]
                        if b.checked_sub(*a) == Some(3) && c.checked_sub(*a) == Some(7) =>
                    {
                        Status::Keep((), nodes.to_vec())
                    }
                    _ => Status::Invalid,
                }),
            ),
            (
                "two decrements and deletion",
                vec![C(5), C(7), C(9)],
                Box::new(|nodes| match nodes {
                    [C(4), C(6)] => Status::Keep((), nodes.to_vec()),
                    _ => Status::Invalid,
                }),
            ),
            (
                "whole draws",
                vec![G(vec![C(2), C(8)]), G(vec![C(1), C(9)])],
                Box::new(|nodes| {
                    if nodes == [G(vec![C(1), C(9)]), G(vec![C(2), C(8)])] {
                        Status::Keep((), nodes.to_vec())
                    } else {
                        Status::Invalid
                    }
                }),
            ),
        ];
        let mut results = String::new();
        for (name, choices, oracle) in cases {
            let mut ce = Counterexample {
                value: (),
                choices,
                steps: 0,
                cache: Cache::new(oracle),
            };
            ce.simplify();
            results.push_str(&format!("{name}: {:?}\n", ce.choices));
        }
        insta::with_settings!({omit_expression => true}, { insta::assert_snapshot!(results); });
    }
    #[test]
    fn rebuilding_evaluates_once_and_retains_its_result() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let mut ce = Counterexample {
            value: "original",
            choices: vec![C(9)],
            steps: 0,
            cache: Cache::new(|_| panic!("rebuilt candidates must not regenerate through replay"))
                .with_rebuild(|numbers| {
                    calls.set(calls.get() + 1);
                    match numbers {
                        [8] => Status::Ignore(vec![C(8)]),
                        [7] => Status::Invalid,
                        _ => Status::Keep("rebuilt property result", vec![G(vec![C(0)])]),
                    }
                }),
        };
        assert!(!ce.flat_attempt(&[8]));
        assert!(!ce.flat_attempt(&[7]));
        assert!(ce.flat_attempt(&[6]));
        assert!(!ce.flat_attempt(&[6]));
        assert!(!ce.flat_attempt(&[8]));
        assert!(!ce.flat_attempt(&[7]));
        assert_eq!(calls.get(), 3);
        insta::with_settings!({omit_expression => true}, {
            insta::assert_snapshot!(format!("value: {}\nretained: {:?}\nstrict evaluations: {}\nrebuilding evaluations: {}", ce.value, ce.choices, ce.cache.db.len(), calls.get()));
        });
    }
}
