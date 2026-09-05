/// Normalizes line endings to LF and drops non-printable control characters
/// other than tab and newline.
pub(crate) fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_text;

    #[test]
    fn normalizes_line_endings_and_removes_controls() {
        assert_eq!(
            normalize_text("first\r\nsecond\r\u{7}third\tend"),
            "first\nsecond\nthird\tend"
        );
    }
}
