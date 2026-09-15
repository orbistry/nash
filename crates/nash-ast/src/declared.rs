//! Heap-owned declaration refinements, independent of any canonical AST arena.
use crate::{
    AliasArgument, AliasType, DeclaredHole, DeclaredHoleId, DeclaredHoleKind, FieldType,
    ModuleName, PackageName, PackageSource, QualifiedName, Type,
};
use bumpalo::Bump;
use nash_region::{Located, Region};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

/// Clones follow a shared registry root, including after independently built
/// dependency registries are merged. No lock contents borrow a canonical arena.
#[derive(Clone)]
pub struct DeclaredStore {
    state: Arc<Mutex<Registry>>,
}

impl std::fmt::Debug for DeclaredStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.with_root(|root| {
            f.debug_struct("DeclaredStore")
                .field("refinements", &root.solutions.len())
                .finish_non_exhaustive()
        })
    }
}

#[derive(Debug)]
enum Registry {
    Root(Root),
    Redirect(DeclaredStore),
}

#[derive(Debug, Default)]
struct Root {
    solutions: HashMap<DeclaredHoleId, OwnedType>,
    origins: HashMap<DeclaredHoleId, Arc<OwnedHole>>,
}

impl Default for DeclaredStore {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Registry::Root(Root::default()))),
        }
    }
}

/// An immutable point-in-time view. It cannot be merged or committed back into
/// a live registry, which would discard refinements published since the snapshot.
#[derive(Clone, Debug)]
pub struct DeclaredSnapshot {
    solutions: HashMap<DeclaredHoleId, OwnedType>,
}

impl DeclaredSnapshot {
    pub fn resolve<'a>(
        &self,
        bump: &'a Bump,
        hole: &DeclaredHole<'_>,
    ) -> Option<&'a Located<Type<'a>>> {
        if hole.kind == DeclaredHoleKind::Generic {
            return None;
        }
        self.solutions
            .get(&hole.id)
            .map(|typ| typ.materialize(bump))
    }
}

impl DeclaredStore {
    /// Run once under the current root lock. A redirect installed concurrently
    /// with root discovery is followed rather than mutating an obsolete map.
    fn with_root<R>(&self, action: impl FnOnce(&mut Root) -> R) -> R {
        let mut current = self.clone();
        loop {
            let next = {
                let mut state = current.state.lock().expect("declared type store poisoned");
                match &mut *state {
                    Registry::Root(root) => return action(root),
                    Registry::Redirect(next) => next.clone(),
                }
            };
            current = next;
        }
    }

    fn root(&self) -> Self {
        let mut current = self.clone();
        loop {
            let next = {
                let state = current.state.lock().expect("declared type store poisoned");
                match &*state {
                    Registry::Root(_) => None,
                    Registry::Redirect(next) => Some(next.clone()),
                }
            };
            match next {
                Some(next) => current = next,
                None => return current,
            }
        }
    }

    /// Union live roots, not snapshots. Every old handle follows the redirect,
    /// so later refinements and newly discovered child IDs remain visible.
    pub fn merge(&self, other: &Self) {
        loop {
            let left = self.root();
            let right = other.root();
            if Arc::ptr_eq(&left.state, &right.state) {
                return;
            }
            // Both merge callers acquire root locks in the same total order.
            // Redirects also point downward in this order and cannot form cycles.
            let (first, second) = if Arc::as_ptr(&left.state) < Arc::as_ptr(&right.state) {
                (left, right)
            } else {
                (right, left)
            };
            let mut first_state = first.state.lock().expect("declared type store poisoned");
            let mut second_state = second.state.lock().expect("declared type store poisoned");
            let (Registry::Root(target), Registry::Root(source)) =
                (&mut *first_state, &mut *second_state)
            else {
                // Another merge won after discovery; drop both locks and retry.
                continue;
            };
            target.solutions.extend(source.solutions.drain());
            target.origins.extend(source.origins.drain());
            *second_state = Registry::Redirect(first.clone());
            return;
        }
    }

    pub fn snapshot(&self) -> DeclaredSnapshot {
        self.with_root(|root| DeclaredSnapshot {
            solutions: root.solutions.clone(),
        })
    }

    /// Register immutable provenance without publishing an inferred constraint.
    pub fn register(&self, hole: &DeclaredHole<'_>) {
        self.with_root(|root| {
            root.origins
                .entry(hole.id)
                .or_insert_with(|| Arc::new(OwnedHole::capture(hole)));
        });
    }

