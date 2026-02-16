use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_permit(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;

        for arg in args {
            if arg == "--json" {
                json_only = true;
            } else {
                return Err(
                    ApiError::new("PERMIT_DISABLED", "CLI permission grants are disabled")
                        .hint("Use the quill permission prompt UI to approve or deny requests."),
                );
            }
        }

        let mut pending_requests: Vec<PanePermissionRequest> =
            self.pending_permission_requests.values().cloned().collect();
        pending_requests.sort_by_key(|request| request.created_at_ms);

        let pending = pending_requests
            .into_iter()
            .map(|request| {
                json!({
                    "request_id": request.request_id,
                    "token": request.token,
                    "action": request.action,
                    "pane_id": pane_id_to_string(request.pane_id),
                    "origin_pane_id": request.origin_pane_id.map(pane_id_to_string),
                    "created_at_ms": request.created_at_ms,
                })
            })
            .collect::<Vec<Value>>();

        Ok(CommandOutcome::Immediate {
            json_only,
            human: if json_only {
                None
            } else {
                Some(format!(
                    "pending permission requests: {} (approve in quill UI)",
                    pending.len()
                ))
            },
            payload: json!({
                "ok": true,
                "ui_approval": true,
                "pending_count": pending.len(),
                "pending": pending,
            }),
        })
    }
}
