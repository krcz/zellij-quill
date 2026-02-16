use crate::error::{ApiError, ApiResult};
use crate::types::{PipeCommand, SpawnKind, SpawnWhere, TabSelector, TailFrom, WaitMode};
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use std::time::Duration;
use zellij_tile::prelude::PipeMessage;

pub(crate) fn parse_pipe_command(pipe_message: &PipeMessage) -> ApiResult<PipeCommand> {
    let payload = match &pipe_message.payload {
        Some(payload) if !payload.trim().is_empty() => payload.trim().to_string(),
        _ => {
            if !pipe_message.name.trim().is_empty() && pipe_message.name != "zellij-quill" {
                pipe_message.name.clone()
            } else if let Some(cmd) = pipe_message.args.get("cmd") {
                cmd.clone()
            } else {
                return Err(ApiError::new(
                    "INVALID_REQUEST",
                    "No pipe payload/command found",
                ));
            }
        }
    };

    let argv = if payload.starts_with('{') || payload.starts_with('[') {
        parse_json_payload_to_argv(&payload)?
    } else {
        parse_argv(&payload)?
    };

    if argv.is_empty() {
        return Err(ApiError::new("INVALID_REQUEST", "Command payload is empty"));
    }

    Ok(PipeCommand {
        cmd: argv[0].clone(),
        args: argv[1..].to_vec(),
    })
}

