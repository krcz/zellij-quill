use super::*;
use args::parse_args;
use clap::Parser;

#[derive(Parser)]
#[clap(no_binary_name = true, trailing_var_arg = true)]
struct SendArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(long = "no-newline")]
    no_newline: bool,
    #[clap(long)]
    enter: bool,
    #[clap(long)]
    keys: Option<String>,
    #[clap(long = "raw-bytes")]
    raw_bytes: Option<String>,
    #[clap(long)]
    token: Option<String>,
    #[clap(allow_hyphen_values = true)]
    text: Vec<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true, trailing_var_arg = true)]
struct RunArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(long = "no-enter")]
    no_enter: bool,
    #[clap(long)]
    wait: Option<String>,
    #[clap(long)]
    timeout: Option<String>,
    #[clap(long)]
    token: Option<String>,
    #[clap(long)]
    since: Option<String>,
    #[clap(allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct InterruptArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(long)]
    sigint: bool,
    #[clap(long, conflicts_with = "sigint")]
    sigkill: bool,
    #[clap(long)]
    token: Option<String>,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_send(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<SendArgs>(raw_args)?;

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "send requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id)?;

        let mut bytes_to_write = Vec::new();
        let text = if args.text.is_empty() {
            None
        } else {
            Some(args.text.join(" "))
        };

        if let Some(text) = text {
            bytes_to_write.extend_from_slice(text.as_bytes());
            if !args.no_newline {
                bytes_to_write.push(b'\n');
            }
        }

        if let Some(keys) = args.keys {
            if keys.trim().eq_ignore_ascii_case("<C-c>") {
                send_sigint_to_pane_id(pane_id);
            } else {
                bytes_to_write.extend(parse_key_spec(&keys)?);
            }
        }

        if let Some(raw_hex) = args.raw_bytes {
            bytes_to_write.extend(parse_hex_bytes(&raw_hex)?);
        }

        if args.enter {
            bytes_to_write.push(b'\n');
        }

        if !bytes_to_write.is_empty() {
            write_to_pane_id(bytes_to_write, pane_id);
        }

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json {
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
        raw_args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<RunArgs>(raw_args)?;

        let timeout = match &args.timeout {
            Some(t) => parse_duration(t)?,
            None => Duration::from_secs(30),
        };

        if args.command.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "run requires a command string after --",
            ));
        }

        if args.wait.is_some() && pipe_id.is_none() {
            return Err(ApiError::new(
                "PIPE_REQUIRED",
                "run --wait requires a CLI pipe source",
            ));
        }

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "run requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id)?;
        let mut command_text = args.command.join(" ");
        if !args.no_enter {
            command_text.push('\n');
        }
        write_chars_to_pane_id(&command_text, pane_id);

        if let Some(wait_regex) = args.wait {
            let pipe_id = pipe_id.ok_or_else(|| {
                ApiError::new("PIPE_REQUIRED", "run --wait requires a CLI pipe source")
            })?;

            let regex = compile_search_regex(&wait_regex, false, false)?;
            let since_line_count = if let Some(token) = args.since {
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
                    json_only: args.json,
                    since_line_count,
                }),
            );
            self.schedule_job_timer();
            return Ok(CommandOutcome::Async);
        }

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json {
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
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<InterruptArgs>(raw_args)?;

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "interrupt requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id)?;
        if args.sigkill {
            send_sigkill_to_pane_id(pane_id);
        } else {
            send_sigint_to_pane_id(pane_id);
        }

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human: if args.json {
                None
            } else if args.sigkill {
                Some(format!("sent SIGKILL to {}", pane_id_to_string(pane_id)))
            } else {
                Some(format!("sent SIGINT to {}", pane_id_to_string(pane_id)))
            },
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
                "signal": if args.sigkill { "sigkill" } else { "sigint" },
            }),
        })
    }
}
