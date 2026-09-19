pub mod json;
pub mod terminal;
pub use crate::Coverage;

/// Convert the compiler's one-based byte columns without splitting UTF-8.
pub(crate) fn offset(source: &str, position: nash_region::Position) -> usize {
    let start = source
        .split_inclusive('\n')
        .take(position.line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let mut offset = (start + position.column.saturating_sub(1)).min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
pub(crate) fn source_text(source: &str, region: nash_region::Region) -> &str {
    source
        .get(offset(source, region.start)..offset(source, region.end))
        .unwrap_or("")
}
/// Zero-based row within the displayed assertion source.
pub(crate) fn row(site: nash_region::Region, capture: nash_region::Region) -> usize {
    capture.start.line.saturating_sub(site.start.line)
}
/// Zero-based terminal display column on the capture's source line. The first
/// line starts at the assertion region; subsequent lines retain source indentation.
pub(crate) fn column(
    source: &str,
    site: nash_region::Region,
    capture: nash_region::Region,
) -> usize {
    let start = if capture.start.line <= site.start.line {
        offset(source, site.start)
    } else {
        offset(source, nash_region::Position::new(capture.start.line, 1))
    };
    let end = offset(source, capture.start).max(start);
    unicode_width::UnicodeWidthStr::width(&source[start..end])
}
