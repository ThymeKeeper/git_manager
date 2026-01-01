use super::App;
use super::types::{AppMode, GitCommand, FocusedPane, FileStatus, StatusFile};
use std::process::Command;

impl App {
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

    pub fn submit_remote_host(&mut self) {
        if self.remote_host_input.trim().is_empty() {
            self.set_status_message("✗ Remote URL cannot be empty".to_string());
            self.mode = AppMode::Normal;
            return;
        }

        let url = self.remote_host_input.clone();
        self.mode = AppMode::Normal;
        self.remote_host_input.clear();

        let result = self.execute_set_remote_host(&url);
        match result {
            Ok(msg) => self.set_status_message(format!("✓ {}", msg)),
            Err(e) => self.set_status_message(format!("✗ Error: {}", e)),
        }
    }

    pub fn remote_host_input_char(&mut self, c: char) {
        self.remote_host_input.push(c);
    }

    pub fn remote_host_backspace(&mut self) {
        self.remote_host_input.pop();
    }

    pub fn cancel_remote_host_input(&mut self) {
        self.remote_host_input.clear();
        self.mode = AppMode::Normal;
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

}