    /// Materialize one refinement while preserving referenced hole identities.
    /// Clone its immutable owned root under the lock, then release the lock before
    /// allocating into the caller's arena. Generic captures are never global.
    pub fn resolve<'a>(
        &self,
        bump: &'a Bump,
        hole: &DeclaredHole<'_>,
    ) -> Option<&'a Located<Type<'a>>> {
        if hole.kind == DeclaredHoleKind::Generic {
            return None;
        }
        let owned = self.with_root(|root| root.solutions.get(&hole.id).cloned());
        owned.map(|typ| typ.materialize(bump))
    }

    /// Publish a successful solve and all referenced child descriptors atomically.
    /// None clears a refinement when its ID becomes a union's primary root.
    /// Collect the staging iterator and graph before taking the registry lock.
    pub fn commit(&self, staged: impl IntoIterator<Item = (DeclaredHoleId, Option<OwnedType>)>) {
        let staged: Vec<_> = staged.into_iter().collect();
        if staged.is_empty() {
            return;
        }
        let mut origins = HashMap::new();
        let mut seen = HashSet::new();
        for (_, typ) in &staged {
            if let Some(typ) = typ {
                typ.collect_origins(&mut origins, &mut seen);
            }
        }
        self.with_root(|root| {
            root.origins.extend(origins);
            for (id, typ) in staged {
                match typ {
                    Some(typ) => {
                        root.solutions.insert(id, typ);
                    }
                    None => {
                        root.solutions.remove(&id);
                    }
                }
            }
        });
    }
}

/// An immutable owned type graph. Snapshot cloning only increments Arc counts;
/// every name, package identity, source region and alias body is owned here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedType(Arc<Located<OwnedNode>>);

