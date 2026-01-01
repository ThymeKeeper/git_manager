use super::App;
use super::types::{AppMode, FocusedPane, FileStatus};
use std::time::Instant;

impl App {
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
                        self.details_scroll_offset = 0;
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
                        Command::new("git")
                            .args(&["diff", "--cached", "--", &file.path])
                            .output()
                    }
                    FileStatus::Modified | FileStatus::Deleted => {
                        Command::new("git")
                            .args(&["diff", "--", &file.path])
                            .output()
                    }
                    FileStatus::Untracked => {
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
            self.details_scroll_offset = 0;
            self.details_horizontal_offset = 0;
            self.mode = AppMode::FileDiffView;
        }
    }

    pub fn close_file_diff_view(&mut self) {
        self.mode = AppMode::Normal;
        self.details_scroll_offset = 0;
        self.details_horizontal_offset = 0;
    }

    pub(super) fn update_selection(&mut self) {
        if let Some(idx) = self.selected_commit_idx {
            if let Some(node) = self.graph_nodes.get(idx) {
                self.graph.trace_ancestry(&node.commit.id);
            }
        }
        self.load_current_diff();
    }

    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        if let Some(selected_idx) = self.selected_commit_idx {
            let selected_row = selected_idx * 2;

            if selected_row < self.scroll_offset {
                self.scroll_offset = selected_row;
            }

            if selected_row >= self.scroll_offset + viewport_height.saturating_sub(1) {
                self.scroll_offset = selected_row.saturating_sub(viewport_height.saturating_sub(2));
            }
        }
    }

    pub fn adjust_command_scroll(&mut self, viewport_height: usize) {
        let selected_idx = self.selected_command_idx;

        if selected_idx < self.command_scroll_offset {
            self.command_scroll_offset = selected_idx;
        }

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
}
