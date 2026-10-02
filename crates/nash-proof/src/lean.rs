use crate::{Domain, ProofProgram, ReturnDomain};
use nash_source::Expect;

pub fn lakefile() -> &'static str {
    r#"import Lake
open Lake DSL
package NashProof
require Blaster from git "https://github.com/input-output-hk/Lean-blaster" @ "bafdd4f7976037cd7bd8e443df04af096dc5b96e"
require PlutusCore from git "https://github.com/input-output-hk/PlutusCoreBlaster" @ "41fe7eadf460dc66bef22b85656bc36408638bdc"
require CardanoLedgerApi from git "https://github.com/input-output-hk/CardanoLedgerApiBlaster" @ "3f9f7c6b6ecf0012e1397bab0a814daa36e70ba2"
"#
}

/// A command-level result protocol avoids admissions and human-log parsing.
const SUPPORT: &str = r##"import Blaster
import PlutusCore.UPLC
import CardanoLedgerApi.V1
import CardanoLedgerApi.V2
import CardanoLedgerApi.V3
open PlutusCore.UPLC.Term (Term Const)
open PlutusCore.UPLC.Utils
open CardanoLedgerApi.IsData.Class (toTerm)
set_option maxRecDepth 100000
set_option maxHeartbeats 0

namespace NashProof

open PlutusCore.UPLC.CekMachine
open PlutusCore.UPLC.PlutusScript
open PlutusCore.UPLC.Term (Program)

-- Preserve a running state at fuel zero. Upstream runSteps turns it into Error.
def runSteps (semantics : PlutusCore.Default.BuiltinSemanticsVariant) (state : State) (fuel : Nat) : State :=
  match fuel, state with
  | _, .Halt _ => state
  | _, .Error => state
  | 0, _ => state
  | fuel + 1, _ => runSteps semantics (step semantics state) fuel

def execute (script : PlutusScript) (params : List PlutusCore.UPLC.Term.Term) (fuel : Nat) : State :=
  let semantics : PlutusCore.Default.BuiltinSemanticsVariant := match script.lang with
    | .PlutusV1 | .PlutusV2 => .defaultFunSemanticsVariantD
    | .PlutusV3 => .defaultFunSemanticsVariantE
  match script.script with
  | .Program _ body => runSteps semantics (initialState (applyParams body params)) fuel

-- Rejection includes an unfinished state at the configured execution limit.
def isRejected (state : State) : Prop := ¬ PlutusCore.UPLC.Utils.isSuccessful state

-- A postcondition returns a Boolean; False is a refutation, not an execution error.
def conditionHolds (state : State) : Prop :=
  match state with
  | .Halt (.VCon (.Bool value)) => value = true
  | _ => False

open Lean Elab Command Term Meta

syntax (name := prepare) "#nash_prep" ident ident ident num : command
@[command_elab prepare]
def prepareImpl : CommandElab := fun stx => do
  let declaration ← withoutModifyingEnv <| runTermElabM fun _ => do
    let some script ← resolveId? stx[2] | throwError "unknown proof script"
    let some inputs ← resolveId? stx[3] | throwError "unknown proof input conversion"
    let application ← Meta.lambdaTelescope (← Meta.etaExpand inputs) fun xs _ =>
      mkLambdaFVars xs (mkApp3 (mkConst ``execute) script
        (mkAppN inputs xs) (mkNatLit (TSyntax.getNat ⟨stx[4]⟩)))
    let (expression, _) ← Blaster.Optimize.Optimize.main application |>.run default
    return Declaration.defnDecl {
      name := stx[1].getId, levelParams := [], type := ← inferType expression,
      value := expression, hints := .abbrev, safety := .safe }
  modifyEnv (addNoncomputable · stx[1].getId)
  liftCoreM <| addDecl declaration

syntax (name := verify) "#nash_verify" str num "[" term "]" : command
@[command_elab verify]
def verifyImpl : CommandElab := fun stx => do
  let saved ← get
  let result ← runTermElabM fun _ => do
    let expression ← instantiateMVars (← withSynthesize (postpone := .partial) <| elabTerm stx[4] none)
    let options : Blaster.Options.BlasterOptions := { timeout := some (TSyntax.getNat ⟨stx[2]⟩) }
    let env : Blaster.Optimize.TranslateEnv := { (default : Blaster.Optimize.TranslateEnv) with optEnv.options.solverOptions := options }
    let ((result, _), _) ← Blaster.Smt.Translate.main expression |>.run env
    return result
  -- Blaster logs a mismatched expected result as an elaboration error. The structured
  -- protocol handles all outcomes, so remove only this command's solver messages.
  set saved
  let (status, cex) := match result with
    | .Valid => ("valid", ([] : List String))
    | .Falsified cex => ("falsified", cex)
    | .Undetermined => ("unknown", [])
  let json := Json.mkObj [("query", toJson (TSyntax.getString ⟨stx[1]⟩)), ("status", toJson status), ("counterexample", toJson cex)]
  liftIO <| IO.println ("@@NASH_PROOF@@" ++ json.compress)
