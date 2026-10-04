use nash_ir::build::Builder;
use nash_plutus::arena::Arena;
#[path = "../../tests/support/constant_fold.rs"]
mod inputs;

#[test]
fn constant_evaluation() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    for (name, before, fails) in inputs::cases(&b) {
        let fixture = crate::harness::prepare_fixture(&arena, before);
        assert_eq!(fixture.evaluated.result.starts_with("error:"), fails);
        insta::assert_snapshot!(name, fixture.snapshot());
        fixture.assert_equivalent(&arena);
    }
}
