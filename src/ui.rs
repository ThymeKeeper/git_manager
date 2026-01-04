use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;
use crate::renderer::Renderer;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),      // Main area
            Constraint::Length(1),    // Status bar
        ])
        .split(f.area());

    // If file diff view is active, show it fullscreen
    use crate::app::AppMode;
    if app.mode == AppMode::FileDiffView {
        draw_file_diff_fullscreen(f, app, chunks[0]);
        draw_status_bar(f, app, chunks[1]);
        return;
    }

    // If details pane is expanded, show it fullscreen
    if app.details_expanded {
        draw_commit_details(f, app, chunks[0]);
        draw_status_bar(f, app, chunks[1]);
        return;
    }

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),  // Left: Graph + Commit list
            Constraint::Percentage(50),  // Right: Details + Actions
        ])
        .split(chunks[0]);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70),  // Graph/Commit list
            Constraint::Percentage(30),  // Git actions
        ])
        .split(main_chunks[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70),  // Commit details
            Constraint::Percentage(30),  // Git status
        ])
        .split(main_chunks[1]);

    // Draw commit graph/list
    draw_commit_graph(f, app, left_chunks[0]);

    // Draw git actions/commands
    draw_git_actions(f, app, left_chunks[1]);

    // Draw git status
    draw_git_status(f, app, right_chunks[1]);

    // Draw commit details
    draw_commit_details(f, app, right_chunks[0]);

    // Draw status bar
    draw_status_bar(f, app, chunks[1]);

    // Draw dialogs based on mode
    if app.mode == AppMode::Confirm {
        draw_confirmation_dialog(f, app);
    } else if app.mode == AppMode::CommitMessage {
        draw_commit_message_dialog(f, app);
    } else if app.mode == AppMode::BranchName {
        draw_branch_name_dialog(f, app);
    } else if app.mode == AppMode::SelectBranch {
        draw_branch_selection_dialog(f, app);
    } else if app.mode == AppMode::SelectBranchToDelete {
        draw_delete_branch_selection_dialog(f, app);
    } else if app.mode == AppMode::SetUserName {
        draw_config_input_dialog(f, app, "Set Git User Name", &app.config_input);
    } else if app.mode == AppMode::SetUserEmail {
        draw_config_input_dialog(f, app, "Set Git User Email", &app.config_input);
    } else if app.mode == AppMode::SetRemoteHost {
        draw_config_input_dialog(f, app, "Set Remote URL", &app.remote_host_input);
    } else if app.mode == AppMode::SquashCountInput {
        draw_squash_count_dialog(f, app);
    } else if app.mode == AppMode::RewordMessage {
        draw_reword_message_dialog(f, app);
    } else if app.mode == AppMode::AssignBranchName {
        draw_assign_branch_name_dialog(f, app);
    }
}

