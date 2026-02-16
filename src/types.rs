use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};
use zellij_tile::prelude::{PaneId, PaneInfo, PaneManifest};

pub(crate) const REQUEST_ID_KEY: &str = "zellij_quill_request_id";
pub(crate) const MAX_RECENT_TERMINAL_PANES: usize = 256;
pub(crate) const DEFAULT_TIMER_SECS: f64 = 0.2;
pub(crate) const SESSION_TOKEN_ENV_VAR: &str = "ZELLIJ_QUILL_TOKEN";
pub(crate) const ORIGIN_PANE_ENV_VAR: &str = "ZELLIJ_PANE_ID";
pub(crate) const PERMISSION_REQUEST_CONTEXT_KEY: &str = "zellij_quill_permission_request_id";

#[derive(Default)]
pub(crate) struct QuillPlugin {
    pub(crate) pane_manifest: Option<PaneManifest>,
    pub(crate) focused_tab_index: Option<usize>,
    pub(crate) focused_pane_id: Option<PaneId>,
    pub(crate) recent_terminal_panes: VecDeque<PaneId>,
    pub(crate) jobs: std::collections::HashMap<String, Job>,
    pub(crate) exec_jobs_by_request_id: std::collections::HashMap<String, String>,
    pub(crate) marks: std::collections::HashMap<String, ScrollbackMark>,
    pub(crate) next_job_id: u64,
    pub(crate) next_request_id: u64,
    pub(crate) next_mark_id: u64,
    pub(crate) require_token: bool,
    pub(crate) enforce_pane_permissions: bool,
    pub(crate) configured_token: Option<String>,
    pub(crate) token_from_session_env: Option<String>,
    pub(crate) token_pane_permissions: std::collections::HashMap<String, Vec<PaneId>>,
    pub(crate) pending_permission_requests:
        std::collections::HashMap<String, PanePermissionRequest>,
    pub(crate) next_permission_request_id: u64,
    pub(crate) permissions_denied: bool,
    pub(crate) permissions_granted: bool,
    pub(crate) session_env_permission_granted: bool,
    pub(crate) current_pipe_origin_pane_id: Option<PaneId>,
    pub(crate) permission_prompt_panes: std::collections::HashMap<u32, String>,
    pub(crate) pending_permission_commands:
        std::collections::HashMap<String, Vec<PendingPermissionCommand>>,
}

#[derive(Debug)]
pub(crate) enum Job {
    Wait(WaitJob),
    Exec(ExecJob),
    Spawn(SpawnJob),
}

#[derive(Debug)]
pub(crate) struct WaitJob {
    pub(crate) pipe_id: String,
    pub(crate) pane_id: PaneId,
    pub(crate) regex: Regex,
    pub(crate) last_n: usize,
    pub(crate) window: usize,
    pub(crate) mode: WaitMode,
    pub(crate) deadline: Instant,
    pub(crate) next_poll_at: Instant,
    pub(crate) interval: Duration,
    pub(crate) json_only: bool,
    pub(crate) since_line_count: Option<usize>,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum WaitMode {
    Any,
    All,
}

#[derive(Debug)]
pub(crate) struct ExecJob {
    pub(crate) pipe_id: String,
    pub(crate) request_id: String,
    pub(crate) command: Vec<String>,
    pub(crate) deadline: Instant,
    pub(crate) json_only: bool,
}

#[derive(Debug)]
pub(crate) struct SpawnJob {
    pub(crate) pipe_id: String,
    pub(crate) expected_pane_id: Option<PaneId>,
    pub(crate) baseline_panes: HashSet<PaneId>,
    pub(crate) deadline: Instant,
    pub(crate) json_only: bool,
    pub(crate) token: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PanePermissionRequest {
    pub(crate) request_id: String,
    pub(crate) token: String,
    pub(crate) pane_id: PaneId,
    pub(crate) origin_pane_id: Option<PaneId>,
    pub(crate) created_at_ms: u128,
}

#[derive(Debug, Clone)]
pub(crate) struct ScrollbackMark {
    pub(crate) token: String,
    pub(crate) pane_id: PaneId,
    pub(crate) line_count: usize,
    pub(crate) created_at_ms: u128,
}

#[derive(Debug)]
pub(crate) enum CommandOutcome {
    Immediate {
        json_only: bool,
        human: Option<String>,
        payload: Value,
    },
    Async,
}

#[derive(Debug, Clone)]
pub(crate) struct PipeCommand {
    pub(crate) cmd: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingPermissionCommand {
    pub(crate) pipe_id: String,
    pub(crate) parsed: PipeCommand,
    pub(crate) origin_pane_id: Option<PaneId>,
}

#[derive(Debug)]
pub(crate) struct PaneLines {
    pub(crate) all: Vec<String>,
    pub(crate) viewport_start: usize,
    pub(crate) viewport_end_exclusive: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GrepMatch {
    pub(crate) line_number: usize,
    pub(crate) line: String,
    pub(crate) before: Vec<String>,
    pub(crate) after: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum TabSelector {
    Focused,
    All,
    Index(usize),
}

impl TabSelector {
    pub(crate) fn matches(&self, candidate: usize, focused: usize) -> bool {
        match self {
            TabSelector::Focused => candidate == focused,
            TabSelector::All => true,
            TabSelector::Index(index) => candidate == *index,
        }
    }
}

impl std::str::FromStr for TabSelector {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "focused" => Ok(TabSelector::Focused),
            "all" => Ok(TabSelector::All),
            other => other
                .parse::<usize>()
                .map(TabSelector::Index)
                .map_err(|_| format!("Invalid --tab value: {other}")),
        }
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum TailFrom {
    End,
    Viewport,
    Top,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum SpawnKind {
    Terminal,
    Command,
}

#[derive(Debug, Clone, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub(crate) enum SpawnWhere {
    Tiled,
    Floating,
    NearPlugin,
    InPlace,
    Background,
}

pub(crate) fn pane_id_from_info(info: &PaneInfo) -> PaneId {
    if info.is_plugin {
        PaneId::Plugin(info.id)
    } else {
        PaneId::Terminal(info.id)
    }
}

pub(crate) fn pane_id_to_string(pane_id: PaneId) -> String {
    match pane_id {
        PaneId::Terminal(id) => format!("id:{id}"),
        PaneId::Plugin(id) => format!("plugin:{id}"),
    }
}
