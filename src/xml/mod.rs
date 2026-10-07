//! XML helpers: typed edits, diffs, highlighting.

pub mod diff;
pub mod edit;
pub mod highlight;

/// Escape text for an XML attribute or text node.
///
/// Every value that reaches a hand-built XML string (lease hostnames come from
/// the guest; names and paths from user input) must go through this.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod escape_tests {
    #[test]
    fn escapes_markup() {
        assert_eq!(super::escape("a'/><x y='&"), "a&apos;/&gt;&lt;x y=&apos;&amp;");
    }
}
