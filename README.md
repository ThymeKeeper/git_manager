# git_manager

A terminal-based user interface for managing Git repositories with an intuitive railway-style commit graph visualization.

<img width="2880" height="1800" alt="git-tui" src="https://github.com/user-attachments/assets/271b2736-14bd-49dc-b08f-e4e886da9f7c" />

## Overview

git_manager is a fast, keyboard-driven TUI that provides a visual representation of your Git history using a railway-track metaphor. Each branch occupies a "lane" in the visualization, making it easy to understand complex branching and merging patterns at a glance.

## Features

### Commit Graph Visualization
- **Railway-style layout**: Branches are displayed as parallel tracks with clear merge and branch points
- **Ancestry highlighting**: Commits not in the current branch's history are automatically greyed out
- **Smart column management**: Efficient lane allocation with automatic compaction when branches merge

### Git Operations
- View commit history with author, timestamp, and message
- Navigate commits with arrow keys
- Branch operations (create, checkout, delete)
- Interactive git operations from within the TUI

### Performance
- Fast rendering with efficient graph layout algorithms
- Pre-calculated ancestry information for instant visual feedback
- Optimized for repositories with complex branching structures

## Installation

### Requirements
- Rust toolchain (2024 edition)
- Git

### Build from Source

```bash
cargo build --release
```

The binary will be available at `target/release/git_manager`

## Usage

Run git_manager from within any Git repository:

```bash
git_manager
```

### Keybindings

- `↑/↓` - Navigate commits
- `Enter` - View commit details
- Arrow keys - Navigate the commit graph
- `q` - Quit

## Architecture

git_manager uses a topological sort algorithm to order commits and a railway layout system to assign visual columns to branches. The graph rendering separates node rows (commits) from edge rows (connections between commits), allowing for clean visual representation of complex merge patterns.

### Key Components

- **Graph Module**: Handles commit graph construction and topological sorting
- **Railway Layout**: Manages column allocation and lane compaction
- **Renderer**: Converts graph structure into terminal UI elements
- **App Module**: Coordinates UI state and Git operations

## Technical Details

- Built with [ratatui](https://github.com/ratatui-org/ratatui) for terminal UI
- Uses [git2-rs](https://github.com/rust-lang/git2-rs) for Git operations
- Implements custom graph layout algorithms optimized for Git DAGs

## Contributing

This is a personal project, but feedback and suggestions are welcome.
