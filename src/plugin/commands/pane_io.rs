use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_send(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut no_newline = false;
        let mut force_enter = false;
        let mut keys: Option<String> = None;
        let mut raw_hex: Option<String> = None;
        let mut token: Option<String> = None;
        let mut positional = Vec::new();

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
            } else if arg == "--no-newline" {
                no_newline = true;
            } else if arg == "--enter" {
                force_enter = true;
            } else if arg == "--keys" {
                i += 1;
                keys = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --keys"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--keys") {
                keys = Some(value);
            } else if arg == "--raw-bytes" {
                i += 1;
                raw_hex = Some(
                    args.get(i)
                        .ok_or_else(|| {
                            ApiError::new("INVALID_ARGS", "Missing value for --raw-bytes")
                        })?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--raw-bytes") {
                raw_hex = Some(value);
            } else if arg == "--token" {
                i += 1;
                token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                token = Some(value);
            } else if arg == "--" {
                positional.extend(args[i + 1..].iter().cloned());
                break;
            } else {
                positional.push(arg.clone());
            }
            i += 1;
        }

        self.validate_auth(token.as_deref())?;

        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "send requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "send")?;

        let mut bytes_to_write = Vec::new();
        let text = if positional.is_empty() {
            None
        } else {
            Some(positional.join(" "))
        };

        if let Some(text) = text {
            bytes_to_write.extend_from_slice(text.as_bytes());
            if !no_newline {
                bytes_to_write.push(b'\n');
            }
        }

        if let Some(keys) = keys {
            if keys.trim().eq_ignore_ascii_case("<C-c>") {
                send_sigint_to_pane_id(pane_id);
            } else {
                bytes_to_write.extend(parse_key_spec(&keys)?);
            }
        }

        if let Some(raw_hex) = raw_hex {
            bytes_to_write.extend(parse_hex_bytes(&raw_hex)?);
        }

        if force_enter {
            bytes_to_write.push(b'\n');
        }

        if !bytes_to_write.is_empty() {
            write_to_pane_id(bytes_to_write, pane_id);
        }

        Ok(CommandOutcome::Immediate {
            json_only,
            human: if json_only {
                None
            } else {
                Some(format!("sent to {}", pane_id_to_string(pane_id)))
            },
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
            }),
        })
    }

    pub(in crate::plugin) fn cmd_run(
        &mut self,
        args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut no_enter = false;
        let mut wait_regex: Option<String> = None;
        let mut timeout = Duration::from_secs(30);
        let mut token: Option<String> = None;
        let mut since: Option<String> = None;
        let mut command_tokens = Vec::new();

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
            } else if arg == "--no-enter" {
                no_enter = true;
            } else if arg == "--wait" {
                i += 1;
                wait_regex = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --wait"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--wait") {
                wait_regex = Some(value);
            } else if arg == "--timeout" {
                i += 1;
                timeout = parse_duration(args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --timeout")
                })?)?;
            } else if let Some(value) = opt_value(arg, "--timeout") {
                timeout = parse_duration(&value)?;
            } else if arg == "--token" {
                i += 1;
                token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                token = Some(value);
            } else if arg == "--since" {
                i += 1;
                since = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --since"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--since") {
                since = Some(value);
            } else if arg == "--" {
                command_tokens.extend(args[i + 1..].iter().cloned());
                break;
            } else {
                command_tokens.push(arg.clone());
            }
            i += 1;
        }

        self.validate_auth(token.as_deref())?;

        if command_tokens.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "run requires a command string after --",
            ));
        }

        if wait_regex.is_some() && pipe_id.is_none() {
            return Err(ApiError::new(
                "PIPE_REQUIRED",
                "run --wait requires a CLI pipe source",
            ));
        }

        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "run requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "run")?;
        let mut command_text = command_tokens.join(" ");
        if !no_enter {
            command_text.push('\n');
        }
        write_chars_to_pane_id(&command_text, pane_id);

        if let Some(wait_regex) = wait_regex {
            let pipe_id = pipe_id.ok_or_else(|| {
                ApiError::new("PIPE_REQUIRED", "run --wait requires a CLI pipe source")
            })?;

            let regex = compile_search_regex(&wait_regex, false, false)?;
            let since_line_count = if let Some(token) = since {
                Some(self.resolve_since_line_count(&token, pane_id)?)
            } else {
                None
            };

            self.block_pipe(pipe_id);
            let job_id = self.next_job_id();
            self.jobs.insert(
                job_id,
                Job::Wait(WaitJob {
                    pipe_id: pipe_id.to_string(),
                    pane_id,
                    regex,
                    last_n: 1,
                    window: 400,
                    mode: WaitMode::Any,
                    deadline: Instant::now() + timeout,
                    next_poll_at: Instant::now(),
                    interval: Duration::from_millis(200),
                    json_only,
                    since_line_count,
                }),
            );
            self.schedule_job_timer();
            return Ok(CommandOutcome::Async);
        }

        Ok(CommandOutcome::Immediate {
            json_only,
            human: if json_only {
                None
            } else {
                Some(format!("ran in {}", pane_id_to_string(pane_id)))
            },
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
                "wait": false,
            }),
        })
    }

    pub(in crate::plugin) fn cmd_interrupt(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut use_sigkill = false;
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
            } else if arg == "--sigint" {
                use_sigkill = false;
            } else if arg == "--sigkill" {
                use_sigkill = true;
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
                    format!("Unknown flag for interrupt: {arg}"),
                ));
            }
            i += 1;
        }

        self.validate_auth(token.as_deref())?;

        let pane_id = pane_id
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "interrupt requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "interrupt")?;
        if use_sigkill {
            send_sigkill_to_pane_id(pane_id);
        } else {
            send_sigint_to_pane_id(pane_id);
        }

        Ok(CommandOutcome::Immediate {
            json_only,
            human: if json_only {
                None
            } else if use_sigkill {
                Some(format!("sent SIGKILL to {}", pane_id_to_string(pane_id)))
            } else {
                Some(format!("sent SIGINT to {}", pane_id_to_string(pane_id)))
            },
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
                "signal": if use_sigkill { "sigkill" } else { "sigint" },
            }),
        })
    }
}
