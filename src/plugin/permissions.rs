use crate::error::ApiError;
use crate::types::*;
use crate::util::unix_time_ms;
use serde_json::json;
use std::collections::BTreeMap;
use zellij_tile::prelude::*;

impl QuillPlugin {
    pub(super) fn capture_pipe_origin_pane_id(&mut self, pipe_message: &PipeMessage) {
        let value = pipe_message
            .args
            .get(ORIGIN_PANE_ENV_VAR)
            .or_else(|| pipe_message.args.get("zellij_pane_id"))
            .or_else(|| pipe_message.args.get("pane_id"))
            .cloned()
            .or_else(|| std::env::var(ORIGIN_PANE_ENV_VAR).ok());
        self.current_pipe_origin_pane_id = value.and_then(|v| parse_origin_pane_id(&v));
    }

    pub(super) fn expected_auth_token(&mut self) -> Option<String> {
        self.configured_token
            .clone()
            .or_else(|| self.session_env_token())
            .or_else(|| self.token_from_session_env.clone())
    }

    pub(super) fn actor_token(&mut self, provided_token: Option<&str>) -> Option<String> {
        match provided_token {
            Some(token) if !token.trim().is_empty() => Some(token.to_string()),
            _ => self.expected_auth_token(),
        }
    }

    pub(super) fn has_pane_permission(&self, token: &str, pane_id: PaneId) -> bool {
        self.token_pane_permissions
            .get(token)
            .map(|panes| panes.contains(&pane_id))
            .unwrap_or(false)
    }

    pub(super) fn grant_pane_permission(&mut self, token: &str, pane_id: PaneId) {
        let panes = self
            .token_pane_permissions
            .entry(token.to_string())
            .or_default();
        if !panes.contains(&pane_id) {
            panes.push(pane_id);
        }
    }

    pub(super) fn pane_permissions_for(&self, token: &str) -> Vec<PaneId> {
        self.token_pane_permissions
            .get(token)
            .cloned()
            .unwrap_or_default()
    }

    fn next_permission_request_id(&mut self) -> String {
        self.next_permission_request_id += 1;
        format!(
            "perm-{}-{}",
            unix_time_ms(),
            self.next_permission_request_id
        )
    }

    fn current_origin_pane(&self) -> Option<PaneId> {
        self.current_pipe_origin_pane_id
    }

    fn write_permission_dialog(&self, origin_pane_id: Option<PaneId>, message: &str) {
        if let Some(origin_pane_id) = origin_pane_id {
            let rendered = format!("\n[quill permission]\n{message}\n");
            write_chars_to_pane_id(&rendered, origin_pane_id);
        }
    }

    fn queue_permission_request(
        &mut self,
        token: &str,
        pane_id: PaneId,
        action: &str,
        origin_pane_id: Option<PaneId>,
    ) -> PanePermissionRequest {
        let request = PanePermissionRequest {
            request_id: self.next_permission_request_id(),
            token: token.to_string(),
            pane_id,
            action: action.to_string(),
            origin_pane_id,
            created_at_ms: unix_time_ms(),
        };
        self.pending_permission_requests
            .insert(request.request_id.clone(), request.clone());
        request
    }

    fn pending_permission_request(
        &self,
        token: &str,
        pane_id: PaneId,
        action: &str,
    ) -> Option<PanePermissionRequest> {
        self.pending_permission_requests
            .values()
            .find(|request| {
                request.token == token && request.pane_id == pane_id && request.action == action
            })
            .cloned()
    }

    fn has_prompt_for_request(&self, request_id: &str) -> bool {
        self.permission_prompt_panes
            .values()
            .any(|existing| existing == request_id)
    }

    fn open_permission_prompt(&mut self, request: &PanePermissionRequest) -> bool {
        let script = r#"printf '\n[quill permission]\nToken: %s\nPane: %s\nAction: %s\n\nAllow this request? [y/N]: ' "$1" "$2" "$3"; read answer; case "$answer" in y|Y|yes|YES) exit 0;; *) exit 1;; esac"#;
        let args = vec![
            "-c".to_string(),
            script.to_string(),
            "quill-approve".to_string(),
            request.token.clone(),
            pane_id_to_string(request.pane_id),
            request.action.clone(),
        ];
        let command = CommandToRun::new_with_args("sh", args);

        let mut context = BTreeMap::new();
        context.insert(
            PERMISSION_REQUEST_CONTEXT_KEY.to_string(),
            request.request_id.clone(),
        );

        let opened = open_command_pane_floating(command.clone(), None, context.clone())
            .or_else(|| open_command_pane(command, context));

        if let Some(PaneId::Terminal(prompt_pane_id)) = opened {
            self.permission_prompt_panes
                .insert(prompt_pane_id, request.request_id.clone());
            rename_pane_with_id(PaneId::Terminal(prompt_pane_id), "quill-approval");
            return true;
        }

        false
    }

