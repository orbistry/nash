use bumpalo::Bump;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Highlight fragments with Nash's literal parsers and keyword/operator catalog.
/// Unfinished examples are still printable; unrecognized characters stay escaped.
pub fn highlight(source: &str) -> String {
    let arena = Bump::new();
    let mut rest = source;
    let mut output = String::new();
    while !rest.is_empty() {
        let first = rest.chars().next().unwrap();
        let (len, class) = if rest.starts_with("--") {
            (rest.find('\n').unwrap_or(rest.len()), "comment")
        } else if rest.starts_with("{-") {
            let mut depth = 1;
            let mut end = 2;
            while end < rest.len() && depth != 0 {
                if rest[end..].starts_with("{-") {
                    depth += 1;
                    end += 2;
                } else if rest[end..].starts_with("-}") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += rest[end..].chars().next().unwrap().len_utf8();
                }
            }
            (end, "comment")
        } else if rest.starts_with('#') && rest.as_bytes().get(1) == Some(&b'"')
            || first == '"'
            || first.is_ascii_digit()
        {
            let mut parser = nash_parse::Parser::new(&arena, rest);
            let class = if rest.starts_with("#\"") {
                let _ = parser.bytes_literal(|_, _| (), |_, _, _| ());
                "string"
            } else if first == '"' {
                let _ = parser.string_literal(|_, _| (), |_, _, _| ());
                "string"
            } else {
                let _ = parser.number_literal(|_, _| (), |_, _, _| ());
                "number"
            };
            (
                (rest.len() - parser.remaining().len()).max(first.len_utf8()),
                class,
            )
        } else if first.is_alphabetic() || first == '_' || first == '\'' {
            let len = rest
                .char_indices()
                .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '\'')
                .last()
                .map_or(first.len_utf8(), |(i, c)| i + c.len_utf8());
            let word = &rest[..len];
            let class = if nash_parse::keyword::is_reserved(word) {
                "keyword"
            } else if first.is_uppercase() {
                "type"
            } else {
                ""
            };
            (len, class)
        } else if first.is_ascii() && nash_parse::symbol::is_binop_char(first as u8) {
            let len = rest
                .bytes()
                .take_while(|b| nash_parse::symbol::is_binop_char(*b))
                .count();
            (len, "operator")
        } else {
            (first.len_utf8(), "")
        };
        let token = escape(&rest[..len]);
        if class.is_empty() {
            output.push_str(&token);
        } else {
            output.push_str(&format!("<span class=\"{class}\">{token}</span>"));
        }
        rest = &rest[len..];
    }
    output
}