fn draw_commit_graph(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::FocusedPane;

    let is_focused = app.focused_pane == FocusedPane::CommitGraph;

    let block = Block::default()
        .title("Commit Graph")
        .borders(Borders::ALL)
        .style(Style::default().fg(if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        }));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Guard against terminal being too small
    if inner_area.height < 2 || inner_area.width < 10 {
        let msg = Paragraph::new("Terminal too small")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner_area);
        return;
    }

    if app.graph_nodes.is_empty() {
        let msg = Paragraph::new("No commits found or not in a git repository")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner_area);
        return;
    }

    // Adjust scroll to keep selection visible
    app.adjust_scroll(inner_area.height as usize);

    // Render commit graph
    let renderer = Renderer::new();
    let mut all_lines = Vec::new();

    // Calculate active columns at each row
    // Track which commits are "live" (have unprocessed parents)
    let mut active_at_row: Vec<Vec<usize>> = Vec::new();
    let mut col_ancestry_at_row: Vec<std::collections::HashMap<usize, bool>> = Vec::new();

    for i in 0..app.graph_nodes.len() {
        let mut active: Vec<usize> = Vec::new();
        let mut col_to_in_current_branch: std::collections::HashMap<usize, bool> = std::collections::HashMap::new();

        // A column is active if there's a "live" commit in that column
        // A commit is live if we've seen it but haven't seen all its parents yet
        for j in 0..=i {
            let node = &app.graph_nodes[j];

            // Check if this commit has any unprocessed parents
            let has_unprocessed_parents = node.commit.parents.iter().any(|parent_id| {
                // Parent is unprocessed if it appears after current row
                app.graph_nodes.iter()
                    .enumerate()
                    .skip(i + 1)
                    .any(|(_, n)| &n.commit.id == parent_id)
            });

            if has_unprocessed_parents && !active.contains(&node.column) {
                active.push(node.column);
                col_to_in_current_branch.insert(node.column, node.in_current_branch);
            }

            // Also include MergeFrom source columns for commits that have merge connections
            // These columns need to stay active until we reach the source commit
            // Only include them for rows AFTER the merge commit (j < i), not at the merge commit itself
            if j < i {
                for (conn_idx, connection) in node.connections.iter().enumerate() {
                    if let crate::graph::Connection::MergeFrom(source_col) = connection {
                        // Find which parent this MergeFrom refers to
                        let merge_from_count = node.connections.iter()
                            .take(conn_idx + 1)
                            .filter(|c| matches!(c, crate::graph::Connection::MergeFrom(_)))
                            .count();

                        // Check if THIS specific parent is in the future
                        if let Some(parent_id) = node.commit.parents.get(merge_from_count) {
                            let parent_not_yet_seen = app.graph_nodes.iter()
                                .skip(i + 1)
                                .any(|n| &n.commit.id == parent_id);

                            if parent_not_yet_seen && !active.contains(source_col) {
                                active.push(*source_col);
                                col_to_in_current_branch.insert(*source_col, node.in_current_branch);
                            }
                        }
                    }
                }
            }
        }

        // Always include current commit's column
        if i < app.graph_nodes.len() {
            let current_node = &app.graph_nodes[i];
            if !active.contains(&current_node.column) {
                active.push(current_node.column);
            }
            col_to_in_current_branch.insert(current_node.column, current_node.in_current_branch);
        }

        // Include all columns up to the maximum for commits that haven't reached parents
        // This ensures we don't miss any columns due to sparse column assignments
        for col in 0..app.graph_width {
            if !active.contains(&col) {
                let col_live_node = (0..=i).find(|&j| {
                    let node = &app.graph_nodes[j];
                    node.column == col && node.commit.parents.iter().any(|parent_id| {
                        // Check if parent is after current row (not including current row)
                        app.graph_nodes.iter().skip(i + 1).any(|n| &n.commit.id == parent_id)
                    })
                });
                if let Some(j) = col_live_node {
                    active.push(col);
                    let node = &app.graph_nodes[j];
                    col_to_in_current_branch.insert(col, node.in_current_branch);
                }
            }
        }

        active.sort();
        active.dedup();
        active_at_row.push(active);
        col_ancestry_at_row.push(col_to_in_current_branch);
    }

    for (idx, node) in app.graph_nodes.iter().enumerate() {
        let on_ancestry_path = app.graph.is_on_ancestry_path(&node.commit.id);
        let sync_status = crate::graph::SyncStatus::Synced;
        let active_cols = active_at_row.get(idx).map(|v| v.as_slice()).unwrap_or(&[]);
        let col_ancestry = col_ancestry_at_row.get(idx).cloned().unwrap_or_default();
        let is_selected = Some(idx) == app.selected_commit_idx;

        // Check if commit is not in current branch's history
        let not_in_current_branch = app.is_commit_not_in_current_branch(&node.commit.id);

        // Node row
        let node_row = renderer.render_node_row(
            node,
            app.graph_width,
            active_cols,
            &col_ancestry,
            on_ancestry_path,
            sync_status,
            not_in_current_branch,
        );

        // Add commit message next to the graph
        let mut node_line_spans = node_row.spans;
        node_line_spans.push(Span::raw(" "));

        // Highlight selected commit or commits selected for branch assignment
        let is_marked_for_branch = app.is_commit_selected(&node.commit.id);
        let message_style = if Some(idx) == app.selected_commit_idx {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if is_marked_for_branch {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if not_in_current_branch {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let short_msg = node.commit.message.lines().next().unwrap_or("");
        let short_msg = if short_msg.len() > 50 {
            // Use char_indices to avoid UTF-8 boundary issues
            let truncate_pos = short_msg.char_indices()
                .nth(47)
                .map(|(idx, _)| idx)
                .unwrap_or(short_msg.len());
            format!("{}...", &short_msg[..truncate_pos])
        } else {
            short_msg.to_string()
        };

        node_line_spans.push(Span::styled(
            format!("{} {}", node.commit.short_id, short_msg),
            message_style,
        ));

        all_lines.push(Line::from(node_line_spans));

        // Edge row with proper connection rendering
        if idx < app.graph_nodes.len() - 1 {
            let next_node = &app.graph_nodes[idx + 1];

            // For edge rows, only show vertical lines for commits that are PASSING THROUGH this edge
            // i.e., commits that were live before this edge and still live after
            let mut edge_active_cols: Vec<usize> = Vec::new();
            // Track which column is owned by which commit (for ancestry-based styling)
            let mut col_to_in_current_branch: std::collections::HashMap<usize, bool> = std::collections::HashMap::new();

            // Only include columns where commits from current row or earlier still have
            // unprocessed parents at the next row or beyond
            for j in 0..=idx {
                let node = &app.graph_nodes[j];

                // Skip the current node if it's directly connecting to its parent in the next row
                // (i.e., if next_node is the direct parent of current node in a different column)
                let is_current_branching = j == idx &&
                    node.commit.parents.contains(&next_node.commit.id) &&
                    node.column != next_node.column;

                if is_current_branching {
                    continue;
                }

                let has_unprocessed_parents = node.commit.parents.iter().any(|parent_id| {
                    app.graph_nodes.iter()
                        .enumerate()
                        .skip(idx + 1)  // At or after the next row
                        .any(|(_, n)| &n.commit.id == parent_id)
                });

                if has_unprocessed_parents && !edge_active_cols.contains(&node.column) {
                    edge_active_cols.push(node.column);
                    // Track the ancestry status of the commit in this column
                    col_to_in_current_branch.insert(node.column, node.in_current_branch);
                }

                // Also track MergeFrom source columns that haven't reached their source commit yet
                // We need to check if the SPECIFIC parent that this MergeFrom refers to is in the future
                for (conn_idx, connection) in node.connections.iter().enumerate() {
                    if let crate::graph::Connection::MergeFrom(source_col) = connection {
                        // Find which parent this MergeFrom refers to
                        let merge_from_count = node.connections.iter()
                            .take(conn_idx + 1)
                            .filter(|c| matches!(c, crate::graph::Connection::MergeFrom(_)))
                            .count();

                        // Check if THIS specific parent is in the future
                        if let Some(parent_id) = node.commit.parents.get(merge_from_count) {
                            let parent_not_yet_seen = app.graph_nodes.iter()
                                .skip(idx + 1)
                                .any(|n| &n.commit.id == parent_id);

                            if parent_not_yet_seen && !edge_active_cols.contains(source_col) {
                                edge_active_cols.push(*source_col);
                                // Track ancestry for merge source column too
                                col_to_in_current_branch.insert(*source_col, node.in_current_branch);
                            }
                        }
                    }
                }
            }

            edge_active_cols.sort();
            edge_active_cols.dedup();

            let edge_row = renderer.render_edge_row(
                node,
                next_node,
                app.graph_width,
                &edge_active_cols,
                &col_to_in_current_branch,
                on_ancestry_path,
                sync_status,
                &app.graph_nodes,
                not_in_current_branch,
            );
            all_lines.push(edge_row);
        }
    }

    // Extract visible lines based on scroll offset
    let viewport_height = inner_area.height as usize;
    let visible_lines: Vec<Line> = all_lines
        .into_iter()
        .skip(app.scroll_offset)
        .take(viewport_height)
        .collect();

    // Render without wrapping (text will be truncated by the terminal)
    let paragraph = Paragraph::new(visible_lines);

    f.render_widget(paragraph, inner_area);
}

fn draw_commit_details(f: &mut Frame, app: &App, area: Rect) {
    use crate::app::FocusedPane;
    use chrono::{DateTime, Utc};

    let is_focused = app.focused_pane == FocusedPane::CommitDetails;

    let block = Block::default()
        .title("Commit Details")
        .borders(Borders::ALL)
        .style(Style::default().fg(if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        }));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(idx) = app.selected_commit_idx {
        if let Some(node) = app.graph_nodes.get(idx) {
            let commit = &node.commit;

            // Format timestamp
            let datetime = DateTime::from_timestamp(commit.timestamp, 0)
                .unwrap_or_else(|| DateTime::UNIX_EPOCH);
            let formatted_date = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

            // Get branches pointing to this commit
            let branches = app.get_all_branches_for_commit(&commit.id);
            let (branch_label, branch_text, branch_color) = if branches.is_empty() {
                ("Branch: ", "not branch tip".to_string(), Color::DarkGray)
            } else {
                ("Branch: ", branches.join(", "), Color::Cyan)
            };

            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Commit: ", Style::default().fg(Color::Yellow)),
                    Span::raw(&commit.id),
                ]),
                Line::from(vec![
                    Span::styled(branch_label, Style::default().fg(Color::Yellow)),
                    Span::styled(branch_text, Style::default().fg(branch_color)),
                ]),
                Line::from(vec![
                    Span::styled("Date: ", Style::default().fg(Color::Yellow)),
                    Span::raw(&formatted_date),
                ]),
                Line::from(vec![
                    Span::styled("Author: ", Style::default().fg(Color::Yellow)),
                    Span::raw(&commit.author),
                ]),
                Line::from(vec![
                    Span::styled("Message: ", Style::default().fg(Color::Yellow)),
                ]),
            ];

            // Word-wrap the commit message (UTF-8 safe)
            let wrap_width = area.width.saturating_sub(2) as usize; // Account for padding
            for line in commit.message.lines() {
                if line.is_empty() {
                    lines.push(Line::from(""));
                } else {
                    // Wrap long lines using character indices to avoid UTF-8 boundary issues
                    let chars: Vec<char> = line.chars().collect();
                    let mut start = 0;

                    while start < chars.len() {
                        let end = (start + wrap_width).min(chars.len());
                        let mut actual_end = end;

                        // Try to break at word boundary if not at end of line
                        if end < chars.len() {
                            // Find last whitespace in this chunk
                            if let Some(pos) = chars[start..end].iter().rposition(|c| c.is_whitespace()) {
                                actual_end = start + pos + 1;
                            }
                        }

                        let chunk: String = chars[start..actual_end].iter().collect();
                        let trimmed = chunk.trim_end().to_string();
                        lines.push(Line::from(Span::raw(trimmed)));
                        start = actual_end;
                    }
                }
            }

            lines.push(Line::from(""));

            if let Some(ref diff) = app.current_diff {
                lines.push(Line::from(Span::styled(
                    "Diff:",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(""));

                // Split diff into lines and add color coding
                for diff_line in diff.lines() {
                    let style = if diff_line.starts_with('+') {
                        Style::default().fg(Color::Green)
                    } else if diff_line.starts_with('-') {
                        Style::default().fg(Color::Red)
                    } else if diff_line.starts_with("@@") {
                        Style::default().fg(Color::Cyan)
                    } else if diff_line.starts_with("diff --git") {
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                    } else if diff_line.starts_with("index ") || diff_line.starts_with("---") || diff_line.starts_with("+++") {
                        Style::default().fg(Color::Gray)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::from(Span::styled(diff_line, style)));
                }
            }

            // Apply scroll offset (vertical and horizontal)
            let viewport_height = inner_area.height as usize;
            let h_offset = app.details_horizontal_offset;

            let visible_lines: Vec<Line> = lines
                .into_iter()
                .skip(app.details_scroll_offset)
                .take(viewport_height)
                .map(|line| {
                    // Apply horizontal scrolling by skipping characters
                    let spans: Vec<Span> = line.spans.into_iter().map(|span| {
                        let content = span.content.to_string();
                        let trimmed = if content.len() > h_offset {
                            content.chars().skip(h_offset).collect()
                        } else {
                            String::new()
                        };
                        Span::styled(trimmed, span.style)
                    }).collect();
                    Line::from(spans)
                })
                .collect();

            let paragraph = Paragraph::new(visible_lines);
            f.render_widget(paragraph, inner_area);
        }
    } else {
        let msg = Paragraph::new("Select a commit to view details")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner_area);
    }
}

