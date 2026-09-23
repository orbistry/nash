use cryptoxide::{blake2b::Blake2b, digest::Digest};
use nash_plutus::{arena::Arena, binder::DeBruijn, constant::Constant, term::Term, typ::Type};
pub type Choice = u64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Trace {
    Choice(Choice),
    Group(Vec<Trace>),
}

impl Trace {
    pub fn flatten(nodes: &[Self]) -> Vec<Choice> {
        fn visit(nodes: &[Trace], out: &mut Vec<Choice>) {
            for node in nodes {
                match node {
                    Trace::Choice(n) => out.push(*n),
                    Trace::Group(children) => visit(children, out),
                }
            }
        }
        let mut out = Vec::new();
        visit(nodes, &mut out);
        out
    }

    pub fn to_term<'a>(&self, arena: &'a Arena) -> &'a Term<'a, DeBruijn> {
        match self {
            Self::Choice(n) => Term::constr(
                arena,
                0,
                arena.alloc([Term::integer_from(arena, i128::from(*n))]),
            ),
            Self::Group(children) => {
                Term::constr(arena, 1, arena.alloc([Self::list_to_term(children, arena)]))
            }
        }
    }

    pub fn list_to_term<'a>(nodes: &[Self], arena: &'a Arena) -> &'a Term<'a, DeBruijn> {
        nodes
            .iter()
            .rev()
            .fold(Term::constr(arena, 0, &[]), |rest, child| {
                Term::constr(arena, 1, arena.alloc([child.to_term(arena), rest]))
            })
    }

    pub fn from_term(term: &Term<'_, DeBruijn>) -> Result<Self, String> {
        match term {
            Term::Constr {
                tag: 0,
                fields: [Term::Constant(Constant::Integer(n))],
            } => Ok(Self::Choice(
                u64::try_from(*n).map_err(|_| "choice does not fit u64")?,
            )),
            Term::Constr {
                tag: 1,
                fields: [children],
            } => Ok(Self::Group(Self::list_from_term(children)?)),
            _ => Err("malformed choice tree".into()),
        }
    }

    pub fn list_from_term(mut term: &Term<'_, DeBruijn>) -> Result<Vec<Self>, String> {
        let mut nodes = Vec::new();
        loop {
            match term {
                Term::Constr { tag: 0, fields: [] } => return Ok(nodes),
                Term::Constr {
                    tag: 1,
                    fields: [child, rest],
                } => {
                    nodes.push(Self::from_term(child)?);
                    term = rest;
                }
                _ => return Err("malformed choice tree list".into()),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prng {
    Seeded {
        seed: [u8; 32],
        choices: Vec<Trace>,
    },
    Replayed {
        remaining: Vec<Trace>,
        choices: Vec<Trace>,
    },
    /// Build a structural proposal from explicit choices. Never used to accept a failure.
    Rebuilding {
        remaining: Vec<Choice>,
        choices: Vec<Trace>,
    },
}
impl Prng {
    pub fn from_seed(seed: u32) -> Self {
        let mut bytes = [0; 32];
        let mut hash = Blake2b::new(32);
        hash.input(&seed.to_be_bytes());
        hash.result(&mut bytes);
        Self::Seeded {
            seed: bytes,
            choices: vec![],
        }
    }
    pub fn from_trace(choices: &[Trace]) -> Self {
        Self::Replayed {
            remaining: choices.to_vec(),
            choices: vec![],
        }
    }
    pub fn rebuild(choices: &[Choice]) -> Self {
        Self::Rebuilding {
            remaining: choices.to_vec(),
            choices: vec![],
        }
    }
    /// Consumed history is accumulated in reverse by Nash at each scope.
    pub fn choices(&self) -> Vec<Trace> {
        let choices = match self {
            Self::Seeded { choices, .. }
            | Self::Replayed { choices, .. }
            | Self::Rebuilding { choices, .. } => choices,
        };
        choices.iter().rev().cloned().collect()
    }
    pub fn next_iteration(self) -> Self {
        match self {
            Self::Seeded { seed, .. } => Self::Seeded {
                seed,
                choices: vec![],
            },
            p => p,
        }
    }
    pub fn to_term<'a>(&self, arena: &'a Arena) -> &'a Term<'a, DeBruijn> {
        match self {
            Self::Seeded { seed, choices } => Term::constr(
                arena,
                0,
                arena.alloc([
                    Term::byte_string(arena, arena.alloc(*seed)),
                    Trace::list_to_term(choices, arena),
                ]),
            ),
            Self::Rebuilding { remaining, choices } => {
                let values = remaining
                    .iter()
                    .map(|n| Constant::integer_from(arena, i128::from(*n)))
                    .collect::<Vec<_>>();
                Term::constr(
                    arena,
                    2,
                    arena.alloc([
                        Term::constant(
                            arena,
                            Constant::proto_list(
                                arena,
                                Type::integer(arena),
                                arena.alloc_slice_copy(&values),
                            ),
                        ),
                        Trace::list_to_term(choices, arena),
                    ]),
                )
            }
            Self::Replayed { remaining, choices } => Term::constr(
                arena,
                1,
                arena.alloc([
                    Trace::list_to_term(remaining, arena),
                    Trace::list_to_term(choices, arena),
                ]),
            ),
        }
    }
    pub fn from_term(term: &Term<'_, DeBruijn>) -> Result<Self, String> {
        match term {
            Term::Constr {
                tag: 0,
                fields: [Term::Constant(Constant::ByteString(seed)), choices],
            } => Ok(Self::Seeded {
                seed: (*seed)
                    .try_into()
                    .map_err(|_| "PRNG seed must be 32 bytes")?,
                choices: Trace::list_from_term(choices)?,
            }),
            Term::Constr {
                tag: 1,
                fields: [remaining, choices],
            } => Ok(Self::Replayed {
                remaining: Trace::list_from_term(remaining)?,
                choices: Trace::list_from_term(choices)?,
            }),
            Term::Constr {
                tag: 2,
                fields:
                    [
                        Term::Constant(Constant::ProtoList(Type::Integer, values)),
                        choices,
                    ],
            } => {
                let remaining = values
                    .iter()
                    .map(|value| match value {
                        Constant::Integer(n) => {
                            u64::try_from(*n).map_err(|_| "choice does not fit u64".to_owned())
                        }
                        _ => Err("malformed rebuilding choice".to_owned()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Rebuilding {
                    remaining,
                    choices: Trace::list_from_term(choices)?,
                })
            }
            _ => Err("malformed PRNG term".into()),
        }
    }
}
