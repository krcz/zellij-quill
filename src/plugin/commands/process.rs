use super::*;
use args::parse_args;
use clap::Parser;

fn parse_env_pair_str(s: &str) -> Result<(String, String), String> {
    let Some((key, val)) = s.split_once('=') else {
        return Err(format!("Invalid env pair, expected KEY=VAL: {s}"));
    };
    Ok((key.to_string(), val.to_string()))
}

#[derive(Parser)]
#[clap(no_binary_name = true, trailing_var_arg = true)]
struct ExecArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    cwd: Option<String>,
    #[clap(long = "env", value_parser = parse_env_pair_str, number_of_values = 1)]
    env_vars: Vec<(String, String)>,
    #[clap(long, default_value = "60")]
    timeout: String,
    #[clap(long)]
    token: Option<String>,
    #[clap(allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Parser)]
#[clap(no_binary_name = true, trailing_var_arg = true)]
struct SpawnArgs {
    #[clap(long)]
    json: bool,
    #[clap(long, arg_enum, default_value = "terminal")]
    kind: SpawnKind,
    #[clap(long = "where", arg_enum, default_value = "tiled")]
    where_to: SpawnWhere,
    #[clap(long)]
    cwd: Option<String>,
    #[clap(long)]
    name: Option<String>,
    #[clap(long, default_value = "2")]
    wait: String,
    #[clap(long)]
    token: Option<String>,
    #[clap(allow_hyphen_values = true)]
    command: Vec<String>,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_exec(
        &mut self,
        raw_args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<ExecArgs>(raw_args)?;
        let timeout = parse_duration(&args.timeout)?;

        if args.command.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "exec requires a command after --",
            ));
        }

        self.validate_auth(args.token.as_deref())?;

        let pipe_id = pipe_id.ok_or_else(|| {
            ApiError::new(
                "PIPE_REQUIRED",
                "exec requires a CLI pipe source for asynchronous response",
            )
        })?;

        let cwd = args.cwd.map(PathBuf::from);
        let env_vars: BTreeMap<String, String> = args.env_vars.into_iter().collect();

        let request_id = self.next_request_id();
        let mut context = BTreeMap::new();
        context.insert(REQUEST_ID_KEY.to_string(), request_id.clone());

        let command_refs: Vec<&str> = args.command.iter().map(String::as_str).collect();
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
                command: args.command,
                deadline: Instant::now() + timeout,
                json_only: args.json,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }

    pub(in crate::plugin) fn cmd_spawn(
        &mut self,
        raw_args: &[String],
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<SpawnArgs>(raw_args)?;
        let wait_for = parse_duration(&args.wait)?;

        let actor_token = self.ensure_token_can_create_from_origin(args.token.as_deref())?;

        if matches!(args.kind, SpawnKind::Command) && args.command.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "spawn --kind command requires a command after --",
            ));
        }

        if matches!(args.kind, SpawnKind::Terminal)
            && matches!(args.where_to, SpawnWhere::Background)
        {
            return Err(ApiError::new(
                "INVALID_ARGS",
                "spawn --kind terminal does not support --where background",
            ));
        }

        let cwd = args.cwd.map(PathBuf::from);
        let baseline = self.collect_known_pane_ids();

        let expected_pane_id = match args.kind {
            SpawnKind::Terminal => {
                let target_path = cwd.clone().unwrap_or_else(|| PathBuf::from("."));
                match args.where_to {
                    SpawnWhere::Tiled => open_terminal(target_path),
                    SpawnWhere::Floating => open_terminal_floating(target_path, None),
                    SpawnWhere::NearPlugin => open_terminal_near_plugin(target_path),
                    SpawnWhere::InPlace => open_terminal_in_place(target_path),
                    SpawnWhere::Background => None,
                }
            }
            SpawnKind::Command => {
                let path = PathBuf::from(&args.command[0]);
                let cmd_args: Vec<String> = args.command[1..].to_vec();
                let mut cmd = CommandToRun::new_with_args(path, cmd_args);
                cmd.cwd = cwd.clone();
                let context = BTreeMap::new();
                match args.where_to {
                    SpawnWhere::Tiled => open_command_pane(cmd, context),
                    SpawnWhere::Floating => open_command_pane_floating(cmd, None, context),
                    SpawnWhere::NearPlugin => open_command_pane_near_plugin(cmd, context),
                    SpawnWhere::InPlace => open_command_pane_in_place(cmd, context),
                    SpawnWhere::Background => open_command_pane_background(cmd, context),
                }
            }
        };

        if let (Some(name), Some(pane_id)) = (&args.name, expected_pane_id) {
            rename_pane_with_id(pane_id, name);
        }

        if let (Some(token), Some(pane_id)) = (&actor_token, expected_pane_id) {
            self.grant_pane_permission(token, pane_id);
        }

        if wait_for.is_zero() {
            return Ok(CommandOutcome::Immediate {
                json_only: args.json,
                human: if args.json {
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
                json_only: args.json,
                token: actor_token,
            }),
        );
        self.schedule_job_timer();

        Ok(CommandOutcome::Async)
    }
}
