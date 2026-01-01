use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Commit {
    pub id: String,
    pub short_id: String,
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub commit: Commit,
    pub column: usize,
    pub connections: Vec<Connection>,
    pub in_current_branch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Connection {
    Vertical,                  // │ straight down to parent
    MergeFrom(usize),          // ╮ incoming from column N
    BranchTo(usize),           // ╯ outgoing to column N
    ShiftLeft(usize),          // ╭╯ move left by N columns (void collapse)
    PassThrough(usize),        // │ line passing by (other branch)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncStatus {
    Synced,      // exists on both local and remote
    LocalOnly,   // ahead of remote (unpushed)
    RemoteOnly,  // behind remote (not pulled)
    Diverged,    // both (merge/rebase needed)
}

pub struct RailwayLayout {
    pub active_lanes: Vec<Option<String>>,  // column -> commit_id occupying it
}

impl RailwayLayout {
    pub fn new() -> Self {
        Self {
            active_lanes: Vec::new(),
        }
    }

    pub fn allocate_column(&mut self, commit_id: &str) -> usize {
        // First: try to reuse an empty lane
        for (i, lane) in self.active_lanes.iter_mut().enumerate() {
            if lane.is_none() {
                *lane = Some(commit_id.to_string());
                return i;
            }
        }
        // Otherwise: expand right
        self.active_lanes.push(Some(commit_id.to_string()));
        self.active_lanes.len() - 1
    }

    pub fn release_column(&mut self, col: usize) {
        if col < self.active_lanes.len() {
            self.active_lanes[col] = None;
        }
    }

    pub fn compact_lanes(&mut self, dead_column: usize) -> Vec<(usize, usize)> {
        // Returns list of (from_col, to_col) shifts
        let mut shifts = Vec::new();

        if dead_column < self.active_lanes.len() {
            self.active_lanes.remove(dead_column);
            // Everything to the right shifts left
            for i in dead_column..self.active_lanes.len() {
                shifts.push((i + 1, i));
            }
        }

        shifts
    }

    pub fn get_active_columns(&self) -> Vec<usize> {
        self.active_lanes
            .iter()
            .enumerate()
            .filter_map(|(i, lane)| if lane.is_some() { Some(i) } else { None })
            .collect()
    }

    pub fn width(&self) -> usize {
        self.active_lanes.len()
    }
}

pub struct CommitGraph {
    pub commits: HashMap<String, Commit>,
    pub nodes: Vec<GraphNode>,
    pub ancestry_path: HashSet<String>,  // commits on the selected path
}

impl CommitGraph {
    pub fn new() -> Self {
        Self {
            commits: HashMap::new(),
            nodes: Vec::new(),
            ancestry_path: HashSet::new(),
        }
    }

    pub fn add_commit(&mut self, commit: Commit) {
        self.commits.insert(commit.id.clone(), commit);
    }

    pub fn build_graph(&mut self) {
        // Build parent-child relationships
        let commit_ids: Vec<String> = self.commits.keys().cloned().collect();

        for commit_id in &commit_ids {
            if let Some(commit) = self.commits.get(commit_id) {
                let parents = commit.parents.clone();
                for parent_id in parents {
                    if let Some(parent) = self.commits.get_mut(&parent_id) {
                        if !parent.children.contains(commit_id) {
                            parent.children.push(commit_id.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn topological_sort(&self) -> Vec<String> {
        // Kahn's algorithm for topological sort
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut result = Vec::new();

        // Calculate in-degrees
        for commit in self.commits.values() {
            in_degree.entry(commit.id.clone()).or_insert(0);
            for child_id in &commit.children {
                *in_degree.entry(child_id.clone()).or_insert(0) += 1;
            }
        }

        // Find all nodes with in-degree 0 (roots)
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|&(_, &degree)| degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Sort by timestamp to get consistent ordering, with commit ID as tiebreaker
        queue.sort_by(|a, b| {
            let time_a = self.commits.get(a).map(|c| c.timestamp).unwrap_or(0);
            let time_b = self.commits.get(b).map(|c| c.timestamp).unwrap_or(0);
            time_b.cmp(&time_a)  // newest first
                .then_with(|| a.cmp(b))  // Use commit ID as tiebreaker for determinism
        });

        while let Some(commit_id) = queue.pop() {
            result.push(commit_id.clone());

            if let Some(commit) = self.commits.get(&commit_id) {
                let mut children = commit.children.clone();
                // Sort children by timestamp (newest first), with commit ID as tiebreaker
                // Since queue is LIFO, newest-first sorting means newest is pushed first
                // and processed last, putting it later in the result (oldest-to-newest).
                // After reversal, newest commits appear first.
                children.sort_by(|a, b| {
                    let time_a = self.commits.get(a).map(|c| c.timestamp).unwrap_or(0);
                    let time_b = self.commits.get(b).map(|c| c.timestamp).unwrap_or(0);
                    time_b.cmp(&time_a)  // newest first
                        .then_with(|| a.cmp(b))  // Use commit ID as tiebreaker for determinism
                });

                for child_id in children {
                    if let Some(degree) = in_degree.get_mut(&child_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(child_id);
                        }
                    }
                }
            }
        }

        result
    }

    pub fn trace_ancestry(&mut self, commit_id: &str) {
        self.ancestry_path.clear();
        let mut stack = vec![commit_id.to_string()];

        while let Some(current_id) = stack.pop() {
            if self.ancestry_path.contains(&current_id) {
                continue;
            }

            self.ancestry_path.insert(current_id.clone());

            if let Some(commit) = self.commits.get(&current_id) {
                for parent_id in &commit.parents {
                    stack.push(parent_id.clone());
                }
            }
        }
    }

    pub fn is_on_ancestry_path(&self, commit_id: &str) -> bool {
        self.ancestry_path.contains(commit_id)
    }
}
