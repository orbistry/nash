use aiken_lang::{
    ast::{Definition, Span, UntypedArg, UntypedModule},
    parser::extra::{Comment, ModuleExtra, comments_before},
};
use std::{iter::Peekable, slice::Iter};

fn take<'a>(
    comments: &mut Peekable<Iter<'a, Span>>,
    position: usize,
    source: &'a str,
) -> Option<String> {
    let docs: Vec<&str> = comments_before(comments, position, source);
    (!docs.is_empty()).then(|| docs.join("\n"))
}

fn arguments<'a>(
    args: &mut [UntypedArg],
    comments: &mut Peekable<Iter<'a, Span>>,
    source: &'a str,
) {
    for arg in args {
        if let Some(doc) = take(comments, arg.location.start, source) {
            arg.doc = Some(doc);
        }
    }
}

pub(crate) fn attach<'a>(module: &mut UntypedModule, extra: &'a ModuleExtra, source: &'a str) {
    module.docs = extra
        .module_comments
        .iter()
        .map(|span| Comment::from((span, source)).content.to_owned())
        .collect();
    let mut comments = extra.doc_comments.iter().peekable();
    let mut definitions: Vec<_> = module.definitions.iter_mut().collect();
    definitions.sort_by_key(|definition| definition.location().start);
    for definition in definitions {
        if let Some(doc) = take(&mut comments, definition.location().start, source) {
            definition.put_doc(doc);
        }
        match definition {
            Definition::Fn(function) => arguments(&mut function.arguments, &mut comments, source),
            Definition::Validator(validator) => {
                arguments(&mut validator.params, &mut comments, source);
                for handler in validator
                    .handlers
                    .iter_mut()
                    .chain(std::iter::once(&mut validator.fallback))
                {
                    if let Some(doc) = take(&mut comments, handler.location.start, source) {
                        handler.doc = Some(doc);
                    }
                    arguments(&mut handler.arguments, &mut comments, source);
                }
            }
            Definition::DataType(data) => {
                for constructor in &mut data.constructors {
                    if let Some(doc) = take(&mut comments, constructor.location.start, source) {
                        constructor.put_doc(doc);
                    }
                    for argument in &mut constructor.arguments {
                        if let Some(doc) = take(&mut comments, argument.location.start, source) {
                            argument.put_doc(doc);
                        }
                    }
                }
            }
            Definition::Test(function) | Definition::Benchmark(function) => {
                for argument in &mut function.arguments {
                    if let Some(doc) = take(&mut comments, argument.arg.location.start, source) {
                        argument.arg.doc = Some(doc);
                    }
                }
            }
            Definition::TypeAlias(_) | Definition::Use(_) | Definition::ModuleConstant(_) => {}
        }
    }
}
