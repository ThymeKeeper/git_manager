use git2::{Repository, Oid, Commit as GitCommit, DiffOptions, DiffFormat};
use crate::graph::{Commit, CommitGraph, SyncStatus};

pub struct GitRepo {
    pub repo: Repository,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self, git2::Error> {
        let repo = Repository::open(path)?;
        Ok(Self { repo })
    }

    pub fn open_current_dir() -> Result<Self, git2::Error> {
        let repo = Repository::open(".")?;
        Ok(Self { repo })
    }

    pub fn load_commits(&self) -> Result<CommitGraph, git2::Error> {
        let mut graph = CommitGraph::new();
        let mut revwalk = self.repo.revwalk()?;

        // Walk all references (branches, tags, etc.) - this ensures we see all commits
        // regardless of which branch is currently checked out
        revwalk.push_glob("refs/heads/*")?;  // All local branches
        revwalk.push_glob("refs/remotes/*")?;  // All remote branches
        revwalk.push_glob("refs/tags/*")?;  // All tags

        // Also push HEAD to ensure current position is included even if detached
        if let Ok(head) = self.repo.head() {
            if let Some(target) = head.target() {
                let _ = revwalk.push(target);
            }
        }

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

        for oid in revwalk {
            let oid = oid?;
            let git_commit = self.repo.find_commit(oid)?;

            let commit = self.git_commit_to_commit(&git_commit)?;
            graph.add_commit(commit);
        }

        graph.build_graph();
        Ok(graph)
    }

    fn git_commit_to_commit(&self, git_commit: &GitCommit) -> Result<Commit, git2::Error> {
        let id = git_commit.id().to_string();
        let short_id = git_commit.as_object().short_id()?.as_str().unwrap_or("").to_string();

        let parents: Vec<String> = git_commit
            .parents()
            .map(|p| p.id().to_string())
            .collect();

        let message = git_commit.message().unwrap_or("").to_string();
        let author = git_commit.author().name().unwrap_or("Unknown").to_string();
        let timestamp = git_commit.time().seconds();

        Ok(Commit {
            id,
            short_id,
            parents,
            children: Vec::new(),  // Will be filled by graph.build_graph()
            message,
            author,
            timestamp,
        })
    }

    pub fn get_head_commit_id(&self) -> Result<String, git2::Error> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    pub fn get_current_branch(&self) -> Result<String, git2::Error> {
        let head = self.repo.head()?;

        if head.is_branch() {
            Ok(head.shorthand().unwrap_or("HEAD").to_string())
        } else {
            Ok("HEAD (detached)".to_string())
        }
    }

    pub fn get_branch_commit_id(&self, branch_name: &str) -> Result<String, git2::Error> {
        let reference = self.repo.find_reference(&format!("refs/heads/{}", branch_name))?;
        let commit = reference.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    pub fn get_branch_for_commit(&self, commit_id: &str) -> Result<String, git2::Error> {
        let oid = Oid::from_str(commit_id).map_err(|_| {
            git2::Error::from_str("Invalid commit ID")
        })?;

        // First check if this commit is a branch HEAD
        let branch_iter = self.repo.branches(Some(git2::BranchType::Local))?;
        let mut branch_heads = Vec::new();

        for branch_result in branch_iter {
            if let Ok((branch, _)) = branch_result {
                if let Some(name) = branch.name()? {
                    let branch_ref = branch.get();
                    if let Ok(branch_commit) = branch_ref.peel_to_commit() {
                        if branch_commit.id() == oid {
                            branch_heads.push(name.to_string());
                        }
                    }
                }
            }
        }

        // If it's a branch HEAD, return the first one (prefer master/main)
        if !branch_heads.is_empty() {
            branch_heads.sort_by(|a, b| {
                let a_priority = if a == "master" || a == "main" { 0 } else { 1 };
                let b_priority = if b == "master" || b == "main" { 0 } else { 1 };
                a_priority.cmp(&b_priority).then_with(|| a.cmp(b))
            });
            return Ok(branch_heads[0].clone());
        }

        // Not a branch HEAD - find which branch contains this commit
        // Prefer master/main, then alphabetically
        let mut containing_branches = Vec::new();
        let branch_iter = self.repo.branches(Some(git2::BranchType::Local))?;

        for branch_result in branch_iter {
            if let Ok((branch, _)) = branch_result {
                if let Some(name) = branch.name()? {
                    let branch_ref = branch.get();
                    if let Ok(branch_commit) = branch_ref.peel_to_commit() {
                        // Check if this commit is reachable from this branch
                        let mut revwalk = self.repo.revwalk()?;
                        revwalk.push(branch_commit.id())?;

                        for rev_oid in revwalk {
                            if let Ok(rev_oid) = rev_oid {
                                if rev_oid == oid {
                                    containing_branches.push(name.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !containing_branches.is_empty() {
            // Sort to prefer master/main
            containing_branches.sort_by(|a, b| {
                let a_priority = if a == "master" || a == "main" { 0 } else { 1 };
                let b_priority = if b == "master" || b == "main" { 0 } else { 1 };
                a_priority.cmp(&b_priority).then_with(|| a.cmp(b))
            });
            return Ok(containing_branches[0].clone());
        }

        Ok("(unknown)".to_string())
    }

    pub fn get_commit_diff(&self, commit_id: &str) -> Result<String, git2::Error> {
        let oid = Oid::from_str(commit_id).map_err(|_| {
            git2::Error::from_str("Invalid commit ID")
        })?;

        let commit = self.repo.find_commit(oid)?;
        let tree = commit.tree()?;

        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let mut diff_output = Vec::new();
        let mut diff_opts = DiffOptions::new();

        let diff = if let Some(parent_tree) = parent_tree {
            self.repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut diff_opts))?
        } else {
            self.repo.diff_tree_to_tree(None, Some(&tree), Some(&mut diff_opts))?
        };

        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            diff_output.extend_from_slice(line.content());
            true
        })?;

        Ok(String::from_utf8_lossy(&diff_output).to_string())
    }

    pub fn get_sync_status(&self, _commit_id: &str) -> SyncStatus {
        // For now, return Synced as default
        // TODO: Implement actual local/remote comparison
        SyncStatus::Synced
    }

    pub fn get_repo_status(&self) -> Result<RepoStatus, git2::Error> {
        let mut status = RepoStatus {
            staged: Vec::new(),
            unstaged: Vec::new(),
            untracked: Vec::new(),
        };

        let statuses = self.repo.statuses(None)?;

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let entry_status = entry.status();

            if entry_status.is_index_new() || entry_status.is_index_modified() || entry_status.is_index_deleted() {
                status.staged.push(path.clone());
            }

            if entry_status.is_wt_modified() || entry_status.is_wt_deleted() {
                status.unstaged.push(path.clone());
            }

            if entry_status.is_wt_new() {
                status.untracked.push(path);
            }
        }

        Ok(status)
    }
}

pub struct RepoStatus {
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}
