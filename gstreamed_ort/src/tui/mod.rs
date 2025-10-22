pub mod app;
pub mod ui;
mod events;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ort::session::Session;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::process_video;
use app::{App, TuiMessage};

const UI_FPS: u64 = 30;
const UI_FRAME_TIME: Duration = Duration::from_millis(1000 / UI_FPS);

pub fn process_video_with_tui(
    path: &Path,
    live: bool,
    session: Session,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for worker thread to send updates
    let (tx, rx) = mpsc::channel();

    // Spawn worker thread for video processing
    let path_clone = path.to_path_buf();
    let worker = thread::spawn(move || {
        process_video::process_video_internal(&path_clone, live, session, Some(tx))
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
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel
    let (tx, rx) = mpsc::channel();

    // Spawn worker thread
    let device_clone = device.to_string();
    let worker = thread::spawn(move || {
        process_video::process_webcam_internal(&device_clone, live, session, Some(tx))
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
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            app.quit();
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Char(' ') => {
                            app.toggle_pause();
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
            }
        }

        // Process messages from worker thread
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TuiMessage::Finished => {
                    app.mark_finished();
                }
                _ => app.update(msg),
            }
        }

        if app.should_quit() {
            break;
        }

        // Small sleep to prevent busy-waiting
        thread::sleep(Duration::from_millis(5));
    }

    Ok(())
}
