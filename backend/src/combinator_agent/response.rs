/// Extracts the outer JSON object from a model reply that may be wrapped in a
/// Markdown fence or brief prose.
pub(super) fn extract_json(text: &str) -> &str {
    let fenced = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    match (fenced.find('{'), fenced.rfind('}')) {
        (Some(start), Some(end)) if end > start => &fenced[start..=end],
        _ => fenced,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extracts_fenced_json_from_a_model_reply() {
        assert_eq!(
            super::extract_json("Here is the result:\n```json\n{\"ok\":true}\n```"),
            "{\"ok\":true}"
        );
    }
}
