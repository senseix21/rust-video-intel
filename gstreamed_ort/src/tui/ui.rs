use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Sparkline,
    },
    Frame,
};

use crate::tui::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_main_content(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(
        " GStreamer ML Inference Dashboard v0.2.0 - {} ",
        if app.is_finished {
            "FINISHED"
        } else if app.is_paused {
            "PAUSED"
        } else {
            "RUNNING"
        }
    );

    let status_color = if app.is_finished {
        Color::Green
    } else if app.is_paused {
        Color::Yellow
    } else {
        Color::Cyan
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(&title, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled("[Q]", Style::default().fg(Color::Red)),
        Span::raw("uit "),
        Span::styled("[P/Space]", Style::default().fg(Color::Yellow)),
        Span::raw("ause "),
        Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
        Span::raw("Scroll"),
    ]))
    .block(Block::default().borders(Borders::ALL));

    f.render_widget(header, area);
}

fn draw_main_content(f: &mut Frame, app: &App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Video info + progress
            Constraint::Min(10),   // Content area
        ])
        .split(area);

    draw_video_info(f, app, main_chunks[0]);

    // Split content into left and right panels
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    draw_left_panel(f, app, content_chunks[0]);
    draw_right_panel(f, app, content_chunks[1]);
}

fn draw_video_info(f: &mut Frame, app: &App, area: Rect) {
    let progress_text = if let Some(total) = app.total_frames {
        format!(
            "Frame: {}/{} ({:.1}%) | {}x{} | FPS: {:.1}",
            app.frame_num, total, app.progress_percentage(), app.width, app.height, app.fps
        )
    } else {
        format!(
            "Frame: {} | {}x{} | FPS: {:.1}",
            app.frame_num, app.width, app.height, app.fps
        )
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(3)])
        .split(area);

    // File info
    let file_info = Paragraph::new(format!("File: {}", app.filename))
        .block(Block::default().borders(Borders::TOP | Borders::LEFT | Borders::RIGHT));
    f.render_widget(file_info, chunks[0]);

    // Progress bar
    let progress = if let Some(_) = app.total_frames {
        app.progress_percentage() as u16
    } else {
        0
    };

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Cyan))
        .label(progress_text)
        .percent(progress.min(100));
    f.render_widget(gauge, chunks[1]);
}

fn draw_left_panel(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_performance_stats(f, app, chunks[0]);
    draw_class_distribution(f, app, chunks[1]);
}

fn draw_performance_stats(f: &mut Frame, app: &App, area: Rect) {
    let perf = &app.current_perf;
    
    let text = vec![
        Line::from(format!("  Inference:   {:.2} ms", perf.inference_ms)),
        Line::from(format!("  Preprocess:  {:.2} ms", perf.preprocess_ms)),
        Line::from(format!("  Postprocess: {:.2} ms", perf.postprocess_ms)),
        Line::from(format!("  Total:       {:.2} ms", perf.total_ms)),
        Line::from(""),
        Line::from(format!("  Avg FPS: {:.1}", app.avg_fps)),
    ];

    // Create sparkline data for inference time
    let sparkline_data: Vec<u64> = app
        .perf_history
        .iter()
        .map(|p| p.inference_ms as u64)
        .collect();

    let perf_text = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("📊 Performance"),
        );

    // Split area for text and sparkline
    let perf_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(7), Constraint::Length(3)])
        .split(area);

    f.render_widget(perf_text, perf_chunks[0]);

    if !sparkline_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title("Inference (ms)"))
            .data(&sparkline_data)
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(sparkline, perf_chunks[1]);
    }
}

fn draw_class_distribution(f: &mut Frame, app: &App, area: Rect) {
    let mut counts: Vec<_> = app.class_counts.iter().collect();
    counts.sort_by(|a, b| b.1.cmp(a.1));

    let items: Vec<ListItem> = counts
        .iter()
        .take(10)
        .map(|(class, count)| {
            let bar_width = (*count * 20 / counts.first().map(|(_, c)| **c).unwrap_or(1).max(1)).min(20);
            let bar = "█".repeat(bar_width);
            ListItem::new(format!("{:<12} {:<20} {}", class, bar, count))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("📈 Class Distribution (Total: {})", app.total_detections)),
    );

    f.render_widget(list, area);
}

fn draw_right_panel(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_detections_table(f, app, chunks[0]);
    draw_selected_detail(f, app, chunks[1]);
}

fn draw_detections_table(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["ID", "Class", "Conf", "Color", "Position"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .current_detections
        .iter()
        .enumerate()
        .map(|(idx, det)| {
            let style = if idx == app.selected_index {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let id = det.tracker_id
                .map(|id| format!("#{}", id))
                .unwrap_or_else(|| "-".to_string());

            let color_info = det.attributes.color_info
                .as_ref()
                .map(|c| c.color_name.clone())
                .unwrap_or_else(|| "N/A".to_string());

            let position = format!(
                "({:.0},{:.0})",
                det.bbox.xmin, det.bbox.ymin
            );

            Row::new(vec![
                Cell::from(id),
                Cell::from(det.class_name.clone()),
                Cell::from(format!("{:.2}", det.confidence)),
                Cell::from(color_info),
                Cell::from(position),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(15),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("🎯 Live Detections (Frame #{})", app.frame_num)),
    );

    f.render_widget(table, area);
}

fn draw_selected_detail(f: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(det) = app.get_selected_detection() {
        let mut lines = vec![
            Line::from(format!("  Class: {}", det.class_name)),
            Line::from(format!("  Confidence: {:.3}", det.confidence)),
        ];

        if let Some(id) = det.tracker_id {
            lines.push(Line::from(format!("  Tracking ID: #{}", id)));
        }

        lines.push(Line::from(format!(
            "  BBox: ({:.0}, {:.0}, {:.0}, {:.0})",
            det.bbox.xmin, det.bbox.ymin, det.bbox.xmax, det.bbox.ymax
        )));

        if let Some(color) = &det.attributes.color_info {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("  Color: {} {:?}", color.color_name, color.rgb)));
        }

        if let Some(person_attrs) = &det.attributes.person_attrs {
            lines.push(Line::from(""));
            if let Some(gender) = &person_attrs.gender {
                lines.push(Line::from(format!("  Gender: {}", gender)));
            }
            if let Some(age) = &person_attrs.age_group {
                lines.push(Line::from(format!("  Age: {}", age)));
            }
        }

        lines
    } else {
        vec![Line::from("  No detection selected")]
    };

    let paragraph = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .title("🔍 Selected Object Details"),
    );

    f.render_widget(paragraph, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let status = if app.is_finished {
        format!("✓ Processing complete. {} total detections.", app.total_detections)
    } else {
        format!(
            "Processing frame {}... {} detections in current frame | {} total",
            app.frame_num,
            app.current_detections.len(),
            app.total_detections
        )
    };

    let footer = Paragraph::new(status)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}
