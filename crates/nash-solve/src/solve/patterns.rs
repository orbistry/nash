use super::*;
use nash_constrain::error::{PCategory, PContext};

/// Match whether the former pattern state would own any fresh variables.
/// The caller must select the pattern's rank before inference begins.
pub(super) fn pattern_owns_vars(pattern: &Located<nash_ast::Pattern<'_>>) -> bool {
    use nash_ast::Pattern;
    match &pattern.value {
        Pattern::Anything | Pattern::Var(_) | Pattern::Unit | Pattern::Bool { .. } => false,
        Pattern::Alias { pattern, .. } => pattern_owns_vars(pattern),
        Pattern::Constructor(ctor) => {
            !ctor.union.parameters.is_empty()
                || ctor
                    .arguments
                    .iter()
                    .any(|arg| pattern_owns_vars(arg.pattern))
        }
        Pattern::Tuple { .. }
        | Pattern::List(_)
        | Pattern::Cons { .. }
        | Pattern::Record(_)
        | Pattern::Int(_)
        | Pattern::Str(_)
        | Pattern::Bytes(_) => true,
    }
}

impl<'a> Solver<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_pattern(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        mut state: State<'a>,
        pattern: &Located<nash_ast::Pattern<'a>>,
        expected: PExpected<'a, Variable>,
        headers: &mut BTreeMap<&'a str, Located<Variable>>,
    ) -> State<'a> {
        use nash_ast::Pattern;
        let region = pattern.region;
        let expected_var = match expected {
            PExpected::NoExpectation(var) | PExpected::FromContext(_, _, var) => var,
        };
        match &pattern.value {
            Pattern::Anything => state,
            Pattern::Var(name) => {
                headers.insert(name, Located::at(region, expected_var));
                state
            }
            Pattern::Alias {
                pattern: inner,
                name,
            } => {
                headers.insert(name, Located::at(region, expected_var));
                self.infer_pattern(uf, rank, state, inner, expected, headers)
            }
            Pattern::Unit | Pattern::Bool { .. } => {
                let (name, category) = if matches!(pattern.value, Pattern::Unit) {
                    ("unit", PCategory::Unit)
                } else {
                    ("bool", PCategory::Bool)
                };
                let actual = self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::App1(
                        nash_ast::primitives::builtin_home(),
                        name,
                        Vec::new(),
                    )),
                );
                self.pattern_equal(uf, rank, state, region, category, actual, expected)
            }
            Pattern::Tuple {
                first,
                second,
                rest,
            } => {
                let first_var = self.register(uf, rank, Content::FlexVar(None));
                let second_var = self.register(uf, rank, Content::FlexVar(None));
                let rest_vars: Vec<_> = rest
                    .iter()
                    .map(|_| self.register(uf, rank, Content::FlexVar(None)))
                    .collect();
                let actual = self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::Tuple1(first_var, second_var, rest_vars.clone())),
                );
                state =
                    self.pattern_equal(uf, rank, state, region, PCategory::Tuple, actual, expected);
                for (item, var) in rest.iter().zip(rest_vars).rev() {
                    state = self.infer_pattern(
                        uf,
                        rank,
                        state,
                        item,
                        PExpected::NoExpectation(var),
                        headers,
                    );
                }
                state = self.infer_pattern(
                    uf,
                    rank,
                    state,
                    second,
                    PExpected::NoExpectation(second_var),
                    headers,
                );
                self.infer_pattern(
                    uf,
                    rank,
                    state,
                    first,
                    PExpected::NoExpectation(first_var),
                    headers,
                )
            }
            Pattern::Constructor(ctor) => {
                let pairs: Vec<_> = ctor
                    .union
                    .parameters
                    .iter()
                    .map(|name| (*name, self.register(uf, rank, Content::FlexVar(Some(name)))))
                    .collect();
                let variables: BTreeMap<_, _> = pairs.iter().copied().collect();
                let actual = self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::App1(
                        ctor.reference.home,
                        ctor.reference.union,
                        pairs.iter().map(|(_, var)| *var).collect(),
                    )),
                );
                state = self.pattern_equal(
                    uf,
                    rank,
                    state,
                    region,
                    PCategory::Ctor(ctor.reference.name),
                    actual,
                    expected,
                );
                for arg in ctor.arguments.iter().rev() {
                    state = self.infer_canonical_pattern(
                        uf,
                        rank,
                        state,
                        arg.pattern,
                        PExpected::FromContext(
                            region,
                            PContext::CtorArg(ctor.reference.name, arg.index as usize),
                            arg.typ,
                        ),
                        &variables,
                        headers,
                    );
                }
                state
            }
            Pattern::List(items) => {
                let entry = self.register(uf, rank, Content::FlexVar(None));
                let list = self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::App1(
                        nash_ast::primitives::builtin_home(),
                        "list",
                        vec![entry],
                    )),
                );
                state =
                    self.pattern_equal(uf, rank, state, region, PCategory::List, list, expected);
                for (index, item) in items.iter().enumerate().rev() {
                    state = self.infer_pattern(
                        uf,
                        rank,
                        state,
                        item,
                        PExpected::FromContext(region, PContext::ListEntry(index), entry),
                        headers,
                    );
                }
                state
            }
            Pattern::Cons { head, tail } => {
                let entry = self.register(uf, rank, Content::FlexVar(None));
                let list = self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::App1(
                        nash_ast::primitives::builtin_home(),
                        "list",
                        vec![entry],
                    )),
                );
                state =
                    self.pattern_equal(uf, rank, state, region, PCategory::List, list, expected);
                state = self.infer_pattern(
                    uf,
                    rank,
                    state,
                    head,
                    PExpected::NoExpectation(entry),
                    headers,
                );
                // The tail has its own structural expectation; a failed outer
                // comparison must not poison this independent equation.
                let list = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "list", vec![entry]),
                );
                self.infer_pattern(
                    uf,
                    rank,
                    state,
                    tail,
                    PExpected::FromContext(region, PContext::Tail, list),
                    headers,
                )
            }
            Pattern::Record(fields) => {
                let record = self.register(uf, rank, Content::FlexVar(None));
                // Allocate in source order, then execute field checks in the
                // reverse order used by the former rev_cons list.
                let fields_with_vars: Vec<_> = fields
                    .iter()
                    .map(|field| {
                        let var = self.register(uf, rank, Content::FlexVar(None));
                        headers
                            .entry(*field)
                            .or_insert_with(|| Located::at(region, var));
                        (*field, var)
                    })
                    .collect();
                if fields.is_empty() {
                    state = self.field(
                        uf,
                        rank,
                        state,
                        DeferredField {
                            region,
                            context: type_::FieldContext::Pattern,
                            record,
                            field: None,
                        },
                    );
                }
                for field in fields_with_vars.into_iter().rev() {
                    state = self.field(
                        uf,
                        rank,
                        state,
                        DeferredField {
                            region,
                            context: type_::FieldContext::Pattern,
                            record,
                            field: Some(field),
                        },
                    );
                }
                self.pattern_equal(uf, rank, state, region, PCategory::Record, record, expected)
            }
            Pattern::Int(_) | Pattern::Str(_) | Pattern::Bytes(_) => {
                let (trait_name, category) = match pattern.value {
                    Pattern::Int(_) => ("FromInt", PCategory::Int),
                    Pattern::Bytes(_) => ("FromBytes", PCategory::Bytes),
                    _ => ("FromString", PCategory::Str),
                };
                let actual = self.register(uf, rank, Content::FlexVar(None));
                let annotation = type_::literal_annotation(
                    self.bump,
                    &[type_::literal_trait(trait_name), type_::eq_trait()],
                );
                state = self.foreign(
                    uf,
                    rank,
                    state,
                    region,
                    nash_ast::NodeId::pattern(pattern),
                    "literal",
                    annotation,
                    Expected::NoExpectation(actual),
                );
                self.pattern_equal(uf, rank, state, region, category, actual, expected)
            }
        }
    }
}

impl<'a> Solver<'a, '_> {
    /// Canonical structures are materialized separately for alias bindings and
    /// pattern equations. Canonical variables still resolve through the same
    /// substitution, even when inference has already learned their structure.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_canonical_pattern(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        state: State<'a>,
        pattern: &Located<nash_ast::Pattern<'a>>,
        expected: PExpected<'a, &'a Located<CanType<'a>>>,
        variables: &BTreeMap<&'a str, Variable>,
        headers: &mut BTreeMap<&'a str, Located<Variable>>,
    ) -> State<'a> {
        let (PExpected::NoExpectation(typ) | PExpected::FromContext(_, _, typ)) = expected;
        let var = self.src_type_to_var(uf, rank, variables, typ);
        match &pattern.value {
            nash_ast::Pattern::Alias {
                pattern: inner,
                name,
            } => {
                headers.insert(name, Located::at(pattern.region, var));
                self.infer_canonical_pattern(uf, rank, state, inner, expected, variables, headers)
            }
            _ => self.infer_pattern(
                uf,
                rank,
                state,
                pattern,
                expected.type_replace(var),
                headers,
            ),
        }
    }
}
