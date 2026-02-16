use super::*;

impl QuillPlugin {
    pub(in crate::plugin) fn cmd_panes(
        &mut self,
        args: &[String],
    ) -> Result<CommandOutcome, ApiError> {
        let mut json_only = false;
        let mut only_focused = false;
        let mut include_plugin_panes = false;
        let mut tab_selector = TabSelector::Focused;
        let mut title_regex: Option<Regex> = None;
        let mut columns: Option<Vec<String>> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--json" {
                json_only = true;
            } else if arg == "--focused" {
                only_focused = true;
            } else if arg == "--all" {
                include_plugin_panes = true;
            } else if arg == "--tab" {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --tab"))?;
                tab_selector = parse_tab_selector(value)?;
            } else if let Some(value) = opt_value(arg, "--tab") {
                tab_selector = parse_tab_selector(&value)?;
            } else if arg == "--match-title" {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    ApiError::new("INVALID_ARGS", "Missing value for --match-title")
                })?;
                title_regex = Some(Regex::new(value).map_err(|e| {
                    ApiError::new("INVALID_REGEX", format!("Invalid --match-title regex: {e}"))
                })?);
            } else if let Some(value) = opt_value(arg, "--match-title") {
                title_regex = Some(Regex::new(&value).map_err(|e| {
                    ApiError::new("INVALID_REGEX", format!("Invalid --match-title regex: {e}"))
                })?);
            } else if arg == "--columns" {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| ApiError::new("INVALID_ARGS", "Missing value for --columns"))?;
                columns = Some(parse_columns(value));
            } else if let Some(value) = opt_value(arg, "--columns") {
                columns = Some(parse_columns(&value));
            } else {
                return Err(ApiError::new(
                    "INVALID_ARGS",
                    format!("Unknown flag for panes: {arg}"),
                ));
            }
            i += 1;
        }

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
                if only_focused && !(pane.is_focused && !pane.is_plugin) {
                    continue;
                }
                if !include_plugin_panes && pane.is_plugin {
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

        let human = if json_only {
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
            json_only,
            human,
            payload: json!({
                "ok": true,
                "focused_tab": focused_tab,
                "panes": pane_rows,
            }),
        })
    }
}
