//! Formats surface syntax without loading dependencies or expanding macros.
mod declarations;
mod doc;
mod expr;
mod printer;
#[cfg(test)]
mod tests;
mod types;

/// Format one complete module at 80 columns with four-space indentation.
/// Parse errors use Nash's normal diagnostic model.
pub fn format(source: &str) -> Result<String, Box<nash_report::Report>> {
    let arena = bumpalo::Bump::new();
    let mut parser = nash_parse::Parser::new(&arena, source);
    let module = parser.module().map_err(|error| {
        Box::new(nash_report::syntax::to_report(
            &nash_report::Source::new(source),
            &nash_parse::error::Error::ParseError(&error),
        ))
    })?;
    Ok(printer::Printer::new(source, &module)
        .module(&module)
        .render(80))
}
