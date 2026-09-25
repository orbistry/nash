//! Implicit scope for application modules. These imports never enter the source AST.
use bumpalo::Bump;
use nash_ast::{ModuleName, primitives};
use nash_region::{Located, Region};
use nash_source::{Exposed, Exposing, Import, Privacy};
use std::collections::BTreeMap;

pub const MODULES: &[(&str, &[&str])] = &[
    ("Primitive", &["*"]),
    ("Builtin", &[]),
    ("Prelude", &["*"]),
    ("Eq", &["Eq"]),
    ("Ord", &["Ord"]),
    ("Show", &["Show"]),
    ("Num", &["Num"]),
    ("Integral", &["Integral"]),
    ("Semigroup", &["Semigroup"]),
    ("Monoid", &["Monoid"]),
    ("Functor", &["Functor"]),
    ("Applicative", &["Applicative"]),
    ("Monad", &["Monad"]),
    ("Lift", &["Lift"]),
    ("Data", &["ToData", "FromData", "Validate", "Decode"]),
    (
        "Literal",
        &["FromInt", "FromString", "FromBytes", "FromBool", "FromUnit"],
    ),
    ("Bool", &["Bool", "not", "and", "or", "xor"]),
    ("Unit", &["Unit"]),
    ("Option", &["Option", "type option"]),
    ("Ordering", &["Ordering", "type ordering"]),
    ("Cons", &["type cons"]),
    ("Int", &[]),
    ("Rational", &[]),
    ("Crypto", &[]),
    ("Bytes", &[]),
    ("String", &[]),
    ("List", &[]),
    ("Map", &[]),
    ("Pair", &[]),
    ("Array", &[]),
    ("Prop", &[]),
    ("Test", &[]),
    ("Cardano.Tx", &[]),
    ("Cardano.Address", &[]),
    ("Cardano.Value", &[]),
    ("Cardano.Time", &[]),
];

pub fn imports<'a>(
    bump: &'a Bump,
    home: ModuleName<'a>,
    interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>,
) -> Vec<&'a Import<'a>> {
    if home.package == Some(primitives::BASE)
        || !interfaces.is_some_and(|all| {
            all.get("Prelude")
                .is_some_and(|i| i.home.package == Some(primitives::BASE))
        })
    {
        return vec![];
    }
    MODULES
        .iter()
        .filter_map(|(module, names)| {
            let interface = interfaces?.get(module)?;
            if interface.home
                != (ModuleName {
                    package: Some(primitives::BASE),
                    name: module,
                })
            {
                return None;
            }
            let exposing = if *names == ["*"] {
                Exposing::Open
            } else {
                Exposing::Explicit(bump.alloc_slice_fill_iter(names.iter().map(|name| {
                    let exposed = if let Some(name) = name.strip_prefix("type ") {
                        Exposed::LowerType {
                            name: bump.alloc(Located::at_zero(name)),
                            privacy: Privacy::Public(Region::zero()),
                        }
                    } else if name.as_bytes()[0].is_ascii_uppercase() {
                        Exposed::Upper {
                            name: bump.alloc(Located::at_zero(*name)),
                            privacy: Privacy::Private,
                        }
                    } else {
                        Exposed::Lower(bump.alloc(Located::at_zero(*name)))
                    };
                    &*bump.alloc(exposed)
                })))
            };
            Some(&*bump.alloc(Import {
                import: bump.alloc(Located::at_zero(*module)),
                alias: None,
                exposing: bump.alloc(exposing),
            }))
        })
        .collect()
}

pub(crate) fn test_imports<'a>(
    bump: &'a Bump,
    home: ModuleName<'a>,
    interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>,
) -> Vec<&'a Import<'a>> {
    if !imports(bump, home, interfaces)
        .iter()
        .any(|i| i.import.value == "Test")
    {
        return vec![];
    }
    vec![bump.alloc(Import {
        import: bump.alloc(Located::at_zero("Test")),
        alias: None,
        exposing: bump.alloc(Exposing::Explicit(bump.alloc_slice_copy(&[
            &*bump.alloc(Exposed::Lower(bump.alloc(Located::at_zero("label")))),
        ]))),
    })]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_imports_leave_the_source_ast_unchanged() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\nvalue = coerce ()\n";
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let mut prelude = crate::kinds::builtin_interface(&bump);
        prelude.home = ModuleName {
            package: Some(primitives::BASE),
            name: "Prelude",
        };
        prelude.values = &[];
        let interfaces = BTreeMap::from([("Prelude", prelude)]);
        assert!(module.imports.is_empty());
        crate::canonicalize(
            &bump,
            crate::Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap();
        assert!(module.imports.is_empty());
    }

    #[test]
    fn builtin_contains_only_real_plutus_functions() {
        let bump = Bump::new();
        let interface = crate::kinds::builtin_interface(&bump);
        assert!(interface.unions.is_empty());
        assert!(interface.traits.is_empty());
        assert!(interface.aliases.is_empty());
        assert_eq!(interface.values.len(), primitives::BUILTINS.len());
        assert!(!interface.values.iter().any(|value| value.name == "coerce"));
    }
}
