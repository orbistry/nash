use super::BuildError;
use crate::compile::{Solved, SolvedModule};
use nash_ast::{AliasType, QualifiedName, Type, Union};
use nash_frontend::{
    ModuleKey, ModuleName, PackageId, PackageSourceId, SourceBoundaryBinding, SourceEntryPoint,
};
use nash_region::{Located, Region};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeIdentity {
    pub module: ModuleKey,
    pub name: String,
}

#[derive(Clone, Debug)]
pub enum SolvedType {
    Variable(String),
    DeclarationHole {
        id: nash_ast::DeclaredHoleId,
        kind: nash_ast::DeclaredHoleKind,
        owner: TypeIdentity,
        region: Region,
    },
    Named {
        identity: TypeIdentity,
        arguments: Vec<SolvedType>,
    },
    Function {
        arguments: Vec<SolvedType>,
        result: Box<SolvedType>,
    },
    Application {
        function: Box<SolvedType>,
        arguments: Vec<SolvedType>,
    },
    Tuple(Vec<SolvedType>),
    Record(Vec<(String, SolvedType)>),
    Alias {
        identity: TypeIdentity,
        arguments: Vec<(String, SolvedType)>,
        target: Box<SolvedType>,
    },
}

#[derive(Clone, Debug)]
pub struct ConstructorLayout {
    pub name: String,
    pub tag: u64,
    pub fields: Vec<(Option<String>, SolvedType)>,
}

#[derive(Clone, Debug)]
pub struct SolvedDataLayout {
    pub parameters: Vec<String>,
    pub encoding: nash_source::DataEncoding,
    pub constructors: Vec<ConstructorLayout>,
}

#[derive(Clone, Debug)]
pub struct SolvedBoundaryBinding {
    pub name: String,
    pub label: String,
    pub position: usize,
    pub typ: SolvedType,
    pub region: Region,
}

#[derive(Clone, Debug)]
pub struct SolvedHandlerMetadata {
    pub purpose: String,
    pub docs: Option<String>,
    pub arguments: Vec<SolvedBoundaryBinding>,
    pub datum: Option<usize>,
    pub redeemer: Option<usize>,
    pub region: Region,
}

#[derive(Clone, Debug)]
pub struct EntryPointId {
    pub module: ModuleKey,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ValidatorMetadata {
    pub id: EntryPointId,
    pub docs: Option<String>,
    pub parameters: Vec<SolvedBoundaryBinding>,
    pub handlers: Vec<SolvedHandlerMetadata>,
    pub layouts: Arc<BTreeMap<TypeIdentity, SolvedDataLayout>>,
}

struct Builder<'s, 'a> {
    solved: &'s Solved<'a>,
    visited: BTreeSet<QualifiedName<'a>>,
    layouts: BTreeMap<TypeIdentity, SolvedDataLayout>,
}

impl<'s, 'a> Builder<'s, 'a> {
    fn identity(&self, reference: QualifiedName<'a>) -> Result<TypeIdentity, BuildError> {
        let module = if reference.home == nash_ast::primitives::builtin_home() {
            ModuleKey {
                package: PackageId {
                    name: None,
                    version: String::new(),
                    source: PackageSourceId::Compiler,
                },
                module: ModuleName::new("Builtin"),
            }
        } else {
            self.solved
                .modules
                .iter()
                .find(|module| module.module.name == reference.home)
                .map(|module| module.key.clone())
                .ok_or_else(|| BuildError {
                    module: reference.home.name.to_owned(),
                    message: format!("missing solved owner of type {}", reference.name),
                })?
        };
        Ok(TypeIdentity {
            module,
            name: reference.name.to_owned(),
        })
    }

    fn union(&self, reference: QualifiedName<'a>) -> Option<&'a Union<'a>> {
        if reference.home == nash_ast::primitives::builtin_home() {
            return nash_ast::primitives::data_union(reference.name);
        }
        self.solved
            .modules
            .iter()
            .find(|module| module.module.name == reference.home)?
            .module
            .unions
            .iter()
            .find(|union| union.value.name.value == reference.name)
            .map(|union| &union.value)
    }

    fn layout(&mut self, reference: QualifiedName<'a>) -> Result<(), BuildError> {
        if !self.visited.insert(reference) {
            return Ok(());
        }
        let Some(union) = self.union(reference) else {
            return Ok(());
        };
        let Some(layout) = union.data_layout else {
            return Ok(());
        };
        let mut constructors = Vec::with_capacity(union.ctors.len());
        for (index, constructor) in union.ctors.iter().enumerate() {
            let mut fields = Vec::with_capacity(constructor.arguments.len());
            for (position, typ) in constructor.arguments.iter().enumerate() {
                let label = constructor
                    .labels
                    .and_then(|labels| labels.get(position))
                    .map(|label| (*label).to_owned());
                fields.push((label, self.typ(typ)?));
            }
            constructors.push(ConstructorLayout {
                name: constructor.name.to_owned(),
                tag: layout.tags[index],
                fields,
            });
        }
        let identity = self.identity(reference)?;
        self.layouts.insert(
            identity,
            SolvedDataLayout {
                parameters: union
                    .parameters
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                encoding: layout.encoding,
                constructors,
            },
        );
        Ok(())
    }