#[derive(Debug, PartialEq, Eq)]
enum OwnedNode {
    Hole,
    DeclaredHole(Arc<OwnedHole>),
    Function {
        arguments: Vec<OwnedType>,
        result: OwnedType,
    },
    Lambda {
        from: OwnedType,
        to: OwnedType,
    },
    Var(String),
    App {
        head: OwnedType,
        args: Vec<OwnedType>,
    },
    Named {
        reference: OwnedQualifiedName,
        args: Vec<OwnedType>,
    },
    Record {
        fields: Vec<OwnedField>,
    },
    Tuple {
        first: OwnedType,
        second: OwnedType,
        rest: Vec<OwnedType>,
    },
    Alias {
        reference: OwnedQualifiedName,
        arguments: Vec<OwnedArgument>,
        remaining: Vec<String>,
        target: OwnedAlias,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedHole {
    id: DeclaredHoleId,
    kind: DeclaredHoleKind,
    owner: OwnedQualifiedName,
    region: Region,
}

impl OwnedHole {
    fn capture(hole: &DeclaredHole<'_>) -> Self {
        Self {
            id: hole.id,
            kind: hole.kind,
            owner: OwnedQualifiedName::capture(hole.owner),
            region: hole.region,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedField {
    index: u16,
    field: String,
    typ: OwnedType,
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedArgument {
    name: String,
    typ: OwnedType,
}

#[derive(Debug, PartialEq, Eq)]
enum OwnedAlias {
    Open(OwnedType),
    Filled { body: OwnedType, typ: OwnedType },
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedQualifiedName {
    package: Option<OwnedPackage>,
    module: String,
    name: String,
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedPackage {
    author: String,
    project: String,
    version: String,
    source: OwnedSource,
    compilation: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
enum OwnedSource {
    Local(Vec<u8>),
    Github,
    Gitlab,
    Bitbucket,
    Compiler,
}

impl OwnedType {
    /// Copy a canonical graph into independently owned storage. Holes are copied
    /// by semantic identity and origin; refinements live only in DeclaredStore.
    pub fn capture(typ: &Located<Type<'_>>) -> Self {
        Self::capture_node(typ, &mut HashMap::new())
    }

    fn capture_node(typ: &Located<Type<'_>>, memo: &mut HashMap<usize, Self>) -> Self {
        let key = std::ptr::from_ref(typ) as usize;
        if let Some(owned) = memo.get(&key) {
            return owned.clone();
        }
        let value = match &typ.value {
            Type::Hole => OwnedNode::Hole,
            Type::DeclaredHole(hole) => OwnedNode::DeclaredHole(Arc::new(OwnedHole::capture(hole))),
            Type::Function { arguments, result } => OwnedNode::Function {
                arguments: arguments
                    .iter()
                    .map(|arg| Self::capture_node(arg, memo))
                    .collect(),
                result: Self::capture_node(result, memo),
            },
            Type::Lambda { from, to } => OwnedNode::Lambda {
                from: Self::capture_node(from, memo),
                to: Self::capture_node(to, memo),
            },
            Type::Var(name) => OwnedNode::Var((*name).to_owned()),
            Type::App { head, args } => OwnedNode::App {
                head: Self::capture_node(head, memo),
                args: args
                    .iter()
                    .map(|arg| Self::capture_node(arg, memo))
                    .collect(),
            },
            Type::Named { reference, args } => OwnedNode::Named {
                reference: OwnedQualifiedName::capture(*reference),
                args: args
                    .iter()
                    .map(|arg| Self::capture_node(arg, memo))
                    .collect(),
            },
            Type::Record { fields } => OwnedNode::Record {
                fields: fields
                    .iter()
                    .map(|field| OwnedField {
                        index: field.index,
                        field: field.field.to_owned(),
                        typ: Self::capture_node(field.typ, memo),
                    })
                    .collect(),
            },
            Type::Tuple {
                first,
                second,
                rest,
            } => OwnedNode::Tuple {
                first: Self::capture_node(first, memo),
                second: Self::capture_node(second, memo),
                rest: rest
                    .iter()
                    .map(|arg| Self::capture_node(arg, memo))
                    .collect(),
            },
            Type::Alias {
                reference,
                arguments,
                remaining,
                target,
            } => OwnedNode::Alias {
                reference: OwnedQualifiedName::capture(*reference),
                arguments: arguments
                    .iter()
                    .map(|arg| OwnedArgument {
                        name: arg.name.to_owned(),
                        typ: Self::capture_node(arg.typ, memo),
                    })
                    .collect(),
                remaining: remaining.iter().map(|name| (*name).to_owned()).collect(),
                target: match target {
                    AliasType::Open(typ) => OwnedAlias::Open(Self::capture_node(typ, memo)),
                    AliasType::Filled { body, typ } => OwnedAlias::Filled {
                        body: Self::capture_node(body, memo),
                        typ: Self::capture_node(typ, memo),
                    },
                },
            },
        };
        let owned = Self(Arc::new(Located::at(typ.region, value)));
        memo.insert(key, owned.clone());
        owned
    }

    fn collect_origins(
        &self,
        origins: &mut HashMap<DeclaredHoleId, Arc<OwnedHole>>,
        seen: &mut HashSet<usize>,
    ) {
        if !seen.insert(Arc::as_ptr(&self.0) as usize) {
            return;
        }
        match &self.0.value {
            OwnedNode::DeclaredHole(hole) => {
                origins.entry(hole.id).or_insert_with(|| hole.clone());
            }
            OwnedNode::Function { arguments, result } => {
                for argument in arguments {
                    argument.collect_origins(origins, seen);
                }
                result.collect_origins(origins, seen);
            }
            OwnedNode::Lambda { from, to } => {
                from.collect_origins(origins, seen);
                to.collect_origins(origins, seen);
            }
            OwnedNode::App { head, args } => {
                head.collect_origins(origins, seen);
                for argument in args {
                    argument.collect_origins(origins, seen);
                }
            }
            OwnedNode::Named { args, .. } => {
                for argument in args {
                    argument.collect_origins(origins, seen);
                }
            }
            OwnedNode::Record { fields } => {
                for field in fields {
                    field.typ.collect_origins(origins, seen);
                }
            }
            OwnedNode::Tuple {
                first,
                second,
                rest,
            } => {
                first.collect_origins(origins, seen);
                second.collect_origins(origins, seen);
                for field in rest {
                    field.collect_origins(origins, seen);
                }
            }
            OwnedNode::Alias {
                arguments, target, ..
            } => {
                for argument in arguments {
                    argument.typ.collect_origins(origins, seen);
                }
                match target {
                    OwnedAlias::Open(typ) => typ.collect_origins(origins, seen),
                    OwnedAlias::Filled { body, typ } => {
                        body.collect_origins(origins, seen);
                        typ.collect_origins(origins, seen);
                    }
                }
            }
            OwnedNode::Hole | OwnedNode::Var(_) => {}
        }
    }

    /// Allocate only borrowed, drop-free canonical nodes in the destination bump.
    /// No Arc, String, Vec or lock is ever moved into arena storage.
    pub fn materialize<'a>(&self, bump: &'a Bump) -> &'a Located<Type<'a>> {
        self.materialize_node(bump, &mut HashMap::new())
    }

    fn materialize_node<'a>(
        &self,
        bump: &'a Bump,
        memo: &mut HashMap<usize, &'a Located<Type<'a>>>,
    ) -> &'a Located<Type<'a>> {
        let key = Arc::as_ptr(&self.0) as usize;
        if let Some(typ) = memo.get(&key) {
            return typ;
        }
        let value = match &self.0.value {
            OwnedNode::Hole => Type::Hole,
            OwnedNode::DeclaredHole(hole) => Type::DeclaredHole(bump.alloc(DeclaredHole::with_id(
                hole.id,
                hole.kind,
                hole.owner.materialize(bump),
                hole.region,
            ))),
            OwnedNode::Function { arguments, result } => Type::Function {
                arguments: bump.alloc_slice_fill_iter(
                    arguments.iter().map(|arg| arg.materialize_node(bump, memo)),
                ),
                result: result.materialize_node(bump, memo),
            },
            OwnedNode::Lambda { from, to } => Type::Lambda {
                from: from.materialize_node(bump, memo),
                to: to.materialize_node(bump, memo),
            },
            OwnedNode::Var(name) => Type::Var(bump.alloc_str(name)),
            OwnedNode::App { head, args } => Type::App {
                head: head.materialize_node(bump, memo),
                args: bump
                    .alloc_slice_fill_iter(args.iter().map(|arg| arg.materialize_node(bump, memo))),
            },
            OwnedNode::Named { reference, args } => Type::Named {
                reference: reference.materialize(bump),
                args: bump
                    .alloc_slice_fill_iter(args.iter().map(|arg| arg.materialize_node(bump, memo))),
            },
            OwnedNode::Record { fields } => Type::Record {
                fields: bump.alloc_slice_fill_iter(fields.iter().map(|field| FieldType {
                    index: field.index,
                    field: bump.alloc_str(&field.field),
                    typ: field.typ.materialize_node(bump, memo),
                })),
            },
            OwnedNode::Tuple {
                first,
                second,
                rest,
            } => Type::Tuple {
                first: first.materialize_node(bump, memo),
                second: second.materialize_node(bump, memo),
                rest: bump
                    .alloc_slice_fill_iter(rest.iter().map(|arg| arg.materialize_node(bump, memo))),
            },
            OwnedNode::Alias {
                reference,
                arguments,
                remaining,
                target,
            } => Type::Alias {
                reference: reference.materialize(bump),
                arguments: bump.alloc_slice_fill_iter(arguments.iter().map(|arg| AliasArgument {
                    name: bump.alloc_str(&arg.name),
                    typ: arg.typ.materialize_node(bump, memo),
                })),
                remaining: bump
                    .alloc_slice_fill_iter(remaining.iter().map(|name| &*bump.alloc_str(name))),
                target: match target {
                    OwnedAlias::Open(typ) => AliasType::Open(typ.materialize_node(bump, memo)),
                    OwnedAlias::Filled { body, typ } => AliasType::Filled {
                        body: body.materialize_node(bump, memo),
                        typ: typ.materialize_node(bump, memo),
                    },
                },
            },
        };
        let typ = bump.alloc(Located::at(self.0.region, value));
        memo.insert(key, typ);
        typ
    }
}

impl OwnedQualifiedName {
    fn capture(reference: QualifiedName<'_>) -> Self {
        Self {
            package: reference.home.package.map(|package| OwnedPackage {
                author: package.author.to_owned(),
                project: package.project.to_owned(),
                version: package.version.to_owned(),
                source: match package.source {
                    PackageSource::Local(path) => OwnedSource::Local(path.to_vec()),
                    PackageSource::Github => OwnedSource::Github,
                    PackageSource::Gitlab => OwnedSource::Gitlab,
                    PackageSource::Bitbucket => OwnedSource::Bitbucket,
                    PackageSource::Compiler => OwnedSource::Compiler,
                },
                compilation: package.compilation,
            }),
            module: reference.home.name.to_owned(),
            name: reference.name.to_owned(),
        }
    }

    fn materialize<'a>(&self, bump: &'a Bump) -> QualifiedName<'a> {
        QualifiedName {
            home: ModuleName {
                package: self.package.as_ref().map(|package| PackageName {
                    author: bump.alloc_str(&package.author),
                    project: bump.alloc_str(&package.project),
                    version: bump.alloc_str(&package.version),
                    source: match &package.source {
                        OwnedSource::Local(path) => {
                            PackageSource::Local(bump.alloc_slice_copy(path))
                        }
                        OwnedSource::Github => PackageSource::Github,
                        OwnedSource::Gitlab => PackageSource::Gitlab,
                        OwnedSource::Bitbucket => PackageSource::Bitbucket,
                        OwnedSource::Compiler => PackageSource::Compiler,
                    },
                    compilation: package.compilation,
                }),
                name: bump.alloc_str(&self.module),
            },
            name: bump.alloc_str(&self.name),
        }
    }
}
