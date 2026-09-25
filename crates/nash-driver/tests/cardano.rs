mod support;

use nash_plutus::{arena::Arena, data::PlutusData, syn, term::Term};
use std::path::Path;

#[tokio::test]
async fn ledger_contexts() {
    let source = include_str!("fixtures/cardano/Context.nash");
    let output = support::compile_validator(source).await;
    let mut results = String::new();
    for index in 0..7 {
        let arena = Arena::new();
        let file = format!("context-{index}.cbor");
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cardano/golden")
                .join(&file),
        )
        .unwrap();
        let data = PlutusData::from_cbor(&arena, &bytes).unwrap();
        let program = syn::parse_program(&arena, &output.uplc).unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(
                    &arena,
                    PlutusData::constr(
                        &arena,
                        0,
                        arena.alloc_slice_copy(&[data, PlutusData::byte_string(&arena, &bytes)]),
                    ),
                ),
            )
            .eval(&arena);
        assert!(result.term.is_ok(), "{file}: {:?}", result.term);
        results.push_str(&format!("{file}: {:?}\n", result.term));
    }
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(results);
    });
}

#[tokio::test]
async fn ledger_interval_semantics() {
    let source = include_str!("fixtures/cardano/Intervals.nash");
    let output = support::compile_validator(source).await;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cardano/golden");
    let arena = Arena::new();
    let range_bytes = std::fs::read(fixtures.join("intervals.cbor")).unwrap();
    let result_bytes = std::fs::read(fixtures.join("interval-results.cbor")).unwrap();
    let ranges_data = PlutusData::from_cbor(&arena, &range_bytes).unwrap();
    let PlutusData::List(ranges) = ranges_data else {
        panic!("Haskell interval list")
    };
    let PlutusData::Constr { fields, .. } = PlutusData::from_cbor(&arena, &result_bytes).unwrap()
    else {
        panic!("Haskell result tuple")
    };
    let [
        PlutusData::List(empty),
        PlutusData::List(members),
        PlutusData::List(contains),
    ] = *fields
    else {
        panic!("Haskell results")
    };
    let mut results = String::new();
    for (index, range) in ranges.iter().enumerate() {
        let evaluation_arena = Arena::new();
        let data = PlutusData::constr(
            &evaluation_arena,
            0,
            evaluation_arena.alloc_slice_copy(&[
                range,
                empty[index],
                PlutusData::list(&evaluation_arena, &members[index * 5..(index + 1) * 5]),
                ranges_data,
                PlutusData::list(
                    &evaluation_arena,
                    &contains[index * ranges.len()..(index + 1) * ranges.len()],
                ),
            ]),
        );
        let program = syn::parse_program(&evaluation_arena, &output.uplc).unwrap();
        let result = program
            .apply(&evaluation_arena, Term::data(&evaluation_arena, data))
            .eval(&evaluation_arena);
        assert!(result.term.is_ok(), "interval {index}: {:?}", result.term);
        results.push_str(&format!("{index}: {range:?} => {:?}\n", result.term));
    }
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(results);
    });
}

#[tokio::test]
async fn malformed_contexts() {
    let source = include_str!("fixtures/cardano/Validate.nash");
    let output = support::compile_validator(source).await;
    let decode_source = include_str!("fixtures/cardano/Decode.nash");
    let decoder = support::compile_validator(decode_source).await;
    let arena = Arena::new();
    let bytes = include_bytes!("fixtures/cardano/golden/context-0.cbor");
    let valid = PlutusData::from_cbor(&arena, bytes).unwrap();
    let PlutusData::Constr { fields, .. } = valid else {
        unreachable!()
    };
    let PlutusData::Constr {
        fields: tx_fields, ..
    } = fields[0]
    else {
        unreachable!()
    };
    let mut cases = vec![
        ("valid context".to_string(), valid, true),
        (
            "wrong context tag".into(),
            PlutusData::constr(&arena, 1, fields),
            false,
        ),
        (
            "missing context field".into(),
            PlutusData::constr(&arena, 0, &fields[..2]),
            false,
        ),
        (
            "extra context field".into(),
            PlutusData::constr(
                &arena,
                0,
                arena.alloc_slice_copy(&[fields[0], fields[1], fields[2], fields[2]]),
            ),
            false,
        ),
    ];
    for (index, name) in [
        "inputs",
        "reference inputs",
        "outputs",
        "fee",
        "mint",
        "certificates",
        "withdrawals",
        "validity range",
        "signatories",
        "redeemers",
        "datums",
        "transaction id",
        "votes",
        "proposals",
        "current treasury",
        "treasury donation",
    ]
    .iter()
    .enumerate()
    {
        let mut bad_tx = tx_fields.to_vec();
        bad_tx[index] = if matches!(bad_tx[index], PlutusData::Integer(_)) {
            PlutusData::byte_string(&arena, b"wrong")
        } else {
            PlutusData::integer_from(&arena, 0)
        };
        let mut bad_context = fields.to_vec();
        bad_context[0] = PlutusData::constr(&arena, 0, arena.alloc_slice_copy(&bad_tx));
        cases.push((
            format!("wrong {name} shape"),
            PlutusData::constr(&arena, 0, arena.alloc_slice_copy(&bad_context)),
            false,
        ));
    }
    let mut results = String::new();
    for (name, data, expected) in cases {
        let evaluation_arena = Arena::new();
        let program = syn::parse_program(&evaluation_arena, &output.uplc).unwrap();
        let result = program
            .apply(&evaluation_arena, Term::data(&evaluation_arena, data))
            .eval(&evaluation_arena);
        assert_eq!(result.term.is_ok(), expected, "{name}: {:?}", result.term);
        let input = PlutusData::constr(
            &evaluation_arena,
            0,
            evaluation_arena.alloc_slice_copy(&[
                PlutusData::integer_from(&evaluation_arena, i128::from(expected)),
                data,
            ]),
        );
        let decode_result = syn::parse_program(&evaluation_arena, &decoder.uplc)
            .unwrap()
            .apply(&evaluation_arena, Term::data(&evaluation_arena, input))
            .eval(&evaluation_arena);
        assert!(
            decode_result.term.is_ok(),
            "decode {name}: {:?}",
            decode_result.term
        );
        results.push_str(&format!(
            "{name}: validate {:?}; decode matches expected: {:?}\n",
            result.term, decode_result.term
        ));
    }
    insta::with_settings!({description => format!("{source}\n{decode_source}"), omit_expression => true}, {
        insta::assert_snapshot!(results);
    });
}

#[tokio::test]
async fn ledger_value_operations() {
    let source = include_str!("fixtures/cardano/Values.nash");
    let output = support::compile_validator(source).await;
    let arena = Arena::new();
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cardano/golden/values.cbor"),
    )
    .unwrap();
    let data = PlutusData::from_cbor(&arena, &bytes).unwrap();
    let program = syn::parse_program(&arena, &output.uplc).unwrap();
    let result = program.apply(&arena, Term::data(&arena, data)).eval(&arena);
    assert!(result.term.is_ok(), "{:?}", result.term);
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(format!("{:?}", result.term));
    });
}
