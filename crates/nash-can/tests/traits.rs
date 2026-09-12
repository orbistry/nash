mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use indoc::indoc;

fn parse<'a>(bump: &'a Bump, source: &str) -> &'a nash_source::Module<'a> {
    let source = bump.alloc_str(source);
    bump.alloc(nash_parse::Parser::new(bump, source).module().unwrap())
}

#[test]
fn superclass_and_default_method() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (Same)
        import Builtin exposing (..)

        trait Eq 'a where
            eq : 'a -> 'a -> bool

        trait Eq 'a => Same 'a where
            same : 'a -> 'a -> bool
            same x y = eq x y
    "
        )),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        source,
    )
    .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!((&result.module.traits, &result.module.exports));
    });
}

#[test]
fn superclass_cycle() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait Second 'a => First 'a where
            first : 'a -> 'a

        trait First 'a => Second 'a where
            second : 'a -> 'a
    "
        )),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
        ));
    });
}

#[test]
fn method_requires_each_trait_parameter() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait Convert 'a 'b where
            convert : 'a -> 'a
    "
        )),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
        ));
    });
}

#[test]
fn method_quantifiers_have_independent_kinds() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait Carry 'a where
            ordinary : 'a -> List 'b -> 'a
            higher : 'a -> 'b 'c -> 'a
    "
        )),
    );
    let result = nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap();
    let trait_ = &result.module.traits[0].value;
    let variables: Vec<_> = trait_
        .methods
        .iter()
        .map(|m| (m.name.value, m.annotation.free_vars, m.annotation.context))
        .collect();
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!((&trait_.kinds, variables));
    });
}

#[test]
fn method_predicate_checks_argument_kind() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)
        import Builtin exposing (..)

        trait BigOnly ('a : Big) where
            bigOnly : 'a -> 'a

        trait Uses 'a where
            use : BigOnly int => 'a -> 'a
    "
        )),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(
                &bump,
                nash_can::Context {
                    package: None,
                    interfaces: Some(&interfaces)
                },
                source
            )
            .unwrap_err()
        ));
    });
}

#[test]
fn default_body_checks_nested_annotation_kinds() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)
        import Builtin exposing (..)

        trait Keep 'a where
            keep : 'a -> 'a
            keep x =
                let
                    bad : list ('b -> 'b)
                    bad = []
                in
                x
    "
        )),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(
                &bump,
                nash_can::Context {
                    package: None,
                    interfaces: Some(&interfaces)
                },
                source
            )
            .unwrap_err()
        ));
    });
}

#[test]
fn mutually_referencing_method_contexts_are_not_superclass_cycles() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait First 'a where
            first : Second 'a => 'a -> 'a

        trait Second 'a where
            second : First 'a => 'a -> 'a
    "
        )),
    );
    let result = nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap();
    let schemes: Vec<_> = result
        .module
        .traits
        .iter()
        .map(|t| (t.value.name.value, t.value.kinds))
        .collect();
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!(schemes);
    });
}

#[test]
fn default_parameter_cannot_shadow_local_method() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait Keep 'a where
            keep : 'a -> 'a
            keep keep = keep
    "
        )),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
        ));
    });
}

#[test]
fn imported_trait_methods_retain_context_and_defaults() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let interface = {
        let source_bump = &bump;
        let source = parse(
            source_bump,
            snapshot_inputs.record(indoc!(
                "
            module Identity exposing (Keep)

            trait Keep 'a where
                keep : 'a -> 'a
                keep x = x
        "
            )),
        );
        let can =
            nash_can::canonicalize(source_bump, nash_can::Context::default(), source).unwrap();
        nash_can::from_module(source_bump, &can.module, &Default::default())
    };
    let interfaces = std::collections::BTreeMap::from([("Identity", interface)]);
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)
        import Identity as I exposing (Keep)

        local : Keep 'a => 'a -> 'a
        local x = keep x

        qualified : I.Keep 'a => 'a -> 'a
        qualified x = I.keep x
    "
        )),
    );
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        source,
    )
    .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let mut decls = result.module.decls;
    let mut references = Vec::new();
    while let nash_ast::Decls::Declare { definition, next } = decls {
        let nash_ast::Def::TypedDef { body, context, .. } = definition else {
            panic!("typed definition")
        };
        assert_eq!(context.len(), 1);
        let nash_ast::Expr::Call { function, .. } = &body.value else {
            panic!("method call")
        };
        let nash_ast::Expr::VarMethod { trait_, method, .. } = &function.value else {
            panic!("method reference")
        };
        references.push((*trait_, *method));
        decls = next;
    }
    assert_eq!(references.len(), 2);
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!((interfaces["Identity"].traits, references));
    });
}

