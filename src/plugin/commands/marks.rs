use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_mark(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut token: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--json" {
                json_only = true;
            } else if arg == "--pane" {
                i += 1;
                pane_id = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --pane"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--pane") {
                pane_id = Some(value);
            } else if arg == "--token" {
                i += 1;
                token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                token = Some(value);
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    format!("Unknown flag for mark: {arg}"),
                ));
            }
            i += 1;
        }

        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "mark requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "mark")?;
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
            json_only,
            human: if json_only { None } else { Some(token.clone()) },
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
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut mark_token: Option<String> = None;
        let mut auth_token: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--json" {
                json_only = true;
            } else if arg == "--token" {
                i += 1;
                mark_token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                mark_token = Some(value);
            } else if arg == "--auth-token" {
                i += 1;
                auth_token = Some(
                    args.get(i)
                        .ok_or_else(|| {
                            ApiError::new("INVALID_ARGS", "Missing value for --auth-token")
                        })?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--auth-token") {
                auth_token = Some(value);
            } else if mark_token.is_none() {
                mark_token = Some(arg.clone());
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    "since accepts at most one token",
                ));
            }
            i += 1;
        }

        let token =
            mark_token.ok_or_else(|| ApiError::new("INVALID_ARGS", "since requires a token"))?;
        let mark = self
            .marks
            .get(&token)
            .ok_or_else(|| {
                ApiError::new("INVALID_MARK", "Unknown mark token")
                    .hint("Create one with `mark --pane ...` and pass it to --since.")
            })?
            .clone();
        self.ensure_token_can_access_pane(auth_token.as_deref(), mark.pane_id, "since")?;

        let current_line_count = self.read_pane_lines(mark.pane_id, true)?.all.len();
        let truncated = mark.line_count > current_line_count;
        let available_since =
            current_line_count.saturating_sub(mark.line_count.min(current_line_count));

        Ok(CommandOutcome::Immediate {
            json_only,
            human: if json_only {
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
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;

        for arg in args {
            if arg == "--json" {
                json_only = true;
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    format!("Unknown flag for token: {arg}"),
                ));
            }
        }

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
            json_only,
            human: if json_only { None } else { Some(token.clone()) },
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
