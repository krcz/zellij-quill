use crate::error::ApiError;
use crate::parsing::{parse_bool, parse_pipe_command};
use crate::types::{CommandOutcome, PendingPermissionCommand, PipeCommand, QuillPlugin};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;

impl ZellijPlugin for QuillPlugin {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.require_token = parse_bool(configuration.get("require_token").map(String::as_str));
        self.enforce_pane_permissions = match configuration
            .get("enable_pane_permissions")
            .map(String::as_str)
        {
            Some(value) => parse_bool(Some(value)),
            None => true,
        };
        self.configured_token = configuration.get("token").cloned();

        subscribe(&[
            EventType::PaneUpdate,
            EventType::TabUpdate,
            EventType::Timer,
            EventType::RunCommandResult,
            EventType::CommandPaneExited,
            EventType::PermissionRequestResult,
            EventType::PaneClosed,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::ReadPaneContents,
            PermissionType::WriteToStdin,
            PermissionType::RunCommands,
            PermissionType::ReadCliPipes,
            PermissionType::OpenTerminalsOrPlugins,
            PermissionType::ReadSessionEnvironmentVariables,
        ]);

        self.refresh_focus_from_host();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PaneUpdate(pane_manifest) => {
                self.on_pane_update(pane_manifest);
            }
            Event::TabUpdate(tabs) => {
                self.focused_tab_index = tabs.iter().find(|t| t.active).map(|t| t.position);
                self.refresh_focus_from_host();
            }
            Event::RunCommandResult(exit_code, stdout, stderr, context) => {
                self.on_run_command_result(exit_code, stdout, stderr, context);
            }
            Event::CommandPaneExited(terminal_pane_id, exit_code, context) => {
                self.on_command_pane_exited(terminal_pane_id, exit_code, context);
            }
            Event::Timer(_) => {
                self.poll_jobs();
            }
            Event::PermissionRequestResult(status) => match status {
                PermissionStatus::Granted => {
                    self.permissions_granted = true;
                    self.permissions_denied = false;
                    self.session_env_permission_granted = true;
                    self.session_env_token();
                    self.refresh_focus_from_host();
                }
                PermissionStatus::Denied => {
                    self.permissions_granted = false;
                    self.permissions_denied = true;
                    self.session_env_permission_granted = false;
                }
            },
            Event::PaneClosed(closed_pane_id) => {
                self.recent_terminal_panes
                    .retain(|pane_id| pane_id != &closed_pane_id);
            }
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(pipe_id) => Some(pipe_id.clone()),
            _ => None,
        };
        self.capture_pipe_origin_pane_id(&pipe_message);

        let parsed = match parse_pipe_command(&pipe_message) {
            Ok(parsed) => parsed,
            Err(e) => {
                self.send_error(pipe_id.as_deref(), false, e);
                self.current_pipe_origin_pane_id = None;
                return false;
            }
        };

        self.run_parsed_command(&parsed, pipe_id.as_deref(), false);
        self.current_pipe_origin_pane_id = None;
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {}
}

impl QuillPlugin {
    pub(super) fn run_parsed_command(
        &mut self,
        parsed: &PipeCommand,
        pipe_id: Option<&str>,
        unblock_immediate: bool,
    ) {
        let outcome = self.dispatch_command(parsed, pipe_id);
        match outcome {
            Ok(CommandOutcome::Immediate {
                json_only,
                human,
                payload,
            }) => {
                self.send_response(
                    pipe_id,
                    json_only,
                    human.as_deref(),
                    payload,
                    unblock_immediate,
                );
            }
            Ok(CommandOutcome::Async) => {}
            Err(err) => {
                if self.defer_until_permission_approved(parsed, pipe_id, &err) {
                    return;
                }
                self.send_error_with_unblock(pipe_id, false, err, unblock_immediate);
            }
        }
    }

    fn permission_request_id_from_error(error: &ApiError) -> Option<String> {
        if error.code != "PERMISSION_REQUIRED" {
            return None;
        }

        error
            .meta
            .as_ref()
            .and_then(|meta| meta.get("request_id"))
            .and_then(|request_id| request_id.as_str())
            .map(ToOwned::to_owned)
    }

    fn defer_until_permission_approved(
        &mut self,
        parsed: &PipeCommand,
        pipe_id: Option<&str>,
        error: &ApiError,
    ) -> bool {
        let Some(pipe_id) = pipe_id else {
            return false;
        };

        let Some(request_id) = Self::permission_request_id_from_error(error) else {
            return false;
        };

        self.block_pipe(pipe_id);
        self.pending_permission_commands.insert(
            request_id,
            PendingPermissionCommand {
                pipe_id: pipe_id.to_string(),
                parsed: parsed.clone(),
                origin_pane_id: self.current_pipe_origin_pane_id,
            },
        );
        true
    }

    pub(super) fn continue_pending_permission_command(&mut self, request_id: &str) {
        let Some(pending) = self.pending_permission_commands.remove(request_id) else {
            return;
        };

        let previous_origin = self.current_pipe_origin_pane_id;
        self.current_pipe_origin_pane_id = pending.origin_pane_id;
        self.run_parsed_command(&pending.parsed, Some(&pending.pipe_id), true);
        self.current_pipe_origin_pane_id = previous_origin;
    }

    pub(super) fn deny_pending_permission_command(&mut self, request_id: &str) {
        let Some(pending) = self.pending_permission_commands.remove(request_id) else {
            return;
        };

        self.send_error_with_unblock(
            Some(&pending.pipe_id),
            false,
            ApiError::new(
                "PERMISSION_DENIED",
                "Permission request was denied in UI prompt",
            ),
            true,
        );
    }

    pub(super) fn dispatch_command(
        &mut self,
        parsed: &PipeCommand,
        pipe_id: Option<&str>,
    ) -> Result<CommandOutcome, ApiError> {
        if self.permissions_denied {
            return Err(
                ApiError::new("PERMISSION_STATUS", "Plugin permissions were denied")
                    .hint("Allow requested plugin permissions in Zellij and reload the plugin."),
            );
        }

        match parsed.cmd.as_str() {
            "panes" => self.cmd_panes(&parsed.args),
            "send" => self.cmd_send(&parsed.args),
            "run" => self.cmd_run(&parsed.args, pipe_id),
            "tail" | "read" => self.cmd_tail(&parsed.args),
            "grep" => self.cmd_grep(&parsed.args),
            "wait" => self.cmd_wait(&parsed.args, pipe_id),
            "exec" => self.cmd_exec(&parsed.args, pipe_id),
            "spawn" => self.cmd_spawn(&parsed.args, pipe_id),
            "interrupt" => self.cmd_interrupt(&parsed.args),
            "mark" => self.cmd_mark(&parsed.args),
            "since" => self.cmd_since(&parsed.args),
            "token" => self.cmd_token(&parsed.args),
            "permit" => self.cmd_permit(&parsed.args),
            "help" => Ok(self.help_response(false)),
            unknown => Err(
                ApiError::new("UNKNOWN_COMMAND", format!("Unknown command: {unknown}"))
                    .hint("Use `help` for available commands."),
            ),
        }
    }
}
