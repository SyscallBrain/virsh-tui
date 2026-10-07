//! keyhints widget.
//! KeyHints.
pub fn hints(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, l)| format!("{k} {l}"))
        .collect::<Vec<_>>()
        .join("  ")
}