end NashProof
"##;

pub fn render(program: &ProofProgram, fuel: u32, postcondition_fuel: u32, timeout: u32) -> String {
    let mut text = SUPPORT.to_owned();
    let version = match program.plutus_version {
        nash_config::PlutusVersion::V1 => 1,
        nash_config::PlutusVersion::V2 => 2,
        nash_config::PlutusVersion::V3 => 3,
    };
    // Embed flat bytes, so moving an exported project cannot break script paths.
    text.push_str(&format!("\ndef script : PlutusCore.UPLC.PlutusScript.PlutusScript :=\n  {{ lang := .PlutusV{version}, script := flatEncodedScriptFromHexM \"{}\" }}\n", hex::encode(&program.flat)));
    let mut binders = String::new();
    let mut args = Vec::new();
    let mut assumptions = Vec::new();
    let mut parameters = Vec::new();
    for (i, domain) in program.domains.iter().enumerate() {
        let name = format!("input{i}");
        let (ty, term, assumption) = domain.input(&name);
        binders.push_str(&format!(" ({name} : {ty})"));
        parameters.push((
            name.clone(),
            ty.clone(),
            term.clone(),
            *domain == Domain::Bool,
        ));
        args.push(name);
        if let Some(assumption) = assumption {
            assumptions.push(assumption);
        }
    }
    prepare(&mut text, "prepared", "script", &parameters, fuel);
    let state = format!("(prepared {})", args.join(" "));
    let quantify = |body: &str| {
        let pre = if assumptions.is_empty() {
            String::new()
        } else {
            format!("{} → ", assumptions.join(" → "))
        };
        if binders.is_empty() {
            format!("{pre}{body}")
        } else {
            format!("∀{binders}, {pre}{body}")
        }
    };
    let claim = if let Some(postcondition) = &program.postcondition {
        let (ty, term, pattern) = postcondition.result.input("returned");
        text.push_str(&format!("\ndef postconditionScript : PlutusCore.UPLC.PlutusScript.PlutusScript :=\n  {{ lang := .PlutusV{version}, script := flatEncodedScriptFromHexM \"{}\" }}\n", hex::encode(&postcondition.flat)));
        let mut post_parameters = parameters.clone();
        post_parameters.push((
            "returned".into(),
            ty,
            term,
            postcondition.result == ReturnDomain::Bool,
        ));
        prepare(
            &mut text,
            "preparedPostcondition",
            "postconditionScript",
            &post_parameters,
            postcondition_fuel,
        );
        let result = if postcondition.result == ReturnDomain::Unit {
            "()"
        } else {
            "returned"
        };
        let check = format!("(preparedPostcondition {} {result})", args.join(" "));
        let complete = format!(
            "match {state} with | .Halt (.VCon ({pattern})) => isSuccessful {check} ∨ isUnsuccessful {check} | _ => True"
        );
        text.push_str(&format!(
            "\n#nash_verify \"postcondition_completion\" {timeout} [{}]\n",
            quantify(&complete)
        ));
        format!(
            "match {state} with | .Halt (.VCon ({pattern})) => NashProof.conditionHolds {check} | .Halt _ => False | _ => True"
        )
    } else {
        match program.expect {
            Expect::Pass => format!("isSuccessful {state}"),
            Expect::Fail => format!("NashProof.isRejected {state}"),
            // Refuting universal success finds a script error or fuel exhaustion.
            Expect::FailOnce => format!("isSuccessful {state}"),
        }
    };
    text.push_str(&format!(
        "\n#nash_verify \"property\" {timeout} [{}]\n",
        quantify(&claim)
    ));
    text
}

