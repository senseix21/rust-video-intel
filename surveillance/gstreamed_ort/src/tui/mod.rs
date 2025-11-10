pub mod app;
pub mod roi;
pub mod ui;
mod events;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ort::session::Session;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::process_video;
use crate::tui::app::{App, TuiMessage, TuiMode};

const UI_FPS: u64 = 30;
const UI_FRAME_TIME: Duration = Duration::from_millis(1000 / UI_FPS);

pub fn process_video_with_tui(
    path: &Path,
    live: bool,
    session: Session,
    conf_threshold: f32,
    nms_threshold: f32,
) -> Result<()> {
    // Disable GStreamer debug output to prevent TUI interference
    std::env::set_var("GST_DEBUG", "0");
    std::env::set_var("GST_DEBUG_NO_COLOR", "1");
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create channel for worker thread to send updates
    let (tx, rx) = mpsc::channel();

    // Spawn worker thread for video processing
    let path_clone = path.to_path_buf();
    let worker = thread::spawn(move || {
        process_video::process_video_internal(&path_clone, live, session, Some(tx), conf_threshold, nms_threshold)
    });

    // Run TUI
    let result = run_tui_loop(&mut terminal, rx);

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Wait for worker thread
    let _ = worker.join();

    result
}

pub fn process_webcam_with_tui(
    device: &str,
    live: bool,
    session: Session,
    conf_threshold: f32,
    nms_threshold: f32,
) -> Result<()> {
    // Disable GStreamer debug output to prevent TUI interference
    std::env::set_var("GST_DEBUG", "0");
    std::env::set_var("GST_DEBUG_NO_COLOR", "1");
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create channel
    let (tx, rx) = mpsc::channel();

    // Spawn worker thread
    let device_clone = device.to_string();
    let worker = thread::spawn(move || {
        process_video::process_webcam_internal(&device_clone, live, session, Some(tx), conf_threshold, nms_threshold)
    });

    // Run TUI
    let result = run_tui_loop(&mut terminal, rx);

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = worker.join();

    result
}

fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: Receiver<TuiMessage>,
) -> Result<()> {
    let mut app = App::new();
    let mut last_render = Instant::now();

    loop {
        // Throttle rendering to UI_FPS
        if last_render.elapsed() >= UI_FRAME_TIME {
            terminal.draw(|f| ui::draw(f, &app))?;
            last_render = Instant::now();
        }

        // Handle keyboard input (non-blocking)
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.tui_mode {
                        TuiMode::Monitor => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    app.quit();
                                }
                                KeyCode::Esc => {
                                    app.quit();
                                }
                                KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Char(' ') => {
                                    app.toggle_pause();
                                }
                                KeyCode::Char('z') | KeyCode::Char('Z') => {
                                    app.enter_zone_list();
                                }
                                KeyCode::Up => app.scroll_up(),
                                KeyCode::Down => app.scroll_down(),
                                KeyCode::PageUp => app.page_up(),
                                KeyCode::PageDown => app.page_down(),
                                KeyCode::Home => app.scroll_home(),
                                KeyCode::End => app.scroll_end(),
                                KeyCode::Enter => app.select_current(),
                                _ => {}
                            }
                        }
                        TuiMode::ZoneList => {
                            match key.code {
                                KeyCode::Esc => {
                                    app.exit_to_monitor();
                                }
                                KeyCode::Up => {
                                    app.select_previous_zone();
                                }
                                KeyCode::Down => {
                                    app.select_next_zone();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') => {
                                    app.create_new_zone();
                                }
                                KeyCode::Char('e') | KeyCode::Char('E') => {
                                    app.edit_selected_zone();
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    app.delete_selected_zone();
                                }
                                KeyCode::Char(' ') => {
                                    app.toggle_selected_zone();
                                }
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    app.quit();
                                }
                                _ => {}
                            }
                        }
                        TuiMode::ZoneEdit => {
                            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                            let alt = key.modifiers.contains(KeyModifiers::ALT);
                            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                            let step = if shift { 0.01 } else { 0.05 };
                            
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_zone_edit();
                                }
                                KeyCode::Char('s') | KeyCode::Char('S') if !alt && !ctrl => {
                                    app.save_zone_draft();
                                }
                                // Move entire zone (HJKL vim-style, no modifiers needed)
                                KeyCode::Char('h') | KeyCode::Char('H') if !ctrl => {
                                    app.move_zone(-step, 0.0);
                                }
                                KeyCode::Char('l') | KeyCode::Char('L') if !ctrl => {
                                    app.move_zone(step, 0.0);
                                }
                                KeyCode::Char('k') | KeyCode::Char('K') if !ctrl => {
                                    app.move_zone(0.0, -step);
                                }
                                KeyCode::Char('j') | KeyCode::Char('J') if !ctrl => {
                                    app.move_zone(0.0, step);
                                }
                                // Move entire zone (Alt + Arrows OR Alt+WASD - if terminal supports)
                                KeyCode::Left if alt => {
                                    app.move_zone(-step, 0.0);
                                }
                                KeyCode::Right if alt => {
                                    app.move_zone(step, 0.0);
                                }
                                KeyCode::Up if alt => {
                                    app.move_zone(0.0, -step);
                                }
                                KeyCode::Down if alt => {
                                    app.move_zone(0.0, step);
                                }
                                // WASD alternative for movement (Alt+WASD - if terminal supports)
                                KeyCode::Char('a') | KeyCode::Char('A') if alt => {
                                    app.move_zone(-step, 0.0);
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') if alt => {
                                    app.move_zone(step, 0.0);
                                }
                                KeyCode::Char('w') | KeyCode::Char('W') if alt => {
                                    app.move_zone(0.0, -step);
                                }
                                KeyCode::Char('s') | KeyCode::Char('S') if alt => {
                                    app.move_zone(0.0, step);
                                }
                                // Adjust top-left corner (Ctrl + Arrows)
                                KeyCode::Left if ctrl => {
                                    app.adjust_zone_bbox(-step, 0.0, 0.0, 0.0);
                                }
                                KeyCode::Right if ctrl => {
                                    app.adjust_zone_bbox(step, 0.0, 0.0, 0.0);
                                }
                                KeyCode::Up if ctrl => {
                                    app.adjust_zone_bbox(0.0, -step, 0.0, 0.0);
                                }
                                KeyCode::Down if ctrl => {
                                    app.adjust_zone_bbox(0.0, step, 0.0, 0.0);
                                }
                                // Adjust bottom-right corner (default)
                                KeyCode::Left => {
                                    app.adjust_zone_bbox(0.0, 0.0, -step, 0.0);
                                }
                                KeyCode::Right => {
                                    app.adjust_zone_bbox(0.0, 0.0, step, 0.0);
                                }
                                KeyCode::Up => {
                                    app.adjust_zone_bbox(0.0, 0.0, 0.0, -step);
                                }
                                KeyCode::Down => {
                                    app.adjust_zone_bbox(0.0, 0.0, 0.0, step);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Process messages from worker thread
        let mut received_update = false;
        while let Ok(msg) = rx.try_recv() {
            received_update = true;
            match msg {
                TuiMessage::Finished => {
                    app.mark_finished();
                }
                _ => app.update(msg),
            }
        }
        
        // Force render if we received an update
        if received_update {
            terminal.draw(|f| ui::draw(f, &app))?;
            last_render = Instant::now();
        }

        if app.should_quit() {
            break;
        }

        // Small sleep to prevent busy-waiting
        thread::sleep(Duration::from_millis(5));
    }

    Ok(())
}
