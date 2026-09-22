//! Choice-sequence shrinking with exact-input memoization.
use crate::prng::Choice;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq)]
pub enum Status<T> {
    Keep(T),
    Ignore,
    Invalid,
}
type Oracle<'a, T> = Box<dyn FnMut(&[Choice]) -> Status<T> + 'a>;
pub struct Cache<'a, T> {
    db: BTreeMap<Vec<Choice>, Status<T>>,
    run: Oracle<'a, T>,
}
impl<'a, T: Clone + PartialEq> Cache<'a, T> {
    pub fn new(run: impl FnMut(&[Choice]) -> Status<T> + 'a) -> Self {
        Self {
            db: BTreeMap::new(),
            run: Box::new(run),
        }
    }
    pub fn size(&self) -> usize {
        self.db.len()
    }
    pub fn get(&mut self, choices: &[Choice]) -> Status<T> {
        // Generator functions can inspect replay length, so an extension need not
        // have the same outcome as a successful prefix. Cache exact inputs only.
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
    pub choices: Vec<Choice>,
    pub cache: Cache<'a, T>,
    pub steps: usize,
}
impl<T: Clone + PartialEq> Counterexample<'_, T> {
    fn consider(&mut self, choices: &[Choice]) -> bool {
        if choices == self.choices {
            return true;
        }
        if !(choices.len() < self.choices.len()
            || (choices.len() == self.choices.len() && choices < self.choices.as_slice()))
        {
            return false;
        }
        self.steps += 1;
        match self.cache.get(choices) {
            Status::Keep(value) => {
                self.value = value;
                self.choices = choices.to_vec();
                true
            }
            _ => false,
        }
    }
    fn replace(&mut self, replacements: Vec<(usize, Choice)>) -> bool {
        let mut choices = self.choices.clone();
        for (i, v) in replacements {
            let Some(item) = choices.get_mut(i) else {
                return false;
            };
            *item = v;
        }
        self.consider(&choices)
    }
    fn binary(
        &mut self,
        mut lo: Choice,
        mut hi: Choice,
        f: impl Fn(Choice) -> Vec<(usize, Choice)>,
    ) {
        if self.replace(f(lo)) {
            return;
        }
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.replace(f(mid)) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
    }
    pub fn simplify(&mut self) {
        loop {
            let before = self.choices.clone();
            for k in [8, 4, 2, 1] {
                let mut end = self.choices.len();
                while end >= k {
                    let i = end - k;
                    let mut candidate = self.choices.clone();
                    candidate.drain(i..end);
                    let mut accepted = self.consider(&candidate);
                    if !accepted && i > 0 && candidate[i - 1] > 0 {
                        candidate[i - 1] -= 1;
                        accepted = self.consider(&candidate);
                    }
                    end = if accepted {
                        i.min(self.choices.len()) + k.min(self.choices.len().saturating_sub(i))
                    } else {
                        end - 1
                    };
                }
            }
            for k in [8, 4, 2] {
                for end in (k..=self.choices.len()).rev() {
                    self.replace((end - k..end).map(|i| (i, 0)).collect());
                }
            }
            for i in (0..self.choices.len()).rev() {
                self.binary(0, self.choices[i], |v| vec![(i, v)]);
            }
            for k in [8, 4, 2] {
                for end in (k..=self.choices.len()).rev() {
                    let mut sorted = self.choices[end - k..end].to_vec();
                    sorted.sort_unstable();
                    self.replace((end - k..end).zip(sorted).collect());
                }
            }
            for distance in [2, 1] {
                for j in (distance..self.choices.len()).rev() {
                    let i = j - distance;
                    if self.choices[i] > self.choices[j] {
                        self.replace(vec![(i, self.choices[j]), (j, self.choices[i])]);
                    }
                    let (a, b) = (self.choices[i], self.choices[j]);
                    if a > 0 && b <= Choice::MAX - a {
                        self.binary(0, a, |v| vec![(i, v), (j, b + a - v)]);
                    }
                }
            }
            if before == self.choices {
                break;
            }
        }
    }
}
