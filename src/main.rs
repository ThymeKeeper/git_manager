mod app;
mod git;
mod graph;
mod renderer;
mod ui;

use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize app
    app.init()?;

    loop {
        // Clear expired status messages
        app.clear_expired_status_message();

        // Check git validation results (runs once when complete)
        app.check_validation_results();

        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                use app::AppMode;

                match app.mode {
                    AppMode::Normal => {
                        use app::FocusedPane;

                        match key.code {
                            KeyCode::F(1) => {
                                app.toggle_help();
                            }
                            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.quit();
                            }
                            KeyCode::Char('r') => {
                                app.refresh();
                            }
                            KeyCode::Tab => {
                                app.next_pane();
                            }
                            KeyCode::Up => {
                                match app.focused_pane {
                                    FocusedPane::CommitGraph => app.move_selection_up(),
                                    FocusedPane::GitActions => app.command_up(),
                                    FocusedPane::CommitDetails => app.details_scroll_up(),
                                    FocusedPane::GitStatus => app.file_up(),
                                }
                            }
                            KeyCode::Down => {
                                match app.focused_pane {
                                    FocusedPane::CommitGraph => app.move_selection_down(),
                                    FocusedPane::GitActions => app.command_down(),
                                    FocusedPane::CommitDetails => app.details_scroll_down(),
                                    FocusedPane::GitStatus => app.file_down(),
                                }
                            }
                            KeyCode::Left => {
                                if app.focused_pane == FocusedPane::CommitDetails {
                                    app.details_scroll_left();
                                }
                            }
                            KeyCode::Right => {
                                if app.focused_pane == FocusedPane::CommitDetails {
                                    app.details_scroll_right();
                                }
                            }
                            KeyCode::Enter => {
                                match app.focused_pane {
                                    FocusedPane::CommitGraph => app.load_current_diff(),
                                    FocusedPane::GitActions => app.execute_selected_command(),
                                    FocusedPane::CommitDetails => {
                                        // Toggle fullscreen details pane
                                        app.details_expanded = !app.details_expanded;
                                    }
                                    FocusedPane::GitStatus => {
                                        app.open_file_diff_view();
                                    }
                                }
                            }
                            KeyCode::Char(' ') => {
                                if app.focused_pane == FocusedPane::GitStatus {
                                    app.toggle_file_staging();
                                }
                            }
                            KeyCode::Esc => {
                                // Exit fullscreen mode if active
                                if app.details_expanded {
                                    app.details_expanded = false;
                                }
                            }
                            _ => {}
                        }
                    }
                    AppMode::Confirm => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                app.confirm_command();
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.cancel_command();
                            }
                            _ => {}
                        }
                    }
                    AppMode::CommitMessage => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_commit_message();
                            }
                            KeyCode::Esc => {
                                app.cancel_commit_message();
                            }
                            KeyCode::Backspace => {
                                app.commit_message_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.commit_message_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::BranchName => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_branch_name();
                            }
                            KeyCode::Esc => {
                                app.cancel_branch_name();
                            }
                            KeyCode::Backspace => {
                                app.branch_name_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.branch_name_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::SelectBranch => {
                        match key.code {
                            KeyCode::Up => {
                                app.branch_selection_up();
                            }
                            KeyCode::Down => {
                                app.branch_selection_down();
                            }
                            KeyCode::Enter => {
                                app.select_branch();
                            }
                            KeyCode::Esc => {
                                app.cancel_branch_selection();
                            }
                            _ => {}
                        }
                    }
                    AppMode::SelectBranchToDelete => {
                        match key.code {
                            KeyCode::Up => {
                                app.delete_branch_selection_up();
                            }
                            KeyCode::Down => {
                                app.delete_branch_selection_down();
                            }
                            KeyCode::Enter => {
                                app.select_branch_to_delete();
                            }
                            KeyCode::Esc => {
                                app.cancel_delete_branch_selection();
                            }
                            _ => {}
                        }
                    }
                    AppMode::SetUserName => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_user_name();
                            }
                            KeyCode::Esc => {
                                app.cancel_config_input();
                            }
                            KeyCode::Backspace => {
                                app.config_input_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.config_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::SetUserEmail => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_user_email();
                            }
                            KeyCode::Esc => {
                                app.cancel_config_input();
                            }
                            KeyCode::Backspace => {
                                app.config_input_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.config_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::SetRemoteHost => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_remote_host();
                            }
                            KeyCode::Esc => {
                                app.cancel_remote_host_input();
                            }
                            KeyCode::Backspace => {
                                app.remote_host_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.remote_host_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::SquashCountInput => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_squash_count();
                            }
                            KeyCode::Esc => {
                                app.cancel_squash_count();
                            }
                            KeyCode::Backspace => {
                                app.squash_count_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.squash_count_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::RewordMessage => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_reword_message();
                            }
                            KeyCode::Esc => {
                                app.cancel_reword_message();
                            }
                            KeyCode::Backspace => {
                                app.reword_message_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.reword_message_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::SelectCommitsForBranch => {
                        match key.code {
                            KeyCode::Char(' ') => {
                                app.toggle_commit_selection();
                            }
                            KeyCode::Enter => {
                                app.finish_commit_selection();
                            }
                            KeyCode::Esc => {
                                app.cancel_commit_selection();
                            }
                            KeyCode::Down => {
                                app.move_selection_down();
                            }
                            KeyCode::Up => {
                                app.move_selection_up();
                            }
                            _ => {}
                        }
                    }
                    AppMode::AssignBranchName => {
                        match key.code {
                            KeyCode::Enter => {
                                app.submit_assign_branch_name();
                            }
                            KeyCode::Esc => {
                                app.cancel_assign_branch_name();
                            }
                            KeyCode::Backspace => {
                                app.assign_branch_name_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.assign_branch_name_input_char(c);
                            }
                            _ => {}
                        }
                    }
                    AppMode::FileDiffView => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.close_file_diff_view();
                            }
                            KeyCode::Down => {
                                app.details_scroll_down();
                            }
                            KeyCode::Up => {
                                app.details_scroll_up();
                            }
                            KeyCode::Left => {
                                app.details_scroll_left();
                            }
                            KeyCode::Right => {
                                app.details_scroll_right();
                            }
                            _ => {}
                        }
                    }
                    AppMode::Help => {
                        match key.code {
                            KeyCode::F(1) | KeyCode::Esc | KeyCode::Char('q') => {
                                app.toggle_help();
                            }
                            KeyCode::Up => {
                                app.scroll_help_up();
                            }
                            KeyCode::Down => {
                                app.scroll_help_down();
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
