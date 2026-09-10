//! Type names as they can be written in the current source module.
use crate::doc::Doc;
use nash_ast::ModuleName;
use nash_source::{Exposed, Exposing, Import, Module};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct Localizer {
    imports: BTreeMap<String, ImportInfo>,
    local_module: Option<String>,
    local_package: Option<(String, String)>,
    local_unions: BTreeSet<String>,
    bare_primitives: BTreeSet<String>,
}
#[derive(Clone, Debug)]
struct ImportInfo {
    alias: Option<String>,
    exposing: Option<BTreeSet<String>>,
}
impl Localizer {
    pub fn from_module(module: &Module<'_>, defaults: &[&Import<'_>]) -> Self {
        let mut this = Self {
            local_module: Some(module.name.map_or("Main", |name| name.value).to_owned()),
            bare_primitives: nash_ast::primitives::PRIMITIVES
                .iter()
                .map(|primitive| primitive.name.to_owned())
                .collect(),
            local_unions: module
                .unions
                .iter()
                .map(|union| union.value.name.value.to_owned())
                .collect(),
            ..Self::default()
        };
        for import in defaults.iter().chain(module.imports.iter()) {
            this.add_import(import);
        }
        for name in module
            .unions
            .iter()
            .map(|union| union.value.name.value)
            .chain(module.aliases.iter().map(|alias| alias.value.name.value))
        {
            this.bare_primitives.remove(name);
        }
        this.imports.insert(
            module.name.map_or("Main", |n| n.value).into(),
            ImportInfo {
                alias: None,
                exposing: None,
            },
        );
        this
    }
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            imports: names
                .into_iter()
                .map(|n| {
                    (
                        n.into(),
                        ImportInfo {
                            alias: None,
                            exposing: None,
                        },
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn with_package(mut self, package: Option<nash_ast::PackageName<'_>>) -> Self {
        self.local_package =
            package.map(|package| (package.author.to_owned(), package.project.to_owned()));
        self
    }

    pub fn is_local_union(&self, home: ModuleName<'_>, name: &str) -> bool {
        self.local_module.as_deref() == Some(home.name)
            && self
                .local_package
                .as_ref()
                .map(|(author, project)| (author.as_str(), project.as_str()))
                == home
                    .package
                    .map(|package| (package.author, package.project))
            && self.local_unions.contains(name)
    }

    fn add_import(&mut self, import: &Import<'_>) {
        let exposing: Option<BTreeSet<String>> = match import.exposing {
            Exposing::Open => None,
            Exposing::Explicit(names) => Some(
                names
                    .iter()
                    .filter_map(|e| match e {
                        Exposed::Upper { name, .. } | Exposed::LowerType { name, .. } => {
                            Some(name.value.to_owned())
                        }
                        Exposed::Lower(_) | Exposed::Operator { .. } => None,
                    })
                    .collect(),
            ),
        };
        if import.import.value != "Builtin" {
            match &exposing {
                None => self.bare_primitives.clear(),
                Some(names) => self.bare_primitives.retain(|name| !names.contains(name)),
            }
        }
        self.imports.insert(
            import.import.value.into(),
            ImportInfo {
                alias: import.alias.map(str::to_owned),
                exposing,
            },
        );
    }
    pub fn to_string(&self, home: ModuleName<'_>, name: &str) -> String {
        if home == nash_ast::primitives::builtin_home()
            && nash_ast::primitives::PRIMITIVES
                .iter()
                .any(|primitive| primitive.name == name)
            && self.local_module.is_some()
        {
            return if self.bare_primitives.contains(name) {
                name.to_owned()
            } else {
                format!("Builtin.{name}")
            };
        }
        match self.imports.get(home.name) {
            Some(ImportInfo { exposing: None, .. }) => name.into(),
            Some(ImportInfo {
                alias,
                exposing: Some(names),
            }) => {
                if names.contains(name) {
                    name.into()
                } else {
                    format!("{}.{name}", alias.as_deref().unwrap_or(home.name))
                }
            }
            None => format!("{}.{name}", home.name),
        }
    }
    pub fn to_doc(&self, home: ModuleName<'_>, name: &str) -> Doc {
        Doc::text(self.to_string(home, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_types_and_alias() {
        use nash_region::Located;
        let name = Located::at_zero("Other");
        let little = Located::at_zero("little");
        let exposed = Exposed::LowerType {
            name: &little,
            privacy: nash_source::Privacy::Private,
        };
        let entries = [&exposed];
        let exposing = Exposing::Explicit(&entries);
        let import = Import {
            import: &name,
            alias: Some("O"),
            exposing: &exposing,
        };
        let mut localizer = Localizer::default();
        localizer.add_import(&import);
        let home = ModuleName {
            package: None,
            name: "Other",
        };
        insta::assert_snapshot!(localizer.to_string(home,"little"), @"little");
        insta::assert_snapshot!(localizer.to_string(home,"Thing"), @"O.Thing");
    }
    #[test]
    fn names() {
        let home = ModuleName {
            package: None,
            name: "Other",
        };
        insta::assert_snapshot!(Localizer::from_names(["Other"]).to_string(home,"Thing"), @"Thing");
        insta::assert_snapshot!(Localizer::default().to_string(home,"Thing"), @"Other.Thing");
    }
    #[test]
    fn source_module_does_not_invent_default_imports() {
        let arena = bumpalo::Bump::new();
        let module = nash_parse::Parser::new(&arena, b"module Local exposing (..)\nx = 1\n")
            .module()
            .unwrap();
        let localizer = Localizer::from_module(&module, &[]);
        insta::assert_snapshot!(localizer.to_string(ModuleName{package:None,name:"Local"},"Own"), @"Own");
        insta::assert_snapshot!(localizer.to_string(nash_ast::primitives::builtin_home(),"Int"), @"Int");
    }
    #[test]
    fn local_unions_are_not_imported_aliases_or_other_packages() {
        let bump = bumpalo::Bump::new();
        let module = nash_parse::Parser::new(
            &bump,
            b"module Local exposing (..)\ntype Token = Token\ntype alias Wrapper = Token\n",
        )
        .module()
        .unwrap();
        let localizer = Localizer::from_module(&module, &[]);
        let home = ModuleName {
            package: None,
            name: "Local",
        };
        assert!(localizer.is_local_union(home, "Token"));
        assert!(!localizer.is_local_union(home, "Wrapper"));
        assert!(!localizer.is_local_union(
            ModuleName {
                name: "Other",
                ..home
            },
            "Token"
        ));
        assert!(!localizer.is_local_union(
            ModuleName {
                package: Some(nash_ast::primitives::CORE),
                ..home
            },
            "Token"
        ));
    }

    #[test]
    fn shadowed_primitive_keeps_qualified_builtin_identity() {
        let bump = bumpalo::Bump::new();
        let module =
            nash_parse::Parser::new(&bump, b"module Local exposing (..)\ntype Int = Custom\n")
                .module()
                .unwrap();
        let localizer = Localizer::from_module(&module, &[]);
        assert_eq!(
            localizer.to_string(nash_ast::primitives::builtin_home(), "Int"),
            "Builtin.Int"
        );
        assert_eq!(
            localizer.to_string(nash_ast::primitives::builtin_home(), "unit"),
            "unit"
        );
        assert_eq!(
            localizer.to_string(
                ModuleName {
                    package: None,
                    name: "Local"
                },
                "Int"
            ),
            "Int"
        );
    }
}
