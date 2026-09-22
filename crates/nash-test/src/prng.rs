use cryptoxide::{blake2b::Blake2b, digest::Digest};
use nash_plutus::{arena::Arena, binder::DeBruijn, constant::Constant, term::Term, typ::Type};
pub type Choice = u64;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prng {
    Seeded {
        seed: [u8; 32],
        choices: Vec<Choice>,
    },
    Replayed {
        choices: Vec<Choice>,
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
    pub fn from_choices(choices: &[Choice]) -> Self {
        Self::Replayed {
            choices: choices.to_vec(),
        }
    }
    pub fn choices(&self) -> Vec<Choice> {
        match self {
            Self::Seeded { choices, .. } => choices.iter().rev().copied().collect(),
            Self::Replayed { choices } => choices.clone(),
        }
    }
    /// Keep the seed chain but reset this iteration's choice history.
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
        let (tag, first, choices) = match self {
            Self::Seeded { seed, choices } => {
                (0, Term::byte_string(arena, arena.alloc(*seed)), choices)
            }
            Self::Replayed { choices } => {
                (1, Term::integer_from(arena, choices.len() as i128), choices)
            }
        };
        let items = arena.alloc(
            choices
                .iter()
                .map(|&n| Constant::integer_from(arena, i128::from(n)))
                .collect::<Vec<_>>(),
        );
        Term::constr(
            arena,
            tag,
            arena.alloc([
                first,
                Term::constant(
                    arena,
                    Constant::proto_list(arena, Type::integer(arena), items),
                ),
            ]),
        )
    }
    pub fn from_term(term: &Term<'_, DeBruijn>) -> Result<Self, String> {
        let Term::Constr {
            tag,
            fields:
                [
                    Term::Constant(first),
                    Term::Constant(Constant::ProtoList(Type::Integer, items)),
                ],
        } = term
        else {
            return Err("malformed PRNG term".into());
        };
        let choices = items
            .iter()
            .map(|i| match i {
                Constant::Integer(n) => {
                    u64::try_from(*n).map_err(|_| "PRNG choice does not fit u64".to_string())
                }
                _ => Err("non-integer PRNG choice".into()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        match (tag, first) {
            (0, Constant::ByteString(bytes)) => Ok(Self::Seeded {
                seed: (*bytes)
                    .try_into()
                    .map_err(|_| "PRNG seed must be 32 bytes")?,
                choices,
            }),
            (1, Constant::Integer(n)) if usize::try_from(*n).ok() == Some(choices.len()) => {
                Ok(Self::Replayed { choices })
            }
            _ => Err("malformed PRNG constructor".into()),
        }
    }
}
