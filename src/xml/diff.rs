//! Line diffs for the Pending panel (pretty-print + similar).

/// Diff line kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Context,
    Minus,
    Plus,
    Hunk,
}

/// One diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Normalize XML through xmltree so quoting/formatting matches on both sides.
fn normalize(text: &str) -> String {
    match xmltree::Element::parse(text.as_bytes()) {
        Ok(elem) => super::edit::pretty(&elem),
        Err(_) => text.to_string(),
    }
}

/// Diff two texts (XML-aware): returns hunked lines with 1 line of context.
pub fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    let before = normalize(before);
    let after = normalize(after);
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &a, &b);
    let mut out = Vec::new();
    for op in ops {
        match op {
            similar::DiffOp::Equal { old_index, len, .. } => {
                if len > 2 {
                    out.push(DiffLine {
                        kind: DiffKind::Context,
                        text: format!("  {}", a[old_index]),
                    });
                    out.push(DiffLine {
                        kind: DiffKind::Hunk,
                        text: String::from("  …"),
                    });
                    out.push(DiffLine {
                        kind: DiffKind::Context,
                        text: format!("  {}", a[old_index + len - 1]),
                    });
                } else {
                    for i in 0..len {
                        out.push(DiffLine {
                            kind: DiffKind::Context,
                            text: format!("  {}", a[old_index + i]),
                        });
                    }
                }
            }
            similar::DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for i in 0..old_len {
                    out.push(DiffLine {
                        kind: DiffKind::Minus,
                        text: format!("- {}", a[old_index + i]),
                    });
                }
            }
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for i in 0..new_len {
                    out.push(DiffLine {
                        kind: DiffKind::Plus,
                        text: format!("+ {}", b[new_index + i]),
                    });
                }
            }
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for i in 0..old_len {
                    out.push(DiffLine {
                        kind: DiffKind::Minus,
                        text: format!("- {}", a[old_index + i]),
                    });
                }
                for i in 0..new_len {
                    out.push(DiffLine {
                        kind: DiffKind::Plus,
                        text: format!("+ {}", b[new_index + i]),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{DiffKind, diff_lines};

    #[test]
    fn minus_plus() {
        let d = diff_lines("<a>1</a>\n", "<a>2</a>\n");
        assert!(d.iter().any(|l| l.kind == DiffKind::Minus));
        assert!(d.iter().any(|l| l.kind == DiffKind::Plus));
    }
}
