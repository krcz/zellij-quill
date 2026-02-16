use super::*;
use args::parse_args;
use clap::Parser;

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct TailArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(short = 'n', long = "lines", default_value = "200")]
    lines: usize,
    #[clap(long, arg_enum, default_value = "end")]
    from: TailFrom,
    #[clap(long = "no-strip-ansi")]
    no_strip_ansi: bool,
    #[clap(long = "strip-ansi", hide = true)]
    strip_ansi: bool,
    #[clap(long)]
    since: Option<String>,
    #[clap(long)]
    token: Option<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct GrepArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(short = 'n')]
    line_numbers: bool,
    #[clap(short = 'C', long)]
    context: Option<usize>,
    #[clap(short = 'i', long = "ignore-case")]
    ignore_case: bool,
    #[clap(short = 'F', long = "fixed-strings")]
    fixed_strings: bool,
    #[clap(long, default_value = "2000")]
    last: usize,
    #[clap(long)]
    since: Option<String>,
    #[clap(long)]
    token: Option<String>,
    pattern: Option<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct WaitArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    pane: Option<String>,
    #[clap(long)]
    regex: Option<String>,
    #[clap(long, default_value = "1")]
    last: usize,
    #[clap(long, default_value = "30")]
    timeout: String,
    #[clap(long, default_value = "0.2")]
    interval: String,
    #[clap(long, default_value = "400")]
    window: usize,
    #[clap(long, arg_enum, default_value = "any")]
    mode: WaitMode,
    #[clap(long)]
    since: Option<String>,
    #[clap(long)]
    token: Option<String>,
    /// Positional regex (alternative to --regex)
    regex_positional: Option<String>,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_tail(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<TailArgs>(raw_args)?;
        let strip_ansi = !args.no_strip_ansi;

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "tail requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id, "tail")?;
        let pane_lines = self.read_pane_lines(pane_id, strip_ansi)?;
        let since_line_count = if let Some(token) = args.since {
            Some(self.resolve_since_line_count(&token, pane_id)?)
        } else {
            None
        };

        let mut source_lines = pane_lines.all.clone();
        let mut base_offset = 0usize;
        let mut truncated_since = false;
        if let Some(since_count) = since_line_count {
            if since_count > source_lines.len() {
                truncated_since = true;
                base_offset = 0;
            } else {
                base_offset = since_count;
                source_lines = source_lines.split_off(since_count);
            }
        }

        let lines = args.lines;
        let selected = match args.from {
            TailFrom::End => {
                if source_lines.len() <= lines {
                    source_lines
                } else {
                    source_lines[source_lines.len() - lines..].to_vec()
                }
            }
            TailFrom::Top => source_lines.into_iter().take(lines).collect(),
            TailFrom::Viewport => {
                let start = pane_lines.viewport_start.saturating_sub(base_offset);
                let end = pane_lines
                    .viewport_end_exclusive
                    .saturating_sub(base_offset)
                    .min(source_lines.len());
                if start >= end {
                    Vec::new()
                } else {
                    let viewport_only = &source_lines[start..end];
                    if viewport_only.len() <= lines {
                        viewport_only.to_vec()
                    } else {
                        viewport_only[viewport_only.len() - lines..].to_vec()
                    }
                }
            }
        };

        let human = if args.json {
            None
        } else {
            Some(selected.join("\n"))
        };

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human,
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
                "lines": selected,
                "truncated_since": truncated_since,
                "count": selected.len(),
            }),
        })
    }

    pub(in crate::plugin) fn cmd_grep(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<GrepArgs>(raw_args)?;
        let context = args.context.unwrap_or(0);

        let pattern = args
            .pattern
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "grep requires a <pattern> argument"))?;
        let regex = compile_search_regex(&pattern, args.ignore_case, args.fixed_strings)?;
        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "grep requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id, "grep")?;

        let pane_lines = self.read_pane_lines(pane_id, true)?;
        let since_line_count = if let Some(token) = args.since {
            Some(self.resolve_since_line_count(&token, pane_id)?)
        } else {
            None
        };

        let all_lines = if let Some(since_count) = since_line_count {
            if since_count > pane_lines.all.len() {
                Vec::new()
            } else {
                pane_lines.all[since_count..].to_vec()
            }
        } else {
            pane_lines.all.clone()
        };

        let (candidate_lines, offset) = if all_lines.len() <= args.last {
            (all_lines, 0usize)
        } else {
            let start = all_lines.len() - args.last;
            (all_lines[start..].to_vec(), start)
        };

        let matches = build_grep_matches(&candidate_lines, &regex, context, offset);

        let human = if args.json {
            None
        } else {
            let mut lines = Vec::new();
            for m in &matches {
                if args.line_numbers {
                    lines.push(format!("{}:{}", m.line_number, m.line));
                } else {
                    lines.push(m.line.clone());
                }
            }
            Some(lines.join("\n"))
        };

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human,
            payload: json!({
                "ok": true,
                "pane_id": pane_id_to_string(pane_id),
                "pattern": pattern,
                "match_count": matches.len(),
                "matches": matches,
            }),
        })
    }

    pub(in crate::plugin) fn cmd_wait(
        &mut self,
        raw_args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<WaitArgs>(raw_args)?;

        let timeout = parse_duration(&args.timeout)?;
        let interval = parse_duration(&args.interval)?;

        let regex_pattern = args
            .regex
            .or(args.regex_positional)
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "wait requires --regex <pattern>"))?;
        let regex = compile_search_regex(&regex_pattern, false, false)?;

        let pane_id = args
            .pane
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "wait requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(args.token.as_deref(), pane_id, "wait")?;
        let since_line_count = if let Some(token) = args.since {
            Some(self.resolve_since_line_count(&token, pane_id)?)
        } else {
            None
        };

        if let Some(matched_lines) = self.wait_condition_matches(
            pane_id,
            &regex,
            args.last,
            args.window,
            &args.mode,
            since_line_count,
        )? {
            return Ok(CommandOutcome::Immediate {
                json_only: args.json,
                human: if args.json {
                    None
                } else {
                    Some("match found".to_string())
                },
                payload: json!({
                    "ok": true,
                    "pane_id": pane_id_to_string(pane_id),
                    "matched": true,
                    "lines": matched_lines,
                }),
            });
        }

        let pipe_id = pipe_id.ok_or_else(|| {
            ApiError::new(
                "PIPE_REQUIRED",
                "wait requires a CLI pipe source for asynchronous response",
            )
        })?;

        self.block_pipe(pipe_id);
        let job_id = self.next_job_id();
        self.jobs.insert(
            job_id,
            Job::Wait(WaitJob {
                pipe_id: pipe_id.to_string(),
                pane_id,
                regex,
                last_n: args.last,
                window: args.window,
                mode: args.mode,
                deadline: Instant::now() + timeout,
                next_poll_at: Instant::now() + interval,
                interval,
                json_only: args.json,
                since_line_count,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }
}
