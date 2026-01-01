use crate::graph::{GraphNode, Connection, SyncStatus};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphType {
    Commit,
    CommitHead,
    Vertical,
    Horizontal,
    TopRight,
    BottomRight,
    TopLeft,
    BottomLeft,
    TeeRight,
    TeeLeft,
    Cross,
}

pub struct Renderer {
    pub head_commit_id: Option<String>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            head_commit_id: None,
        }
    }

    pub fn set_head_commit(&mut self, commit_id: String) {
        self.head_commit_id = Some(commit_id);
    }

    fn select_glyph(&self, glyph_type: GlyphType, on_bold_path: bool, is_head: bool) -> char {
        match (glyph_type, on_bold_path, is_head) {
            // Commits
            (GlyphType::CommitHead, _, _) => '◉',
            (GlyphType::Commit, true, _) => '●',
            (GlyphType::Commit, false, _) => '○',

            // Verticals
            (GlyphType::Vertical, true, _) => '┃',
            (GlyphType::Vertical, false, _) => '│',

            // Horizontals
            (GlyphType::Horizontal, true, _) => '━',
            (GlyphType::Horizontal, false, _) => '─',

            // Corners
            (GlyphType::TopRight, true, _) => '┓',
            (GlyphType::TopRight, false, _) => '╮',
            (GlyphType::BottomRight, true, _) => '┛',
            (GlyphType::BottomRight, false, _) => '╯',
            (GlyphType::TopLeft, true, _) => '┏',
            (GlyphType::TopLeft, false, _) => '╭',
            (GlyphType::BottomLeft, true, _) => '┗',
            (GlyphType::BottomLeft, false, _) => '╰',

            // Tees
            (GlyphType::TeeRight, true, _) => '┣',
            (GlyphType::TeeRight, false, _) => '├',
            (GlyphType::TeeLeft, true, _) => '┫',
            (GlyphType::TeeLeft, false, _) => '┤',

            // Crosses
            (GlyphType::Cross, true, _) => '│',
            (GlyphType::Cross, false, _) => '│',
        }
    }

    pub fn commit_style(&self, sync: SyncStatus, _on_ancestry_path: bool, not_in_current_branch: bool) -> Style {
        // Grey out commits not in current branch's history
        if not_in_current_branch {
            return Style::default().fg(Color::DarkGray);
        }

        let base_color = match sync {
            SyncStatus::Synced => Color::White,
            SyncStatus::LocalOnly => Color::Green,
            SyncStatus::RemoteOnly => Color::Red,
            SyncStatus::Diverged => Color::Yellow,
        };

        Style::default().fg(base_color)
    }

    pub fn render_node_row(
        &self,
        node: &GraphNode,
        width: usize,
        active_columns: &[usize],
        col_to_in_current_branch: &std::collections::HashMap<usize, bool>,
        on_ancestry_path: bool,
        sync_status: SyncStatus,
        not_in_current_branch: bool,
    ) -> Line<'static> {
        let mut spans = Vec::new();
        let is_head = self.head_commit_id.as_ref() == Some(&node.commit.id);

        let current_node_style = self.commit_style(sync_status, on_ancestry_path, not_in_current_branch);

        // Get columns being merged in (for merge commits)
        let merge_sources: Vec<usize> = node.connections.iter()
            .filter_map(|c| if let Connection::MergeFrom(source) = c { Some(*source) } else { None })
            .collect();

        let is_merge = !merge_sources.is_empty();

        for col in 0..width {
            // Determine the style for this column
            let col_style = if col == node.column {
                // Current node's column uses current node's style
                current_node_style
            } else if let Some(&in_current_branch) = col_to_in_current_branch.get(&col) {
                // Use the pre-calculated ancestry status for this column
                self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
            } else {
                // Default to current node's style for non-active columns
                current_node_style
            };

            if col == node.column {
                // Render commit marker
                let glyph = if is_head {
                    self.select_glyph(GlyphType::CommitHead, on_ancestry_path, true)
                } else {
                    self.select_glyph(GlyphType::Commit, on_ancestry_path, false)
                };

                // If this is a merge commit, add merge indicator after the commit node
                if is_merge {
                    spans.push(Span::styled(format!("{}", glyph), current_node_style));
                    spans.push(Span::styled("─".to_string(), current_node_style));
                } else {
                    spans.push(Span::styled(format!("{} ", glyph), current_node_style));
                }
            } else if is_merge && merge_sources.contains(&col) {
                // For merge commits, show the merge connection on the node row
                spans.push(Span::styled("╮ ".to_string(), current_node_style));
            } else if is_merge && col > node.column && col < *merge_sources.iter().min().unwrap_or(&col) {
                // Horizontal line connecting to merge source
                // Check if there's a vertical line at this position - if so, use cross
                if active_columns.contains(&col) && !merge_sources.contains(&col) {
                    spans.push(Span::styled("│─".to_string(), current_node_style));
                } else {
                    spans.push(Span::styled("──".to_string(), current_node_style));
                }
            } else if active_columns.contains(&col) && !merge_sources.contains(&col) {
                // Render vertical line for active lane with per-column styling
                let glyph = self.select_glyph(GlyphType::Vertical, false, false);
                spans.push(Span::styled(format!("{} ", glyph), col_style));
            } else {
                spans.push(Span::raw("  "));
            }
        }

        Line::from(spans)
    }

    pub fn render_edge_row(
        &self,
        current_node: &GraphNode,
        next_node: &GraphNode,
        width: usize,
        active_columns: &[usize],
        col_to_in_current_branch: &std::collections::HashMap<usize, bool>,
        on_ancestry_path: bool,
        sync_status: SyncStatus,
        all_nodes: &[GraphNode],
        not_in_current_branch: bool,
    ) -> Line<'static> {
        let mut spans = Vec::new();
        let current_node_style = self.commit_style(sync_status, on_ancestry_path, not_in_current_branch);

        // Check connections from both nodes:
        // - next_node's BranchTo: next branches from current to its own column
        // - next_node's MergeFrom: next merges from another column
        // - current_node's MergeFrom: current is a merge commit, show split to merged parents

        // Check parent relationships in both directions
        let is_parent = current_node.commit.parents.contains(&next_node.commit.id);
        let is_child = next_node.commit.parents.contains(&current_node.commit.id);

        let branch_targets: Vec<usize> = if is_parent {
            current_node.connections.iter()
                .filter_map(|c| if let Connection::BranchTo(target) = c {
                    if *target == next_node.column { Some(*target) } else { None }
                } else { None })
                .collect()
        } else {
            Vec::new()
        };

        // Check if current node is a merge commit (has MergeFrom connections)
        let current_merge_sources: Vec<usize> = current_node.connections.iter()
            .filter_map(|c| if let Connection::MergeFrom(source) = c { Some(*source) } else { None })
            .collect();

        let is_current_merge = !current_merge_sources.is_empty();

        // Build a map of what to render in each column
        let mut col_chars: Vec<String> = vec![String::new(); width];

        // First, mark all active lanes with vertical lines
        for &col in active_columns {
            col_chars[col] = "│ ".to_string();
        }

        // Check if any commits in active columns (other than current) are merging into next_node
        // Only check commits that are "live" (rendered but haven't reached parent yet)
        let mut other_merging_columns: Vec<usize> = Vec::new();
        let next_node_id = &next_node.commit.id;

        // Find current and next node indices
        let current_idx = all_nodes.iter().position(|n| &n.commit.id == &current_node.commit.id);
        let next_idx = all_nodes.iter().position(|n| &n.commit.id == next_node_id);

        if let (Some(curr_idx), Some(nx_idx)) = (current_idx, next_idx) {
            for &col in active_columns {
                if col != current_node.column {
                    // Only check commits between current and next that are in this column
                    // and have next_node as their immediate parent
                    let has_merge = all_nodes[..=curr_idx].iter().any(|n| {
                        n.column == col &&
                        n.commit.parents.contains(next_node_id) &&
                        // Check if this commit's parent (next_node) hasn't been rendered yet
                        n.commit.parents.iter().all(|p| {
                            all_nodes[curr_idx+1..].iter().any(|parent_node| &parent_node.commit.id == p)
                        })
                    });

                    if has_merge {
                        other_merging_columns.push(col);
                    }
                }
            }
        }

        // Track which columns have horizontal lines from current node's branch (should use current node's style)
        let mut current_branch_cols: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Track leftward branch sources: target_col -> source_col
        let mut leftward_branch_sources: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

        // Track which columns are leftward merge targets (tee should use next_node's style)
        let mut leftward_merge_targets: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Handle different edge cases
        let has_branch = !branch_targets.is_empty();

        if is_current_merge {
            // Current node is a merge commit - merge was shown on node row, just continue lines here
            col_chars[current_node.column] = "│ ".to_string();

            for &source_col in &current_merge_sources {
                if source_col > current_node.column {
                    // Just show vertical continuation for merge source columns
                    col_chars[source_col] = "│ ".to_string();
                }
            }
        } else if has_branch {
            // This commit branches to parent in different column
            for &target_col in &branch_targets {
                if target_col > current_node.column {
                    // Branch goes right
                    col_chars[current_node.column] = "├─".to_string();
                    for c in (current_node.column + 1)..target_col {
                        // Check if there's a vertical line or bend - if so, connect through it
                        if col_chars[c] == "│ " {
                            col_chars[c] = "│─".to_string();
                        } else if col_chars[c] == "╯ " {
                            col_chars[c] = "╯─".to_string();
                        } else {
                            col_chars[c] = "──".to_string();
                        }
                        current_branch_cols.insert(c);
                    }
                    col_chars[target_col] = "╮ ".to_string();
                } else if target_col < current_node.column {
                    // Branch goes left (merging back into next_node)
                    // Store as ├─ and ╯  but will split during rendering
                    // Record the source column for this leftward branch
                    leftward_branch_sources.insert(target_col, current_node.column);

                    // Record that this target column is a leftward merge target
                    // (The tee represents merging into next_node at the target column)
                    leftward_merge_targets.insert(target_col);

                    for c in target_col..current_node.column {
                        if c == target_col {
                            // At the target column (merge point)
                            if col_chars[c] == "│ " {
                                // Vertical line continues - use tee with horizontal
                                col_chars[c] = "├─".to_string();
                            } else if col_chars[c] == "╯ " {
                                // Bend from another merge - connect through it
                                col_chars[c] = "╯─".to_string();
                            } else {
                                // No vertical line - use curve
                                col_chars[c] = "╭─".to_string();
                            }
                        } else {
                            // Between target and current
                            if col_chars[c] == "│ " {
                                // Vertical line crossing - use cross
                                col_chars[c] = "│─".to_string();
                            } else if col_chars[c] == "╯ " {
                                // Bend from another merge - connect through it
                                col_chars[c] = "╯─".to_string();
                            } else {
                                // Just horizontal
                                col_chars[c] = "──".to_string();
                            }
                        }
                        // Don't add any columns to current_branch_cols for leftward merges
                        // They should all use passthrough styling
                        // BUT track all columns in leftward_branch_sources so horizontal parts
                        // of combined characters can be styled correctly
                        leftward_branch_sources.insert(c, current_node.column);
                    }
                    // Set source column - but preserve any horizontal line that's passing through
                    if col_chars[current_node.column].ends_with('─') {
                        col_chars[current_node.column] = "╯─".to_string();
                    } else {
                        col_chars[current_node.column] = "╯ ".to_string();
                    }
                    // Don't add current_node.column - it should use passthrough styling
                }
            }
        } else {
            // Simple vertical continuation
            col_chars[current_node.column] = "│ ".to_string();
        }

        // Ensure current node's column has a line if it's being merged into next node
        if is_child && col_chars[current_node.column].is_empty() {
            col_chars[current_node.column] = "│ ".to_string();
        }

        // Track which columns have horizontal lines from other merging branches
        // Map column -> source column (to look up style from col_to_in_current_branch)
        let mut merge_branch_cols: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

        // Handle other columns merging into next_node
        for &merge_col in &other_merging_columns {
            if merge_col > next_node.column {
                // Merging from right to left
                for c in (next_node.column + 1)..merge_col {
                    if col_chars[c] == "│ " {
                        col_chars[c] = "│─".to_string();
                    } else if col_chars[c] == "╯ " {
                        col_chars[c] = "╯─".to_string();
                    } else if col_chars[c].is_empty() {
                        col_chars[c] = "──".to_string();
                    }
                    merge_branch_cols.insert(c, merge_col);
                    // Also track for ╯─ and │─ horizontal styling
                    leftward_branch_sources.insert(c, merge_col);
                }
                // Preserve any horizontal line passing through
                if col_chars[merge_col].ends_with('─') {
                    col_chars[merge_col] = "╯─".to_string();
                } else {
                    col_chars[merge_col] = "╯ ".to_string();
                }
                merge_branch_cols.insert(merge_col, merge_col);

                // Update the target column to show the merge point
                if col_chars[next_node.column] == "│ " {
                    col_chars[next_node.column] = "├─".to_string();
                }
                // Track for ├─ horizontal styling - but only if this merge is closer than existing
                // (closest branch should determine the horizontal color)
                let should_update = if let Some(&existing_source) = leftward_branch_sources.get(&next_node.column) {
                    let existing_distance = if existing_source > next_node.column {
                        existing_source - next_node.column
                    } else {
                        next_node.column - existing_source
                    };
                    let new_distance = merge_col - next_node.column;
                    new_distance < existing_distance
                } else {
                    true
                };
                if should_update {
                    leftward_branch_sources.insert(next_node.column, merge_col);
                }
            } else if merge_col < next_node.column {
                // Merging from left to right
                for c in (merge_col + 1)..next_node.column {
                    if col_chars[c] == "│ " {
                        col_chars[c] = "│─".to_string();
                    } else if col_chars[c] == "╯ " {
                        col_chars[c] = "╯─".to_string();
                    } else if col_chars[c].is_empty() {
                        col_chars[c] = "──".to_string();
                    }
                    merge_branch_cols.insert(c, merge_col);
                    // Also track for ╯─ and │─ horizontal styling
                    leftward_branch_sources.insert(c, merge_col);
                }
                // The tee at merge_col is styled based on merge_col (source branch)
                col_chars[merge_col] = "├─".to_string();
                merge_branch_cols.insert(merge_col, merge_col);
                // Track this for ├─ horizontal styling
                leftward_branch_sources.insert(merge_col, merge_col);
                // The bend at next_node.column is part of the branch, styled as the branch
                // Preserve any horizontal line passing through
                if col_chars[next_node.column].ends_with('─') {
                    col_chars[next_node.column] = "╯─".to_string();
                } else {
                    col_chars[next_node.column] = "╯ ".to_string();
                }
                merge_branch_cols.insert(next_node.column, merge_col);
            }
        }

        // Render the spans with per-column styling
        for col in 0..width {
            let char_str = if col < col_chars.len() && !col_chars[col].is_empty() {
                col_chars[col].clone()
            } else {
                "  ".to_string()
            };

            // Determine the style for this column
            let col_style = if current_branch_cols.contains(&col) {
                // Columns with horizontal lines from current node's branch use current node's style
                current_node_style
            } else if let Some(&source_col) = merge_branch_cols.get(&col) {
                // Columns with horizontal lines from other merging branches use their branch's style
                if let Some(&in_current_branch) = col_to_in_current_branch.get(&source_col) {
                    self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                } else {
                    current_node_style
                }
            } else if let Some(&in_current_branch) = col_to_in_current_branch.get(&col) {
                // Use the pre-calculated ancestry status for passthrough vertical lines
                self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
            } else {
                // Default to current node's style for non-active columns
                current_node_style
            };

            // Special case: split ├─ into ├ and ─ with different styles
            // ├ uses the target column style, ─ uses the source column style
            if char_str == "├─" || char_str == "╭─" {
                let (tee_char, horizontal_char) = if char_str == "├─" {
                    ("├", "─")
                } else {
                    ("╭", "─")
                };

                // Determine tee style
                // For leftward merges, tee should use next_node's style (merging into next_node)
                let tee_style = if leftward_merge_targets.contains(&col) {
                    // This is a leftward merge target - use next_node's style
                    let next_in_current_branch = next_node.in_current_branch;
                    self.commit_style(sync_status, on_ancestry_path, !next_in_current_branch)
                } else {
                    // Otherwise use the column's passthrough style
                    col_style
                };

                // Render tee with appropriate style
                spans.push(Span::styled(tee_char.to_string(), tee_style));

                // Look up the source column for this leftward branch
                // The horizontal should match the bend's color
                let horizontal_style = if let Some(&source_col) = leftward_branch_sources.get(&col) {
                    // Determine style the same way the bend (source column) would be styled
                    if current_branch_cols.contains(&source_col) {
                        current_node_style
                    } else if let Some(&merge_source) = merge_branch_cols.get(&source_col) {
                        if let Some(&in_current_branch) = col_to_in_current_branch.get(&merge_source) {
                            self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                        } else {
                            current_node_style
                        }
                    } else if let Some(&in_current_branch) = col_to_in_current_branch.get(&source_col) {
                        self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                    } else {
                        current_node_style
                    }
                } else {
                    col_style
                };

                // Render horizontal with source column style
                spans.push(Span::styled(horizontal_char.to_string(), horizontal_style));
            } else if char_str == "╯─" || char_str == "│─" {
                // Special case: split ╯─ or │─ into two parts with different styles
                // The vertical/bend part uses the column's passthrough style
                // The horizontal part uses the branch line style (similar to ├─)
                let (vertical_char, horizontal_char) = if char_str == "╯─" {
                    ("╯", "─")
                } else {
                    ("│", "─")
                };

                // The vertical/bend part uses the column's own passthrough style
                let vertical_style = if let Some(&in_current_branch) = col_to_in_current_branch.get(&col) {
                    self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                } else {
                    col_style  // Fall back to col_style if not in map
                };

                // The horizontal part: look up source column for leftward branches
                // (same logic as ├─ horizontal styling)
                let horizontal_style = if let Some(&source_col) = leftward_branch_sources.get(&col) {
                    // Determine style the same way the source bend would be styled
                    if current_branch_cols.contains(&source_col) {
                        current_node_style
                    } else if let Some(&merge_source) = merge_branch_cols.get(&source_col) {
                        if let Some(&in_current_branch) = col_to_in_current_branch.get(&merge_source) {
                            self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                        } else {
                            current_node_style
                        }
                    } else if let Some(&in_current_branch) = col_to_in_current_branch.get(&source_col) {
                        self.commit_style(sync_status, on_ancestry_path, !in_current_branch)
                    } else {
                        current_node_style
                    }
                } else {
                    col_style
                };

                spans.push(Span::styled(vertical_char.to_string(), vertical_style));
                spans.push(Span::styled(horizontal_char.to_string(), horizontal_style));
            } else {
                spans.push(Span::styled(char_str, col_style));
            }
        }

        Line::from(spans)
    }
}
