mod types;
mod navigation;
mod dialogs;
mod commands;

pub use types::*;

use crate::git::GitRepo;
use crate::graph::{CommitGraph, GraphNode};
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::Command;

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
    pub branch_ahead: usize,
    pub branch_behind: usize,
    pub has_upstream: bool,
    pub current_diff: Option<String>,
    pub git_user_name: Option<String>,
    pub git_user_email: Option<String>,
    pub git_remote_host: Option<String>,
    pub has_git_repo: bool,
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
    pub remote_host_input: String,
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
            branch_ahead: 0,
            branch_behind: 0,
            has_upstream: false,
            current_diff: None,
            git_user_name: None,
            git_user_email: None,
            git_remote_host: None,
            has_git_repo: false,
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
                GitCommand::SetRemoteHost,
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
            remote_host_input: String::new(),
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
                self.has_git_repo = true;

                // Load current branch
                if let Ok(branch) = repo.get_current_branch() {
                    self.current_branch = Some(branch);
                }

                // Load git user configuration
                self.load_git_user_config();

                // Load git remote host
                self.load_git_remote_host();

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

                // Calculate ahead/behind counts
                self.update_branch_ahead_behind();

                // Start git validation in background
                self.start_git_validation();

                Ok(())
            }
            Err(e) => {
                self.has_git_repo = false;
                self.set_status_message(format!("⚠ No git repository found in current directory"));
                Ok(())  // Don't error out, let the app run with a warning
            }
        }
    }

    fn update_branch_ahead_behind(&mut self) {
        // Reset to 0
        self.branch_ahead = 0;
        self.branch_behind = 0;
        self.has_upstream = false;

        // Get current branch name
        let branch_name = match &self.current_branch {
            Some(name) => name,
            None => return,
        };

        // Check if there's a remote tracking branch
        let output = Command::new("git")
            .args(&["rev-parse", "--abbrev-ref", &format!("{}@{{upstream}}", branch_name)])
            .output();

        let upstream = match output {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => return, // No upstream branch
        };

        // We have an upstream branch
        self.has_upstream = true;

        // Get ahead/behind counts
        let output = Command::new("git")
            .args(&["rev-list", "--left-right", "--count", &format!("{}...{}", upstream, branch_name)])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let counts = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = counts.trim().split_whitespace().collect();
                if parts.len() == 2 {
                    self.branch_behind = parts[0].parse().unwrap_or(0);
                    self.branch_ahead = parts[1].parse().unwrap_or(0);
                }
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

    pub(super) fn load_git_remote_host(&mut self) {
        use std::process::Command;

        // Load remote URL for origin
        if let Ok(output) = Command::new("git")
            .args(&["remote", "get-url", "origin"])
            .output()
        {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !url.is_empty() {
                    self.git_remote_host = Some(url);
                }
            }
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

    pub fn refresh(&mut self) {
        let _ = self.init();
        self.set_status_message("✓ Refreshed".to_string());
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

    pub fn is_commit_selected(&self, commit_id: &str) -> bool {
        self.selected_commit_ids.contains(&commit_id.to_string())
    }
}
