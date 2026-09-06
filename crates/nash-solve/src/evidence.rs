//! Impl evidence for ground canonical predicates, outside the inference loop.
use bumpalo::Bump;
use nash_ast::{Evidence, ImplRef, Pred, QualifiedName, Type};
use nash_can::environment::Tables;
use nash_region::Located;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    MissingImpl,
    NonGround,
    Limit,
}

#[derive(Debug)]
pub struct Error<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: &'a [&'a Located<Type<'a>>],
    pub reason: Failure,
}

/// Resolve a well-kinded canonical predicate using the complete impl table.
/// Nominal aliases retain their identity. Open types are rejected; resolution
/// never chooses a type or kind to make an impl match. Recursive contexts and
/// structural traversal share a bounded work budget.
pub fn resolve<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    pred: &Pred<'a>,
) -> Result<Evidence<'a>, Error<'a>> {
    Resolver {
        bump,
        tables,
        remaining: 16_384,
    }
    .resolve(pred, 0)
}

#[derive(PartialEq, Eq)]
enum Constructor<'a> {
    Named(QualifiedName<'a>),
    Unit,
    Tuple(usize),
    Function,
    Record(Vec<&'a str>),
}

// A region-independent, nominal view for reflexive equality. Alias bodies and
// unsupplied bound parameters are not part of a nominal application.
#[derive(PartialEq, Eq)]
struct Term<'a> {
    con: Constructor<'a>,
    args: Vec<Term<'a>>,
}

struct Resolver<'t, 'a> {
    bump: &'a Bump,
    tables: &'t Tables<'a>,
    remaining: usize,
}

impl<'a> Resolver<'_, 'a> {
    // Charge every node that substitute_type will copy before allocating the
    // result. Open alias bodies are retained; filled bodies are traversed.
    fn substitution_work(&mut self, typ: &Type<'a>, depth: usize) -> Result<(), Failure> {
        self.step(depth)?;
        match typ {
            Type::App { head, args } => {
                self.substitution_work(&head.value, depth + 1)?;
                for arg in *args {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Named { args, .. } => {
                for arg in *args {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Alias {
                arguments, target, ..
            } => {
                for arg in *arguments {
                    self.substitution_work(&arg.typ.value, depth + 1)?;
                }
                if let nash_ast::AliasType::Filled { typ, .. } = target {
                    self.substitution_work(&typ.value, depth + 1)?;
                }
            }
            Type::Lambda { from, to } => {
                self.substitution_work(&from.value, depth + 1)?;
                self.substitution_work(&to.value, depth + 1)?;
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                self.substitution_work(&first.value, depth + 1)?;
                self.substitution_work(&second.value, depth + 1)?;
                for arg in *rest {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Record { fields, .. } => {
                for field in *fields {
                    self.substitution_work(&field.typ.value, depth + 1)?;
                }
            }
            Type::Unit | Type::Var(_) => {}
        }
        Ok(())
    }

    fn step(&mut self, depth: usize) -> Result<(), Failure> {
        if depth >= 128 || self.remaining == 0 {
            return Err(Failure::Limit);
        }
        self.remaining -= 1;
        Ok(())
    }

    fn term(&mut self, typ: &Type<'a>, depth: usize) -> Result<Term<'a>, Failure> {
        self.step(depth)?;
        let (con, args) = match typ {
            Type::Var(_) => return Err(Failure::NonGround),
            Type::App { head, args } => {
                let mut head = self.term(&head.value, depth + 1)?;
                if !matches!(head.con, Constructor::Named(_)) {
                    return Err(Failure::MissingImpl);
                }
                for arg in *args {
                    head.args.push(self.term(&arg.value, depth + 1)?);
                }
                return Ok(head);
            }
            Type::Named { reference, args } => (Constructor::Named(*reference), args.to_vec()),
            Type::Alias {
                reference,
                arguments,
                ..
            } => (
                Constructor::Named(*reference),
                arguments.iter().map(|arg| arg.typ).collect(),
            ),
            Type::Unit => (Constructor::Unit, Vec::new()),
            Type::Tuple {
                first,
                second,
                rest,
            } => (
                Constructor::Tuple(rest.len() + 2),
                [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .collect(),
            ),
            Type::Lambda { from, to } => (Constructor::Function, vec![*from, *to]),
            Type::Record { ext: Some(_), .. } => return Err(Failure::NonGround),
            Type::Record { fields, ext: None } => (
                Constructor::Record(fields.iter().map(|field| field.field).collect()),
                fields.iter().map(|field| field.typ).collect(),
            ),
        };
        let args = args
            .into_iter()
            .map(|arg| self.term(&arg.value, depth + 1))
            .collect::<Result<_, _>>()?;
        Ok(Term { con, args })
    }

    fn resolve(&mut self, pred: &Pred<'a>, depth: usize) -> Result<Evidence<'a>, Error<'a>> {
        let error = |reason| Error {
            trait_: pred.trait_,
            args: pred.args,
            reason,
        };
        self.step(depth).map_err(error)?;
        let terms = pred
            .args
            .iter()
            .map(|arg| self.term(&arg.value, 0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        if pred.trait_ == nash_ast::primitives::eq_trait()
            && self.tables.has_structural_eq()
            && pred.args.len() == 1
            && nash_can::kinds::proves_ground_big(self.bump, &self.tables.kinds, pred.args[0])
        {
            return Ok(Evidence::StructuralEq { typ: pred.args[0] });
        }
        if pred.trait_ == nash_ast::primitives::lift_trait()
            && self.tables.has_reflexive_lift()
            && matches!(terms.as_slice(), [first, second] if first == second)
            && nash_can::kinds::proves_ground_big(self.bump, &self.tables.kinds, pred.args[0])
        {
            return Ok(Evidence::ReflexiveLift { typ: pred.args[0] });
        }
        let mut selected = None;
        for (key, info) in self
            .tables
            .impls
            .iter()
            .filter(|(key, _)| key.trait_ == pred.trait_)
        {
            if let nash_ast::head::Match::Yes(arguments) = nash_ast::head::matches(
                &mut nash_ast::head::Canonical,
                key.heads,
                pred.args,
                info.variables.len(),
                &mut self.remaining,
            )
            .map_err(|_| error(Failure::Limit))?
            {
                selected = Some((*key, *info, arguments));
                break;
            }
        }
        let (key, info, type_args) = selected.ok_or_else(|| error(Failure::MissingImpl))?;
        let substitution = info
            .variables
            .iter()
            .copied()
            .zip(type_args.iter().copied())
            .collect();
        let mut children = Vec::new();
        for context in info.context {
            for arg in context.args {
                self.substitution_work(&arg.value, 0).map_err(error)?;
            }
            let args = self.bump.alloc_slice_fill_iter(
                context
                    .args
                    .iter()
                    .map(|arg| nash_can::types::substitute_type(self.bump, &substitution, arg)),
            );
            children.push(self.resolve(
                &Pred {
                    trait_: context.trait_,
                    args,
                },
                depth + 1,
            )?);
        }
        Ok(Evidence::Impl {
            impl_: ImplRef {
                home: info.home,
                key,
            },
            type_args: self.bump.alloc_slice_copy(&type_args),
            args: self.bump.alloc_slice_fill_iter(children),
        })
    }
}