fn draw_git_actions(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::FocusedPane;

    let is_focused = app.focused_pane == FocusedPane::GitActions;

    let block = Block::default()
        .title("Git Commands")
        .borders(Borders::ALL)
        .style(Style::default().fg(if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        }));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Adjust scroll to keep selection visible
    let viewport_height = inner_area.height as usize;
    app.adjust_command_scroll(viewport_height);

    // Render commands with scrolling
    let items: Vec<ListItem> = app
        .command_list
        .iter()
        .enumerate()
        .skip(app.command_scroll_offset)
        .take(viewport_height)
        .map(|(idx, cmd)| {
            let style = if idx == app.selected_command_idx && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if idx == app.selected_command_idx && is_focused {
                "► "
            } else {
                "  "
            };

            ListItem::new(format!("{}{}", prefix, cmd.description())).style(style)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner_area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some(ref msg) = app.status_message {
        msg.clone()
    } else if !app.has_git_repo {
        "⚠ No git repository found - some features may be unavailable".to_string()
    } else {
        let branch = app.current_branch.as_deref().unwrap_or("unknown");
        let remote_host = app.git_remote_host.as_deref().unwrap_or("no remote");

        // Add ahead/behind info if available
        let ahead_behind = if app.branch_ahead > 0 || app.branch_behind > 0 {
            let mut parts = Vec::new();
            if app.branch_ahead > 0 {
                parts.push(format!("↑{}", app.branch_ahead));
            }
            if app.branch_behind > 0 {
                parts.push(format!("↓{}", app.branch_behind));
            }
            format!(" [{}]", parts.join(" "))
        } else if app.has_upstream {
            // Has upstream and in sync
            " [synced]".to_string()
        } else {
            String::new()
        };

        format!(
            "Branch: {}{} | Remote: {}",
            branch,
            ahead_behind,
            remote_host
        )
    };

    let paragraph = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Rgb(185, 177, 160)).bg(Color::Rgb(90, 90, 90)))
        .alignment(ratatui::layout::Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_git_status(f: &mut Frame, app: &App, area: Rect) {
    use crate::app::FocusedPane;

    let is_focused = app.focused_pane == FocusedPane::GitStatus;

    let block = Block::default()
        .title("Git Status")
        .borders(Borders::ALL)
        .style(Style::default().fg(if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        }));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if app.git_status_files.is_empty() {
        let msg = Paragraph::new("Working tree clean")
            .style(Style::default().fg(Color::Green));
        f.render_widget(msg, inner_area);
        return;
    }

    let items: Vec<ListItem> = app
        .git_status_files
        .iter()
        .enumerate()
        .map(|(idx, file)| {
            let (status_suffix, status_color) = match file.status {
                crate::app::FileStatus::Staged => ("(staged)", Color::Green),
                crate::app::FileStatus::Modified => ("(modified)", Color::Yellow),
                crate::app::FileStatus::Untracked => ("(untracked)", Color::Red),
                crate::app::FileStatus::Deleted => ("(deleted)", Color::Red),
            };

            let style = if Some(idx) == app.selected_file_idx && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            };

            let selection_prefix = if Some(idx) == app.selected_file_idx && is_focused {
                "► "
            } else {
                "  "
            };

            let stage_prefix = match file.status {
                crate::app::FileStatus::Staged => "✓ ",
                _ => "",
            };

            ListItem::new(format!("{}{}{} {}", selection_prefix, stage_prefix, file.path, status_suffix)).style(style)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner_area);
}

