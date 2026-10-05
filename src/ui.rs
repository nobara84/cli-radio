use crate::app::{App, Mode};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
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
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
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
    frame.render_widget(
        Paragraph::new(search).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CLI Radio ")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        rows[0],
    );
    let columns = Layout::default()
        .direction(if area.width >= 80 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(rows[1]);
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
    let name = app
        .active
        .as_ref()
        .map(|s| safe(&s.name))
        .unwrap_or_else(|| "Select a station and press Enter".into());
    let details = format!(
        "{}\n{}\n{}\n{} {}\nVolume: {}%\nStatus: {}",
        name,
        safe(&app.metadata.artist),
        safe(&app.metadata.title),
        safe(&app.metadata.codec),
        safe(&app.metadata.bitrate),
        app.config.volume,
        app.reconnect.status(Instant::now())
    );
    frame.render_widget(
        Paragraph::new(details).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Now Playing "),
        ),
        columns[1],
    );
    frame.render_widget(
        Paragraph::new(safe(&app.message))
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
    frame.render_widget(Paragraph::new("↑↓/jk Select · Enter Play · Space Stop/Play · +/- Vol\nf Fav · / Search · a Add · e Edit · d Delete · q Quit").style(Style::default().fg(Color::Cyan)), rows[3]);
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