fn interface_from_source<'a>(bump: &'a Bump, source: &str) -> nash_can::Interface<'a> {
    let can =
        nash_can::canonicalize(bump, nash_can::Context::default(), parse(bump, source)).unwrap();
    nash_can::from_module(bump, &can.module, &Default::default())
}

#[test]
fn private_trait_metadata_does_not_expose_names() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let interface = interface_from_source(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Identity exposing (Keep)

        trait Hidden ('a : Big) where
            hidden : 'a -> 'a

        trait Hidden 'a => Keep 'a where
            keep : 'a -> 'a
    "
        )),
    );
    assert_eq!(interface.traits.len(), 2);
    let interfaces = std::collections::BTreeMap::from([("Identity", interface)]);
    let context = nash_can::Context {
        package: None,
        interfaces: Some(&interfaces),
    };
    let valid = parse(
        &bump,
        snapshot_inputs.record("module Main exposing (..)\nimport Identity exposing (Keep)\n\ntrait Keep 'a => Child 'a where\n    child : 'a -> 'a\n"),
    );
    let can = nash_can::canonicalize(&bump, context, valid).unwrap();
    let hidden_trait = parse(
        &bump,
        snapshot_inputs.record("module Main exposing (..)\nimport Identity exposing (..)\n\nf : Identity.Hidden 'a => 'a -> 'a\nf x = x\n"),
    );
    let hidden_method = parse(
        &bump,
        snapshot_inputs.record(
            "module Main exposing (..)\nimport Identity exposing (..)\n\nf x = Identity.hidden x\n",
        ),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        assert_eq!(can.module.traits[0].value.kinds, &[&nash_ast::Kind::Type]);
        insta::assert_snapshot!([
            snapshot_inputs.errors_before(1, &nash_can::canonicalize(&bump, context, hidden_trait).unwrap_err()),
            snapshot_inputs.errors(&nash_can::canonicalize(&bump, context, hidden_method).unwrap_err()),
        ].join("\n"));
    });
}

#[test]
fn imported_trait_and_method_ambiguity() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let a = interface_from_source(
        &bump,
        snapshot_inputs
            .record("module A exposing (Keep)\n\ntrait Keep 'a where\n    keep : 'a -> 'a\n"),
    );
    let b = interface_from_source(
        &bump,
        snapshot_inputs
            .record("module B exposing (Keep)\n\ntrait Keep 'a where\n    keep : 'a -> 'a\n"),
    );
    let interfaces = std::collections::BTreeMap::from([("A", a), ("B", b)]);
    let context = nash_can::Context {
        package: None,
        interfaces: Some(&interfaces),
    };
    let trait_source = parse(
        &bump,
        snapshot_inputs.record("module Main exposing (..)\nimport A exposing (Keep)\nimport B exposing (Keep)\n\nf : Keep 'a => 'a -> 'a\nf x = x\n"),
    );
    let method_source = parse(
        &bump,
        snapshot_inputs.record("module Main exposing (..)\nimport A exposing (Keep)\nimport B exposing (Keep)\n\nf x = keep x\n"),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!([
            snapshot_inputs.errors_before(1, &nash_can::canonicalize(&bump, context, trait_source).unwrap_err()),
            snapshot_inputs.errors(&nash_can::canonicalize(&bump, context, method_source).unwrap_err()),
        ].join("\n"));
    });
}

#[test]
fn higher_kinded_method_context() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait Applicative 'f where
            pure : 'a -> 'f 'a

        trait Traversable 't where
            traverse : Applicative 'f => ('a -> 'f 'b) -> 't 'a -> 'f ('t 'b)
    "
        )),
    );
    let can = nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap();
    let schemes: Vec<_> = can
        .module
        .traits
        .iter()
        .map(|t| (t.value.name.value, t.value.kinds))
        .collect();
    let context = can.module.traits[1].value.methods[0].annotation.context;
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!((schemes, context));
    });
}

#[test]
fn methods_share_the_module_value_namespace() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let source = parse(
        &bump,
        snapshot_inputs.record(indoc!(
            "
        module Main exposing (..)

        trait First 'a where
            duplicate : 'a -> 'a

        trait Second 'a where
            duplicate : 'a -> 'a
    "
        )),
    );
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&
            nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
        ));
    });
}
