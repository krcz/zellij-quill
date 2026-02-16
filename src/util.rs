use crate::types::GrepMatch;
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ANSI_CSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("valid ANSI CSI regex"));
static ANSI_OSC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\][^\x07]*(?:\x07|\x1b\\\\)").expect("valid ANSI OSC regex"));

pub(crate) fn column_value(row: &Value, column: &str) -> String {
    row.get(column)
        .map(|value| match value {
            Value::Null => "".to_string(),
            Value::String(s) => s.clone(),
            _ => value.to_string(),
        })
        .unwrap_or_default()
}

pub(crate) fn strip_ansi_sequences(input: &str) -> String {
    let without_osc = ANSI_OSC_RE.replace_all(input, "");
    ANSI_CSI_RE.replace_all(&without_osc, "").to_string()
}

pub(crate) fn build_grep_matches(
    lines: &[String],
    regex: &Regex,
    context: usize,
    line_number_offset: usize,
) -> Vec<GrepMatch> {
    let mut out = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if !regex.is_match(line) {
            continue;
        }

        let before_start = idx.saturating_sub(context);
        let before = lines[before_start..idx].to_vec();

        let after_end = (idx + context + 1).min(lines.len());
        let after = if idx + 1 >= after_end {
            Vec::new()
        } else {
            lines[idx + 1..after_end].to_vec()
        };

        out.push(GrepMatch {
            line_number: line_number_offset + idx + 1,
            line: line.clone(),
            before,
            after,
        });
    }

    out
}

pub(crate) fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_matcher_returns_context() {
        let lines = vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
            "betamax".to_string(),
        ];
        let regex = Regex::new("beta").unwrap();
        let matches = build_grep_matches(&lines, &regex, 1, 0);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 2);
        assert_eq!(matches[0].before, vec!["alpha"]);
        assert_eq!(matches[0].after, vec!["gamma"]);
        assert_eq!(matches[1].line_number, 4);
        assert_eq!(matches[1].before, vec!["gamma"]);
        assert_eq!(matches[1].after, Vec::<String>::new());
    }
}
