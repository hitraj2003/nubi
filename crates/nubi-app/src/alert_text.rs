//! Match the original alert expressions, including JavaScript UTF-16 slice limits.
fn truncate_utf16(value: &str, limit: usize) -> String {
    // JS can retain a lone surrogate at the boundary. Rust strings cannot; use
    // its visible replacement glyph rather than retaining an extra code point.
    String::from_utf16_lossy(&value.encode_utf16().take(limit).collect::<Vec<_>>())
}
fn js_whitespace(value: char) -> bool {
    matches!(value as u32, 0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}
pub fn notch_body(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut space = false;
    for ch in value.chars() {
        if js_whitespace(ch) {
            if !space {
                normalized.push(' ');
            }
            space = true;
        } else {
            normalized.push(ch);
            space = false;
        }
    }
    truncate_utf16(&normalized, 160)
}
pub fn card_body(value: &str) -> String {
    truncate_utf16(value, 200)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_alert_expressions_match_emoji_boundaries_and_whitespace() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/alert-text.json")).unwrap();
        let mut examples = Vec::new();
        for case in fixture["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            for (name, actual) in [("top", notch_body(input)), ("card", card_body(input))] {
                let units: Vec<u16> = case[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as u16)
                    .collect();
                examples.push(serde_json::json!({"surface": name, "input": input, "original": String::from_utf16_lossy(&units), "native": actual, "units": units.len()}));
                assert_eq!(
                    actual,
                    String::from_utf16_lossy(&units),
                    "{name}: {input:?}"
                );
                assert!(actual.encode_utf16().count() <= if name == "top" { 160 } else { 200 });
            }
        }

        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native/alerts");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("text-comparison.json"),
            serde_json::to_vec(&examples).unwrap(),
        )
        .unwrap();
    }
}
