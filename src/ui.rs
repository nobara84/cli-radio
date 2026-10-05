use crate::{
    app::{App, Mode},
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
    let area = frame.area();
    if area.width < 38 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(
                "CLI Radio\nTerminal too small (minimum 38×12).\nq: Quit / Ctrl+C: Quit",
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
        Constraint::Length(2),
    ])
    .split(area);
    let search = format!(
        "Search: {}{}",
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
        let list_height = if rows[1].height < 15 { 3 } else { 5 };
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
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Stations · a: Add "),
        )
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, columns[0], &mut state);
    now_playing(frame, app, columns[1]);
    frame.render_widget(
        Paragraph::new(safe(&app.message))
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
    let help = if area.width < 65 {
        "↑↓/jk Enter Play Space Stop +/- Vol\n/ Find f Fav a Add e Edit d Del q Quit"
    } else {
        "↑↓/jk Select · Enter Play · Space Stop/Play · +/- Vol\nf Fav · / Search · a Add · e Edit · d Delete · q Quit"
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::Cyan)),
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
                "{} Name: {}\n{} URL: {}\n\nTab: switch field · Enter: Save · Esc: Cancel",
                if !field { "›" } else { " " },
                safe(name),
                if *field { "›" } else { " " },
                safe(url)
            );
            frame.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: true }).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(if id.is_some() {
                            " Edit Station "
                        } else {
                            " Add Station "
                        }),
                ),
                popup,
            );
        }
        Mode::Delete(_) => {
            let popup = centered(area, 55, 5);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new("Delete selected station?\ny: Delete · n/Esc: Cancel")
                    .block(Block::default().borders(Borders::ALL).title(" Confirm ")),
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
fn volume_line(volume: u8, width: u16) -> String {
    let volume = volume.min(100);
    let label = format!("Volume: {volume}%");
    let cells = usize::from(width).saturating_sub(label.len());
    if cells == 0 {
        return label;
    }
    let filled = cells * usize::from(volume) / 100;
    format!(
        "Volume {}{} {volume}%",
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Now Playing ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let now = Instant::now();
    let uptime = format_uptime(now.saturating_duration_since(app.session_started));
    let stats = format!("Disconnects: {} · Uptime: {uptime}", app.disconnects);
    let name = app
        .active
        .as_ref()
        .map(|s| safe(&s.name))
        .unwrap_or_else(|| "Select a station · Enter to play".into());
    let artist = available(&app.metadata.artist);
    let title = available(&app.metadata.title);
    let audio = format!(
        "{} · {}",
        available(&app.metadata.codec),
        available(&app.metadata.bitrate)
    );
    let status = match app.reconnect.phase {
        Phase::Playing => "● Playing".into(),
        Phase::Disconnected => "Connection lost".into(),
        _ => app.reconnect.status(now),
    };
    let status_style = Style::default()
        .fg(match app.reconnect.phase {
            Phase::Playing => Color::Green,
            Phase::Stopped => Color::Gray,
            _ => Color::Yellow,
        })
        .add_modifier(Modifier::BOLD);
    if inner.height < 7 {
        // Tiny stacked layouts keep playback controls and session statistics visible.
        let mut lines = vec![
            Line::styled(name, Style::default().add_modifier(Modifier::BOLD)),
            Line::from(format!("{artist} · {title}")),
        ];
        if inner.height >= 6 {
            lines.push(Line::from(audio));
        }
        if inner.height >= 5 {
            lines.push(Line::from(volume_line(app.config.volume, inner.width)));
            lines.push(Line::styled(status, status_style));
        } else {
            let remaining = inner
                .width
                .saturating_sub(status.chars().count() as u16 + 3);
            let volume = if remaining >= 13 {
                format!(" · {}", volume_line(app.config.volume, remaining))
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(status, status_style),
                Span::raw(volume),
            ]));
        }
        lines.push(Line::from(stats));
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let roomy = inner.height >= 15;
    let stats_height = if stats.chars().count() > usize::from(inner.width) {
        2
    } else {
        1
    };
    let sections = Layout::vertical([
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(if roomy { 1 } else { 0 }),
        Constraint::Length(2),
        Constraint::Length(stats_height),
    ])
    .split(inner);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(Color::Gray);
    let details = if roomy {
        vec![
            Line::styled(name, bold),
            Line::from(""),
            Line::styled("Artist", muted),
            Line::styled(artist, bold),
            Line::styled("Title", muted),
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
        Paragraph::new(volume_line(app.config.volume, inner.width)),
        sections[1],
    );
    frame.render_widget(
        Paragraph::new(status)
            .style(status_style)
            .wrap(Wrap { trim: true }),
        sections[3],
    );
    frame.render_widget(Paragraph::new(stats).wrap(Wrap { trim: true }), sections[4]);
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
        for width in 13..120 {
            for volume in [0, 5, 85, 100] {
                assert!(volume_line(volume, width).chars().count() <= usize::from(width));
            }
        }
        assert!(!volume_line(100, 5).contains('█'));
        assert!(!volume_line(0, 30).contains('█'));
        assert!(!volume_line(100, 30).contains('░'));
    }
}
