use super::*;
use args::parse_args;
use clap::Parser;

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct MarkArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(long)]
    token: Option<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct SinceArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    token: Option<String>,
    #[clap(long = "auth-token")]
    auth_token: Option<String>,
    /// Positional mark token (alternative to --token)
    mark_token_positional: Option<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct TokenArgs {
    #[clap(long)]
    json: bool,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_mark(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<MarkArgs>(raw_args)?;

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "mark requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id)?;
        let pane_lines = self.read_pane_lines(pane_id, true)?;
        let token = self.next_mark_token();
        let created_at_ms = unix_time_ms();

        let mark = ScrollbackMark {
            token: token.clone(),
            pane_id,
            line_count: pane_lines.all.len(),
            created_at_ms,
        };
        self.marks.insert(token.clone(), mark.clone());

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json { None } else { Some(token.clone()) },
            payload: json!({
                "ok": true,
                "token": token,
                "pane_id": pane_id_to_string(mark.pane_id),
                "line_count": mark.line_count,
                "created_at_ms": mark.created_at_ms,
            }),
        })
    }

    pub(in crate::plugin) fn cmd_since(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<SinceArgs>(raw_args)?;

        let mark_token = args
            .mark_token_positional
            .or(args.token)
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "since requires a token"))?;

        let mark = self
            .marks
            .get(&mark_token)
            .ok_or_else(|| {
                ApiError::new("INVALID_MARK", "Unknown mark token")
                    .hint("Create one with `mark --pane ...` and pass it to --since.")
            })?
            .clone();
        self.ensure_token_can_access_pane(args.auth_token.as_deref(), mark.pane_id)?;

        let current_line_count = self.read_pane_lines(mark.pane_id, true)?.all.len();
        let truncated = mark.line_count > current_line_count;
        let available_since =
            current_line_count.saturating_sub(mark.line_count.min(current_line_count));

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json {
                None
            } else {
                Some(format!("{} lines since {}", available_since, mark.token))
            },
            payload: json!({
                "ok": true,
                "token": mark.token,
                "pane_id": pane_id_to_string(mark.pane_id),
                "line_count_at_mark": mark.line_count,
                "current_line_count": current_line_count,
                "available_since": available_since,
                "truncated": truncated,
            }),
        })
    }

    pub(in crate::plugin) fn cmd_token(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<TokenArgs>(raw_args)?;

        let (token, created, source) = if let Some(token) = self.session_env_token() {
            (token, false, "session_env")
        } else if let Some(token) = self.token_from_session_env.clone() {
            (token, false, "plugin_cache")
        } else {
            let token = format!(
                "{}-{}",
                generate_random_name().to_ascii_lowercase(),
                unix_time_ms()
            );
            self.token_from_session_env = Some(token.clone());
            (token, true, "generated")
        };

        let export_command = format!(
            "export {SESSION_TOKEN_ENV_VAR}={}",
            shell_single_quote(&token)
        );

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json { None } else { Some(token.clone()) },
            payload: json!({
                "ok": true,
                "token": token,
                "created": created,
                "source": source,
                "env_var": SESSION_TOKEN_ENV_VAR,
                "export_command": export_command,
                "set_env_supported": false,
            }),
        })
    }

    pub(in crate::plugin) fn help_response(&self, json_only: bool) -> CommandOutcome {
        let commands = vec![
            "panes",
            "send",
            "run",
            "tail",
            "read",
            "grep",
            "wait",
            "exec",
            "spawn",
            "interrupt",
            "mark",
            "since",
            "token",
            "permit",
        ];
        CommandOutcome::Immediate {
            json_only,
            human: if json_only {
                None
            } else {
                Some(format!("commands: {}", commands.join(", ")))
            },
            payload: json!({
                "ok": true,
                "commands": commands,
            }),
        }
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
