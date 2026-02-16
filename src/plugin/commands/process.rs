use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_exec(
        &mut self,
        args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut cwd: Option<PathBuf> = None;
        let mut env_vars: BTreeMap<String, String> = BTreeMap::new();
        let mut timeout = Duration::from_secs(60);
        let mut token: Option<String> = None;
        let mut command = Vec::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--json" {
                json_only = true;
            } else if arg == "--cwd" {
                i += 1;
                cwd = Some(PathBuf::from(args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --cwd")
                })?));
            } else if let Some(value) = opt_value(arg, "--cwd") {
                cwd = Some(PathBuf::from(value));
            } else if arg == "--env" {
                i += 1;
                let env = args
                    .get(i)
                    .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --env"))?;
                let (key, value) = parse_env_pair(env)?;
                env_vars.insert(key, value);
            } else if let Some(value) = opt_value(arg, "--env") {
                let (key, value) = parse_env_pair(&value)?;
                env_vars.insert(key, value);
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
            } else if arg == "--" {
                command.extend(args[i + 1..].iter().cloned());
                break;
            } else {
                command.push(arg.clone());
            }
            i += 1;
        }

        if command.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "exec requires a command after --",
            ));
        }

        self.validate_auth(token.as_deref())?;

        let pipe_id = pipe_id.ok_or_else(|| {
            ApiError::new(
                "PIPE_REQUIRED",
                "exec requires a CLI pipe source for asynchronous response",
            )
        })?;

        let request_id = self.next_request_id();
        let mut context = BTreeMap::new();
        context.insert(REQUEST_ID_KEY.to_string(), request_id.clone());

        let command_refs: Vec<&str> = command.iter().map(String::as_str).collect();
        if cwd.is_some() || !env_vars.is_empty() {
            run_command_with_env_variables_and_cwd(
                &command_refs,
                env_vars,
                cwd.unwrap_or_else(|| PathBuf::from(".")),
                context,
            );
        } else {
            run_command(&command_refs, context);
        }

        self.block_pipe(pipe_id);
        let job_id = self.next_job_id();
        self.exec_jobs_by_request_id
            .insert(request_id.clone(), job_id.clone());
        self.jobs.insert(
            job_id,
            Job::Exec(ExecJob {
                pipe_id: pipe_id.to_string(),
                request_id,
                command,
                deadline: Instant::now() + timeout,
                json_only,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }

    pub(in crate::plugin) fn cmd_spawn(
        &mut self,
        args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut kind = SpawnKind::Terminal;
        let mut where_to = SpawnWhere::Tiled;
        let mut cwd: Option<PathBuf> = None;
        let mut name: Option<String> = None;
        let mut wait_for = Duration::from_secs(2);
        let mut token: Option<String> = None;
        let mut command = Vec::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--json" {
                json_only = true;
            } else if arg == "--kind" {
                i += 1;
                kind =
                    parse_spawn_kind(args.get(i).ok_or_else(|| {
                        ApiError::new("INVALID_ARGS", "Missing value for --kind")
                    })?)?;
            } else if let Some(value) = opt_value(arg, "--kind") {
                kind = parse_spawn_kind(&value)?;
            } else if arg == "--where" {
                i += 1;
                where_to =
                    parse_spawn_where(args.get(i).ok_or_else(|| {
                        ApiError::new("INVALID_ARGS", "Missing value for --where")
                    })?)?;
            } else if let Some(value) = opt_value(arg, "--where") {
                where_to = parse_spawn_where(&value)?;
            } else if arg == "--cwd" {
                i += 1;
                cwd = Some(PathBuf::from(args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --cwd")
                })?));
            } else if let Some(value) = opt_value(arg, "--cwd") {
                cwd = Some(PathBuf::from(value));
            } else if arg == "--name" {
                i += 1;
                name = Some(
                    args.get(i)
                        .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --name"))?
                        .to_string(),
                );
            } else if let Some(value) = opt_value(arg, "--name") {
                name = Some(value);
            } else if arg == "--wait" {
                i += 1;
                wait_for =
                    parse_duration(args.get(i).ok_or_else(|| {
                        ApiError::new("INVALID_ARGS", "Missing value for --wait")
                    })?)?;
            } else if let Some(value) = opt_value(arg, "--wait") {
                wait_for = parse_duration(&value)?;
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
                command.extend(args[i + 1..].iter().cloned());
                break;
            } else {
                command.push(arg.clone());
            }
            i += 1;
        }

        self.validate_auth(token.as_deref())?;
        let actor_token = self.ensure_token_can_create_from_origin(token.as_deref(), "spawn")?;

        if matches!(kind, SpawnKind::Command) && command.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "spawn --kind command requires a command after --",
            ));
        }

        if matches!(kind, SpawnKind::Terminal) && matches!(where_to, SpawnWhere::Background) {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "spawn --kind terminal does not support --where background",
            ));
        }

        let baseline = self.collect_known_pane_ids();

        let expected_pane_id = match kind {
            SpawnKind::Terminal => {
                let target_path = cwd.clone().unwrap_or_else(|| PathBuf::from("."));
                match where_to {
                    SpawnWhere::Tiled => open_terminal(target_path),
                    SpawnWhere::Floating => open_terminal_floating(target_path, None),
                    SpawnWhere::NearPlugin => open_terminal_near_plugin(target_path),
                    SpawnWhere::InPlace => open_terminal_in_place(target_path),
                    SpawnWhere::Background => None,
                }
            }
            SpawnKind::Command => {
                let path = PathBuf::from(&command[0]);
                let args: Vec<String> = command[1..].to_vec();
                let mut cmd = CommandToRun::new_with_args(path, args);
                cmd.cwd = cwd.clone();
                let context = BTreeMap::new();
                match where_to {
                    SpawnWhere::Tiled => open_command_pane(cmd, context),
                    SpawnWhere::Floating => open_command_pane_floating(cmd, None, context),
                    SpawnWhere::NearPlugin => open_command_pane_near_plugin(cmd, context),
                    SpawnWhere::InPlace => open_command_pane_in_place(cmd, context),
                    SpawnWhere::Background => open_command_pane_background(cmd, context),
                }
            }
        };

        if let (Some(name), Some(pane_id)) = (&name, expected_pane_id) {
            rename_pane_with_id(pane_id, name);
        }

        if let (Some(token), Some(pane_id)) = (&actor_token, expected_pane_id) {
            self.grant_pane_permission(token, pane_id);
        }

        if wait_for.is_zero() {
            return Ok(CommandOutcome::Immediate {
                json_only,
                human: if json_only {
                    None
                } else {
                    Some(format!(
                        "spawned {}",
                        expected_pane_id
                            .map(pane_id_to_string)
                            .unwrap_or_else(|| "(unknown pane id)".to_string())
                    ))
                },
                payload: json!({
                    "ok": true,
                    "pane_id": expected_pane_id.map(pane_id_to_string),
                    "waited": false,
                }),
            });
        }

        let pipe_id = pipe_id.ok_or_else(|| {
            ApiError::new("PIPE_REQUIRED", "spawn --wait requires a CLI pipe source")
        })?;

        self.block_pipe(pipe_id);
        let job_id = self.next_job_id();
        self.jobs.insert(
            job_id,
            Job::Spawn(SpawnJob {
                pipe_id: pipe_id.to_string(),
                expected_pane_id,
                baseline_panes: baseline,
                deadline: Instant::now() + wait_for,
                json_only,
                token: actor_token,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }
}
