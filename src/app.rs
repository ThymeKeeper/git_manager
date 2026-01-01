use crate::git::GitRepo;
use crate::graph::{CommitGraph, GraphNode};
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitValidationResult {
    pub git_version: Option<String>,
    pub version_ok: bool,
    pub failed_commands: Vec<String>,
    pub warnings: Vec<String>,
}

impl GitValidationResult {
    pub fn new() -> Self {
        Self {
            git_version: None,
            version_ok: false,
            failed_commands: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn has_issues(&self) -> bool {
        !self.version_ok || !self.failed_commands.is_empty()
    }

    pub fn get_summary(&self) -> String {
        let mut msg = String::new();

        if let Some(version) = &self.git_version {
            if !self.version_ok {
                msg.push_str(&format!("⚠ Git version {} may not be fully supported (recommend 2.23+)\n", version));
            }
        } else {
            msg.push_str("⚠ Could not detect git version\n");
        }

        if !self.failed_commands.is_empty() {
            msg.push_str(&format!("⚠ {} git command(s) failed validation:\n", self.failed_commands.len()));
            for cmd in &self.failed_commands {
                msg.push_str(&format!("  - {}\n", cmd));
            }
        }

        for warning in &self.warnings {
            msg.push_str(&format!("⚠ {}\n", warning));
        }

        msg
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,
    Confirm,
    CommitMessage,
    BranchName,
    SelectBranch,
    SelectBranchToDelete,
    SetUserName,
    SetUserEmail,
    SquashCountInput,
    RewordMessage,
    SelectCommitsForBranch,
    AssignBranchName,
    FileDiffView,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPane {
    CommitGraph,
    GitActions,
    CommitDetails,
    GitStatus,
}

#[derive(Debug, Clone)]
pub enum GitCommand {
    Checkout,
    CreateBranch,
    ForceDeleteBranch,
    Reset,
    ResetSoft,
    ResetHard,
    CherryPick,
    Revert,
    Rebase,
    Merge,
    SquashCommits,
    Reword,
    AssignToBranch,
    Add,
    Commit,
    Push,
    Pull,
    PullAll,
    SetUserName,
    SetUserEmail,
}

impl GitCommand {
    pub fn description(&self) -> &str {
        match self {
            GitCommand::Checkout => "checkout",
            GitCommand::CreateBranch => "create branch",
            GitCommand::ForceDeleteBranch => "force delete branch",
            GitCommand::Reset => "reset --mixed",
            GitCommand::ResetSoft => "reset --soft",
            GitCommand::ResetHard => "reset --hard",
            GitCommand::CherryPick => "cherry-pick",
            GitCommand::Revert => "revert",
            GitCommand::Rebase => "rebase",
            GitCommand::Merge => "merge",
            GitCommand::SquashCommits => "squash last N commits",
            GitCommand::Reword => "reword commit message",
            GitCommand::AssignToBranch => "assign commits to branch",
            GitCommand::Add => "add -A",
            GitCommand::Commit => "commit",
            GitCommand::Push => "push",
            GitCommand::Pull => "pull",
            GitCommand::PullAll => "fetch and sync all branches from remote",
            GitCommand::SetUserName => "config user.name",
            GitCommand::SetUserEmail => "config user.email",
        }
    }

    pub fn needs_confirmation(&self) -> bool {
        match self {
            GitCommand::Checkout | GitCommand::Reset | GitCommand::ResetSoft | GitCommand::ResetHard | GitCommand::Rebase | GitCommand::Merge | GitCommand::CherryPick | GitCommand::Revert | GitCommand::Push => true,
            _ => false,
        }
    }

    pub fn confirmation_message(&self) -> &str {
        match self {
            GitCommand::Checkout => "Checkout this commit. Continue?",
            GitCommand::ForceDeleteBranch => "Force delete branch. This may orphan commits. Continue?",
            GitCommand::Reset => "Reset HEAD and unstage changes. Continue?",
            GitCommand::ResetSoft => "Reset HEAD but keep all changes staged. Continue?",
            GitCommand::ResetHard => "WARNING: Reset HEAD and DISCARD ALL CHANGES. Continue?",
            GitCommand::Rebase => "Rebase can rewrite history. Continue?",
            GitCommand::Merge => "Merge the selected commit into current branch. Continue?",
            GitCommand::CherryPick => "Cherry-pick the selected commit onto current branch. Continue?",
            GitCommand::Revert => "Revert the selected commit on current branch. Continue?",
            GitCommand::SquashCommits => "Squash commits. This will combine multiple commits into one. Continue?",
            GitCommand::Push => "Push changes to remote repository. Continue?",
            _ => "Are you sure?",
        }
    }
}

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub focused_pane: FocusedPane,
    pub details_expanded: bool,
    pub graph: CommitGraph,
    pub graph_nodes: Vec<GraphNode>,
    pub graph_width: usize,
    pub active_columns: Vec<usize>,
    pub selected_commit_idx: Option<usize>,
    pub current_branch: Option<String>,
    pub current_diff: Option<String>,
    pub git_user_name: Option<String>,
    pub git_user_email: Option<String>,
    pub git_repo: Option<GitRepo>,
    pub scroll_offset: usize,
    pub details_scroll_offset: usize,
    pub details_horizontal_offset: usize,
    pub command_scroll_offset: usize,
    pub command_list: Vec<GitCommand>,
    pub selected_command_idx: usize,
    pub status_message: Option<String>,
    pub status_message_time: Option<Instant>,
    pub pending_command: Option<GitCommand>,
    pub pending_command_message: Option<String>,
    pub git_status_files: Vec<StatusFile>,
    pub selected_file_idx: Option<usize>,
    pub commit_message_input: String,
    pub branch_name_input: String,
    pub pending_branch_commit_id: Option<String>,
    pub available_branches: Vec<String>,
    pub selected_branch_idx: usize,
    pub pending_checkout_commit_id: Option<String>,
    pub config_input: String,
    pub squash_count_input: String,
    pub pending_squash_commit_id: Option<String>,
    pub reword_message_input: String,
    pub pending_reword_commit_id: Option<String>,
    pub git_validation: Arc<Mutex<Option<GitValidationResult>>>,
    pub validation_checked: bool,
    pub selected_commit_ids: Vec<String>,
    pub assign_branch_name_input: String,
    pub commits_not_in_current_branch: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct StatusFile {
    pub path: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Staged,
    Modified,
    Untracked,
    Deleted,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            mode: AppMode::Normal,
            focused_pane: FocusedPane::CommitGraph,
            details_expanded: false,
            graph: CommitGraph::new(),
            graph_nodes: Vec::new(),
            graph_width: 0,
            active_columns: Vec::new(),
            selected_commit_idx: None,
            current_branch: None,
            current_diff: None,
            git_user_name: None,
            git_user_email: None,
            git_repo: None,
            scroll_offset: 0,
            details_scroll_offset: 0,
            details_horizontal_offset: 0,
            command_scroll_offset: 0,
            command_list: vec![
                GitCommand::Checkout,
                GitCommand::CreateBranch,
                GitCommand::ForceDeleteBranch,
                GitCommand::Reset,
                GitCommand::ResetSoft,
                GitCommand::ResetHard,
                GitCommand::CherryPick,
                GitCommand::Revert,
                GitCommand::Rebase,
                GitCommand::Merge,
                GitCommand::SquashCommits,
                GitCommand::Reword,
                GitCommand::AssignToBranch,
                GitCommand::Add,
                GitCommand::Commit,
                GitCommand::Push,
                GitCommand::Pull,
                GitCommand::PullAll,
                GitCommand::SetUserName,
                GitCommand::SetUserEmail,
            ],
            selected_command_idx: 0,
            status_message: None,
            status_message_time: None,
            pending_command: None,
            pending_command_message: None,
            git_status_files: Vec::new(),
            selected_file_idx: None,
            commit_message_input: String::new(),
            branch_name_input: String::new(),
            pending_branch_commit_id: None,
            available_branches: Vec::new(),
            selected_branch_idx: 0,
            pending_checkout_commit_id: None,
            config_input: String::new(),
            squash_count_input: String::new(),
            pending_squash_commit_id: None,
            reword_message_input: String::new(),
            pending_reword_commit_id: None,
            git_validation: Arc::new(Mutex::new(None)),
            validation_checked: false,
            selected_commit_ids: Vec::new(),
            assign_branch_name_input: String::new(),
            commits_not_in_current_branch: std::collections::HashSet::new(),
        }
    }

    pub fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Try to open git repository
        match GitRepo::open_current_dir() {
            Ok(repo) => {
                // Load current branch
                if let Ok(branch) = repo.get_current_branch() {
                    self.current_branch = Some(branch);
                }

                // Load git user configuration
                self.load_git_user_config();

                // Load commits
                let mut graph = repo.load_commits()?;

                // Get main branch commit ID (try "master" first, then "main")
                let main_branch_commit = repo.get_branch_commit_id("master")
                    .or_else(|_| repo.get_branch_commit_id("main"))
                    .ok();

                // Perform topological sort (returns oldest-to-newest)
                let sorted_commits = graph.topological_sort();

                // Calculate which commits are not in current branch BEFORE creating nodes
                // This populates commits_not_in_current_branch HashSet
                self.commits_not_in_current_branch.clear();
                for commit_id in &sorted_commits {
                    if !self.is_ancestor_of_head(commit_id) {
                        self.commits_not_in_current_branch.insert(commit_id.clone());
                    }
                }

                // Reverse to newest-to-oldest for column assignment
                // This ensures the newest child continues in parent's lane (main line)
                let mut newest_first = sorted_commits.clone();
                newest_first.reverse();
                self.graph_nodes = self.assign_columns(&mut graph, &newest_first, main_branch_commit, &repo);

                self.graph = graph;
                self.git_repo = Some(repo);

                // Select first commit by default
                if !self.graph_nodes.is_empty() {
                    self.selected_commit_idx = Some(0);
                    // Trace ancestry for the first commit
                    if let Some(node) = self.graph_nodes.first() {
                        self.graph.trace_ancestry(&node.commit.id);
                    }
                    // Load diff for the first commit
                    self.load_current_diff();
                }

                // Load git status
                self.load_git_status();

                // Start git validation in background
                self.start_git_validation();

                Ok(())
            }
            Err(e) => {
                Err(format!("Failed to open git repository: {}", e).into())
            }
        }
    }

    fn assign_columns(&mut self, graph: &mut CommitGraph, sorted_commits: &[String], main_branch_commit: Option<String>, repo: &GitRepo) -> Vec<GraphNode> {
        use crate::graph::Connection;
        use std::collections::{HashMap, HashSet};

        let mut nodes = Vec::new();
        let mut commit_columns: HashMap<String, usize> = HashMap::new();

        // Step 1: Identify main branch (follow first-parent chain from master/main)
        let mut main_branch: HashSet<String> = HashSet::new();
        let start_commit = main_branch_commit.or_else(|| sorted_commits.first().cloned());

        if let Some(head_commit_id) = start_commit {
            let mut current = head_commit_id.clone();
            while let Some(commit) = graph.commits.get(&current) {
                main_branch.insert(current.clone());
                if let Some(first_parent) = commit.parents.first() {
                    current = first_parent.clone();
                } else {
                    break;
                }
            }
        }

        // Step 2: Assign column 0 to all main branch commits
        for commit_id in &main_branch {
            commit_columns.insert(commit_id.clone(), 0);
        }

        // PASS 1: Assign columns using simple lane reuse (when lanes have no live commits)
        let mut next_column = 1;
        let mut active_lanes: HashSet<usize> = HashSet::new();
        active_lanes.insert(0); // Lane 0 (master) is always active

        // Track which commits are "live" in each lane (seen but parent not processed yet)
        let mut lane_live_commits: HashMap<usize, HashSet<String>> = HashMap::new();
        lane_live_commits.insert(0, HashSet::new());

        for (commit_idx, commit_id) in sorted_commits.iter().enumerate() {
            if let Some(commit) = graph.commits.get(commit_id).cloned() {
                let mut connections = Vec::new();

                // Get this commit's column (already assigned for main branch)
                let column = if let Some(&col) = commit_columns.get(commit_id) {
                    // Already assigned, add to live commits for this lane
                    lane_live_commits.entry(col).or_insert_with(HashSet::new).insert(commit_id.clone());
                    col
                } else {
                    // Not on main branch, try to reuse an inactive lane (simple check: no live commits)
                    let mut col = None;
                    for lane in 1..next_column {
                        let is_empty = lane_live_commits.get(&lane).map(|s| s.is_empty()).unwrap_or(true);
                        if is_empty {
                            col = Some(lane);
                            break;
                        }
                    }

                    // If no inactive lane found, use next_column
                    let col = col.unwrap_or_else(|| {
                        let c = next_column;
                        next_column += 1;
                        c
                    });

                    commit_columns.insert(commit_id.clone(), col);
                    active_lanes.insert(col);
                    lane_live_commits.entry(col).or_insert_with(HashSet::new).insert(commit_id.clone());

                    col
                };

                // Handle parent relationships
                if !commit.parents.is_empty() {
                    if commit.parents.len() == 1 {
                        // Single parent
                        let parent_id = &commit.parents[0];
                        let parent_col = commit_columns.get(parent_id).copied();

                        if let Some(pcol) = parent_col {
                            if pcol != column {
                                // Parent in different column, branch to it
                                // DON'T remove from live commits yet - keep lane active until parent is reached
                                // The lane stays occupied to show the branch line extending to the parent
                                connections.push(Connection::BranchTo(pcol));
                            } else {
                                // Same column, vertical connection
                                // DON'T remove from live commits yet - keep lane active until parent is reached
                                connections.push(Connection::Vertical);
                            }
                        } else {
                            // Parent not assigned yet, give it our column
                            connections.push(Connection::Vertical);
                            commit_columns.insert(parent_id.clone(), column);
                        }

                        // Remove this commit from live commits now that it's been processed
                        // But only if the parent has already been processed (is earlier in the list)
                        // For commits whose parents come later (feature branches), keep them live
                        let parent_processed = sorted_commits.iter()
                            .take(commit_idx)
                            .any(|id| id == parent_id);

                        if parent_processed {
                            if let Some(live) = lane_live_commits.get_mut(&column) {
                                live.remove(commit_id);
                            }
                        }
                    } else {
                        // Merge commit
                        let first_parent_id = &commit.parents[0];

                        // Ensure first parent has same column as merge commit
                        commit_columns.entry(first_parent_id.clone()).or_insert(column);

                        // Handle other parents (merged branches)
                        for parent_id in commit.parents.iter().skip(1) {
                            let parent_col = if let Some(&col) = commit_columns.get(parent_id) {
                                col
                            } else {
                                // Assign new column for this merged branch
                                let mut col = None;
                                for lane in 1..next_column {
                                    if lane_live_commits.get(&lane).map(|s| s.is_empty()).unwrap_or(true) {
                                        col = Some(lane);
                                        break;
                                    }
                                }
                                let col = col.unwrap_or_else(|| {
                                    let c = next_column;
                                    next_column += 1;
                                    c
                                });
                                commit_columns.insert(parent_id.clone(), col);
                                active_lanes.insert(col);
                                // Mark this lane as live by adding a placeholder for the parent branch
                                // This prevents other commits from reusing this lane until the branch is processed
                                lane_live_commits.entry(col).or_insert_with(HashSet::new).insert(parent_id.clone());
                                col
                            };
                            connections.push(Connection::MergeFrom(parent_col));
                        }

                        // This merge commit is done, remove from live commits
                        if let Some(live) = lane_live_commits.get_mut(&column) {
                            live.remove(commit_id);
                        }
                    }
                } else {
                    // No parents (root commit), remove from live commits
                    if let Some(live) = lane_live_commits.get_mut(&column) {
                        live.remove(commit_id);
                    }
                }

                nodes.push(GraphNode {
                    commit: commit.clone(),
                    column,
                    connections,
                    in_current_branch: !self.is_commit_not_in_current_branch(commit_id),
                });
            }
        }

        // PASS 2: Compact lanes by detecting non-overlapping branches and reassigning
        if true {
        // Build extent map for each lane (position range in sorted_commits)
        // This needs to account for:
        // 1. Commits in the lane
        // 2. Merge commits that create branch lines in the lane
        // 3. Edge rows after commits
        let mut lane_extent_map: HashMap<usize, (usize, usize)> = HashMap::new();

        for (idx, commit_id) in sorted_commits.iter().enumerate() {
            if let Some(&col) = commit_columns.get(commit_id) {
                // Add this commit's extent - extend to parent location
                if let Some(commit) = graph.commits.get(commit_id) {
                    // Find the furthest parent index for this commit
                    let mut max_parent_idx = idx;
                    for parent_id in &commit.parents {
                        if let Some(parent_idx) = sorted_commits.iter().position(|id| id == parent_id) {
                            max_parent_idx = max_parent_idx.max(parent_idx);
                        }
                    }
                    // Extend by 1 for the edge row after the furthest parent
                    lane_extent_map.entry(col)
                        .and_modify(|e| { e.0 = e.0.min(idx); e.1 = e.1.max(max_parent_idx + 1); })
                        .or_insert((idx, max_parent_idx + 1));

                    // Check if this is a merge commit that creates branch lines in other columns
                    if commit.parents.len() > 1 {
                        // This is a merge commit - check parent columns
                        for parent_id in &commit.parents {
                            if let Some(&parent_col) = commit_columns.get(parent_id) {
                                if parent_col != col {
                                    // Parent is in a different column - this creates a branch line
                                    // The branch line starts from the row AFTER this merge commit
                                    // (the merge commit itself is in the main column, the branch line
                                    // starts on the edge row below it)
                                    lane_extent_map.entry(parent_col)
                                        .and_modify(|e| { e.0 = e.0.min(idx + 1); })
                                        .or_insert((idx + 1, idx + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Try to compact higher lanes into lower lanes if they don't overlap
        let max_lane = commit_columns.values().max().copied().unwrap_or(0);

        // Build set of lanes that actually have commits (not just extents from merge edges)
        let mut lanes_with_commits: HashSet<usize> = HashSet::new();
        for &col in commit_columns.values() {
            lanes_with_commits.insert(col);
        }

        let mut lane_mapping: HashMap<usize, usize> = HashMap::new();

        // Initialize with identity mapping
        for lane in 0..=max_lane {
            lane_mapping.insert(lane, lane);
        }

        for source_lane in 1..=max_lane {
                // Only try to compact lanes that actually have commits
                if !lanes_with_commits.contains(&source_lane) {
                    continue;
                }

                if let Some(&(src_start, src_end)) = lane_extent_map.get(&source_lane) {
                    // Try to find a lower lane to move this to
                    let mut target_lane = source_lane;

                    for candidate in 1..source_lane {
                        // Check if candidate lane has actual commits
                        let has_commits = lanes_with_commits.contains(&candidate);

                        if !has_commits {
                            // Candidate lane has no commits, can definitely use it
                            target_lane = candidate;
                            lane_extent_map.insert(candidate, (src_start, src_end));
                            break;
                        } else if let Some(&(cand_start, cand_end)) = lane_extent_map.get(&candidate) {
                            // Lane has commits, check if ranges don't overlap
                            // Two ranges [a,b] and [c,d] don't overlap if b <= c OR d <= a
                            // This allows lanes that end exactly where another begins (adjacent rows)
                            // Since we've already extended extents by 1 for edge rows, adjacent is safe
                            let no_overlap = src_end <= cand_start || cand_end <= src_start;
                            if no_overlap {
                                target_lane = candidate;
                                // Update candidate's extent to include source
                                lane_extent_map.entry(candidate).and_modify(|e| {
                                    e.0 = e.0.min(src_start);
                                    e.1 = e.1.max(src_end);
                                });
                                break;
                            }
                        }
                    }

                    if target_lane != source_lane {
                        // Update lanes_with_commits: source lane no longer has commits, target lane now has them
                        lanes_with_commits.remove(&source_lane);
                        lanes_with_commits.insert(target_lane);
                    }
                    *lane_mapping.get_mut(&source_lane).unwrap() = target_lane;
                }
            }

        // Apply lane mapping to all commits and nodes
        for col in commit_columns.values_mut() {
            if let Some(&new_col) = lane_mapping.get(col) {
                *col = new_col;
            }
        }

        for node in nodes.iter_mut() {
            if let Some(&new_col) = lane_mapping.get(&node.column) {
                node.column = new_col;
            }
            // Also update connections that reference columns
            for conn in node.connections.iter_mut() {
                match conn {
                    Connection::BranchTo(col) => {
                        if let Some(&new_col) = lane_mapping.get(col) {
                            *col = new_col;
                        }
                    }
                    Connection::MergeFrom(col) => {
                        if let Some(&new_col) = lane_mapping.get(col) {
                            *col = new_col;
                        }
                    }
                    _ => {}
                }
            }
        }

        } // End if false

        // Update graph width
        self.graph_width = commit_columns.values().max().copied().unwrap_or(0) + 1;

        // Safety check: ensure all node columns and connection references are within bounds
        let max_col = self.graph_width.saturating_sub(1);
        for node in nodes.iter_mut() {
            node.column = node.column.min(max_col);
            for conn in node.connections.iter_mut() {
                match conn {
                    Connection::BranchTo(col) => *col = (*col).min(max_col),
                    Connection::MergeFrom(col) => *col = (*col).min(max_col),
                    _ => {}
                }
            }
        }

        // Calculate active columns
        let mut active_cols: Vec<usize> = commit_columns.values().copied().collect();
        active_cols.sort();
        active_cols.dedup();
        self.active_columns = active_cols;

        nodes
    }

    pub fn move_selection_up(&mut self) {
        if let Some(idx) = self.selected_commit_idx {
            if idx > 0 {
                self.selected_commit_idx = Some(idx - 1);
                self.update_selection();
            }
        } else if !self.graph_nodes.is_empty() {
            self.selected_commit_idx = Some(0);
            self.update_selection();
        }
    }

    pub fn move_selection_down(&mut self) {
        if let Some(idx) = self.selected_commit_idx {
            if idx + 1 < self.graph_nodes.len() {
                self.selected_commit_idx = Some(idx + 1);
                self.update_selection();
            }
        } else if !self.graph_nodes.is_empty() {
            self.selected_commit_idx = Some(0);
            self.update_selection();
        }
    }


    pub fn load_current_diff(&mut self) {
        if let Some(idx) = self.selected_commit_idx {
            if let Some(node) = self.graph_nodes.get(idx) {
                if let Some(ref repo) = self.git_repo {
                    if let Ok(diff) = repo.get_commit_diff(&node.commit.id) {
                        self.current_diff = Some(diff);
                        self.details_scroll_offset = 0; // Reset scroll when loading new diff
                    }
                }
            }
        }
    }

    pub fn load_file_diff(&mut self) {
        use std::process::Command;

        if let Some(idx) = self.selected_file_idx {
            if let Some(file) = self.git_status_files.get(idx) {
                let diff_output = match file.status {
                    FileStatus::Staged => {
                        // Show staged diff
                        Command::new("git")
                            .args(&["diff", "--cached", "--", &file.path])
                            .output()
                    }
                    FileStatus::Modified | FileStatus::Deleted => {
                        // Show unstaged diff
                        Command::new("git")
                            .args(&["diff", "--", &file.path])
                            .output()
                    }
                    FileStatus::Untracked => {
                        // Show file contents for untracked files
                        Command::new("git")
                            .args(&["diff", "--no-index", "/dev/null", &file.path])
                            .output()
                    }
                };

                if let Ok(output) = diff_output {
                    if output.status.success() || !output.stdout.is_empty() {
                        self.current_diff = Some(String::from_utf8_lossy(&output.stdout).to_string());
                        self.details_scroll_offset = 0;
                    }
                }
            }
        }
    }

    pub fn open_file_diff_view(&mut self) {
        if self.selected_file_idx.is_some() {
            self.load_file_diff();
            // Reset scroll offsets for file diff viewer
            self.details_scroll_offset = 0;
            self.details_horizontal_offset = 0;
            self.mode = AppMode::FileDiffView;
        }
    }

    pub fn close_file_diff_view(&mut self) {
        self.mode = AppMode::Normal;
        // Reset scroll offsets when returning to commit details
        self.details_scroll_offset = 0;
        self.details_horizontal_offset = 0;
    }

    fn update_selection(&mut self) {
        // Automatically trace ancestry when selection changes
        if let Some(idx) = self.selected_commit_idx {
            if let Some(node) = self.graph_nodes.get(idx) {
                self.graph.trace_ancestry(&node.commit.id);
            }
        }
        // Also load the diff for the new selection
        self.load_current_diff();
    }

    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        if let Some(selected_idx) = self.selected_commit_idx {
            // Each commit takes 2 lines (node row + edge row), except the last one
            let selected_row = selected_idx * 2;

            // Scroll up if selection is above viewport
            if selected_row < self.scroll_offset {
                self.scroll_offset = selected_row;
            }

            // Scroll down if selection is below viewport
            // Leave some margin at the bottom
            if selected_row >= self.scroll_offset + viewport_height.saturating_sub(1) {
                self.scroll_offset = selected_row.saturating_sub(viewport_height.saturating_sub(2));
            }
        }
    }

    pub fn adjust_command_scroll(&mut self, viewport_height: usize) {
        let selected_idx = self.selected_command_idx;

        // Scroll up if selection is above viewport
        if selected_idx < self.command_scroll_offset {
            self.command_scroll_offset = selected_idx;
        }

        // Scroll down if selection is below viewport
        if selected_idx >= self.command_scroll_offset + viewport_height {
            self.command_scroll_offset = selected_idx.saturating_sub(viewport_height - 1);
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_message_time = Some(Instant::now());
    }

    pub fn clear_expired_status_message(&mut self) {
        if let Some(time) = self.status_message_time {
            if time.elapsed().as_secs() >= 3 {
                self.status_message = None;
                self.status_message_time = None;
            }
        }
    }

    fn load_git_user_config(&mut self) {
        use std::process::Command;

        // Load user.name
        if let Ok(output) = Command::new("git")
            .args(&["config", "user.name"])
            .output()
        {
            if output.status.success() {
                let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !name.is_empty() {
                    self.git_user_name = Some(name);
                }
            }
        }

        // Load user.email
        if let Ok(output) = Command::new("git")
            .args(&["config", "user.email"])
            .output()
        {
            if output.status.success() {
                let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !email.is_empty() {
                    self.git_user_email = Some(email);
                }
            }
        }
    }

    pub fn next_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::CommitGraph => FocusedPane::GitActions,
            FocusedPane::GitActions => FocusedPane::CommitDetails,
            FocusedPane::CommitDetails => FocusedPane::GitStatus,
            FocusedPane::GitStatus => FocusedPane::CommitGraph,
        };
    }

    pub fn details_scroll_up(&mut self) {
        if self.details_scroll_offset > 0 {
            self.details_scroll_offset -= 1;
        }
    }

    pub fn details_scroll_down(&mut self) {
        self.details_scroll_offset += 1;
    }

    pub fn details_scroll_left(&mut self) {
        if self.details_horizontal_offset > 0 {
            self.details_horizontal_offset = self.details_horizontal_offset.saturating_sub(5);
        }
    }

    pub fn details_scroll_right(&mut self) {
        self.details_horizontal_offset += 5;
    }

    pub fn command_up(&mut self) {
        if self.selected_command_idx > 0 {
            self.selected_command_idx -= 1;
        }
    }

    pub fn command_down(&mut self) {
        if self.selected_command_idx + 1 < self.command_list.len() {
            self.selected_command_idx += 1;
        }
    }

    pub fn execute_selected_command(&mut self) {
        if let Some(command) = self.command_list.get(self.selected_command_idx).cloned() {
            // Check if command needs confirmation
            if command.needs_confirmation() {
                // Generate detailed confirmation message for commands that need it
                let detailed_message = match command {
                    GitCommand::Checkout | GitCommand::Reset | GitCommand::ResetSoft | GitCommand::ResetHard | GitCommand::Merge | GitCommand::Rebase | GitCommand::CherryPick | GitCommand::Revert => {
                        if let Some(idx) = self.selected_commit_idx {
                            if let Some(node) = self.graph_nodes.get(idx) {
                                let selected_id = &node.commit.id;
                                let selected_short = &selected_id[..7];
                                let selected_branch = self.get_branch_name_for_commit(selected_id);

                                let current_branch = self.current_branch.as_deref().unwrap_or("detached HEAD");

                                let source_desc = if let Some(ref branch) = selected_branch {
                                    format!("{} ({})", branch, selected_short)
                                } else {
                                    format!("commit {}", selected_short)
                                };

                                match command {
                                    GitCommand::Checkout => {
                                        Some(format!(
                                            "Checkout {}?\n\nThis will update your working directory to match this commit.\nYou may enter detached HEAD state if this is not a branch tip.\nUncommitted changes may be lost if they conflict.",
                                            source_desc
                                        ))
                                    }
                                    GitCommand::Reset => {
                                        let is_ancestor = self.is_ancestor_of_head(selected_id);
                                        let cross_branch_warning = if !is_ancestor {
                                            "\n\n⚠️  WARNING: This commit is not on your current branch's history!\nResetting here will ORPHAN your branch's commits."
                                        } else {
                                            ""
                                        };
                                        Some(format!(
                                            "Reset {} to {}?\n\nThis will:\n- Move HEAD to this commit\n- Unstage all changes\n- Keep your working directory files unchanged\n- Commits after this point will become unreachable{}",
                                            current_branch, source_desc, cross_branch_warning
                                        ))
                                    }
                                    GitCommand::ResetSoft => {
                                        let is_ancestor = self.is_ancestor_of_head(selected_id);
                                        let cross_branch_warning = if !is_ancestor {
                                            "\n\n⚠️  WARNING: This commit is not on your current branch's history!\nResetting here will ORPHAN your branch's commits."
                                        } else {
                                            ""
                                        };
                                        Some(format!(
                                            "Soft reset {} to {}?\n\nThis will:\n- Move HEAD to this commit\n- Keep all changes staged\n- Keep your working directory files unchanged\n- Commits after this point will become unreachable{}",
                                            current_branch, source_desc, cross_branch_warning
                                        ))
                                    }
                                    GitCommand::ResetHard => {
                                        let is_ancestor = self.is_ancestor_of_head(selected_id);
                                        let cross_branch_warning = if !is_ancestor {
                                            "\n\n⚠️  CROSS-BRANCH RESET DETECTED!\nThis commit is not on your current branch's history!\nResetting here will ORPHAN your branch's commits AND DISCARD ALL YOUR WORK!"
                                        } else {
                                            ""
                                        };
                                        Some(format!(
                                            "⚠️  HARD RESET {} to {}? ⚠️\n\nWARNING: This will:\n- Move HEAD to this commit\n- DISCARD all staged changes\n- DISCARD all working directory changes\n- Commits after this point will become unreachable\n\nTHIS CANNOT BE UNDONE!{}",
                                            current_branch, source_desc, cross_branch_warning
                                        ))
                                    }
                                    GitCommand::Merge => {
                                        Some(format!(
                                            "Merge {} into {}?\n\nThis will create a merge commit on your current branch.",
                                            source_desc, current_branch
                                        ))
                                    }
                                    GitCommand::Rebase => {
                                        Some(format!(
                                            "Rebase {} onto {}?\n\nThis will replay your current branch's commits on top of {}.",
                                            current_branch, source_desc, source_desc
                                        ))
                                    }
                                    GitCommand::CherryPick => {
                                        Some(format!(
                                            "Cherry-pick {} onto {}?\n\nThis will apply the changes from this commit as a new commit on your current branch.",
                                            source_desc, current_branch
                                        ))
                                    }
                                    GitCommand::Revert => {
                                        Some(format!(
                                            "Revert {} on {}?\n\nThis will create a new commit that undoes the changes from this commit.",
                                            source_desc, current_branch
                                        ))
                                    }
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                self.pending_command = Some(command);
                self.pending_command_message = detailed_message;
                self.mode = AppMode::Confirm;
            } else {
                let result = self.execute_command(command);
                match result {
                    Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                    Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
                }
            }
        }
    }

    pub fn confirm_command(&mut self) {
        if let Some(command) = self.pending_command.take() {
            self.pending_command_message = None;
            self.mode = AppMode::Normal;

            // Special handling for ForceDeleteBranch with selected branch
            if matches!(command, GitCommand::ForceDeleteBranch) && !self.available_branches.is_empty() {
                if let Some(branch_name) = self.available_branches.get(self.selected_branch_idx).cloned() {
                    self.available_branches.clear();
                    self.selected_branch_idx = 0;
                    let result = self.execute_force_delete_branch(&branch_name);
                    match result {
                        Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                        Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
                    }
                }
            } else if matches!(command, GitCommand::SquashCommits) {
                // Special handling for SquashCommits
                if let Some(squash_info) = self.pending_squash_commit_id.take() {
                    // Parse "commit_id:count" format
                    let parts: Vec<&str> = squash_info.split(':').collect();
                    if parts.len() == 2 {
                        let commit_id = parts[0];
                        if let Ok(count) = parts[1].parse::<usize>() {
                            let result = self.execute_squash_commits(commit_id, count);
                            match result {
                                Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                                Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
                            }
                        }
                    }
                }
            } else {
                let result = self.execute_command(command);
                match result {
                    Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                    Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
                }
            }
        }
    }

    pub fn cancel_command(&mut self) {
        self.pending_command = None;
        self.pending_command_message = None;
        self.available_branches.clear();
        self.selected_branch_idx = 0;
        self.mode = AppMode::Normal;
    }

    fn get_branch_name_for_commit(&self, commit_id: &str) -> Option<String> {
        use std::process::Command;

        // Get all branches that point to this commit
        let output = Command::new("git")
            .args(&["branch", "--points-at", commit_id, "--format=%(refname:short)"])
            .output()
            .ok()?;

        if output.status.success() {
            let branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Return the first branch name, or None if no branches
            branches.first().cloned()
        } else {
            None
        }
    }

    pub fn get_all_branches_for_commit(&self, commit_id: &str) -> Vec<String> {
        use std::process::Command;

        // Get all branches that point to this commit
        let output = Command::new("git")
            .args(&["branch", "--points-at", commit_id, "--format=%(refname:short)"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        Vec::new()
    }

    fn is_ancestor_of_head(&self, commit_id: &str) -> bool {
        use std::process::Command;

        // Check if commit_id is an ancestor of HEAD
        // git merge-base --is-ancestor <commit> HEAD returns 0 if true
        let result = Command::new("git")
            .args(&["merge-base", "--is-ancestor", commit_id, "HEAD"])
            .status();

        match result {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }

    fn calculate_commits_not_in_current_branch(&mut self) {
        // Calculate once which commits are not in current branch's history
        // This is expensive (calls git), so we only do it on refresh
        self.commits_not_in_current_branch.clear();
        for node in &self.graph_nodes {
            if !self.is_ancestor_of_head(&node.commit.id) {
                self.commits_not_in_current_branch.insert(node.commit.id.clone());
            }
        }
    }

    pub fn is_commit_not_in_current_branch(&self, commit_id: &str) -> bool {
        // Fast lookup - calculated once during refresh
        self.commits_not_in_current_branch.contains(commit_id)
    }

    pub fn open_commit_message_dialog(&mut self) {
        self.commit_message_input.clear();
        self.mode = AppMode::CommitMessage;
    }

    pub fn cancel_commit_message(&mut self) {
        self.commit_message_input.clear();
        self.mode = AppMode::Normal;
    }

    pub fn commit_message_input_char(&mut self, c: char) {
        self.commit_message_input.push(c);
    }

    pub fn commit_message_backspace(&mut self) {
        self.commit_message_input.pop();
    }

    pub fn submit_commit_message(&mut self) {
        if self.commit_message_input.trim().is_empty() {
            self.set_status_message("✗ Commit message cannot be empty".to_string());
            self.mode = AppMode::Normal;
            return;
        }

        let message = self.commit_message_input.clone();
        self.mode = AppMode::Normal;

        let result = self.execute_commit_with_message(&message);
        match result {
            Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
            Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
        }
    }

    pub fn cancel_branch_name(&mut self) {
        self.branch_name_input.clear();
        self.pending_branch_commit_id = None;
        self.mode = AppMode::Normal;
    }

    pub fn branch_selection_up(&mut self) {
        if self.selected_branch_idx > 0 {
            self.selected_branch_idx -= 1;
        }
    }

    pub fn branch_selection_down(&mut self) {
        if self.selected_branch_idx + 1 < self.available_branches.len() {
            self.selected_branch_idx += 1;
        }
    }

    pub fn select_branch(&mut self) {
        if let Some(branch_name) = self.available_branches.get(self.selected_branch_idx).cloned() {
            self.mode = AppMode::Normal;
            self.available_branches.clear();
            self.pending_checkout_commit_id = None;

            let result = self.execute_checkout_branch(&branch_name);
            match result {
                Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
            }
        }
    }

    pub fn cancel_branch_selection(&mut self) {
        self.available_branches.clear();
        self.selected_branch_idx = 0;
        self.pending_checkout_commit_id = None;
        self.mode = AppMode::Normal;
    }

    pub fn delete_branch_selection_up(&mut self) {
        if self.selected_branch_idx > 0 {
            self.selected_branch_idx -= 1;
        }
    }

    pub fn delete_branch_selection_down(&mut self) {
        if self.selected_branch_idx + 1 < self.available_branches.len() {
            self.selected_branch_idx += 1;
        }
    }

    pub fn select_branch_to_delete(&mut self) {
        if self.selected_branch_idx < self.available_branches.len() {
            // Generate confirmation message with selected branch name
            let branch_name = &self.available_branches[self.selected_branch_idx];
            self.pending_command_message = Some(format!(
                "Force delete branch '{}'?\n\n⚠️  WARNING: This will delete the branch using 'git branch -D'.\nCommits that are only reachable from this branch will become orphaned.\nOrphaned commits can be recovered from reflog for ~30 days.",
                branch_name
            ));

            // Move to confirmation dialog with the selected branch
            self.pending_command = Some(GitCommand::ForceDeleteBranch);
            self.mode = AppMode::Confirm;
        }
    }

    pub fn cancel_delete_branch_selection(&mut self) {
        self.available_branches.clear();
        self.selected_branch_idx = 0;
        self.mode = AppMode::Normal;
    }

    pub fn branch_name_input_char(&mut self, c: char) {
        self.branch_name_input.push(c);
    }

    pub fn branch_name_backspace(&mut self) {
        self.branch_name_input.pop();
    }

    pub fn submit_branch_name(&mut self) {
        if self.branch_name_input.trim().is_empty() {
            self.set_status_message("✗ Branch name cannot be empty".to_string());
            self.mode = AppMode::Normal;
            return;
        }

        let branch_name = self.branch_name_input.clone();
        let commit_id = if let Some(id) = self.pending_branch_commit_id.take() {
            id
        } else {
            self.set_status_message("✗ No commit selected".to_string());
            self.mode = AppMode::Normal;
            return;
        };

        self.mode = AppMode::Normal;
        self.branch_name_input.clear();

        let result = self.execute_create_branch_with_name(&commit_id, &branch_name);
        match result {
            Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
            Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
        }
    }

    fn execute_create_branch_with_name(&mut self, commit_id: &str, branch_name: &str) -> Result<String, String> {
        use std::process::Command;

        // Use 'git checkout -b' to create and checkout the branch in one command
        let output = Command::new("git")
            .args(&["checkout", "-b", branch_name, commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Created and checked out branch '{}'", branch_name))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    // Squash count input handlers
    pub fn squash_count_input_char(&mut self, c: char) {
        if c.is_ascii_digit() {
            self.squash_count_input.push(c);
        }
    }

    pub fn squash_count_backspace(&mut self) {
        self.squash_count_input.pop();
    }

    pub fn cancel_squash_count(&mut self) {
        self.squash_count_input.clear();
        self.pending_squash_commit_id = None;
        self.mode = AppMode::Normal;
    }

    pub fn submit_squash_count(&mut self) {
        let count: usize = match self.squash_count_input.trim().parse() {
            Ok(n) if n > 1 => n,
            _ => {
                self.set_status_message("✗ Please enter a number greater than 1".to_string());
                return;
            }
        };

        let commit_id = if let Some(id) = &self.pending_squash_commit_id {
            id.clone()
        } else {
            self.set_status_message("✗ No commit selected".to_string());
            return;
        };

        // Get the commit range starting from the selected commit going backwards
        use std::process::Command;
        let output = Command::new("git")
            .args(&["log", &format!("{}~{}", commit_id, count - 1), "--format=%h %s", &format!("-{}", count), &commit_id])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let commits = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = commits.lines().collect();

                if lines.is_empty() {
                    self.set_status_message("✗ Not enough commits to squash".to_string());
                    self.squash_count_input.clear();
                    return;
                }

                if lines.len() >= 2 {
                    let first = lines.first().unwrap_or(&"");
                    let last = lines.last().unwrap_or(&"");

                    self.pending_command = Some(GitCommand::SquashCommits);
                    self.pending_command_message = Some(format!(
                        "Squash {} commits from selected commit backwards?\n\nFrom: {} (oldest)\nTo:   {} (newest/selected)\n\nThis will combine these commits into one.\nAll commit messages will be combined.",
                        count, last, first
                    ));
                    self.pending_squash_commit_id = Some(format!("{}:{}", commit_id, count));
                    self.mode = AppMode::Confirm;
                } else {
                    self.set_status_message(format!("✗ Only {} commit(s) available", lines.len()).to_string());
                }
            } else {
                self.set_status_message("✗ Not enough commits in history".to_string());
            }
        }

        self.squash_count_input.clear();
    }

    // Reword message input handlers
    pub fn reword_message_input_char(&mut self, c: char) {
        self.reword_message_input.push(c);
    }

    pub fn reword_message_backspace(&mut self) {
        self.reword_message_input.pop();
    }

    pub fn cancel_reword_message(&mut self) {
        self.reword_message_input.clear();
        self.pending_reword_commit_id = None;
        self.mode = AppMode::Normal;
    }

    pub fn submit_reword_message(&mut self) {
        if self.reword_message_input.trim().is_empty() {
            self.set_status_message("✗ Commit message cannot be empty".to_string());
            return;
        }

        if let Some(commit_id) = self.pending_reword_commit_id.take() {
            let new_message = self.reword_message_input.clone();
            let result = self.execute_reword(&commit_id, &new_message);
            self.mode = AppMode::Normal;
            self.reword_message_input.clear();

            match result {
                Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
                Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
            }
        }
    }

    fn execute_reword(&mut self, commit_id: &str, new_message: &str) -> Result<String, String> {
        use std::process::Command;
        use std::env;

        // Safety check: only allow reword on branch tips to avoid rewriting shared history
        let branches_at_commit = self.get_all_branches_for_commit(commit_id);
        if branches_at_commit.is_empty() {
            return Err(format!(
                "Cannot reword: commit {} is not at any branch tip\n\nFor safety, you can only reword commits that are at the head of a branch.\nThis commit is in the middle of history or has been merged into other branches.",
                &commit_id[..7]
            ));
        }

        // First, check if the selected commit is reachable from HEAD
        let merge_base_output = Command::new("git")
            .args(&["merge-base", "--is-ancestor", commit_id, "HEAD"])
            .output()
            .map_err(|e| format!("Failed to check commit ancestry: {}", e))?;

        if !merge_base_output.status.success() {
            let current_branch = self.current_branch.as_deref().unwrap_or("detached HEAD");
            return Err(format!(
                "Cannot reword: commit {} is not on the current branch ({})\n\nYou need to checkout the branch containing this commit first.",
                &commit_id[..7],
                current_branch
            ));
        }

        // Check if this is the HEAD commit (simple case)
        let head_output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .output()
            .map_err(|e| format!("Failed to get HEAD: {}", e))?;

        let head_commit = String::from_utf8_lossy(&head_output.stdout).trim().to_string();

        if commit_id == head_commit {
            // Simple case: HEAD commit, use git commit --amend
            let output = Command::new("git")
                .args(&["commit", "--amend", "-m", new_message])
                .output()
                .map_err(|e| format!("Failed to execute git: {}", e))?;

            if output.status.success() {
                let _ = self.init();
                Ok("Reworded commit message".to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        } else {
            // Complex case: Non-HEAD commit, use interactive rebase
            // Get the parent commit
            let parent_output = Command::new("git")
                .args(&["rev-parse", &format!("{}^", commit_id)])
                .output()
                .map_err(|e| format!("Failed to get parent: {}", e))?;

            if !parent_output.status.success() {
                return Err("Cannot reword the initial commit".to_string());
            }

            let parent = String::from_utf8_lossy(&parent_output.stdout).trim().to_string();

            // Use interactive rebase with automatic editor commands
            // Set GIT_SEQUENCE_EDITOR to mark the commit for reword
            let sequence_cmd = format!("sed -i 's/^pick {}/reword {}/g'", &commit_id[..7], &commit_id[..7]);

            // Create a temp file with the new message
            let temp_msg_path = format!("/tmp/git_manager-reword-{}", commit_id);
            std::fs::write(&temp_msg_path, new_message)
                .map_err(|e| format!("Failed to write temp message: {}", e))?;

            // Start the rebase with automatic sequence editor
            let rebase_output = Command::new("sh")
                .arg("-c")
                .arg(&format!("GIT_SEQUENCE_EDITOR='{}' git rebase -i {}", sequence_cmd, parent))
                .output()
                .map_err(|e| format!("Failed to start rebase: {}", e))?;

            if !rebase_output.status.success() {
                let _ = std::fs::remove_file(&temp_msg_path);
                return Err("Rebase failed. This operation requires a clean working tree.".to_string());
            }

            // Rebase pauses at the reword commit, apply the new message
            let amend_output = Command::new("git")
                .args(&["commit", "--amend", "-m", new_message])
                .output()
                .map_err(|e| format!("Failed to amend: {}", e))?;

            if !amend_output.status.success() {
                let _ = std::fs::remove_file(&temp_msg_path);
                let _ = Command::new("git").args(&["rebase", "--abort"]).output();
                return Err("Failed to amend commit message".to_string());
            }

            // Continue the rebase
            let continue_output = Command::new("git")
                .args(&["rebase", "--continue"])
                .output()
                .map_err(|e| format!("Failed to continue rebase: {}", e))?;

            let _ = std::fs::remove_file(&temp_msg_path);

            if continue_output.status.success() {
                let _ = self.init();
                Ok("Reworded commit message".to_string())
            } else {
                Err("Rebase continuation failed".to_string())
            }
        }
    }

    fn execute_squash_commits(&mut self, commit_id: &str, count: usize) -> Result<String, String> {
        use std::process::Command;

        // Safety check: only allow squash on branch tips to avoid rewriting shared history
        let branches_at_commit = self.get_all_branches_for_commit(commit_id);
        if branches_at_commit.is_empty() {
            return Err(format!(
                "Cannot squash: commit {} is not at any branch tip\n\nFor safety, you can only squash commits that are at the head of a branch.\nThis commit is in the middle of history or has been merged into other branches.",
                &commit_id[..7]
            ));
        }

        // First, check if the selected commit is reachable from HEAD
        let merge_base_output = Command::new("git")
            .args(&["merge-base", "--is-ancestor", commit_id, "HEAD"])
            .output()
            .map_err(|e| format!("Failed to check commit ancestry: {}", e))?;

        if !merge_base_output.status.success() {
            let current_branch = self.current_branch.as_deref().unwrap_or("detached HEAD");
            return Err(format!(
                "Cannot squash: commit {} is not on the current branch ({})\n\nYou need to checkout the branch containing this commit first.",
                &commit_id[..7],
                current_branch
            ));
        }

        // Check if the selected commit is HEAD (simple case)
        let head_output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .output()
            .map_err(|e| format!("Failed to get HEAD: {}", e))?;

        let head_commit = String::from_utf8_lossy(&head_output.stdout).trim().to_string();

        if commit_id == head_commit {
            // Simple case: squashing from HEAD
            // Get the combined commit messages
            let msg_output = Command::new("git")
                .args(&["log", &format!("HEAD~{}", count - 1), "--format=%B", &format!("-{}", count)])
                .output()
                .map_err(|e| format!("Failed to get messages: {}", e))?;

            let combined_message = String::from_utf8_lossy(&msg_output.stdout).to_string();

            // Reset soft to N commits back
            let reset_output = Command::new("git")
                .args(&["reset", "--soft", &format!("HEAD~{}", count)])
                .output()
                .map_err(|e| format!("Failed to reset: {}", e))?;

            if !reset_output.status.success() {
                return Err(String::from_utf8_lossy(&reset_output.stderr).to_string());
            }

            // Create new commit with combined message
            let commit_output = Command::new("git")
                .args(&["commit", "-m", &combined_message])
                .output()
                .map_err(|e| format!("Failed to commit: {}", e))?;

            if commit_output.status.success() {
                let _ = self.init();
                Ok(format!("Squashed {} commits", count))
            } else {
                Err(String::from_utf8_lossy(&commit_output.stderr).to_string())
            }
        } else {
            // Complex case: squashing from a non-HEAD commit
            // This requires interactive rebase
            // Get parent of the oldest commit to squash (commit_id~(count-1))
            let parent_output = Command::new("git")
                .args(&["rev-parse", &format!("{}~{}", commit_id, count)])
                .output()
                .map_err(|e| format!("Failed to get parent: {}", e))?;

            if !parent_output.status.success() {
                return Err("Not enough commits in history to squash".to_string());
            }

            let parent = String::from_utf8_lossy(&parent_output.stdout).trim().to_string();

            // Get the list of commits to squash
            let commits_output = Command::new("git")
                .args(&["log", &format!("{}~{}", commit_id, count - 1), "--format=%H", &format!("-{}", count), commit_id])
                .output()
                .map_err(|e| format!("Failed to get commits: {}", e))?;

            let commits_str = String::from_utf8_lossy(&commits_output.stdout);
            let commit_list: Vec<&str> = commits_str
                .lines()
                .collect();

            if commit_list.is_empty() {
                return Err("No commits to squash".to_string());
            }

            // Create a sequence editor command that marks commits for squashing
            // First commit stays as "pick", rest become "squash"
            let mut sed_cmd = String::new();
            for (i, commit_sha) in commit_list.iter().rev().enumerate() {
                let short_sha = &commit_sha[..7];
                if i == 0 {
                    // Keep first commit as pick
                    continue;
                } else {
                    sed_cmd.push_str(&format!("s/^pick {}/squash {}/g; ", short_sha, short_sha));
                }
            }

            if sed_cmd.is_empty() {
                return Err("Only one commit, cannot squash".to_string());
            }

            // Start interactive rebase with automatic squashing
            let sequence_cmd = format!("sed -i '{}'", sed_cmd);
            let rebase_output = Command::new("sh")
                .arg("-c")
                .arg(&format!("EDITOR=true GIT_SEQUENCE_EDITOR='{}' git rebase -i {}", sequence_cmd, parent))
                .output()
                .map_err(|e| format!("Failed to rebase: {}", e))?;

            if rebase_output.status.success() {
                let _ = self.init();
                Ok(format!("Squashed {} commits", count))
            } else {
                let stderr = String::from_utf8_lossy(&rebase_output.stderr);
                // Parse stderr to give more specific error messages
                if stderr.contains("working tree") || stderr.contains("uncommitted changes") {
                    Err(format!("Squash failed: Working tree has uncommitted changes.\n\nCommit or stash your changes first.\n\n{}", stderr))
                } else if stderr.contains("could not detach HEAD") {
                    Err("Squash failed: Cannot rebase from this commit.\n\nThe commit may not be on the current branch.".to_string())
                } else {
                    Err(format!("Squash failed: {}", stderr))
                }
            }
        }
    }

    pub fn refresh(&mut self) {
        let _ = self.init();
        self.set_status_message("✓ Refreshed".to_string());
    }

    fn execute_command(&mut self, command: GitCommand) -> Result<String, String> {
        match command {
            GitCommand::Add => self.cmd_add(),
            GitCommand::Commit => self.cmd_commit(),
            GitCommand::Push => self.cmd_push(),
            GitCommand::Pull => self.cmd_pull(),
            GitCommand::PullAll => self.cmd_pull_all(),
            GitCommand::SetUserName => self.cmd_set_user_name(),
            GitCommand::SetUserEmail => self.cmd_set_user_email(),
            GitCommand::AssignToBranch => self.cmd_assign_to_branch(),
            _ => {
                let selected_idx = self.selected_commit_idx
                    .ok_or("No commit selected")?;
                let node = self.graph_nodes.get(selected_idx)
                    .ok_or("Invalid commit index")?;
                let commit_id = node.commit.id.clone();

                match command {
                    GitCommand::Checkout => self.cmd_checkout(&commit_id),
                    GitCommand::CreateBranch => self.cmd_create_branch(&commit_id),
                    GitCommand::ForceDeleteBranch => self.cmd_force_delete_branch(&commit_id),
                    GitCommand::Reset => self.cmd_reset(&commit_id),
                    GitCommand::ResetSoft => self.cmd_reset_soft(&commit_id),
                    GitCommand::ResetHard => self.cmd_reset_hard(&commit_id),
                    GitCommand::CherryPick => self.cmd_cherry_pick(&commit_id),
                    GitCommand::Revert => self.cmd_revert(&commit_id),
                    GitCommand::Rebase => self.cmd_rebase(&commit_id),
                    GitCommand::Merge => self.cmd_merge(&commit_id),
                    GitCommand::SquashCommits => self.cmd_squash_commits(&commit_id),
                    GitCommand::Reword => self.cmd_reword(&commit_id),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn cmd_checkout(&mut self, commit_id: &str) -> Result<String, String> {
        // Check if there are branches at this commit
        let branches = self.get_all_branches_for_commit(commit_id);

        match branches.len() {
            0 => {
                // No branches, checkout commit (detached HEAD)
                self.execute_checkout_commit(commit_id)
            }
            1 => {
                // One branch, checkout that branch
                self.execute_checkout_branch(&branches[0])
            }
            _ => {
                // Multiple branches, open selection dialog
                self.available_branches = branches;
                self.selected_branch_idx = 0;
                self.pending_checkout_commit_id = Some(commit_id.to_string());
                self.mode = AppMode::SelectBranch;
                Ok("Select branch to checkout...".to_string())
            }
        }
    }

    fn execute_checkout_commit(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["checkout", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph after checkout
            let _ = self.init();
            Ok(format!("Checked out {} (detached HEAD)", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn execute_checkout_branch(&mut self, branch_name: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["checkout", branch_name])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph after checkout
            let _ = self.init();
            Ok(format!("Checked out branch '{}'", branch_name))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_create_branch(&mut self, commit_id: &str) -> Result<String, String> {
        // Open branch name dialog
        self.branch_name_input.clear();
        self.pending_branch_commit_id = Some(commit_id.to_string());
        self.mode = AppMode::BranchName;
        Ok("Enter branch name...".to_string())
    }

    fn cmd_force_delete_branch(&mut self, commit_id: &str) -> Result<String, String> {
        // Check if there are branches at this commit
        let branches = self.get_all_branches_for_commit(commit_id);

        match branches.len() {
            0 => {
                // No branches, can't delete
                Err("No branches at this commit".to_string())
            }
            1 => {
                // One branch, go directly to confirmation dialog
                self.available_branches = branches;
                self.selected_branch_idx = 0;
                self.pending_command = Some(GitCommand::ForceDeleteBranch);

                // Generate confirmation message
                let branch_name = &self.available_branches[0];
                self.pending_command_message = Some(format!(
                    "Force delete branch '{}'?\n\n⚠️  WARNING: This will delete the branch using 'git branch -D'.\nCommits that are only reachable from this branch will become orphaned.\nOrphaned commits can be recovered from reflog for ~30 days.",
                    branch_name
                ));

                self.mode = AppMode::Confirm;
                Ok("Confirm to delete branch...".to_string())
            }
            _ => {
                // Multiple branches, open selection dialog
                self.available_branches = branches;
                self.selected_branch_idx = 0;
                self.mode = AppMode::SelectBranchToDelete;
                Ok("Select branch to delete...".to_string())
            }
        }
    }

    fn execute_force_delete_branch(&mut self, branch_name: &str) -> Result<String, String> {
        use std::process::Command;

        // Check if trying to delete current branch
        if let Some(ref current) = self.current_branch {
            if current == branch_name {
                return Err("Cannot delete the currently checked out branch".to_string());
            }
        }

        let output = Command::new("git")
            .args(&["branch", "-D", branch_name])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph after deletion
            let _ = self.init();
            Ok(format!("Force deleted branch '{}'", branch_name))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_reset(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        // Default to --mixed reset
        let output = Command::new("git")
            .args(&["reset", "--mixed", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Reset (mixed) to {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_reset_soft(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["reset", "--soft", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Reset (soft) to {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_reset_hard(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["reset", "--hard", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Reset (hard) to {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_cherry_pick(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["cherry-pick", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Cherry-picked {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_revert(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["revert", "--no-edit", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Reverted {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_rebase(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["rebase", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Rebased onto {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_merge(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["merge", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok(format!("Merged {}", &commit_id[..7]))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_squash_commits(&mut self, commit_id: &str) -> Result<String, String> {
        // Open dialog to input number of commits to squash from the selected commit backwards
        self.squash_count_input.clear();
        self.pending_squash_commit_id = Some(commit_id.to_string());
        self.mode = AppMode::SquashCountInput;
        Ok("Enter number of commits to squash...".to_string())
    }

    fn cmd_reword(&mut self, commit_id: &str) -> Result<String, String> {
        use std::process::Command;

        // Get the current commit message
        let output = Command::new("git")
            .args(&["log", "-1", "--format=%B", commit_id])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            let message = String::from_utf8_lossy(&output.stdout).trim().to_string();
            self.reword_message_input = message;
            self.pending_reword_commit_id = Some(commit_id.to_string());
            self.mode = AppMode::RewordMessage;
            Ok("Edit commit message...".to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_add(&mut self) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["add", "-A"])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            Ok("Staged all changes".to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_commit(&mut self) -> Result<String, String> {
        use std::process::Command;

        // Check if there are staged changes
        let status_output = Command::new("git")
            .args(&["diff", "--cached", "--quiet"])
            .status()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if status_output.success() {
            return Err("Nothing to commit (no staged changes)".to_string());
        }

        // Open commit message dialog
        self.open_commit_message_dialog();
        Ok("Opening commit message dialog".to_string())
    }

    fn execute_commit_with_message(&mut self, message: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["commit", "-m", message])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok("Commit created".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.to_string())
        }
    }

    fn cmd_push(&mut self) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["push"])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            Ok("Pushed to remote".to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_pull(&mut self) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["pull"])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload the graph
            let _ = self.init();
            Ok("Pulled from remote".to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn cmd_pull_all(&mut self) -> Result<String, String> {
        use std::process::Command;

        // First, fetch all remote branches
        let fetch = Command::new("git")
            .args(&["fetch", "--all"])
            .output()
            .map_err(|e| format!("Failed to fetch: {}", e))?;

        if !fetch.status.success() {
            return Err("Failed to fetch from remote".to_string());
        }

        // Get all remote branches
        let remote_branches_output = Command::new("git")
            .args(&["branch", "-r", "--format=%(refname:short)"])
            .output()
            .map_err(|e| format!("Failed to list remote branches: {}", e))?;

        if !remote_branches_output.status.success() {
            return Err("Failed to list remote branches".to_string());
        }

        let remote_branches: Vec<String> = String::from_utf8_lossy(&remote_branches_output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.contains("HEAD"))  // Skip HEAD pointer
            .collect();

        // Get all local branches to track what's new
        let local_branches_output = Command::new("git")
            .args(&["branch", "--format=%(refname:short)"])
            .output()
            .map_err(|e| format!("Failed to list local branches: {}", e))?;

        let local_branches: Vec<String> = String::from_utf8_lossy(&local_branches_output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut updated_count = 0;
        let mut created_count = 0;
        let mut skipped_count = 0;
        let mut errors = Vec::new();

        // Process each remote branch
        for remote_branch in remote_branches {
            // Extract branch name (remove "origin/" prefix)
            let branch_name = if let Some(name) = remote_branch.strip_prefix("origin/") {
                name.to_string()
            } else {
                continue;  // Skip non-origin remotes
            };

            let is_new = !local_branches.contains(&branch_name);

            // Use fetch with refspec to create or update local branch without checkout
            let fetch_result = Command::new("git")
                .args(&["fetch", "origin", &format!("{}:{}", branch_name, branch_name)])
                .output();

            match fetch_result {
                Ok(output) => {
                    if output.status.success() {
                        if is_new {
                            created_count += 1;
                        } else {
                            updated_count += 1;
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        // If it's a non-fast-forward error, that's expected (local changes exist)
                        if stderr.contains("non-fast-forward") || stderr.contains("would clobber") {
                            skipped_count += 1;
                        } else if !stderr.trim().is_empty() {
                            errors.push(format!("{}: {}", branch_name, stderr.trim()));
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", branch_name, e));
                }
            }
        }

        // Reload the graph
        let _ = self.init();

        // Build result message
        let total = created_count + updated_count;
        let mut message = if created_count > 0 {
            format!("Updated {} branches (created {} new)", total, created_count)
        } else {
            format!("Updated {} branches", total)
        };

        if skipped_count > 0 {
            message.push_str(&format!(", skipped {} (local changes)", skipped_count));
        }

        if !errors.is_empty() {
            message.push_str(&format!(", {} errors", errors.len()));
        }

        Ok(message)
    }

    fn cmd_set_user_name(&mut self) -> Result<String, String> {
        // Open input dialog
        self.config_input.clear();
        self.mode = AppMode::SetUserName;
        Ok("Enter user name...".to_string())
    }

    fn cmd_set_user_email(&mut self) -> Result<String, String> {
        // Open input dialog
        self.config_input.clear();
        self.mode = AppMode::SetUserEmail;
        Ok("Enter user email...".to_string())
    }

    pub fn config_input_char(&mut self, c: char) {
        self.config_input.push(c);
    }

    pub fn config_input_backspace(&mut self) {
        self.config_input.pop();
    }

    pub fn cancel_config_input(&mut self) {
        self.config_input.clear();
        self.mode = AppMode::Normal;
    }

    pub fn submit_user_name(&mut self) {
        if self.config_input.trim().is_empty() {
            self.set_status_message("✗ User name cannot be empty".to_string());
            self.mode = AppMode::Normal;
            return;
        }

        let name = self.config_input.clone();
        self.mode = AppMode::Normal;
        self.config_input.clear();

        let result = self.execute_set_user_name(&name);
        match result {
            Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
            Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
        }
    }

    pub fn submit_user_email(&mut self) {
        if self.config_input.trim().is_empty() {
            self.set_status_message("✗ User email cannot be empty".to_string());
            self.mode = AppMode::Normal;
            return;
        }

        let email = self.config_input.clone();
        self.mode = AppMode::Normal;
        self.config_input.clear();

        let result = self.execute_set_user_email(&email);
        match result {
            Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
            Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
        }
    }

    fn execute_set_user_name(&mut self, name: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["config", "user.name", name])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload user config to update status bar
            self.load_git_user_config();
            Ok(format!("Set user.name to '{}'", name))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn execute_set_user_email(&mut self, email: &str) -> Result<String, String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["config", "user.email", email])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        if output.status.success() {
            // Reload user config to update status bar
            self.load_git_user_config();
            Ok(format!("Set user.email to '{}'", email))
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    pub fn load_git_status(&mut self) {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["status", "--porcelain"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let status_text = String::from_utf8_lossy(&output.stdout);
                self.git_status_files = status_text
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(|line| {
                        let status_code = &line[..2];
                        let path = line[3..].to_string();

                        let status = match status_code {
                            "A " | "M " | "D " => FileStatus::Staged,  // Staged (added, modified, or deleted)
                            " M" | "MM" => FileStatus::Modified,
                            "??" => FileStatus::Untracked,
                            " D" => FileStatus::Deleted,  // Deleted but not staged
                            _ => FileStatus::Modified,
                        };

                        StatusFile { path, status }
                    })
                    .collect();
            }
        }
    }

    pub fn file_up(&mut self) {
        if let Some(idx) = self.selected_file_idx {
            if idx > 0 {
                self.selected_file_idx = Some(idx - 1);
            }
        } else if !self.git_status_files.is_empty() {
            self.selected_file_idx = Some(0);
        }
    }

    pub fn file_down(&mut self) {
        if let Some(idx) = self.selected_file_idx {
            if idx + 1 < self.git_status_files.len() {
                self.selected_file_idx = Some(idx + 1);
            }
        } else if !self.git_status_files.is_empty() {
            self.selected_file_idx = Some(0);
        }
    }

    pub fn toggle_file_staging(&mut self) {
        use std::process::Command;

        if let Some(idx) = self.selected_file_idx {
            if let Some(file) = self.git_status_files.get(idx) {
                let result = match file.status {
                    FileStatus::Staged => {
                        // Unstage the file
                        Command::new("git")
                            .args(&["reset", "HEAD", &file.path])
                            .output()
                    }
                    FileStatus::Modified | FileStatus::Untracked => {
                        // Stage the file
                        Command::new("git")
                            .args(&["add", &file.path])
                            .output()
                    }
                    FileStatus::Deleted => {
                        // Stage the deletion
                        Command::new("git")
                            .args(&["add", &file.path])
                            .output()
                    }
                };

                if let Ok(output) = result {
                    if output.status.success() {
                        // Reload git status
                        self.load_git_status();
                    }
                }
            }
        }
    }

    pub fn start_git_validation(&self) {
        let validation_result = Arc::clone(&self.git_validation);

        thread::spawn(move || {
            let result = Self::validate_git_environment();

            if let Ok(mut guard) = validation_result.lock() {
                *guard = Some(result);
            }
        });
    }

    fn validate_git_environment() -> GitValidationResult {
        use std::process::Command;

        let mut result = GitValidationResult::new();

        // Test 1: Check git version
        if let Ok(output) = Command::new("git").arg("--version").output() {
            if output.status.success() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                if let Some(version) = version_str.split_whitespace().nth(2) {
                    result.git_version = Some(version.to_string());

                    // Parse version and check if it's >= 2.23 (recommended)
                    if let Some(major_minor) = version.split('.').take(2).collect::<Vec<_>>().get(0..2) {
                        if let (Ok(major), Ok(minor)) = (major_minor[0].parse::<u32>(), major_minor[1].parse::<u32>()) {
                            result.version_ok = major > 2 || (major == 2 && minor >= 23);
                        }
                    }
                }
            }
        } else {
            result.warnings.push("Could not execute git command".to_string());
        }

        // Test 2: Validate git log format
        let log_test = Command::new("git")
            .args(&["log", "--pretty=format:%H|%h|%P|%s|%an|%at", "-1"])
            .output();

        if let Ok(output) = log_test {
            if !output.status.success() || output.stdout.is_empty() {
                result.failed_commands.push("git log --pretty=format:...".to_string());
            } else {
                // Validate format: should have 6 pipe-separated fields
                let log_str = String::from_utf8_lossy(&output.stdout);
                let fields: Vec<&str> = log_str.trim().split('|').collect();
                if fields.len() != 6 {
                    result.warnings.push(format!("git log format validation failed: expected 6 fields, got {}", fields.len()));
                }
            }
        } else {
            result.failed_commands.push("git log".to_string());
        }

        // Test 3: Validate git status --porcelain
        let status_test = Command::new("git")
            .args(&["status", "--porcelain"])
            .output();

        if let Err(_) = status_test {
            result.failed_commands.push("git status --porcelain".to_string());
        }

        // Test 4: Validate git rev-parse
        let revparse_test = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .output();

        if let Ok(output) = revparse_test {
            if !output.status.success() {
                result.warnings.push("Could not get HEAD commit (empty repo?)".to_string());
            }
        } else {
            result.failed_commands.push("git rev-parse HEAD".to_string());
        }

        // Test 5: Validate git branch
        let branch_test = Command::new("git")
            .args(&["branch", "--list"])
            .output();

        if let Err(_) = branch_test {
            result.failed_commands.push("git branch --list".to_string());
        }

        // Test 6: Validate git diff
        let diff_test = Command::new("git")
            .args(&["diff", "--help"])
            .output();

        if let Err(_) = diff_test {
            result.failed_commands.push("git diff".to_string());
        }

        // Test 7: Validate git commit --amend is available
        let commit_test = Command::new("git")
            .args(&["commit", "--help"])
            .output();

        if let Ok(output) = commit_test {
            let help_str = String::from_utf8_lossy(&output.stdout);
            if !help_str.contains("--amend") {
                result.warnings.push("git commit --amend may not be available".to_string());
            }
        } else {
            result.failed_commands.push("git commit".to_string());
        }

        // Test 8: Validate git rebase is available
        let rebase_test = Command::new("git")
            .args(&["rebase", "--help"])
            .output();

        if let Err(_) = rebase_test {
            result.failed_commands.push("git rebase".to_string());
        }

        // Test 9: Validate interactive rebase support
        let interactive_test = Command::new("git")
            .args(&["rebase", "--help"])
            .output();

        if let Ok(output) = interactive_test {
            let help_str = String::from_utf8_lossy(&output.stdout);
            if !help_str.contains("--interactive") && !help_str.contains("-i") {
                result.warnings.push("git rebase -i may not be available".to_string());
            }
        }

        result
    }

    pub fn check_validation_results(&mut self) {
        // Only check once
        if self.validation_checked {
            return;
        }

        let summary_opt = if let Ok(guard) = self.git_validation.lock() {
            if let Some(result) = guard.as_ref() {
                // Validation completed, mark as checked
                self.validation_checked = true;

                if result.has_issues() {
                    Some(result.get_summary())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Drop the guard before calling set_status_message
        if let Some(summary) = summary_opt {
            self.set_status_message(summary);
        }
    }

    fn cmd_assign_to_branch(&mut self) -> Result<String, String> {
        self.selected_commit_ids.clear();
        self.assign_branch_name_input.clear();
        self.mode = AppMode::SelectCommitsForBranch;
        self.focused_pane = FocusedPane::CommitGraph;
        Ok("Select commits with Space, Enter when done, Esc to cancel".to_string())
    }

    pub fn toggle_commit_selection(&mut self) {
        if let Some(idx) = self.selected_commit_idx {
            if let Some(node) = self.graph_nodes.get(idx) {
                let commit_id = node.commit.id.clone();
                if let Some(pos) = self.selected_commit_ids.iter().position(|id| id == &commit_id) {
                    self.selected_commit_ids.remove(pos);
                } else {
                    self.selected_commit_ids.push(commit_id);
                }
            }
        }
    }

    pub fn finish_commit_selection(&mut self) {
        if self.selected_commit_ids.is_empty() {
            self.set_status_message("No commits selected".to_string());
            self.mode = AppMode::Normal;
            return;
        }
        self.mode = AppMode::AssignBranchName;
    }

    pub fn cancel_commit_selection(&mut self) {
        self.selected_commit_ids.clear();
        self.mode = AppMode::Normal;
    }

    pub fn assign_branch_name_input_char(&mut self, c: char) {
        self.assign_branch_name_input.push(c);
    }

    pub fn assign_branch_name_backspace(&mut self) {
        self.assign_branch_name_input.pop();
    }

    pub fn cancel_assign_branch_name(&mut self) {
        self.assign_branch_name_input.clear();
        self.selected_commit_ids.clear();
        self.mode = AppMode::Normal;
    }

    pub fn submit_assign_branch_name(&mut self) {
        let branch_name = self.assign_branch_name_input.trim().to_string();
        if branch_name.is_empty() {
            self.set_status_message("Branch name cannot be empty".to_string());
            return;
        }

        match self.assign_commits_to_branch(&branch_name) {
            Ok(msg) => {
                self.set_status_message(msg);
                self.assign_branch_name_input.clear();
                self.selected_commit_ids.clear();
                self.mode = AppMode::Normal;
                self.refresh();
            }
            Err(e) => {
                self.set_status_message(format!("Error: {}", e));
            }
        }
    }

    fn assign_commits_to_branch(&mut self, branch_name: &str) -> Result<String, String> {
        if self.selected_commit_ids.is_empty() {
            return Err("No commits selected".to_string());
        }

        // Find the newest commit among selected commits by finding it in graph_nodes
        // (graph_nodes is already sorted newest to oldest)
        let newest_commit = self.graph_nodes.iter()
            .find(|node| self.selected_commit_ids.contains(&node.commit.id))
            .map(|node| node.commit.id.clone())
            .ok_or_else(|| "Selected commit not found in graph".to_string())?;

        // Use git branch -f to create or move the branch to point at the newest selected commit
        // This will automatically include all ancestors of that commit in the branch history
        let output = Command::new("git")
            .args(&["branch", "-f", branch_name, &newest_commit])
            .output()
            .map_err(|e| format!("Failed to create/move branch: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to create/move branch: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(format!(
            "Assigned {} commit(s) to branch '{}' at {}",
            self.selected_commit_ids.len(),
            branch_name,
            &newest_commit[..7]
        ))
    }

    pub fn is_commit_selected(&self, commit_id: &str) -> bool {
        self.selected_commit_ids.contains(&commit_id.to_string())
    }
}
