use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Sparkline,
    },
    Frame,
};

use crate::tui::app::{App, TuiMode};

pub fn draw(f: &mut Frame, app: &App) {
    match app.tui_mode {
        TuiMode::Monitor => draw_monitor_mode(f, app),
        TuiMode::ZoneList => draw_zone_list_mode(f, app),
        TuiMode::ZoneEdit => draw_zone_edit_mode(f, app),
    }
}

fn draw_monitor_mode(f: &mut Frame, app: &App) {
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
        .constraints([
            Constraint::Percentage(35), // Performance stats
            Constraint::Percentage(25), // Class distribution  
            Constraint::Percentage(20), // Living beings
            Constraint::Percentage(20), // ROI Zones
        ])
        .split(area);

    draw_performance_stats(f, app, chunks[0]);
    draw_class_distribution(f, app, chunks[1]);
    draw_living_beings(f, app, chunks[2]);
    draw_zone_summary(f, app, chunks[3]);
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
        .take(8)
        .map(|(class, count)| {
            let bar_width = (*count * 15 / counts.first().map(|(_, c)| **c).unwrap_or(1).max(1)).min(15);
            let bar = "█".repeat(bar_width);
            ListItem::new(format!("{:<10} {:<15} {}", class, bar, count))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("📈 Classes (Total: {})", app.total_detections)),
    );

    f.render_widget(list, area);
}

fn draw_living_beings(f: &mut Frame, app: &App, area: Rect) {
    let mut beings: Vec<_> = app.living_beings.values().collect();
    beings.sort_by(|a, b| b.unique_ids.len().cmp(&a.unique_ids.len()));

    let items: Vec<ListItem> = beings
        .iter()
        .map(|being| {
            let unique_count = being.unique_ids.len();
            let icon = match being.class_name.as_str() {
                "person" => "👤",
                "dog" => "🐕",
                "cat" => "🐈",
                "bird" => "🐦",
                "horse" => "🐴",
                _ => "🦓",
            };
            
            let status = if being.last_seen_frame == app.frame_num {
                "LIVE"
            } else if app.frame_num - being.last_seen_frame < 30 {
                "RECENT"
            } else {
                "PAST"
            };
            
            let style_color = match status {
                "LIVE" => Color::Green,
                "RECENT" => Color::Yellow,
                _ => Color::Gray,
            };
            
            ListItem::new(Line::from(vec![
                Span::raw(format!("{} ", icon)),
                Span::styled(
                    format!("{:<8} ", being.class_name),
                    Style::default().fg(style_color).add_modifier(Modifier::BOLD)
                ),
                Span::raw(format!("×{} ", unique_count.max(1))),
                Span::styled(status, Style::default().fg(style_color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("🐾 Living Beings ({} unique)", app.total_living_seen)),
    );

    f.render_widget(list, area);
}

fn draw_zone_summary(f: &mut Frame, app: &App, area: Rect) {
    if app.zones.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from("  No zones configured"),
            Line::from("  Press 'Z' to manage zones"),
        ])
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("🎯 ROI Zones"));
        f.render_widget(empty, area);
        return;
    }
    
    let zone_counts = app.count_zone_detections();
    let enabled_count = app.zones.iter().filter(|z| z.enabled).count();
    
    let items: Vec<ListItem> = app.zones
        .iter()
        .take(5)
        .map(|zone| {
            let count = zone_counts.get(&zone.id).copied().unwrap_or(0);
            let status = if zone.enabled { "✓" } else { "✗" };
            let status_color = if zone.enabled { Color::Green } else { Color::Red };
            
            let name_truncated = if zone.name.len() > 12 {
                format!("{}...", &zone.name[..9])
            } else {
                zone.name.clone()
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(status, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {:<12} ", name_truncated)),
                Span::styled(
                    format!("×{}", count),
                    Style::default().fg(if count > 0 { Color::Cyan } else { Color::Gray })
                ),
            ]))
        })
        .collect();
    
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("🎯 ROI Zones ({}/{})", enabled_count, app.zones.len())),
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
    let header = Row::new(vec!["ID", "Class", "Conf", "Zone", "Color", "Position"])
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
            
            let zone_name = app.get_detection_zone_name(det)
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
                Cell::from(zone_name),
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
        
        // Add zone information
        if let Some(zone_name) = app.get_detection_zone_name(det) {
            lines.push(Line::from(format!("  Zone: {}", zone_name)));
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
            "Processing frame {}... {} detections | {} total | [Z] Zones | [P] Pause | [Q] Quit",
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

// ===== Zone Management UI =====

fn draw_zone_list_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Zone list
            Constraint::Length(5),  // Help
        ])
        .split(f.area());

    // Header
    let zone_count = format!(" | {} zones", app.zones.len());
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " ROI Zone Management ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(&zone_count),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Zone list
    draw_zone_list(f, app, chunks[1]);

    // Help footer
    let help_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("[N]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" New  "),
            Span::styled("[E]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Edit  "),
            Span::styled("[D]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" Delete  "),
            Span::styled("[Space]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" Toggle  "),
            Span::styled("[Esc]", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
            Span::raw(" Back"),
        ]),
    ];
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help, chunks[2]);
}