fn draw_confirmation_dialog(f: &mut Frame, app: &App) {
    // Get confirmation message - use detailed message if available, otherwise fallback
    let message = if let Some(ref msg) = app.pending_command_message {
        msg.as_str()
    } else if let Some(ref cmd) = app.pending_command {
        cmd.confirmation_message()
    } else {
        "Are you sure?"
    };

    // Calculate dynamic height based on message content
    let message_line_count = message.lines().count();
    let total_lines = message_line_count + 6; // +6 for blank lines, instruction line, and borders
    let popup_height = (total_lines as u16).clamp(12, 35); // Min 12, max 35 lines

    // Center the confirmation dialog
    let area = f.area();
    let popup_width = 90;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Confirm Action")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Red));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Split message by newlines to handle multi-line messages
    let mut text = vec![Line::from("")];

    for line in message.lines() {
        text.push(Line::from(Span::styled(
            line,
            Style::default().fg(Color::Yellow),
        )));
    }

    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        "Press 'y' to confirm, 'n' or Esc to cancel",
        Style::default().fg(Color::Gray),
    )));

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_commit_message_dialog(f: &mut Frame, app: &App) {
    // Center the commit message dialog
    let area = f.area();
    let popup_width = 70;
    let popup_height = 8;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Commit Message")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter your commit message:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &app.commit_message_input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to commit, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_branch_name_dialog(f: &mut Frame, app: &App) {
    // Center the branch name dialog
    let area = f.area();
    let popup_width = 70;
    let popup_height = 8;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Create Branch")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter branch name:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &app.branch_name_input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to create, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_branch_selection_dialog(f: &mut Frame, app: &App) {
    // Center the branch selection dialog
    let area = f.area();
    let popup_width = 60;
    let popup_height = (app.available_branches.len() + 5).min(20) as u16;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Select Branch to Checkout")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let items: Vec<ListItem> = app
        .available_branches
        .iter()
        .enumerate()
        .map(|(idx, branch)| {
            let style = if idx == app.selected_branch_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if idx == app.selected_branch_idx {
                "► "
            } else {
                "  "
            };

            ListItem::new(format!("{}{}", prefix, branch)).style(style)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner_area);
}

fn draw_delete_branch_selection_dialog(f: &mut Frame, app: &App) {
    // Center the delete branch selection dialog
    let area = f.area();
    let popup_width = 60;
    let popup_height = (app.available_branches.len() + 6).min(20) as u16;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Select Branch to Force Delete")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Red));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let items: Vec<ListItem> = app
        .available_branches
        .iter()
        .enumerate()
        .map(|(idx, branch)| {
            let style = if idx == app.selected_branch_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if idx == app.selected_branch_idx {
                "► "
            } else {
                "  "
            };

            ListItem::new(format!("{}{}", prefix, branch)).style(style)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner_area);
}

fn draw_config_input_dialog(f: &mut Frame, app: &App, title: &str, input: &str) {
    // Center the config input dialog
    let area = f.area();
    let popup_width = 70;
    let popup_height = 8;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter value:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to save, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_squash_count_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_width = 70;
    let popup_height = 8;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Squash Commits")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter number of commits to squash (from HEAD):",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &app.squash_count_input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to continue, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_reword_message_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_width = 70;
    let popup_height = 12;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Reword Commit Message")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Edit commit message:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &app.reword_message_input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to save, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}



fn draw_assign_branch_name_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_width = 70;
    let popup_height = 10;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Assign Commits to Branch")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));

    let inner_area = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Selected {} commit(s)", app.selected_commit_ids.len()),
            Style::default().fg(Color::Green),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter branch name:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &app.assign_branch_name_input,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to create/move branch, Esc to cancel",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text);
    f.render_widget(paragraph, inner_area);
}

fn draw_file_diff_fullscreen(f: &mut Frame, app: &App, area: Rect) {
    // Get the selected file name for the title
    let file_name = if let Some(idx) = app.selected_file_idx {
        if let Some(file) = app.git_status_files.get(idx) {
            file.path.clone()
        } else {
            "File Diff".to_string()
        }
    } else {
        "File Diff".to_string()
    };

    let block = Block::default()
        .title(format!("File Diff: {}", file_name))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(ref diff) = app.current_diff {
        let mut lines = Vec::new();

        // Split diff into lines and add color coding
        for diff_line in diff.lines() {
            let style = if diff_line.starts_with('+') && !diff_line.starts_with("+++") {
                Style::default().fg(Color::Green)
            } else if diff_line.starts_with('-') && !diff_line.starts_with("---") {
                Style::default().fg(Color::Red)
            } else if diff_line.starts_with("@@") {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };

            lines.push(Line::from(Span::styled(diff_line, style)));
        }

        // Apply both vertical and horizontal scrolling
        let paragraph = Paragraph::new(lines)
            .scroll((app.details_scroll_offset as u16, app.details_horizontal_offset as u16));
        f.render_widget(paragraph, inner_area);
    } else {
        let msg = Paragraph::new("No diff available\n\nPress Esc or 'q' to go back")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner_area);
    }
}
