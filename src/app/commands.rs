use super::App;
use super::types::{AppMode, GitCommand, FocusedPane, FileStatus, StatusFile};
use crate::git::GitRepo;
use std::process::Command;

impl App {
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

    pub(super) fn is_ancestor_of_head(&self, commit_id: &str) -> bool {
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

    pub(super) fn execute_create_branch_with_name(&mut self, commit_id: &str, branch_name: &str) -> Result<String, String> {
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

    pub(super) fn execute_reword(&mut self, commit_id: &str, new_message: &str) -> Result<String, String> {
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

    fn execute_command(&mut self, command: GitCommand) -> Result<String, String> {
        match command {
            GitCommand::Add => self.cmd_add(),
            GitCommand::Commit => self.cmd_commit(),
            GitCommand::Push => self.cmd_push(),
            GitCommand::Pull => self.cmd_pull(),
            GitCommand::PullAll => self.cmd_pull_all(),
            GitCommand::SetUserName => self.cmd_set_user_name(),
            GitCommand::SetUserEmail => self.cmd_set_user_email(),
            GitCommand::SetRemoteHost => self.cmd_set_remote_host(),
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

    pub(super) fn execute_checkout_branch(&mut self, branch_name: &str) -> Result<String, String> {
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

    pub(super) fn execute_commit_with_message(&mut self, message: &str) -> Result<String, String> {
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
            // Reload graph after successful push to update sync status
            let _ = self.init();
            Ok("Pushed to remote".to_string())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();

            // Detect common error scenarios and provide helpful messages
            if error_msg.contains("rejected") && error_msg.contains("fetch first") {
                Err("Push rejected: Remote has changes you don't have locally. Use 'pull' or 'fetch and sync all branches' first.".to_string())
            } else if error_msg.contains("non-fast-forward") {
                Err("Push rejected: Non-fast-forward update. Pull changes first or use force push (dangerous).".to_string())
            } else if error_msg.contains("no upstream branch") || error_msg.contains("has no upstream") {
                Err("Push failed: No upstream branch set. Use 'git push -u origin <branch>' from terminal.".to_string())
            } else if error_msg.contains("Authentication failed") || error_msg.contains("Could not read from remote") {
                Err("Push failed: Authentication error. Check your credentials or SSH keys.".to_string())
            } else {
                // Return the full error message for other cases
                Err(format!("Push failed: {}", error_msg.trim()))
            }
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

    fn cmd_assign_to_branch(&mut self) -> Result<String, String> {
        self.selected_commit_ids.clear();
        self.assign_branch_name_input.clear();
        self.mode = AppMode::SelectCommitsForBranch;
        self.focused_pane = FocusedPane::CommitGraph;
        Ok("Select commits with Space, Enter when done, Esc to cancel".to_string())
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

    fn cmd_set_remote_host(&mut self) -> Result<String, String> {
        // Open input dialog
        self.remote_host_input.clear();
        self.mode = AppMode::SetRemoteHost;
        Ok("Enter remote URL (e.g. https://github.com/user/repo.git)...".to_string())
    }

    pub(super) fn execute_set_user_name(&mut self, name: &str) -> Result<String, String> {
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

    pub(super) fn execute_set_user_email(&mut self, email: &str) -> Result<String, String> {
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

    pub(super) fn execute_set_remote_host(&mut self, url: &str) -> Result<String, String> {
        use std::process::Command;

        // First check if remote 'origin' exists
        let check_output = Command::new("git")
            .args(&["remote", "get-url", "origin"])
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        let output = if check_output.status.success() {
            // Remote exists, update it
            Command::new("git")
                .args(&["remote", "set-url", "origin", url])
                .output()
                .map_err(|e| format!("Failed to execute git: {}", e))?
        } else {
            // Remote doesn't exist, add it
            Command::new("git")
                .args(&["remote", "add", "origin", url])
                .output()
                .map_err(|e| format!("Failed to execute git: {}", e))?
        };

        if output.status.success() {
            // Reload remote host to update status bar
            self.load_git_remote_host();
            Ok(format!("Set remote origin to '{}'", url))
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

    pub(super) fn assign_commits_to_branch(&mut self, branch_name: &str) -> Result<String, String> {
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

}