fn draw_zone_list(f: &mut Frame, app: &App, area: Rect) {
    if app.zones.is_empty() {
        let empty_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from("  No zones configured"),
            Line::from(""),
            Line::from("  Press 'N' to create a new zone"),
        ])
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("Zones"));
        f.render_widget(empty_msg, area);
        return;
    }

    let zone_counts = app.count_zone_detections();
    
    let rows: Vec<Row> = app.zones.iter().enumerate().map(|(i, zone)| {
        let status = if zone.enabled { "✓" } else { "✗" };
        let count = zone_counts.get(&zone.id).copied().unwrap_or(0);
        let area_pct = zone.bbox.area() * 100.0;
        
        let style = if i == app.selected_zone_idx {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        
        let status_color = if zone.enabled { Color::Green } else { Color::Red };
        
        Row::new(vec![
            Cell::from(format!("{}", i + 1)),
            Cell::from(zone.name.clone()),
            Cell::from(Span::styled(status, Style::default().fg(status_color))),
            Cell::from(format!("{}", count)),
            Cell::from(format!("{:.1}%", area_pct)),
            Cell::from(format!("({:.2},{:.2})", zone.bbox.xmin, zone.bbox.ymin)),
            Cell::from(format!("({:.2},{:.2})", zone.bbox.xmax, zone.bbox.ymax)),
        ])
        .style(style)
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),   // #
            Constraint::Min(15),     // Name
            Constraint::Length(6),   // Status
            Constraint::Length(8),   // Objects
            Constraint::Length(7),   // Area
            Constraint::Length(12),  // Top-Left
            Constraint::Length(12),  // Bottom-Right
        ],
    )
    .header(
        Row::new(vec![
            Cell::from("#"),
            Cell::from("Name"),
            Cell::from("Active"),
            Cell::from("Objects"),
            Cell::from("Area"),
            Cell::from("Top-Left"),
            Cell::from("Bot-Right"),
        ])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .bottom_margin(1),
    )
    .block(Block::default().borders(Borders::ALL).title("Zones"));

    f.render_widget(table, area);
}

fn draw_zone_edit_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Editor
            Constraint::Length(3),  // Help
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " Zone Editor ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Editor
    draw_zone_editor(f, app, chunks[1]);

    // Help footer
    let help = Paragraph::new(Line::from(vec![
        Span::styled("[↑↓←→]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" Resize  "),
        Span::styled("[Ctrl+↑↓←→]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" Resize-TL  "),
        Span::styled("[HJKL]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::raw(" Move  "),
        Span::styled("[Shift]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" Fine  "),
        Span::styled("[S]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(" Save  "),
        Span::styled("[Esc]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw(" Cancel"),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help, chunks[2]);
}

fn draw_zone_editor(f: &mut Frame, app: &App, area: Rect) {
    let Some(zone) = &app.zone_draft else {
        let error = Paragraph::new("No zone draft available")
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(error, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // LEFT: Form
    let form_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Name: ", Style::default().fg(Color::Yellow)),
            Span::raw(&zone.name),
        ]),
        Line::from(""),
        Line::from(Span::styled("Top-Left Corner:", Style::default().fg(Color::Cyan))),
        Line::from(format!(
            "  X: {:.1}% ({} px)",
            zone.bbox.xmin * 100.0,
            (zone.bbox.xmin * app.width as f32) as u32
        )),
        Line::from(format!(
            "  Y: {:.1}% ({} px)",
            zone.bbox.ymin * 100.0,
            (zone.bbox.ymin * app.height as f32) as u32
        )),
        Line::from(""),
        Line::from(Span::styled("Bottom-Right Corner:", Style::default().fg(Color::Cyan))),
        Line::from(format!(
            "  X: {:.1}% ({} px)",
            zone.bbox.xmax * 100.0,
            (zone.bbox.xmax * app.width as f32) as u32
        )),
        Line::from(format!(
            "  Y: {:.1}% ({} px)",
            zone.bbox.ymax * 100.0,
            (zone.bbox.ymax * app.height as f32) as u32
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Area: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.1}% of frame", zone.bbox.area() * 100.0)),
        ]),
    ];

    let form = Paragraph::new(form_lines)
        .block(Block::default().borders(Borders::ALL).title("Properties"));
    f.render_widget(form, chunks[0]);

    // RIGHT: Preview
    draw_zone_preview(f, app, zone, chunks[1]);
}

fn draw_zone_preview(f: &mut Frame, app: &App, zone: &crate::tui::roi::RoiZone, area: Rect) {
    let inner = Block::default()
        .borders(Borders::ALL)
        .title("Preview")
        .inner(area);
    
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Preview");
    f.render_widget(block, area);

    // Calculate zone rectangle in preview space
    let preview_w = inner.width as f32;
    let preview_h = inner.height as f32;
    
    let x1 = (zone.bbox.xmin * preview_w) as u16 + inner.x;
    let y1 = (zone.bbox.ymin * preview_h) as u16 + inner.y;
    let x2 = (zone.bbox.xmax * preview_w) as u16 + inner.x;
    let y2 = (zone.bbox.ymax * preview_h) as u16 + inner.y;

    // Draw ASCII box representation
    let mut lines = Vec::new();
    for y in inner.y..inner.y + inner.height {
        let mut line_spans = Vec::new();
        for x in inner.x..inner.x + inner.width {
            let is_border = (y == y1 || y == y2) && (x >= x1 && x <= x2)
                || (x == x1 || x == x2) && (y >= y1 && y <= y2);
            let is_corner = (x == x1 || x == x2) && (y == y1 || y == y2);
            
            if is_corner {
                line_spans.push(Span::styled("┼", Style::default().fg(Color::Yellow)));
            } else if is_border {
                if y == y1 || y == y2 {
                    line_spans.push(Span::styled("─", Style::default().fg(Color::Yellow)));
                } else {
                    line_spans.push(Span::styled("│", Style::default().fg(Color::Yellow)));
                }
            } else {
                line_spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(line_spans));
    }

    let preview = Paragraph::new(lines);
    f.render_widget(preview, inner);
}
