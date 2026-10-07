//! table widget (PLAN.md section 7).
//! Table widget (PLAN.md 7.2): header + hl separator + rows + selection marker.
pub const SELECT_MARK: char = '\u{258c}';
pub const MARK_GLYPH: char = '\u{25c6}';
/// Build a header separator line.
pub fn header_separator(width: usize) -> String {
    "\u{2500}".repeat(width)
}