    fn permission_required_error(&self, request: &PanePermissionRequest) -> ApiError {
        ApiError::new(
            "PERMISSION_REQUIRED",
            "Token is not allowed to access the requested pane",
        )
        .hint("Approve in the quill permission prompt, then retry the command.")
        .meta(json!({
            "request_id": request.request_id,
            "token": request.token,
            "action": request.action,
            "pane_id": pane_id_to_string(request.pane_id),
            "origin_pane_id": request.origin_pane_id.map(pane_id_to_string),
            "approval": "ui_prompt",
        }))
    }

    pub(super) fn on_command_pane_exited(
        &mut self,
        terminal_pane_id: u32,
        exit_code: Option<i32>,
        context: BTreeMap<String, String>,
    ) {
        let request_id = context
            .get(PERMISSION_REQUEST_CONTEXT_KEY)
            .cloned()
            .or_else(|| self.permission_prompt_panes.get(&terminal_pane_id).cloned());

        self.permission_prompt_panes.remove(&terminal_pane_id);

        let Some(request_id) = request_id else {
            return;
        };

        let Some(request) = self.pending_permission_requests.remove(&request_id) else {
            return;
        };

        if exit_code == Some(0) {
            self.grant_pane_permission(&request.token, request.pane_id);
            self.write_permission_dialog(
                request.origin_pane_id,
                &format!(
                    "Approved token `{}` for {}.",
                    request.token,
                    pane_id_to_string(request.pane_id)
                ),
            );
            self.continue_pending_permission_command(&request.request_id);
        } else {
            self.write_permission_dialog(
                request.origin_pane_id,
                &format!(
                    "Denied token `{}` for {}.",
                    request.token,
                    pane_id_to_string(request.pane_id)
                ),
            );
            self.deny_pending_permission_command(&request.request_id);
        }

        close_terminal_pane(terminal_pane_id);
    }

    pub(super) fn ensure_token_can_access_pane(
        &mut self,
        provided_token: Option<&str>,
        pane_id: PaneId,
        action: &str,
    ) -> Result<Option<String>, ApiError> {
        if !self.enforce_pane_permissions {
            return Ok(self.actor_token(provided_token));
        }

        let token = match self.actor_token(provided_token) {
            Some(token) => token,
            None => {
                let origin_pane_id = self.current_origin_pane();
                let dialog = format!(
                    "Action `{action}` requires a token.\nSet {} or pass --token and retry.",
                    SESSION_TOKEN_ENV_VAR
                );
                self.write_permission_dialog(origin_pane_id, &dialog);
                return Err(ApiError::new(
                    "UNAUTHORIZED",
                    "Pane permissions are enabled and require a token",
                )
                .hint(format!(
                    "Provide --token or set {} and retry.",
                    SESSION_TOKEN_ENV_VAR
                )));
            }
        };

        if self.has_pane_permission(&token, pane_id) {
            return Ok(Some(token));
        }

        let request =
            if let Some(existing) = self.pending_permission_request(&token, pane_id, action) {
                existing
            } else {
                let origin_pane_id = self.current_origin_pane();
                self.queue_permission_request(&token, pane_id, action, origin_pane_id)
            };

        if !self.has_prompt_for_request(&request.request_id) {
            if !self.open_permission_prompt(&request) {
                self.write_permission_dialog(
                    request.origin_pane_id,
                    "Failed to open quill approval prompt pane.",
                );
            }
        }

        let dialog = format!(
            "Action `{}` requires access to {} for token `{}`.\nApprove in the quill permission prompt.",
            request.action,
            pane_id_to_string(request.pane_id),
            request.token,
        );
        self.write_permission_dialog(request.origin_pane_id, &dialog);

        Err(self.permission_required_error(&request))
    }

    pub(super) fn ensure_token_can_create_from_origin(
        &mut self,
        provided_token: Option<&str>,
        action: &str,
    ) -> Result<Option<String>, ApiError> {
        if !self.enforce_pane_permissions {
            return Ok(self.actor_token(provided_token));
        }

        let origin_pane_id = self.current_origin_pane().ok_or_else(|| {
            ApiError::new(
                "PERMISSION_REQUIRED",
                "Cannot determine request origin pane for permission prompt",
            )
            .hint(format!(
                "Pass --args {ORIGIN_PANE_ENV_VAR}=$ZELLIJ_PANE_ID or disable pane permissions"
            ))
        })?;

        self.ensure_token_can_access_pane(provided_token, origin_pane_id, action)
    }
}

fn parse_origin_pane_id(value: &str) -> Option<PaneId> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(id) = value.strip_prefix("id:") {
        return id.trim().parse::<u32>().ok().map(PaneId::Terminal);
    }

    if let Some(id) = value.strip_prefix("terminal_") {
        return id.trim().parse::<u32>().ok().map(PaneId::Terminal);
    }

    if let Some(id) = value.strip_prefix("plugin_") {
        return id.trim().parse::<u32>().ok().map(PaneId::Plugin);
    }

    value.parse::<u32>().ok().map(PaneId::Terminal)
}
