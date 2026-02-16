use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_tail(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut lines = 200usize;
        let mut from = TailFrom::End;
        let mut strip_ansi = true;
        let mut since: Option<String> = None;
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
            } else if arg == "-n" || arg == "--lines" {
                i += 1;
                lines = parse_usize_arg(args.get(i), "--lines")?;
            } else if let Some(value) = opt_value(arg, "--lines") {
                lines = parse_usize_literal(&value, "--lines")?;
            } else if arg == "--from" {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --from"))?;
                from = parse_tail_from(value)?;
            } else if let Some(value) = opt_value(arg, "--from") {
                from = parse_tail_from(&value)?;
            } else if arg == "--strip-ansi" {
                strip_ansi = true;
            } else if arg == "--no-strip-ansi" {
                strip_ansi = false;
            } else if arg == "--since" {
                i += 1;
                since = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --since"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--since") {
                since = Some(value);
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
                    format!("Unknown flag for tail/read: {arg}"),
                ));
            }
            i += 1;
        }

        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "tail requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "tail")?;
        let pane_lines = self.read_pane_lines(pane_id, strip_ansi)?;
        let since_line_count = if let Some(token) = since {
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

        let selected = match from {
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

        let human = if json_only {
            None
        } else {
            Some(selected.join("\n"))
        };

        Ok(CommandOutcome::Immediate {
            json_only,
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
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut include_numbers = false;
        let mut context = 0usize;
        let mut ignore_case = false;
        let mut fixed_strings = false;
        let mut last = 2000usize;
        let mut pattern: Option<String> = None;
        let mut since: Option<String> = None;
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
            } else if arg == "-n" {
                include_numbers = true;
            } else if arg == "-C" || arg == "--context" {
                i += 1;
                context = parse_usize_arg(args.get(i), "--context")?;
            } else if let Some(value) = opt_value(arg, "--context") {
                context = parse_usize_literal(&value, "--context")?;
            } else if arg == "-i" || arg == "--ignore-case" {
                ignore_case = true;
            } else if arg == "-F" || arg == "--fixed-strings" {
                fixed_strings = true;
            } else if arg == "--last" {
                i += 1;
                last = parse_usize_arg(args.get(i), "--last")?;
            } else if let Some(value) = opt_value(arg, "--last") {
                last = parse_usize_literal(&value, "--last")?;
            } else if arg == "--since" {
                i += 1;
                since = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --since"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--since") {
                since = Some(value);
            } else if arg == "--token" {
                i += 1;
                token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                token = Some(value);
            } else if arg.starts_with('-') {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    format!("Unknown flag for grep: {arg}"),
                ));
            } else if pattern.is_none() {
                pattern = Some(arg.clone());
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    "grep takes a single pattern argument",
                ));
            }
            i += 1;
        }

        let pattern = pattern
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "grep requires a <pattern> argument"))?;
        let regex = compile_search_regex(&pattern, ignore_case, fixed_strings)?;
        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "grep requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "grep")?;

        let pane_lines = self.read_pane_lines(pane_id, true)?;
        let since_line_count = if let Some(token) = since {
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

        let (candidate_lines, offset) = if all_lines.len() <= last {
            (all_lines, 0usize)
        } else {
            let start = all_lines.len() - last;
            (all_lines[start..].to_vec(), start)
        };

        let matches = build_grep_matches(&candidate_lines, &regex, context, offset);

        let human = if json_only {
            None
        } else {
            let mut lines = Vec::new();
            for m in &matches {
                if include_numbers {
                    lines.push(format!("{}:{}", m.line_number, m.line));
                } else {
                    lines.push(m.line.clone());
                }
            }
            Some(lines.join("\n"))
        };

        Ok(CommandOutcome::Immediate {
            json_only,
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
        args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut pane_id: Option<String> = None;
        let mut regex_pattern: Option<String> = None;
        let mut last_n = 1usize;
        let mut timeout = Duration::from_secs(30);
        let mut interval = Duration::from_millis(200);
        let mut window = 400usize;
        let mut mode = WaitMode::Any;
        let mut since: Option<String> = None;
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
            } else if arg == "--regex" {
                i += 1;
                regex_pattern = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --regex"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--regex") {
                regex_pattern = Some(value);
            } else if arg == "--last" {
                i += 1;
                last_n = parse_usize_arg(args.get(i), "--last")?;
            } else if let Some(value) = opt_value(arg, "--last") {
                last_n = parse_usize_literal(&value, "--last")?;
            } else if arg == "--timeout" {
                i += 1;
                timeout = parse_duration(args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --timeout")
                })?)?;
            } else if let Some(value) = opt_value(arg, "--timeout") {
                timeout = parse_duration(&value)?;
            } else if arg == "--interval" {
                i += 1;
                interval = parse_duration(args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --interval")
                })?)?;
            } else if let Some(value) = opt_value(arg, "--interval") {
                interval = parse_duration(&value)?;
            } else if arg == "--window" {
                i += 1;
                window = parse_usize_arg(args.get(i), "--window")?;
            } else if let Some(value) = opt_value(arg, "--window") {
                window = parse_usize_literal(&value, "--window")?;
            } else if arg == "--mode" {
                i += 1;
                let mode_string = args
                    .get(i)
                    .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --mode"))?;
                mode = parse_wait_mode(mode_string)?;
            } else if let Some(value) = opt_value(arg, "--mode") {
                mode = parse_wait_mode(&value)?;
            } else if arg == "--since" {
                i += 1;
                since = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --since"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--since") {
                since = Some(value);
            } else if arg == "--token" {
                i += 1;
                token = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --token"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--token") {
                token = Some(value);
            } else if arg.starts_with('-') {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    format!("Unknown flag for wait: {arg}"),
                ));
            } else if regex_pattern.is_none() {
                regex_pattern = Some(arg.clone());
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    "wait accepts a single regex positional argument",
                ));
            }
            i += 1;
        }

        let regex_pattern = regex_pattern
            .ok_or_else(|| ApiError::new("INVALID_ARGS", "wait requires --regex <pattern>"))?;
        let regex = compile_search_regex(&regex_pattern, false, false)?;

        let pane_id =
            pane_id.ok_or_else(|| ApiError::new("INVALID_ARGS", "wait requires --pane <name>"))?;
        let pane_id = self.resolve_pane_name(&pane_id)?;
        self.ensure_token_can_access_pane(token.as_deref(), pane_id, "wait")?;
        let since_line_count = if let Some(token) = since {
            Some(self.resolve_since_line_count(&token, pane_id)?)
        } else {
            None
        };

        if let Some(matched_lines) =
            self.wait_condition_matches(pane_id, &regex, last_n, window, &mode, since_line_count)?
        {
            return Ok(CommandOutcome::Immediate {
                json_only,
                human: if json_only {
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
                last_n,
                window,
                mode,
                deadline: Instant::now() + timeout,
                next_poll_at: Instant::now() + interval,
                interval,
                json_only,
                since_line_count,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }
}
