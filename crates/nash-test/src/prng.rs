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
        let choices = match self {
            Self::Seeded { choices, .. } | Self::Replayed { choices } => choices,
        };
        let items = arena.alloc(
            choices
                .iter()
                .map(|&n| Constant::integer_from(arena, i128::from(n)))
                .collect::<Vec<_>>(),
        );
        let list = Term::constant(
            arena,
            Constant::proto_list(arena, Type::integer(arena), items),
        );
        match self {
            Self::Seeded { seed, .. } => Term::constr(
                arena,
                0,
                arena.alloc([Term::byte_string(arena, arena.alloc(*seed)), list]),
            ),
            Self::Replayed { .. } => Term::constr(arena, 1, arena.alloc([list])),
        }
    }
    pub fn from_term(term: &Term<'_, DeBruijn>) -> Result<Self, String> {
        let (seed, items) = match term {
            Term::Constr {
                tag: 0,
                fields:
                    [
                        Term::Constant(Constant::ByteString(seed)),
                        Term::Constant(Constant::ProtoList(Type::Integer, items)),
                    ],
            } => (Some(*seed), *items),
            Term::Constr {
                tag: 1,
                fields: [Term::Constant(Constant::ProtoList(Type::Integer, items))],
            } => (None, *items),
            _ => return Err("malformed PRNG term".into()),
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
        match seed {
            Some(seed) => Ok(Self::Seeded {
                seed: seed.try_into().map_err(|_| "PRNG seed must be 32 bytes")?,
                choices,
            }),
            None => Ok(Self::Replayed { choices }),
        }
    }
}
