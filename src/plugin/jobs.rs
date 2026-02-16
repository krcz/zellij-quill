use crate::error::{ApiError, error_payload};
use crate::types::*;
use regex::Regex;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::*;

impl QuillPlugin {
    pub(super) fn on_pane_update(&mut self, pane_manifest: PaneManifest) {
        self.pane_manifest = Some(pane_manifest.clone());

        for panes in pane_manifest.panes.values() {
            for pane in panes {
                if !pane.is_plugin {
                    self.touch_recent_pane(PaneId::Terminal(pane.id));
                }
            }
        }

        self.refresh_focus_from_host();
        self.resolve_spawn_jobs();
    }

    pub(super) fn on_run_command_result(
        &mut self,
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) {
        let Some(request_id) = context.get(REQUEST_ID_KEY).cloned() else {
            return;
        };
        let Some(job_id) = self.exec_jobs_by_request_id.remove(&request_id) else {
            return;
        };
        let Some(Job::Exec(exec_job)) = self.jobs.remove(&job_id) else {
            return;
        };

        let stdout_text = String::from_utf8_lossy(&stdout).to_string();
        let stderr_text = String::from_utf8_lossy(&stderr).to_string();
        let payload = json!({
            "ok": true,
            "request_id": request_id,
            "command": exec_job.command,
            "exit_code": exit_code,
            "stdout": stdout_text,
            "stderr": stderr_text,
        });

        self.send_response(
            Some(&exec_job.pipe_id),
            exec_job.json_only,
            Some("exec completed"),
            payload,
            true,
        );
    }

    pub(super) fn resolve_spawn_jobs(&mut self) {
        let known_panes = self.collect_known_pane_ids();
        let mut finished = Vec::new();

        for (job_id, job) in &self.jobs {
            let Job::Spawn(spawn_job) = job else {
                continue;
            };

            if let Some(expected) = spawn_job.expected_pane_id {
                if known_panes.contains(&expected) {
                    finished.push((job_id.clone(), expected));
                }
            } else {
                let diff = known_panes
                    .difference(&spawn_job.baseline_panes)
                    .next()
                    .copied();
                if let Some(new_pane_id) = diff {
                    finished.push((job_id.clone(), new_pane_id));
                }
            }
        }

        for (job_id, pane_id) in finished {
            let Some(Job::Spawn(spawn_job)) = self.jobs.remove(&job_id) else {
                continue;
            };
            if let Some(token) = &spawn_job.token {
                self.grant_pane_permission(token, pane_id);
            }
            self.send_response(
                Some(&spawn_job.pipe_id),
                spawn_job.json_only,
                Some("spawn completed"),
                json!({
                    "ok": true,
                    "pane_id": pane_id_to_string(pane_id),
                    "waited": true,
                }),
                true,
            );
        }
    }

    pub(super) fn poll_jobs(&mut self) {
        let now = Instant::now();
        let job_ids: Vec<String> = self.jobs.keys().cloned().collect();

        for job_id in job_ids {
            let Some(job) = self.jobs.remove(&job_id) else {
                continue;
            };

            match job {
                Job::Wait(mut wait_job) => {
                    if now >= wait_job.deadline {
                        self.send_response(
                            Some(&wait_job.pipe_id),
                            wait_job.json_only,
                            Some("wait timed out"),
                            error_payload(ApiError::new("TIMEOUT", "wait timed out").meta(json!({
                                "pane_id": pane_id_to_string(wait_job.pane_id),
                            }))),
                            true,
                        );
                        continue;
                    }

                    if now >= wait_job.next_poll_at {
                        match self.wait_condition_matches(
                            wait_job.pane_id,
                            &wait_job.regex,
                            wait_job.last_n,
                            wait_job.window,
                            &wait_job.mode,
                            wait_job.since_line_count,
                        ) {
                            Ok(Some(lines)) => {
                                self.send_response(
                                    Some(&wait_job.pipe_id),
                                    wait_job.json_only,
                                    Some("match found"),
                                    json!({
                                        "ok": true,
                                        "pane_id": pane_id_to_string(wait_job.pane_id),
                                        "matched": true,
                                        "lines": lines,
                                    }),
                                    true,
                                );
                                continue;
                            }
                            Ok(None) => {
                                wait_job.next_poll_at = now + wait_job.interval;
                            }
                            Err(err) => {
                                self.send_response(
                                    Some(&wait_job.pipe_id),
                                    wait_job.json_only,
                                    Some("wait failed"),
                                    error_payload(err),
                                    true,
                                );
                                continue;
                            }
                        }
                    }

                    self.jobs.insert(job_id, Job::Wait(wait_job));
                }
                Job::Exec(exec_job) => {
                    if now >= exec_job.deadline {
                        self.exec_jobs_by_request_id.remove(&exec_job.request_id);
                        self.send_response(
                            Some(&exec_job.pipe_id),
                            exec_job.json_only,
                            Some("exec timed out"),
                            error_payload(ApiError::new("TIMEOUT", "exec timed out").meta(json!({
                                "request_id": exec_job.request_id,
                            }))),
                            true,
                        );
                        continue;
                    }
                    self.jobs.insert(job_id, Job::Exec(exec_job));
                }
                Job::Spawn(spawn_job) => {
                    if now >= spawn_job.deadline {
                        self.send_response(
                            Some(&spawn_job.pipe_id),
                            spawn_job.json_only,
                            Some("spawn timed out"),
                            error_payload(ApiError::new("TIMEOUT", "spawn timed out")),
                            true,
                        );
                        continue;
                    }
                    self.jobs.insert(job_id, Job::Spawn(spawn_job));
                }
            }
        }

        self.resolve_spawn_jobs();
        self.schedule_job_timer();
    }

    pub(super) fn wait_condition_matches(
        &self,
        pane_id: PaneId,
        regex: &Regex,
        last_n: usize,
        window: usize,
        mode: &WaitMode,
        since_line_count: Option<usize>,
    ) -> Result<Option<Vec<String>>, ApiError> {
        let pane_lines = self.read_pane_lines(pane_id, true)?;
        let all = if let Some(since_count) = since_line_count {
            if since_count >= pane_lines.all.len() {
                Vec::new()
            } else {
                pane_lines.all[since_count..].to_vec()
            }
        } else {
            pane_lines.all
        };

        let recent_window = if all.len() <= window {
            all
        } else {
            all[all.len() - window..].to_vec()
        };

        let sample = if recent_window.len() <= last_n {
            recent_window
        } else {
            recent_window[recent_window.len() - last_n..].to_vec()
        };

        if sample.is_empty() {
            return Ok(None);
        }

        let matched = match mode {
            WaitMode::Any => sample.iter().any(|line| regex.is_match(line)),
            WaitMode::All => sample.iter().all(|line| regex.is_match(line)),
        };

        if matched { Ok(Some(sample)) } else { Ok(None) }
    }
}
