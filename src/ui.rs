use crate::{
    app::{App, Mode},
    i18n::Language,
    reconnect::Phase,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::time::Instant;
fn safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
pub fn draw(frame: &mut Frame, app: &App) {
    if matches!(app.mode, Mode::Info) {
        diagnostics(frame, app);
        return;
    }
    let area = frame.area();
    let lang = app.config.language;
    let footer = lang.footer(area.width);
    if area.width < 38 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(
                lang.text("CLI Radio\nTerminal too small (minimum 38×12).\nq: Quit / Ctrl+C: Quit"),
            ),
            area,
        );
        return;
    }
    let compact = area.height < 20;
    let message_height = if app.message.is_empty() { 0 } else { 1 };
    let rows = Layout::vertical([
        Constraint::Length(if compact { 1 } else { 3 }),
        Constraint::Min(4),
        Constraint::Length(message_height),
        Constraint::Length(footer.len() as u16),
    ])
    .split(area);
    let search = format!(
        "{}: {}{}",
        lang.text("Search"),
        safe(&app.search),
        if matches!(app.mode, Mode::Search) {
            " ▏"
        } else {
            ""
        }
    );
    let header = if compact {
        Paragraph::new(format!("CLI Radio · {search}"))
    } else {
        Paragraph::new(search).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CLI Radio ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
    };
    frame.render_widget(header, rows[0]);
    let columns = if area.width >= 80 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
            .split(rows[1])
    } else {
        let list_height = if app.stations.is_empty() {
            4
        } else if rows[1].height < 15 {
            3
        } else {
            5
        };
        Layout::vertical([Constraint::Length(list_height), Constraint::Min(4)]).split(rows[1])
    };
    let items: Vec<_> = app
        .visible()
        .into_iter()
        .map(|i| {
            let s = &app.stations[i];
            ListItem::new(format!(
                "{} {}",
                if s.favorite { "★" } else { " " },
                safe(&s.name)
            ))
        })
        .collect();
    let mut state = ListState::default().with_selected(if items.is_empty() {
        None
    } else {
        Some(app.selected.min(items.len() - 1))
    });
    let station_block = Block::default().borders(Borders::ALL).title(format!(
        " {} | a: {} ",
        lang.text("Stations"),
        lang.text("Add")
    ));
    if app.stations.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "{}\n{}",
                lang.text("No stations yet"),
                lang.text("Press a to add a station.")
            ))
            .wrap(Wrap { trim: true })
            .block(station_block),
            columns[0],
        );
    } else {
        let list = List::new(items)
            .block(station_block)
            .highlight_symbol("› ")
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_stateful_widget(list, columns[0], &mut state);
    }
    now_playing(frame, app, columns[1]);
    frame.render_widget(
        Paragraph::new(safe(&lang.message(&app.message)))
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(footer.join("\n")).style(Style::default().fg(Color::Cyan)),
        rows[3],
    );
    match &app.mode {
        Mode::Form {
            id,
            name,
            url,
            field,
        } => {
            let popup = centered(area, 80, 8);
            frame.render_widget(Clear, popup);
            let text = format!(
                "{} {}: {}\n{} URL: {}\n\n{}",
                if !field { "›" } else { " " },
                lang.text("Name"),
                safe(name),
                if *field { "›" } else { " " },
                safe(url),
                lang.text("Tab: switch field | Enter: Save | Esc: Cancel")
            );
            frame.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: true }).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(if id.is_some() {
                            lang.text("Edit Station")
                        } else {
                            lang.text("Add Station")
                        }),
                ),
                popup,
            );
        }
        Mode::Delete(_) => {
            let popup = centered(area, 55, 5);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(lang.text("Delete selected station?\ny: Delete | n/Esc: Cancel"))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(lang.text("Confirm")),
                    ),
                popup,
            );
        }
        _ => {}
    }
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub fn format_uptime(elapsed: std::time::Duration) -> String {
    let seconds = elapsed.as_secs();
    let clock = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    );
    if seconds >= 86_400 {
        format!("{}d {clock}", seconds / 86_400)
    } else {
        clock
    }
}
fn volume_line(volume: u8, width: u16, lang: Language) -> String {
    let volume = volume.min(100);
    let name = lang.text("Volume");
    let label = format!("{name}: {volume}%");
    let cells = usize::from(width).saturating_sub(label.len());
    if cells == 0 {
        return label;
    }
    let filled = cells * usize::from(volume) / 100;
    format!(
        "{name} {}{} {volume}%",
        "█".repeat(filled),
        "░".repeat(cells - filled)
    )
}
fn available(text: &str) -> String {
    let text = safe(text);
    if text.trim().is_empty() {
        "—".into()
    } else {
        text
    }
}
fn now_playing(frame: &mut Frame, app: &App, area: Rect) {
    let lang = app.config.language;
    let block = Block::default()
        .borders(if area.height < 8 {
            Borders::TOP
        } else {
            Borders::ALL
        })
        .title(format!(" {} ", lang.text("Now Playing")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let now = Instant::now();
    let uptime = format_uptime(now.saturating_duration_since(app.session_started));
    let stats = format!(
        "{}: {} · {}: {uptime}",
        lang.text("Disconnects"),
        app.disconnects,
        lang.text("Uptime")
    );
    let name = app
        .active
        .as_ref()
        .map(|s| safe(&s.name))
        .unwrap_or_else(|| lang.text("Select a station · Enter to play").into());
    let artist = available(&app.metadata.artist);
    let title = available(&app.metadata.title);
    let audio = format!(
        "{} · {}",
        available(&app.metadata.codec),
        available(&app.metadata.bitrate)
    );
    let buffer = lang.buffer(&app.cache);
    let status = lang.status(&app.reconnect, now);
    let random = lang.random_status(
        app.config.random_mode,
        &app.random_timer,
        now,
        app.config.random_interval_hours,
        inner.width < 50,
    );
    let status_style = Style::default()
        .fg(match app.reconnect.phase {
            Phase::Playing => Color::Green,
            Phase::Stopped => Color::Gray,
            _ => Color::Yellow,
        })
        .add_modifier(Modifier::BOLD);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    if inner.height < 8 {
        let stats = if stats.chars().count() > usize::from(inner.width) {
            format!(
                "{}: {} | {}: {uptime}",
                lang.text("Drops"),
                app.disconnects,
                lang.text("Up")
            )
        } else {
            stats
        };
        let mut lines = vec![Line::styled(format!("{name} | {artist} · {title}"), bold)];
        if inner.height >= 7 {
            lines.push(Line::from(audio));
        }
        if inner.height >= 6 {
            lines.push(Line::from(volume_line(
                app.config.volume,
                inner.width,
                lang,
            )));
        }
        lines.push(Line::from(vec![
            Span::styled(status, status_style),
            Span::raw(if inner.height == 4 {
                format!(" | {random}")
            } else {
                format!(" | {}%", app.config.volume)
            }),
        ]));
        if inner.height >= 4 {
            lines.push(Line::from(buffer));
        }
        if inner.height >= 5 || inner.height < 4 {
            lines.push(Line::from(random));
        }
        lines.push(Line::from(stats));
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let roomy = inner.height >= 16;
    let stats_height = if stats.chars().count() > usize::from(inner.width) {
        2
    } else {
        1
    };
    let random_height = if random.chars().count() > usize::from(inner.width) {
        2
    } else {
        1
    };
    let sections = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(if roomy { 1 } else { 0 }),
        Constraint::Length(if roomy { 2 } else { 1 }),
        Constraint::Length(random_height),
        Constraint::Length(stats_height),
    ])
    .split(inner);
    let muted = Style::default().fg(Color::Gray);
    let details = if roomy {
        vec![
            Line::styled(name, bold),
            Line::from(""),
            Line::styled(lang.text("Artist"), muted),
            Line::styled(artist, bold),
            Line::styled(lang.text("Title"), muted),
            Line::styled(title, bold.fg(Color::Cyan)),
            Line::from(""),
            Line::styled(audio, muted),
        ]
    } else {
        vec![
            Line::styled(name, bold),
            Line::styled(artist, bold),
            Line::styled(title, bold.fg(Color::Cyan)),
            Line::styled(audio, muted),
        ]
    };
    frame.render_widget(
        Paragraph::new(details).wrap(Wrap { trim: true }),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new(volume_line(app.config.volume, inner.width, lang)),
        sections[1],
    );
    frame.render_widget(Paragraph::new(buffer), sections[2]);
    frame.render_widget(
        Paragraph::new(status)
            .style(status_style)
            .wrap(Wrap { trim: true }),
        sections[4],
    );
    frame.render_widget(
        Paragraph::new(random).wrap(Wrap { trim: true }),
        sections[5],
    );
    frame.render_widget(Paragraph::new(stats).wrap(Wrap { trim: true }), sections[6]);
}
fn diagnostics(frame: &mut Frame, app: &App) {
    let lang = app.config.language;
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" cli-radio · {} ", lang.text("Info / Diagnostics")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let now = Instant::now();
    let proxy = app
        .active
        .as_ref()
        .map(|s| app.config.network.resolve(&s.url, &app.environment));
    // Only a fixed status label is exposed, never proxy URLs or environment values.
    let proxy = match proxy {
        Some(Ok(p)) if p.url.is_some() => "active (NO_PROXY may bypass)",
        Some(Err(_)) => "invalid configuration",
        _ => "inactive",
    };
    let mut lines = vec![
        format!("cli-radio {}", env!("CARGO_PKG_VERSION")),
        format!("{}: Markus Schneider", lang.text("Author")),
        "Backend: mpv".into(),
        lang.text("Runtime").into(),
        format!(
            "{}: {}",
            lang.text("Status"),
            lang.status(&app.reconnect, now)
        ),
        format!(
            "{}: {}",
            lang.text("Station"),
            app.active
                .as_ref()
                .map(|s| safe(&s.name))
                .unwrap_or_else(|| "—".into())
        ),
        format!(
            "Codec: {} · Bitrate: {}",
            available(&app.metadata.codec),
            available(&app.metadata.bitrate)
        ),
        lang.buffer(&app.cache),
        format!(
            "{}: {} · {}: {}",
            lang.text("Uptime"),
            format_uptime(now.saturating_duration_since(app.session_started)),
            lang.text("Disconnects"),
            app.disconnects
        ),
        lang.random_status(
            app.config.random_mode,
            &app.random_timer,
            now,
            app.config.random_interval_hours,
            true,
        ),
        format!("Proxy: {}", lang.text(proxy)),
        format!(
            "mpv: {}",
            app.mpv_version
                .as_deref()
                .map(safe)
                .unwrap_or_else(|| "—".into())
        ),
    ];
    if rows[0].height >= 31 {
        lines.push(lang.text("Technical").into());
        lines.extend([
            lang.text("Control: JSON IPC").into(),
            lang.text("mpv.conf: disabled (--no-config)").into(),
            lang.text("Cache: memory only; disk caching disabled")
                .into(),
            lang.text("Readahead: target ~10 s").into(),
            lang.text("Network timeout: 15 s").into(),
            lang.text("Cache-pause watchdog: 30 s").into(),
            lang.text("Position-stall watchdog: 60 s").into(),
            lang.text("Reconnect: enabled, unlimited retries with backoff")
                .into(),
            lang.text("Useful commands").into(),
            "cli-radio --version".into(),
            format!(
                "tail -f {}",
                shell_path(&app.store.state_dir.join("cli-radio.log"))
            ),
            "ps -ww -C mpv -o pid,args".into(),
            "systemctl --user status pipewire".into(),
            "pactl info".into(),
            lang.text("Files").into(),
            format!(
                "Config: {}",
                app.store.config_dir.join("config.toml").display()
            ),
            format!(
                "{}: {}",
                lang.text("Stations"),
                app.store.data_dir.join("stations.toml").display()
            ),
            format!(
                "Log: {}",
                app.store.state_dir.join("cli-radio.log").display()
            ),
        ]);
    } else {
        lines.push(lang.text("Small terminal: runtime only").into());
    }
    // Single-line clipping preserves borders even for arbitrary paths/metadata.
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .map(|s| Line::from(safe(&s)))
                .collect::<Vec<_>>(),
        ),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(lang.text("i / Esc: Back")).style(Style::default().fg(Color::Cyan)),
        rows[1],
    );
}
fn shell_path(path: &std::path::Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn uptime_formats_days_without_wrapping() {
        assert_eq!(format_uptime(Duration::ZERO), "00:00:00");
        assert_eq!(format_uptime(Duration::from_secs(1634)), "00:27:14");
        assert_eq!(format_uptime(Duration::from_secs(86400)), "1d 00:00:00");
        assert_eq!(format_uptime(Duration::from_secs(202472)), "2d 08:14:32");
    }
    #[test]
    fn volume_bar_fits_available_width() {
        for lang in [Language::German, Language::English] {
            for width in 17..120 {
                for volume in [0, 5, 85, 100] {
                    assert!(volume_line(volume, width, lang).chars().count() <= usize::from(width));
                }
            }
        }
        assert!(!volume_line(100, 5, Language::English).contains('█'));
        assert!(!volume_line(0, 30, Language::English).contains('█'));
        assert!(!volume_line(100, 30, Language::English).contains('░'));
    }
}
