use crate::{
    app::{App, Mode},
    player::Control,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
pub async fn handle(app: &mut App, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }
    match &mut app.mode {
        Mode::Search => {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => app.mode = Mode::Normal,
                KeyCode::Backspace => {
                    app.search.pop();
                }
                KeyCode::Char(c) => {
                    app.search.push(c);
                }
                _ => {}
            }
            app.selected = 0;
            return false;
        }
        Mode::Form {
            name, url, field, ..
        } => {
            match key.code {
                KeyCode::Esc => app.mode = Mode::Normal,
                KeyCode::Tab | KeyCode::BackTab => *field = !*field,
                KeyCode::Enter => app.form_commit(),
                KeyCode::Backspace => {
                    if *field {
                        url.pop();
                    } else {
                        name.pop();
                    }
                }
                KeyCode::Char(c) if !c.is_control() => {
                    let value = if *field { url } else { name };
                    if value.len() < 4096 {
                        value.push(c);
                    }
                }
                _ => {}
            }
            return false;
        }
        Mode::Delete(id) => {
            let id = *id;
            match key.code {
                KeyCode::Char('y') => {
                    if app.active.as_ref().is_some_and(|s| s.id == id) {
                        app.stop().await;
                        app.active = None;
                    }
                    app.stations.retain(|s| s.id != id);
                    if app.config.last_station == Some(id) {
                        app.config.last_station = None;
                    }
                    app.selected = app.selected.min(app.visible().len().saturating_sub(1));
                    app.mode = Mode::Normal;
                    app.save();
                }
                KeyCode::Esc | KeyCode::Char('n') => app.mode = Mode::Normal,
                _ => {}
            }
            return false;
        }
        Mode::Normal => {}
    }
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('l') => {
            app.config.language.toggle();
            app.save();
        }
        KeyCode::Char('r') => app.toggle_random(std::time::Instant::now()),
        KeyCode::Down | KeyCode::Char('j') => {
            app.selected = (app.selected + 1).min(app.visible().len().saturating_sub(1))
        }
        KeyCode::Up | KeyCode::Char('k') => app.selected = app.selected.saturating_sub(1),
        KeyCode::Enter => app.play().await,
        KeyCode::Char(' ') => {
            if app.reconnect.desired {
                app.stop().await;
            } else {
                app.play().await;
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('-') => {
            app.config.volume = if key.code == KeyCode::Char('-') {
                app.config.volume.saturating_sub(5)
            } else {
                app.config.volume.saturating_add(5).min(100)
            };
            let _ = app.controls.send(Control::Volume(app.config.volume)).await;
            app.save();
        }
        KeyCode::Char('/') => app.mode = Mode::Search,
        KeyCode::Esc => {
            app.search.clear();
            app.selected = 0;
        }
        KeyCode::Char('a') => {
            app.mode = Mode::Form {
                id: None,
                name: String::new(),
                url: String::new(),
                field: false,
            }
        }
        KeyCode::Char('e') => {
            if let Some(s) = app.selected_station() {
                app.mode = Mode::Form {
                    id: Some(s.id),
                    name: s.name.clone(),
                    url: s.url.clone(),
                    field: false,
                };
            }
        }
        KeyCode::Char('d') => {
            if let Some(s) = app.selected_station() {
                app.mode = Mode::Delete(s.id);
            }
        }
        KeyCode::Char('f') => {
            if let Some(&index) = app.visible().get(app.selected) {
                let id = app.stations[index].id;
                app.stations[index].favorite = !app.stations[index].favorite;
                app.selected = app
                    .visible()
                    .iter()
                    .position(|&i| app.stations[i].id == id)
                    .unwrap_or(0);
                app.save();
            }
        }
        _ => {}
    }
    false
}
