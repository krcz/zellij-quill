use super::*;
use args::parse_args;
use clap::Parser;

#[derive(Parser)]
#[clap(no_binary_name = true)]
struct PanesArgs {
    #[clap(long)]
    json: bool,
    #[clap(long)]
    focused: bool,
    #[clap(long)]
    all: bool,
    #[clap(long)]
    tab: Option<String>,
    #[clap(long = "match-title")]
    match_title: Option<String>,
    #[clap(long)]
    columns: Option<String>,
}

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_panes(
        &mut self,
        raw_args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let args = parse_args::<PanesArgs>(raw_args)?;

        let tab_selector = match args.tab {
            Some(ref s) => s
                .parse::<TabSelector>()
                .map_err(|e| ApiError::new("INVALID_ARGS", e))?,
            None => TabSelector::Focused,
        };

        let title_regex = match args.match_title {
            Some(ref pattern) => Some(Regex::new(pattern).map_err(|e| {
                ApiError::new("INVALID_REGEX", format!("Invalid --match-title regex: {e}"))
            })?),
            None => None,
        };

        let columns = args.columns.as_deref().map(parse_columns);

        let manifest = self
            .pane_manifest
            .as_ref()
            .ok_or_else(|| ApiError::new("NO_PANE_STATE", "Pane state is not available yet"))?;

        let focus = self.current_focus_info();
        let focused_tab = focus
            .map(|(tab, _)| tab)
            .or(self.focused_tab_index)
            .unwrap_or(0);

        let mut rows: Vec<(usize, PaneInfo)> = Vec::new();
        for (tab_position, panes) in &manifest.panes {
            if !tab_selector.matches(*tab_position, focused_tab) {
                continue;
            }
            for pane in panes {
                if args.focused && !(pane.is_focused && !pane.is_plugin) {
                    continue;
                }
                if !args.all && pane.is_plugin {
                    continue;
                }
                if let Some(regex) = &title_regex {
                    if !regex.is_match(&pane.title) {
                        continue;
                    }
                }
                rows.push((*tab_position, pane.clone()));
            }
        }

        rows.sort_by_key(|(tab, pane)| (*tab, pane.id));

        let pane_rows: Vec<Value> = rows
            .iter()
            .map(|(tab_position, pane)| {
                let pane_id = pane_id_from_info(pane);
                let pid = if pane.is_plugin {
                    None
                } else {
                    get_pane_pid(pane_id).ok()
                };
                json!({
                    "pane_id": pane_id_to_string(pane_id),
                    "id": pane.id,
                    "kind": if pane.is_plugin { "plugin" } else { "terminal" },
                    "tab": tab_position,
                    "title": pane.title,
                    "focused": pane.is_focused,
                    "floating": pane.is_floating,
                    "suppressed": pane.is_suppressed,
                    "pid": pid,
                })
            })
            .collect();

        let human = if args.json {
            None
        } else {
            let active_columns = columns.unwrap_or_else(|| {
                vec![
                    "pane_id".to_string(),
                    "tab".to_string(),
                    "focused".to_string(),
                    "kind".to_string(),
                    "title".to_string(),
                ]
            });
            let mut lines = Vec::new();
            lines.push(active_columns.join("\t"));
            for row in &pane_rows {
                let mut cols = Vec::new();
                for col in &active_columns {
                    cols.push(column_value(row, col));
                }
                lines.push(cols.join("\t"));
            }
            Some(lines.join("\n"))
        };

        Ok(CommandOutcome::Immediate {
            json_only: args.json,
            human,
            payload: json!({
                "ok": true,
                "focused_tab": focused_tab,
                "panes": pane_rows,
            }),
        })
    }
}
