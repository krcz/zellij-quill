use super::*;
use args::parse_args;
use clap::Parser;

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct PermitArgs {
    #[clap(long)]
    json: bool,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_permit(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<PermitArgs>(raw_args)?;
        let json_only = args.json;

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
