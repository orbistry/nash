//! Textual UPLC. Closed terms round-trip through `syn` (except ML results,
//! which have no UPLC literal). Free De Bruijn indices print diagnostic names:
//! the text parser cannot preserve an out-of-scope numeric index.
use crate::{
    arena::Arena,
    binder::{DeBruijn, Eval, Name, NamedDeBruijn},
    bls::Compressable,
    builtin::DefaultFunction,
    constant::Constant,
    data::PlutusData,
    program::Program,
    term::Term,
    typ::Type,
};
use std::fmt::Write;

pub trait PrettyVar {
    fn write(&self, output: &mut String);
    fn unique(&self) -> Option<usize> {
        None
    }
    fn index(&self) -> Option<usize> {
        None
    }
}
impl PrettyVar for Name<'_> {
    fn write(&self, output: &mut String) {
        let text = self.text();
        if text.starts_with(|c: char| c.is_ascii_alphabetic())
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-'".contains(c))
        {
            write!(output, "{text}_{}", self.unique()).unwrap();
        } else {
            write!(output, "n{}_{}", hex::encode(text), self.unique()).unwrap();
        }
    }
    fn unique(&self) -> Option<usize> {
        Some(Name::unique(self))
    }
}
impl PrettyVar for DeBruijn {
    fn write(&self, output: &mut String) {
        write!(output, "i{}", Eval::index(self)).unwrap();
    }
    fn index(&self) -> Option<usize> {
        Some(Eval::index(self))
    }
}
impl PrettyVar for NamedDeBruijn<'_> {
    fn write(&self, output: &mut String) {
        write!(output, "i{}", Eval::index(self)).unwrap();
    }
    fn index(&self) -> Option<usize> {
        Some(Eval::index(self))
    }
}

pub fn program<V: PrettyVar>(program: &Program<'_, V>) -> String {
    form(
        &format!(
            "program {}.{}.{}",
            program.version.major(),
            program.version.minor(),
            program.version.patch()
        ),
        vec![term(program.term)],
        false,
    )
}
pub fn term<V: PrettyVar>(term: &Term<'_, V>) -> String {
    enum Frame<'a, 't, V> {
        Visit(&'t Term<'a, V>),
        Form {
            head: String,
            start: usize,
            apply: bool,
        },
        EndLambda {
            name: String,
            start: usize,
        },
    }
    let mut pending = vec![Frame::Visit(term)];
    let mut scope: Vec<(Option<usize>, String)> = Vec::new();
    let mut rendered = Vec::new();
    while let Some(frame) = pending.pop() {
        match frame {
            Frame::Form { head, start, apply } => {
                let children = rendered.split_off(start);
                rendered.push(form(&head, children, apply));
            }
            Frame::EndLambda { name, start } => {
                scope.pop();
                let children = rendered.split_off(start);
                rendered.push(form(&format!("lam {name}"), children, false));
            }
            Frame::Visit(term) => match term {
                Term::Var(v) => {
                    let name = if let Some(index) = v.index() {
                        if index > 0 && index <= scope.len() {
                            scope[scope.len() - index].1.clone()
                        } else {
                            format!("free_i{index}")
                        }
                    } else if let Some(unique) = v.unique()
                        && let Some((_, label)) =
                            scope.iter().rev().find(|(id, _)| *id == Some(unique))
                    {
                        label.clone()
                    } else {
                        let mut name = String::new();
                        v.write(&mut name);
                        name
                    };
                    rendered.push(name);
                }
                Term::Lambda { parameter, body } => {
                    let mut name = String::new();
                    if parameter.index().is_some() {
                        write!(name, "i{}", scope.len()).unwrap();
                    } else {
                        parameter.write(&mut name);
                    }
                    scope.push((parameter.unique(), name.clone()));
                    pending.push(Frame::EndLambda {
                        name,
                        start: rendered.len(),
                    });
                    pending.push(Frame::Visit(body));
                }
                Term::Apply { function, argument } => {
                    pending.push(Frame::Form {
                        head: String::new(),
                        start: rendered.len(),
                        apply: true,
                    });
                    pending.push(Frame::Visit(argument));
                    pending.push(Frame::Visit(function));
                }
                Term::Delay(t) | Term::Force(t) => {
                    let head = if matches!(term, Term::Delay(_)) {
                        "delay"
                    } else {
                        "force"
                    };
                    pending.push(Frame::Form {
                        head: head.into(),
                        start: rendered.len(),
                        apply: false,
                    });
                    pending.push(Frame::Visit(t));
                }
                Term::Constant(c) => rendered.push(constant(c)),
                Term::Builtin(f) => rendered.push(format!("(builtin {})", builtin(**f))),
                Term::Error => rendered.push("(error)".into()),
                Term::Constr { tag, fields } => {
                    pending.push(Frame::Form {
                        head: format!("constr {tag}"),
                        start: rendered.len(),
                        apply: false,
                    });
                    pending.extend(fields.iter().rev().map(|t| Frame::Visit(t)));
                }
                Term::Case { constr, branches } => {
                    pending.push(Frame::Form {
                        head: "case".into(),
                        start: rendered.len(),
                        apply: false,
                    });
                    pending.extend(branches.iter().rev().map(|t| Frame::Visit(t)));
                    pending.push(Frame::Visit(constr));
                }
            },
        }
    }
    rendered.pop().unwrap()
}
fn form(head: &str, children: Vec<String>, apply: bool) -> String {
    let (open, close) = if apply { ('[', ']') } else { ('(', ')') };
    let mut parts = Vec::with_capacity(children.len() + 1);
    if !head.is_empty() {
        parts.push(head.to_owned());
    }
    parts.extend(children);
    let joined = parts.join(" ");
    if joined.len() <= 78 && !joined.contains('\n') {
        return format!("{open}{joined}{close}");
    }
    let mut output = format!("{open}{}", parts.first().map(String::as_str).unwrap_or(""));
    for part in parts.iter().skip(1) {
        output.push_str("\n  ");
        output.push_str(&part.replace('\n', "\n  "));
    }
    output.push(close);
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("BLS Miller-loop results have no UPLC text literal")]
pub struct UnsupportedConstant;