    fn typ(&mut self, typ: &'a Located<Type<'a>>) -> Result<SolvedType, BuildError> {
        Ok(match &typ.value {
            Type::Hole => {
                return Err(BuildError {
                    module: "metadata".to_owned(),
                    message: "unresolved type hole in solved validator metadata".to_owned(),
                });
            }
            Type::DeclaredHole(hole) => {
                let owner = self
                    .solved
                    .modules
                    .iter()
                    .find(|module| module.module.name == hole.owner.home)
                    .ok_or_else(|| BuildError {
                        module: hole.owner.home.name.to_owned(),
                        message: "missing declaration-hole owner".to_owned(),
                    })?;
                match owner.tables.kinds.declared.resolve(self.solved.store, hole) {
                    Some(solution) => self.typ(solution)?,
                    None => SolvedType::DeclarationHole {
                        id: hole.id,
                        kind: hole.kind,
                        owner: self.identity(hole.owner)?,
                        region: hole.region,
                    },
                }
            }
            Type::Var(name) => SolvedType::Variable((*name).to_owned()),
            Type::Named { reference, args } => {
                self.layout(*reference)?;
                SolvedType::Named {
                    identity: self.identity(*reference)?,
                    arguments: args
                        .iter()
                        .map(|arg| self.typ(arg))
                        .collect::<Result<_, _>>()?,
                }
            }
            Type::Function { arguments, result } => SolvedType::Function {
                arguments: arguments
                    .iter()
                    .map(|arg| self.typ(arg))
                    .collect::<Result<_, _>>()?,
                result: Box::new(self.typ(result)?),
            },
            Type::Lambda { from, to } => SolvedType::Function {
                arguments: vec![self.typ(from)?],
                result: Box::new(self.typ(to)?),
            },
            Type::App { head, args } => SolvedType::Application {
                function: Box::new(self.typ(head)?),
                arguments: args
                    .iter()
                    .map(|arg| self.typ(arg))
                    .collect::<Result<_, _>>()?,
            },
            Type::Tuple {
                first,
                second,
                rest,
            } => SolvedType::Tuple(
                [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|item| self.typ(item))
                    .collect::<Result<_, _>>()?,
            ),
            Type::Record { fields } => {
                let mut fields = fields.iter().collect::<Vec<_>>();
                fields.sort_by_key(|field| field.index);
                SolvedType::Record(
                    fields
                        .into_iter()
                        .map(|field| Ok((field.field.to_owned(), self.typ(field.typ)?)))
                        .collect::<Result<_, BuildError>>()?,
                )
            }
            Type::Alias {
                reference,
                arguments,
                target,
                ..
            } => SolvedType::Alias {
                identity: self.identity(*reference)?,
                arguments: arguments
                    .iter()
                    .map(|arg| Ok((arg.name.to_owned(), self.typ(arg.typ)?)))
                    .collect::<Result<_, BuildError>>()?,
                target: Box::new(self.typ(match target {
                    AliasType::Open(body) => body,
                    AliasType::Filled { typ, .. } => typ,
                })?),
            },
        })
    }

    fn binding(
        &mut self,
        source: &SourceBoundaryBinding<'_>,
        typ: &'a Located<Type<'a>>,
    ) -> Result<SolvedBoundaryBinding, BuildError> {
        Ok(SolvedBoundaryBinding {
            name: source.name.to_owned(),
            label: source.label.to_owned(),
            position: source.position,
            typ: self.typ(typ)?,
            region: source.region,
        })
    }
}

fn arguments<'a>(
    mut typ: &'a Located<Type<'a>>,
    count: usize,
) -> Option<Vec<&'a Located<Type<'a>>>> {
    let mut result = Vec::with_capacity(count);
    while result.len() < count {
        match &typ.value {
            Type::Function {
                arguments,
                result: output,
            } => {
                result.extend_from_slice(arguments);
                typ = output;
            }
            Type::Lambda { from, to } => {
                result.push(*from);
                typ = to;
            }
            Type::Alias {
                target: AliasType::Filled { typ: target, .. },
                ..
            } => typ = target,
            _ => return None,
        }
    }
    (result.len() == count).then_some(result)
}

pub(super) fn validator<'a>(
    solved: &Solved<'a>,
    module: &SolvedModule<'a>,
    source: &SourceEntryPoint<'a>,
) -> Result<ValidatorMetadata, BuildError> {
    let mut builder = Builder {
        solved,
        visited: BTreeSet::new(),
        layouts: BTreeMap::new(),
    };
    let mut parameters = Vec::new();
    let mut handlers = Vec::with_capacity(source.handlers.len());
    for (index, handler) in source.handlers.iter().enumerate() {
        let types = module
            .annotations
            .get(handler.function)
            .and_then(|annotation| {
                arguments(
                    annotation.typ,
                    source.parameters.len() + handler.arguments.len(),
                )
            })
            .ok_or_else(|| BuildError {
                module: module.module.name.name.to_owned(),
                message: format!("missing solved handler signature {}", handler.function),
            })?;
        if index == 0 {
            parameters = source
                .parameters
                .iter()
                .zip(&types)
                .map(|(source, typ)| builder.binding(source, typ))
                .collect::<Result<_, _>>()?;
        }
        let bindings = handler
            .arguments
            .iter()
            .zip(types.iter().skip(source.parameters.len()))
            .map(|(source, typ)| builder.binding(source, typ))
            .collect::<Result<_, _>>()?;
        handlers.push(SolvedHandlerMetadata {
            purpose: handler.purpose.to_owned(),
            docs: handler.docs.map(str::to_owned),
            arguments: bindings,
            datum: (handler.purpose == "spend").then_some(0),
            redeemer: (handler.purpose != "else")
                .then_some(usize::from(handler.purpose == "spend")),
            region: handler.region,
        });
    }
    Ok(ValidatorMetadata {
        id: EntryPointId {
            module: module.key.clone(),
            name: source.name.to_owned(),
        },
        docs: source.docs.map(str::to_owned),
        parameters,
        handlers,
        layouts: Arc::new(builder.layouts),
    })
}
