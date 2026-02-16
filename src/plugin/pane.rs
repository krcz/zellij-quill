use crate::error::ApiError;
use crate::types::*;
use crate::util::strip_ansi_sequences;
use zellij_tile::prelude::*;

impl QuillPlugin {
    pub(super) fn resolve_pane_name(&self, pane_name: &str) -> Result<PaneId, ApiError> {
        let pane_name = pane_name.trim();
        if pane_name.is_empty() {
            return Err(ApiError::new("INVALID_ARGS", "Pane name cannot be empty"));
        }

        let manifest = self
            .pane_manifest
            .as_ref()
            .ok_or_else(|| ApiError::new("NO_PANE_STATE", "Pane state is not available yet"))?;

        let mut matches: Vec<PaneInfo> = Vec::new();
        for panes in manifest.panes.values() {
            for pane in panes {
                if pane.is_plugin {
                    continue;
                }
                if pane.title == pane_name {
                    matches.push(pane.clone());
                }
            }
        }

        if matches.is_empty() {
            return Err(ApiError::new(
                "PANE_NOT_FOUND",
                format!("No terminal pane named `{pane_name}`"),
            ));
        }

        if matches.len() > 1 {
            return Err(ApiError::new(
                "AMBIGUOUS_PANE_NAME",
                format!("More than one terminal pane is named `{pane_name}`"),
            )
            .hint("Rename panes to unique names and retry."));
        }

        Ok(pane_id_from_info(&matches.remove(0)))
    }

    pub(super) fn read_pane_lines(
        &self,
        pane_id: PaneId,
        strip_ansi: bool,
    ) -> Result<PaneLines, ApiError> {
        let pane_contents = get_pane_scrollback(pane_id, true).map_err(|e| {
            ApiError::new(
                "SCROLLBACK_ERROR",
                format!("Failed to read pane scrollback: {e}"),
            )
        })?;

        let mut all = Vec::new();
        all.extend(pane_contents.lines_above_viewport.clone());
        let viewport_start = all.len();
        all.extend(pane_contents.viewport.clone());
        let viewport_end_exclusive = all.len();
        all.extend(pane_contents.lines_below_viewport.clone());

        if strip_ansi {
            all = all
                .into_iter()
                .map(|line| strip_ansi_sequences(&line))
                .collect();
        }

        Ok(PaneLines {
            all,
            viewport_start,
            viewport_end_exclusive,
        })
    }

    pub(super) fn session_env_token(&mut self) -> Option<String> {
        if !self.session_env_permission_granted {
            return self.token_from_session_env.clone();
        }

        let token = get_session_environment_variables()
            .get(SESSION_TOKEN_ENV_VAR)
            .cloned()
            .filter(|value| !value.trim().is_empty());

        if let Some(value) = token.clone() {
            self.token_from_session_env = Some(value);
        }

        token
    }

    pub(super) fn validate_auth(&mut self, provided_token: Option<&str>) -> Result<(), ApiError> {
        if !self.require_token {
            return Ok(());
        }

        let expected = self.expected_auth_token().ok_or_else(|| {
            ApiError::new(
                "UNAUTHORIZED",
                "Token required but no configured token is available",
            )
            .hint(format!(
                "Set plugin config `token=...`, export {SESSION_TOKEN_ENV_VAR}, or run `token`.",
            ))
        })?;

        match provided_token {
            Some(token) if token == expected => Ok(()),
            _ => Err(ApiError::new("UNAUTHORIZED", "Missing or invalid token")),
        }
    }

    pub(super) fn resolve_since_line_count(
        &self,
        token: &str,
        pane_id: PaneId,
    ) -> Result<usize, ApiError> {
        let mark = self.marks.get(token).ok_or_else(|| {
            ApiError::new("INVALID_MARK", "Unknown --since token")
                .hint("Create a token with `mark --pane ...`")
        })?;

        if mark.pane_id != pane_id {
            return Err(ApiError::new(
                "INVALID_MARK",
                "--since token belongs to a different pane",
            ));
        }
        Ok(mark.line_count)
    }
}