/// Specializing up to two Boolean parameters avoids carrying their Data-encoding
/// branches through every CEK step. Both values remain covered by the obligation.
/// Other parameters stay symbolic; the cap keeps export size linear in their count.
fn prepare(
    text: &mut String,
    name: &str,
    script: &str,
    parameters: &[(String, String, String, bool)],
    fuel: u32,
) {
    let split: Vec<_> = parameters
        .iter()
        .enumerate()
        .filter(|(_, p)| p.3)
        .take(2)
        .map(|(i, _)| i)
        .collect();
    let binders = |indices: &[usize]| {
        indices
            .iter()
            .map(|&i| format!(" ({} : {})", parameters[i].0, parameters[i].1))
            .collect::<String>()
    };
    let all: Vec<_> = (0..parameters.len()).collect();
    let remaining: Vec<_> = all.iter().copied().filter(|i| !split.contains(i)).collect();
    let arguments = remaining
        .iter()
        .map(|&i| parameters[i].0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    for case in 0..(1 << split.len()) {
        let suffix = if split.is_empty() {
            String::new()
        } else {
            format!("Case{case}")
        };
        let terms = parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if let Some(bit) = split.iter().position(|&j| j == i) {
                    format!("Term.Const (Const.Bool {})", case & (1 << bit) != 0)
                } else {
                    p.2.clone()
                }
            })
            .collect::<Vec<_>>();
        text.push_str(&format!(
            "\ndef {name}Inputs{suffix}{} : List Term := [{}]\n",
            binders(&remaining),
            terms.join(", ")
        ));
        text.push_str(&format!(
            "\n#nash_prep {name}{suffix} {script} {name}Inputs{suffix} {fuel}\n"
        ));
    }
    if !split.is_empty() {
        fn branches(
            parameters: &[(String, String, String, bool)],
            split: &[usize],
            depth: usize,
            case: usize,
            name: &str,
            arguments: &str,
        ) -> String {
            if depth == split.len() {
                return format!("({name}Case{case} {arguments})");
            }
            let yes = branches(
                parameters,
                split,
                depth + 1,
                case | (1 << depth),
                name,
                arguments,
            );
            let no = branches(parameters, split, depth + 1, case, name, arguments);
            format!("(if {} then {yes} else {no})", parameters[split[depth]].0)
        }
        text.push_str(&format!(
            "\nnoncomputable def {name}{} : PlutusCore.UPLC.CekMachine.State :=\n  {}\n",
            binders(&all),
            branches(parameters, &split, 0, 0, name, &arguments)
        ));
    }
}

impl Domain {
    fn input(self, name: &str) -> (String, String, Option<String>) {
        let (ty, term) = match self {
            Self::Int => (
                "PlutusCore.Integer.Integer",
                format!("Term.Const (Const.Integer {name})"),
            ),
            Self::Integer => ("PlutusCore.Integer.Integer", format!("toTerm {name}")),
            Self::Bool => ("Bool", format!("Term.Const (Const.Bool {name})")),
            Self::Bytes => (
                "PlutusCore.ByteString.ByteString",
                format!("Term.Const (Const.ByteString {name})"),
            ),
            Self::ByteString => ("PlutusCore.ByteString.ByteString", format!("toTerm {name}")),
            Self::String => ("String", format!("Term.Const (Const.String {name})")),
            Self::Data => ("PlutusCore.Data.Data", format!("toTerm {name}")),
            Self::Spending(v) | Self::Minting(v) => {
                let purpose = if matches!(self, Self::Spending(_)) {
                    "Spending"
                } else {
                    "Minting"
                };
                let ty = if v == 3 {
                    format!("CardanoLedgerApi.V{v}.ScriptContext")
                } else {
                    format!("CardanoLedgerApi.V{v}.{purpose}Input")
                };
                let term = if v == 3 {
                    format!("toTerm {name}")
                } else {
                    format!("toTerm {name}.ctx")
                };
                return (
                    ty,
                    term,
                    Some(format!(
                        "CardanoLedgerApi.V{v}.valid{purpose}Context {name}"
                    )),
                );
            }
        };
        (ty.to_owned(), term, None)
    }
}

impl ReturnDomain {
    fn input(self, name: &str) -> (String, String, String) {
        let domain = match self {
            Self::Int => Domain::Int,
            Self::Bool => Domain::Bool,
            Self::Bytes => Domain::Bytes,
            Self::String => Domain::String,
            Self::Data => Domain::Data,
            Self::Unit => {
                return (
                    "Unit".into(),
                    "Term.Const Const.Unit".into(),
                    "Const.Unit".into(),
                );
            }
        };
        let (ty, term, _) = domain.input(name);
        let constructor = match self {
            Self::Int => "Integer",
            Self::Bool => "Bool",
            Self::Bytes => "ByteString",
            Self::String => "String",
            Self::Data => "Data",
            Self::Unit => unreachable!(),
        };
        (ty, term, format!("Const.{constructor} {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ledger_input_retains_validity_and_bytecode() {
        let program = ProofProgram {
            module: "M".into(),
            name: "p".into(),
            expect: Expect::Pass,
            domains: vec![Domain::Spending(3)],
            flat: vec![1, 2, 3],
            postcondition: None,
            plutus_version: nash_config::PlutusVersion::V3,
        };
        let source = render(&program, 1000, 1000, 5);
        assert!(source.contains("flatEncodedScriptFromHexM \"010203\""));
        assert!(source.contains("∀ (input0 : CardanoLedgerApi.V3.ScriptContext), CardanoLedgerApi.V3.validSpendingContext input0 →"));
        assert!(source.contains("isSuccessful (prepared input0)"));
        assert!(!source.contains("by blaster"));
    }
}
