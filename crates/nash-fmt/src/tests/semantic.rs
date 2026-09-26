//! Structural round-trip oracle, independent of the printer. Only source positions
//! are omitted; strings, docs, comments, and representation annotations remain.
use nash_region::{Located, Region};
use nash_source::*;
trait Semantic {
    fn write(&self, out: &mut String);
}
fn token(out: &mut String, value: impl std::fmt::Debug) {
    out.push_str(&format!("{value:?};"));
}
impl<T: Semantic + ?Sized> Semantic for &T {
    fn write(&self, out: &mut String) {
        (*self).write(out)
    }
}
impl<T: Semantic> Semantic for [T] {
    fn write(&self, out: &mut String) {
        token(out, self.len());
        for v in self {
            v.write(out);
        }
    }
}
impl<T: Semantic> Semantic for Option<T> {
    fn write(&self, out: &mut String) {
        token(out, self.is_some());
        if let Some(v) = self {
            v.write(out)
        }
    }
}
impl<T: Semantic> Semantic for Located<T> {
    fn write(&self, out: &mut String) {
        self.value.write(out)
    }
}
impl<A: Semantic, B: Semantic> Semantic for (A, B) {
    fn write(&self, out: &mut String) {
        self.0.write(out);
        self.1.write(out)
    }
}
impl Semantic for Region {
    fn write(&self, _: &mut String) {}
}
macro_rules! scalar {($($ty:ty),*)=>{$(impl Semantic for $ty {fn write(&self,out:&mut String){token(out,self)}})*};}
scalar!(
    str,
    bool,
    u8,
    i128,
    Repr,
    VarType,
    Expect,
    Budget,
    Associativity,
    Precedence,
    CommentKind
);
macro_rules! structure {
    ($name:ident {$($field:ident),*})=>{impl Semantic for $name<'_>{fn write(&self,out:&mut String){let Self{$($field),*}=self;token(out,stringify!($name));$($field.write(out);)*}}};
}
macro_rules! variants {
    ($name:ident {$($variant:ident $(($payload:ident))? $({$($field:ident),*})?),* $(,)?})=>{
        impl Semantic for $name<'_>{fn write(&self,out:&mut String){match self{$(Self::$variant $(($payload))? $({$($field),*})? =>{token(out,stringify!($variant));$($payload.write(out);)?$($($field.write(out);)*)?}),*}}}
    };
}
structure!(Module {
    kind,
    name,
    exports,
    docs,
    comments,
    imports,
    values,
    unions,
    aliases,
    traits,
    impls,
    tests,
    binops
});
impl Semantic for ModuleKind {
    fn write(&self, out: &mut String) {
        token(out, matches!(self, Self::Validator(_)));
    }
}
structure!(Import {
    import,
    alias,
    exposing
});
structure!(Value {
    docs,
    name,
    arguments,
    body,
    annotation,
    attributes
});
structure!(Attribute { name, args });
structure!(Trait {
    docs,
    name,
    params,
    supers,
    methods,
    attributes
});
structure!(TraitMethod {
    name,
    annotation,
    default
});
structure!(Impl {
    docs,
    context,
    head,
    methods,
    attributes
});
structure!(Tests { imports, tests });
structure!(Test {
    name,
    expect,
    budget,
    body
});
structure!(Block { stmts, last });
variants!(TestBody{Unit(block),Prop{binders,body}});
structure!(ViaBinder { pattern, generator });
structure!(Annotation { constraints, typ });
structure!(Constraint {
    class,
    module,
    args
});
structure!(Union {
    docs,
    name,
    arguments,
    ctors,
    attributes
});
structure!(Ctor { name, arguments });
variants!(CtorArgs{Positional(args),Labeled(fields)});
structure!(Alias {
    docs,
    name,
    arguments,
    typ,
    attributes
});
structure!(Infix {
    op,
    associativity,
    precedence,
    name
});
variants!(Expr{
    Str(value),Bytes(value),Int(value),Assert(value),Fail(value),Todo(value),
    Trace{message,body},Comptime(value),Do{stmts,last},MacroCall{name,module,args},
    LeftSection{left,operator},RightSection{operator,right},Var{kind,name},VarQual{kind,module,name},
    List(items),Op(op),Negate(value),BinOps{operands,last},Lambda{parameters,body},Call{function,arguments},
    If{branches,final_else},Let{defs,body},Case{scrutinee,arms},Accessor(field),Access{record,field},
    Update{record,fields},Record{fields,grouped},Unit,Tuple{first,second,rest}
});
variants!(Stmt{Let(defs),Bind{pattern,expr},Expr(expr)});
structure!(IfBranch {
    condition,
    then_branch
});
structure!(BinOpOperand { expr, op });
variants!(Def{Define{name,args,body,annotation},Destruct{pattern,body}});
structure!(CaseArm { pattern, body });
structure!(FieldAssign { field, value });
variants!(Pattern{Anything,Var(name),Record(fields),Alias{pattern,name},Unit,Pair{first,second},Tuple{first,second,rest},Ctor{region,name,args},CtorQual{region,module,name,args},List(items),Cons{head,tail},Str(value),Bytes(value),Int(value)});
variants!(Type{Repr{typ,repr},Lambda{from,to},Var(name),VarApp{region,name,args},Type{region,name,args},TypeQual{region,module,name,args},Record(fields),Unit,Tuple{first,second,rest}});
structure!(TypeParam { name, repr });
structure!(FieldType { field, typ });
variants!(Docs{NoDocs(region),YesDocs{overview,comments}});
structure!(SourceComment { region, kind, text });
structure!(Comment { region, snippet });
impl Semantic for Snippet<'_> {
    fn write(&self, out: &mut String) {
        let Self {
            data,
            off_row: _,
            off_col: _,
        } = self;
        data.write(out)
    }
}
variants!(Exposing{Open,Explicit(items)});
variants!(Exposed{Lower(name),Upper{name,privacy},LowerType{name,privacy},Operator{region,op}});
impl Semantic for Privacy {
    fn write(&self, out: &mut String) {
        token(out, matches!(self, Self::Public(_)));
    }
}
pub fn tree(source: &str) -> String {
    let arena = bumpalo::Bump::new();
    let module = nash_parse::Parser::new(&arena, source)
        .module()
        .expect("valid module");
    let mut out = String::new();
    module.write(&mut out);
    out
}
