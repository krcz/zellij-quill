use crate::error::{ApiError, error_payload};
use crate::types::*;
use crate::util::unix_time_ms;
use serde_json::Value;
use std::collections::HashSet;
use zellij_tile::prelude::*;

impl QuillPlugin {
    pub(super) fn send_response(
        &mut self,
        pipe_id: Option<&str>,
        json_only: bool,
        human: Option<&str>,
        payload: Value,
        unblock_after: bool,
    ) {
        let output = if json_only {
            payload.to_string()
        } else {
            let mut parts = Vec::new();
            if let Some(human) = human {
                if !human.is_empty() {
                    parts.push(human.to_string());
                }
            }
            parts.push(format!("@@json {}", payload));
            parts.join("\n")
        };

        if let Some(pipe_id) = pipe_id {
            cli_pipe_output(pipe_id, &output);
            if unblock_after {
                self.unblock_pipe(pipe_id);
            }
        }
    }

    pub(super) fn send_error(&mut self, pipe_id: Option<&str>, json_only: bool, error: ApiError) {
        self.send_error_with_unblock(pipe_id, json_only, error, false);
    }

    pub(super) fn send_error_with_unblock(
        &mut self,
        pipe_id: Option<&str>,
        json_only: bool,
        error: ApiError,
        unblock_after: bool,
    ) {
        self.send_response(
            pipe_id,
            json_only,
            None,
            error_payload(error),
            unblock_after,
        );
    }

    pub(super) fn block_pipe(&self, pipe_id: &str) {
        block_cli_pipe_input(pipe_id);
    }

    pub(super) fn unblock_pipe(&self, pipe_id: &str) {
        unblock_cli_pipe_input(pipe_id);
    }

    pub(super) fn schedule_job_timer(&self) {
        if !self.jobs.is_empty() {
            set_timeout(DEFAULT_TIMER_SECS);
        }
    }

    pub(super) fn collect_known_pane_ids(&self) -> HashSet<PaneId> {
        let mut set = HashSet::new();
        if let Some(manifest) = &self.pane_manifest {
            for panes in manifest.panes.values() {
                for pane in panes {
                    set.insert(pane_id_from_info(pane));
                }
            }
        }
        set
    }

    pub(super) fn touch_recent_pane(&mut self, pane_id: PaneId) {
        self.recent_terminal_panes
            .retain(|candidate| candidate != &pane_id);
        self.recent_terminal_panes.push_front(pane_id);
        while self.recent_terminal_panes.len() > MAX_RECENT_TERMINAL_PANES {
            self.recent_terminal_panes.pop_back();
        }
    }

    fn refresh_focus_from_manifest(&mut self) {
        let Some(manifest) = self.pane_manifest.as_ref() else {
            return;
        };

        if let Some(tab_index) = self.focused_tab_index {
            if let Some(focused) = get_focused_pane(tab_index, manifest) {
                let pane_id = pane_id_from_info(&focused);
                self.focused_pane_id = Some(pane_id);
                if matches!(pane_id, PaneId::Terminal(_)) {
                    self.touch_recent_pane(pane_id);
                }
                return;
            }
        }

        for (tab_index, panes) in &manifest.panes {
            if let Some(focused) = panes.iter().find(|pane| pane.is_focused) {
                self.focused_tab_index = Some(*tab_index);
                let pane_id = pane_id_from_info(focused);
                self.focused_pane_id = Some(pane_id);
                if matches!(pane_id, PaneId::Terminal(_)) {
                    self.touch_recent_pane(pane_id);
                }
                return;
            }
        }
    }

    pub(super) fn refresh_focus_from_host(&mut self) {
        self.refresh_focus_from_manifest();
    }

    pub(super) fn current_focus_info(&self) -> Option<(usize, PaneId)> {
        if let (Some(tab), Some(pane_id)) = (self.focused_tab_index, self.focused_pane_id) {
            return Some((tab, pane_id));
        }

        let manifest = self.pane_manifest.as_ref()?;

        if let Some(tab_index) = self.focused_tab_index {
            if let Some(focused) = get_focused_pane(tab_index, manifest) {
                return Some((tab_index, pane_id_from_info(&focused)));
            }
        }

        for (tab_index, panes) in &manifest.panes {
            if let Some(focused) = panes.iter().find(|pane| pane.is_focused) {
                return Some((*tab_index, pane_id_from_info(focused)));
            }
        }

        None
    }

    pub(super) fn next_job_id(&mut self) -> String {
        self.next_job_id += 1;
        format!("job-{}", self.next_job_id)
    }

    pub(super) fn next_request_id(&mut self) -> String {
        self.next_request_id += 1;
        format!("req-{}-{}", unix_time_ms(), self.next_request_id)
    }

    pub(super) fn next_mark_token(&mut self) -> String {
        self.next_mark_id += 1;
        format!("mark-{}-{}", unix_time_ms(), self.next_mark_id)
    }
}