/// Print a constant for diagnostics; unsupported ML results get an explicit marker.
/// Use `try_constant` when the output must be parseable UPLC.
pub fn constant(c: &Constant<'_>) -> String {
    try_constant(c).unwrap_or_else(|e| format!("<unsupported constant: {e}>"))
}
pub fn try_constant(c: &Constant<'_>) -> Result<String, UnsupportedConstant> {
    ensure_type(c)?;
    Ok(format!("(con {} {})", constant_type(c), value(c)?))
}
fn constant_type(c: &Constant<'_>) -> String {
    match c {
        Constant::Integer(_) => "integer".into(),
        Constant::ByteString(_) => "bytestring".into(),
        Constant::String(_) => "string".into(),
        Constant::Boolean(_) => "bool".into(),
        Constant::Unit => "unit".into(),
        Constant::Data(_) => "data".into(),
        Constant::ProtoList(t, _) => format!("(list {})", typ(t)),
        Constant::ProtoArray(t, _) => format!("(array {})", typ(t)),
        Constant::ProtoPair(a, b, _, _) => format!("(pair {} {})", typ(a), typ(b)),
        Constant::Bls12_381G1Element(_) => "bls12_381_G1_element".into(),
        Constant::Bls12_381G2Element(_) => "bls12_381_G2_element".into(),
        Constant::Bls12_381MlResult(_) => "bls12_381_mlresult".into(),
        Constant::Value(_) => "value".into(),
    }
}
fn typ(t: &Type<'_>) -> String {
    match t {
        Type::Integer => "integer".into(),
        Type::Bool => "bool".into(),
        Type::String => "string".into(),
        Type::ByteString => "bytestring".into(),
        Type::Unit => "unit".into(),
        Type::Data => "data".into(),
        Type::Value => "value".into(),
        Type::List(t) => format!("(list {})", typ(t)),
        Type::Array(t) => format!("(array {})", typ(t)),
        Type::Pair(a, b) => format!("(pair {} {})", typ(a), typ(b)),
        Type::Bls12_381G1Element => "bls12_381_G1_element".into(),
        Type::Bls12_381G2Element => "bls12_381_G2_element".into(),
        Type::Bls12_381MlResult => "bls12_381_mlresult".into(),
    }
}
fn value(c: &Constant<'_>) -> Result<String, UnsupportedConstant> {
    Ok(match c {
        Constant::Integer(i) => i.to_string(),
        Constant::ByteString(b) => format!("#{}", hex::encode(b)),
        Constant::String(s) => quote(s),
        Constant::Boolean(b) => if *b { "True" } else { "False" }.into(),
        Constant::Unit => "()".into(),
        Constant::Data(d) => format!("({})", data(d)),
        Constant::ProtoList(_, xs) | Constant::ProtoArray(_, xs) => format!(
            "[{}]",
            xs.iter()
                .map(|c| value(c))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        Constant::ProtoPair(_, _, a, b) => format!("({}, {})", value(a)?, value(b)?),
        Constant::Bls12_381G1Element(g) => format!("0x{}", hex::encode(g.compress(&Arena::new()))),
        Constant::Bls12_381G2Element(g) => format!("0x{}", hex::encode(g.compress(&Arena::new()))),
        Constant::Bls12_381MlResult(_) => return Err(UnsupportedConstant),
        Constant::Value(v) => format!(
            "[{}]",
            v.entries
                .iter()
                .map(|e| format!(
                    "(#{}, [{}])",
                    hex::encode(e.currency),
                    e.tokens
                        .iter()
                        .map(|t| format!("(#{}, {})", hex::encode(t.name), t.quantity))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    let mut numeric_escape = false;
    for c in s.chars() {
        if c.is_control() || (numeric_escape && c.is_ascii_digit()) {
            match c {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => write!(out, "\\{}", c as u32).unwrap(),
            }
            numeric_escape = !matches!(c, '\n' | '\r' | '\t');
        } else {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
            numeric_escape = false;
        }
    }
    out.push('"');
    out
}
fn ensure_type(c: &Constant<'_>) -> Result<(), UnsupportedConstant> {
    fn check(t: &Type<'_>) -> Result<(), UnsupportedConstant> {
        match t {
            Type::Bls12_381MlResult => Err(UnsupportedConstant),
            Type::List(t) | Type::Array(t) => check(t),
            Type::Pair(a, b) => {
                check(a)?;
                check(b)
            }
            _ => Ok(()),
        }
    }
    match c {
        Constant::ProtoList(t, _) | Constant::ProtoArray(t, _) => check(t),
        Constant::ProtoPair(a, b, _, _) => {
            check(a)?;
            check(b)
        }
        _ => Ok(()),
    }
}
pub fn data(d: &PlutusData<'_>) -> String {
    match d {
        PlutusData::Integer(i) => format!("I {i}"),
        PlutusData::ByteString(b) => format!("B #{}", hex::encode(b)),
        PlutusData::Constr { tag, fields } => format!(
            "Constr {tag} [{}]",
            fields
                .iter()
                .map(|d| data(d))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PlutusData::List(xs) => format!(
            "List [{}]",
            xs.iter().map(|d| data(d)).collect::<Vec<_>>().join(", ")
        ),
        PlutusData::Map(xs) => format!(
            "Map [{}]",
            xs.iter()
                .map(|(k, v)| format!("({}, {})", data(k), data(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Canonical builtin spelling accepted by the UPLC parser.
pub fn builtin(function: DefaultFunction) -> &'static str {
    match function {
        DefaultFunction::AddInteger => "addInteger",
        DefaultFunction::SubtractInteger => "subtractInteger",
        DefaultFunction::EqualsInteger => "equalsInteger",
        DefaultFunction::LessThanEqualsInteger => "lessThanEqualsInteger",
        DefaultFunction::MultiplyInteger => "multiplyInteger",
        DefaultFunction::DivideInteger => "divideInteger",
        DefaultFunction::QuotientInteger => "quotientInteger",
        DefaultFunction::RemainderInteger => "remainderInteger",
        DefaultFunction::ModInteger => "modInteger",
        DefaultFunction::LessThanInteger => "lessThanInteger",
        DefaultFunction::IfThenElse => "ifThenElse",
        DefaultFunction::AppendByteString => "appendByteString",
        DefaultFunction::EqualsByteString => "equalsByteString",
        DefaultFunction::ConsByteString => "consByteString",
        DefaultFunction::SliceByteString => "sliceByteString",
        DefaultFunction::LengthOfByteString => "lengthOfByteString",
        DefaultFunction::IndexByteString => "indexByteString",
        DefaultFunction::LessThanByteString => "lessThanByteString",
        DefaultFunction::LessThanEqualsByteString => "lessThanEqualsByteString",
        DefaultFunction::Sha2_256 => "sha2_256",
        DefaultFunction::Sha3_256 => "sha3_256",
        DefaultFunction::Blake2b_256 => "blake2b_256",
        DefaultFunction::Keccak_256 => "keccak_256",
        DefaultFunction::Blake2b_224 => "blake2b_224",
        DefaultFunction::VerifyEd25519Signature => "verifyEd25519Signature",
        DefaultFunction::VerifyEcdsaSecp256k1Signature => "verifyEcdsaSecp256k1Signature",
        DefaultFunction::VerifySchnorrSecp256k1Signature => "verifySchnorrSecp256k1Signature",
        DefaultFunction::AppendString => "appendString",
        DefaultFunction::EqualsString => "equalsString",
        DefaultFunction::EncodeUtf8 => "encodeUtf8",
        DefaultFunction::DecodeUtf8 => "decodeUtf8",
        DefaultFunction::ChooseUnit => "chooseUnit",
        DefaultFunction::Trace => "trace",
        DefaultFunction::FstPair => "fstPair",
        DefaultFunction::SndPair => "sndPair",
        DefaultFunction::ChooseList => "chooseList",
        DefaultFunction::MkCons => "mkCons",
        DefaultFunction::HeadList => "headList",
        DefaultFunction::TailList => "tailList",
        DefaultFunction::NullList => "nullList",
        DefaultFunction::ChooseData => "chooseData",
        DefaultFunction::ConstrData => "constrData",
        DefaultFunction::MapData => "mapData",
        DefaultFunction::ListData => "listData",
        DefaultFunction::IData => "iData",
        DefaultFunction::BData => "bData",
        DefaultFunction::UnConstrData => "unConstrData",
        DefaultFunction::UnMapData => "unMapData",
        DefaultFunction::UnListData => "unListData",
        DefaultFunction::UnIData => "unIData",
        DefaultFunction::UnBData => "unBData",
        DefaultFunction::EqualsData => "equalsData",
        DefaultFunction::MkPairData => "mkPairData",
        DefaultFunction::MkNilData => "mkNilData",
        DefaultFunction::MkNilPairData => "mkNilPairData",
        DefaultFunction::SerialiseData => "serialiseData",
        DefaultFunction::Bls12_381_G1_Add => "bls12_381_G1_add",
        DefaultFunction::Bls12_381_G1_Neg => "bls12_381_G1_neg",
        DefaultFunction::Bls12_381_G1_ScalarMul => "bls12_381_G1_scalarMul",
        DefaultFunction::Bls12_381_G1_Equal => "bls12_381_G1_equal",
        DefaultFunction::Bls12_381_G1_Compress => "bls12_381_G1_compress",
        DefaultFunction::Bls12_381_G1_Uncompress => "bls12_381_G1_uncompress",
        DefaultFunction::Bls12_381_G1_HashToGroup => "bls12_381_G1_hashToGroup",
        DefaultFunction::Bls12_381_G2_Add => "bls12_381_G2_add",
        DefaultFunction::Bls12_381_G2_Neg => "bls12_381_G2_neg",
        DefaultFunction::Bls12_381_G2_ScalarMul => "bls12_381_G2_scalarMul",
        DefaultFunction::Bls12_381_G2_Equal => "bls12_381_G2_equal",
        DefaultFunction::Bls12_381_G2_Compress => "bls12_381_G2_compress",
        DefaultFunction::Bls12_381_G2_Uncompress => "bls12_381_G2_uncompress",
        DefaultFunction::Bls12_381_G2_HashToGroup => "bls12_381_G2_hashToGroup",
        DefaultFunction::Bls12_381_MillerLoop => "bls12_381_millerLoop",
        DefaultFunction::Bls12_381_MulMlResult => "bls12_381_mulMlResult",
        DefaultFunction::Bls12_381_FinalVerify => "bls12_381_finalVerify",
        DefaultFunction::IntegerToByteString => "integerToByteString",
        DefaultFunction::ByteStringToInteger => "byteStringToInteger",
        DefaultFunction::AndByteString => "andByteString",
        DefaultFunction::OrByteString => "orByteString",
        DefaultFunction::XorByteString => "xorByteString",
        DefaultFunction::ComplementByteString => "complementByteString",
        DefaultFunction::ReadBit => "readBit",
        DefaultFunction::WriteBits => "writeBits",
        DefaultFunction::ReplicateByte => "replicateByte",
        DefaultFunction::ShiftByteString => "shiftByteString",
        DefaultFunction::RotateByteString => "rotateByteString",
        DefaultFunction::CountSetBits => "countSetBits",
        DefaultFunction::FindFirstSetBit => "findFirstSetBit",
        DefaultFunction::Ripemd_160 => "ripemd_160",
        DefaultFunction::ExpModInteger => "expModInteger",
        DefaultFunction::DropList => "dropList",
        DefaultFunction::LengthOfArray => "lengthOfArray",
        DefaultFunction::ListToArray => "listToArray",
        DefaultFunction::IndexArray => "indexArray",
        DefaultFunction::Bls12_381_G1_MultiScalarMul => "bls12_381_G1_multiScalarMul",
        DefaultFunction::Bls12_381_G2_MultiScalarMul => "bls12_381_G2_multiScalarMul",
        DefaultFunction::InsertCoin => "insertCoin",
        DefaultFunction::LookupCoin => "lookupCoin",
        DefaultFunction::UnionValue => "unionValue",
        DefaultFunction::ValueContains => "valueContains",
        DefaultFunction::ValueData => "valueData",
        DefaultFunction::UnValueData => "unValueData",
        DefaultFunction::ScaleValue => "scaleValue",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_terms_print_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let arena = Arena::new();
                let depth = 1024;
                let mut root = Term::<DeBruijn>::error(&arena);
                for _ in 0..depth {
                    root = root.delay(&arena);
                }
                let printed = term(root);
                // Check the traversal preserves every node without parsing the
                // deep output through another recursive consumer.
                assert_eq!(printed.matches("(delay").count(), depth);
                assert_eq!(printed.matches("(error)").count(), 1);
                assert_eq!(printed.matches(')').count(), depth + 1);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
