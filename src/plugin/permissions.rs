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
        origin_pane_id: Option<PaneId>,
    ) -> PanePermissionRequest {
        let request = PanePermissionRequest {
            request_id: self.next_permission_request_id(),
            token: token.to_string(),
            pane_id,
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
    ) -> Option<PanePermissionRequest> {
        self.pending_permission_requests
            .values()
            .find(|request| request.token == token && request.pane_id == pane_id)
            .cloned()
    }

    fn has_prompt_for_request(&self, request_id: &str) -> bool {
        self.permission_prompt_panes
            .values()
            .any(|existing| existing == request_id)
    }

    fn open_permission_prompt(&mut self, request: &PanePermissionRequest) -> bool {
        let script = r#"printf '\n[quill permission]\nToken: %s\nPane: %s\n\nAllow this request? [y/N]: ' "$1" "$2"; read answer; case "$answer" in y|Y|yes|YES) exit 0;; *) exit 1;; esac"#;
        let args = vec![
            "-c".to_string(),
            script.to_string(),
            "quill-approve".to_string(),
            request.token.clone(),
            pane_id_to_string(request.pane_id),
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
    ) -> Result<Option<String>, ApiError> {
        let Some(token) = self.resolve_actor_token_for_pane_access(provided_token)? else {
            return Ok(None);
        };

        self.ensure_specific_token_can_access_pane(token, pane_id)
            .map(Some)
    }

    pub(super) fn ensure_token_can_create_from_origin(
        &mut self,
        provided_token: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        let Some(token) = self.resolve_actor_token_for_pane_access(provided_token)? else {
            return Ok(None);
        };

        if !self.enforce_pane_permissions {
            return Ok(Some(token));
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

        self.ensure_specific_token_can_access_pane(token, origin_pane_id)
            .map(Some)
    }

    fn resolve_actor_token_for_pane_access(
        &mut self,
        provided_token: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        self.validate_auth(provided_token)?;

        if !self.enforce_pane_permissions {
            return Ok(self.actor_token(provided_token));
        }

        match self.actor_token(provided_token) {
            Some(token) => Ok(Some(token)),
            None => {
                let origin_pane_id = self.current_origin_pane();
                let dialog = format!(
                    "Pane access requires a token.\nSet {} or pass --token and retry.",
                    SESSION_TOKEN_ENV_VAR
                );
                self.write_permission_dialog(origin_pane_id, &dialog);
                Err(ApiError::new(
                    "UNAUTHORIZED",
                    "Pane permissions are enabled and require a token",
                )
                .hint(format!(
                    "Provide --token or set {} and retry.",
                    SESSION_TOKEN_ENV_VAR
                )))
            }
        }
    }

    fn ensure_specific_token_can_access_pane(
        &mut self,
        token: String,
        pane_id: PaneId,
    ) -> Result<String, ApiError> {
        if !self.enforce_pane_permissions {
            return Ok(token);
        }

        if self.has_pane_permission(&token, pane_id) {
            return Ok(token);
        }

        let request = if let Some(existing) = self.pending_permission_request(&token, pane_id) {
            existing
        } else {
            let origin_pane_id = self.current_origin_pane();
            self.queue_permission_request(&token, pane_id, origin_pane_id)
        };

        let prompt_available = self.has_prompt_for_request(&request.request_id)
            || self.open_permission_prompt(&request);
        if !prompt_available {
            self.pending_permission_requests.remove(&request.request_id);
            self.write_permission_dialog(
                request.origin_pane_id,
                &format!(
                    "Access to {} requested for token `{}`.\nCould not open the quill approval prompt pane.",
                    pane_id_to_string(request.pane_id),
                    request.token
                ),
            );
            return Err(ApiError::new(
                "PERMISSION_UNAVAILABLE",
                "Unable to open quill approval prompt pane",
            )
            .hint("Retry the command or disable pane permissions."));
        }

        let dialog = format!(
            "Access to {} requested for token `{}`.\nApprove in the quill permission prompt.",
            pane_id_to_string(request.pane_id),
            request.token,
        );
        self.write_permission_dialog(request.origin_pane_id, &dialog);

        Err(self.permission_required_error(&request))
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
