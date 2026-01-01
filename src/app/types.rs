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
    SetRemoteHost,
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
    SetRemoteHost,
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
            GitCommand::SetRemoteHost => "set remote url",
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Staged,
    Modified,
    Untracked,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct StatusFile {
    pub path: String,
    pub status: FileStatus,
}