pub(crate) fn parse_json_payload_to_argv(payload: &str) -> ApiResult<Vec<String>> {
    let value: Value = serde_json::from_str(payload)
        .map_err(|e| ApiError::new("INVALID_JSON", format!("Failed to parse JSON payload: {e}")))?;

    match value {
        Value::Array(array) => {
            let mut argv = Vec::with_capacity(array.len());
            for value in array {
                match value {
                    Value::String(s) => argv.push(s),
                    _ => {
                        return Err(ApiError::new(
                            "INVALID_JSON",
                            "JSON array payload must contain only strings",
                        ));
                    }
                }
            }
            Ok(argv)
        }
        Value::Object(mut map) => {
            let cmd = map
                .remove("cmd")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .ok_or_else(|| {
                    ApiError::new(
                        "INVALID_JSON",
                        "JSON object payload requires a string `cmd`",
                    )
                })?;

            let mut argv = vec![cmd];

            if let Some(Value::Array(positional)) = map.remove("args") {
                for value in positional {
                    if let Value::String(s) = value {
                        argv.push(s);
                    }
                }
            }

            for (key, value) in map {
                let flag = format!("--{}", key.replace('_', "-"));
                match value {
                    Value::Bool(true) => argv.push(flag),
                    Value::Bool(false) | Value::Null => {}
                    Value::String(s) => {
                        argv.push(flag);
                        argv.push(s);
                    }
                    Value::Number(n) => {
                        argv.push(flag);
                        argv.push(n.to_string());
                    }
                    Value::Array(items) => {
                        for item in items {
                            match item {
                                Value::String(s) => {
                                    argv.push(flag.clone());
                                    argv.push(s);
                                }
                                Value::Number(n) => {
                                    argv.push(flag.clone());
                                    argv.push(n.to_string());
                                }
                                Value::Bool(b) => {
                                    argv.push(flag.clone());
                                    argv.push(b.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            Ok(argv)
        }
        _ => Err(ApiError::new(
            "INVALID_JSON",
            "JSON payload must be an object or array",
        )),
    }
}

pub(crate) fn parse_argv(input: &str) -> ApiResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                in_quotes = false;
                continue;
            }
            if ch == '\\' {
                match chars.peek().copied() {
                    Some('"') => {
                        current.push('"');
                        chars.next();
                    }
                    Some('\\') => {
                        current.push('\\');
                        chars.next();
                    }
                    _ => current.push(ch),
                }
                continue;
            }
            current.push(ch);
            continue;
        }

        match ch {
            '"' => in_quotes = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err(ApiError::new(
            "INVALID_ARGS",
            "Unterminated quoted string in command payload",
        ));
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

pub(crate) fn parse_bool(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.trim().to_ascii_lowercase()),
        Some(v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

pub(crate) fn parse_duration(value: &str) -> ApiResult<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::new(
            "INVALID_DURATION",
            "Duration cannot be empty",
        ));
    }

    if let Some(ms) = value.strip_suffix("ms") {
        let ms = parse_f64(ms, "duration")?;
        return Ok(Duration::from_secs_f64(ms / 1000.0));
    }

    if let Some(secs) = value.strip_suffix('s') {
        let secs = parse_f64(secs, "duration")?;
        return Ok(Duration::from_secs_f64(secs));
    }

    let secs = parse_f64(value, "duration")?;
    Ok(Duration::from_secs_f64(secs))
}

pub(crate) fn parse_f64(value: &str, field: &str) -> ApiResult<f64> {
    value.trim().parse::<f64>().map_err(|_| {
        ApiError::new(
            "INVALID_ARGS",
            format!("Invalid numeric value for {field}: {value}"),
        )
    })
}

pub(crate) fn parse_usize_arg(value: Option<&String>, flag: &str) -> ApiResult<usize> {
    parse_usize_literal(
        value.ok_or_else(|| ApiError::new("INVALID_ARGS", format!("Missing value for {flag}")))?,
        flag,
    )
}

pub(crate) fn parse_usize_literal(value: &str, flag: &str) -> ApiResult<usize> {
    value.trim().parse::<usize>().map_err(|_| {
        ApiError::new(
            "INVALID_ARGS",
            format!("Invalid integer value for {flag}: {value}"),
        )
    })
}

pub(crate) fn parse_tab_selector(value: &str) -> ApiResult<TabSelector> {
    match value {
        "focused" => Ok(TabSelector::Focused),
        "all" => Ok(TabSelector::All),
        other => Ok(TabSelector::Index(parse_usize_literal(other, "--tab")?)),
    }
}

pub(crate) fn parse_columns(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub(crate) fn parse_tail_from(value: &str) -> ApiResult<TailFrom> {
    match value {
        "end" => Ok(TailFrom::End),
        "viewport" => Ok(TailFrom::Viewport),
        "top" => Ok(TailFrom::Top),
        _ => Err(ApiError::new(
            "INVALID_ARGS",
            format!("Invalid --from value: {value}"),
        )),
    }
}

pub(crate) fn parse_wait_mode(value: &str) -> ApiResult<WaitMode> {
    match value {
        "any" => Ok(WaitMode::Any),
        "all" => Ok(WaitMode::All),
        _ => Err(ApiError::new(
            "INVALID_ARGS",
            format!("Invalid --mode value: {value}"),
        )),
    }
}

pub(crate) fn parse_spawn_kind(value: &str) -> ApiResult<SpawnKind> {
    match value {
        "terminal" => Ok(SpawnKind::Terminal),
        "command" => Ok(SpawnKind::Command),
        _ => Err(ApiError::new(
            "INVALID_ARGS",
            format!("Invalid --kind value: {value}"),
        )),
    }
}

pub(crate) fn parse_spawn_where(value: &str) -> ApiResult<SpawnWhere> {
    match value {
        "tiled" => Ok(SpawnWhere::Tiled),
        "floating" => Ok(SpawnWhere::Floating),
        "near-plugin" => Ok(SpawnWhere::NearPlugin),
        "in-place" => Ok(SpawnWhere::InPlace),
        "background" => Ok(SpawnWhere::Background),
        _ => Err(ApiError::new(
            "INVALID_ARGS",
            format!("Invalid --where value: {value}"),
        )),
    }
}

pub(crate) fn parse_env_pair(value: &str) -> ApiResult<(String, String)> {
    let Some((key, val)) = value.split_once('=') else {
        return Err(ApiError::new(
            "INVALID_ARGS",
            format!("Invalid env pair, expected KEY=VAL: {value}"),
        ));
    };
    Ok((key.to_string(), val.to_string()))
}

pub(crate) fn compile_search_regex(
    pattern: &str,
    ignore_case: bool,
    fixed_strings: bool,
) -> ApiResult<Regex> {
    let pattern = if fixed_strings {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(ignore_case)
        .build()
        .map_err(|e| ApiError::new("INVALID_REGEX", format!("Invalid regex pattern: {e}")))
}

pub(crate) fn parse_key_spec(spec: &str) -> ApiResult<Vec<u8>> {
    let mut output = Vec::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j >= chars.len() {
                return Err(ApiError::new(
                    "INVALID_KEYS",
                    "Unterminated key token in --keys",
                ));
            }
            let token: String = chars[i + 1..j].iter().collect();
            append_key_token(&mut output, &token)?;
            i = j + 1;
            continue;
        }

        let mut buf = [0u8; 4];
        let s = chars[i].encode_utf8(&mut buf);
        output.extend_from_slice(s.as_bytes());
        i += 1;
    }

    Ok(output)
}

fn append_key_token(output: &mut Vec<u8>, token: &str) -> ApiResult<()> {
    let normalized = token.trim();
    match normalized {
        "Enter" => output.push(b'\n'),
        "Tab" => output.push(b'\t'),
        "Esc" => output.push(0x1b),
        "Backspace" => output.push(0x7f),
        _ => {
            if let Some(ctrl) = normalized.strip_prefix("C-") {
                let ch = ctrl.chars().next().ok_or_else(|| {
                    ApiError::new("INVALID_KEYS", format!("Invalid token <{token}>"))
                })?;
                if !ch.is_ascii() {
                    return Err(ApiError::new(
                        "INVALID_KEYS",
                        format!("Ctrl token must be ASCII: <{token}>"),
                    ));
                }
                let byte = (ch.to_ascii_uppercase() as u8) & 0x1f;
                output.push(byte);
                return Ok(());
            }
            if let Some(alt) = normalized.strip_prefix("A-") {
                let ch = alt.chars().next().ok_or_else(|| {
                    ApiError::new("INVALID_KEYS", format!("Invalid token <{token}>"))
                })?;
                output.push(0x1b);
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                output.extend_from_slice(s.as_bytes());
                return Ok(());
            }

            return Err(ApiError::new(
                "INVALID_KEYS",
                format!("Unsupported key token <{token}>"),
            ));
        }
    }

    Ok(())
}

pub(crate) fn parse_hex_bytes(value: &str) -> ApiResult<Vec<u8>> {
    let mut compact = String::new();
    for ch in value.chars() {
        if ch.is_ascii_hexdigit() {
            compact.push(ch);
        }
    }

    if compact.is_empty() {
        return Ok(Vec::new());
    }

    if compact.len() % 2 != 0 {
        return Err(ApiError::new(
            "INVALID_ARGS",
            "--raw-bytes must contain an even number of hex digits",
        ));
    }

    let mut bytes = Vec::with_capacity(compact.len() / 2);
    let chars: Vec<char> = compact.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let pair: String = [chars[i], chars[i + 1]].into_iter().collect();
        let byte = u8::from_str_radix(&pair, 16).map_err(|_| {
            ApiError::new(
                "INVALID_ARGS",
                format!("Invalid hex byte in --raw-bytes: {pair}"),
            )
        })?;
        bytes.push(byte);
        i += 2;
    }
    Ok(bytes)
}

pub(crate) fn opt_value(arg: &str, option: &str) -> Option<String> {
    arg.strip_prefix(&(option.to_string() + "="))
        .map(|value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_handles_quotes_and_escapes() {
        let parsed = parse_argv("run --pane build-pane -- \"echo \\\"hi\\\"\"").unwrap();
        assert_eq!(
            parsed,
            vec!["run", "--pane", "build-pane", "--", "echo \"hi\""]
        );
    }

    #[test]
    fn tokenizer_rejects_unterminated_quote() {
        let err = parse_argv("run \"unterminated").unwrap_err();
        assert_eq!(err.code, "INVALID_ARGS");
    }

    #[test]
    fn key_parser_supports_ctrl_alt_and_named_keys() {
        let bytes = parse_key_spec("<Enter><Tab><Esc><Backspace><C-c><A-x>").unwrap();
        assert_eq!(bytes, vec![b'\n', b'\t', 0x1b, 0x7f, 0x03, 0x1b, b'x']);
    }

    #[test]
    fn key_parser_rejects_bad_token() {
        let err = parse_key_spec("<Nope>").unwrap_err();
        assert_eq!(err.code, "INVALID_KEYS");
    }

    #[test]
    fn json_object_payload_expands_into_argv() {
        let payload = r#"{"cmd":"grep","pane":"build-pane","ignore_case":true,"args":["error"]}"#;
        let argv = parse_json_payload_to_argv(payload).unwrap();
        assert!(argv.contains(&"grep".to_string()));
        assert!(argv.contains(&"--pane".to_string()));
        assert!(argv.contains(&"build-pane".to_string()));
        assert!(argv.contains(&"--ignore-case".to_string()));
        assert!(argv.contains(&"error".to_string()));
    }

    #[test]
    fn selector_parser_understands_basic_forms() {
        assert_eq!(parse_usize_literal("123", "--lines").unwrap(), 123);
        assert!(parse_usize_literal("x", "--lines").is_err());
        assert!(Regex::new("abc").unwrap().is_match("abc"));
    }
}
