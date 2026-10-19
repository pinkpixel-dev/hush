//! Drawing. Values are masked unless the selected secret is revealed, and
//! the masks are a fixed length so they don't give away a value's size.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};

use super::app::{App, Command, Pane, Screen, Tone};
use super::theme;
use crate::vault::store::Mode;

const MASK: &str = "••••••••••••";
/// Below this width only the focused pane is shown.
const NARROW: u16 = 80;

pub fn draw(frame: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Splash(_) => logo_screen(frame, vec![Line::styled(concat!("v", env!("CARGO_PKG_VERSION")), theme::dim())]),
        Screen::Locked => logo_screen(frame, lock_lines(app)),
        Screen::Main => {
            main(frame, app);
            if let Some(dialog) = app.dialog.as_mut() {
                dialog.render(frame);
            }
        }
    }
}

fn logo_screen(frame: &mut Frame, below: Vec<Line<'static>>) {
    let mut lines: Vec<Line> = theme::LOGO.iter().map(|row| Line::styled(*row, theme::accent())).collect();
    lines.push(Line::raw(""));
    lines.extend(below);
    let height = lines.len() as u16;
    let area = frame.area().centered_vertically(Constraint::Length(height));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn lock_lines(app: &App) -> Vec<Line<'static>> {
    let prompt = match app.session.mode {
        Mode::Keychain => Line::styled("locked · enter to unlock", theme::dim()),
        Mode::Passphrase => Line::from(vec![
            Span::styled("passphrase  ", theme::dim()),
            Span::raw("●".repeat(app.passphrase.chars().count())),
            Span::styled("█", theme::accent()),
        ]),
    };
    let mut lines = vec![prompt, Line::raw("")];
    if let Some(status) = &app.status {
        lines.push(status_line(&status.text, &status.tone));
    }
    lines
}

fn main(frame: &mut Frame, app: &mut App) {
    let [header, body, status, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1)])
            .areas(frame.area());
    draw_header(frame, app, header);

    app.hits.panes.clear();
    if body.width < NARROW {
        draw_pane(frame, app, app.pane, body);
    } else {
        let width = |names: Vec<String>, min, max| {
            let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(0) as u16;
            (longest + 6).clamp(min, max)
        };
        let [projects, envs, secrets] = Layout::horizontal([
            Constraint::Length(width(app.projects(), 14, 26)),
            Constraint::Length(width(app.envs(), 12, 20)),
            Constraint::Fill(1),
        ])
        .areas(body);
        for (pane, area) in [(Pane::Projects, projects), (Pane::Envs, envs), (Pane::Secrets, secrets)] {
            draw_pane(frame, app, pane, area);
        }
    }

    if let Some(s) = &app.status {
        frame.render_widget(Paragraph::new(status_line(&s.text, &s.tone)), status.inner(ratatui::layout::Margin::new(1, 0)));
    }
    draw_footer(frame, app, footer);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![Span::styled(" hush ", theme::accent().patch(theme::bold()))];
    if let Some(project) = &app.sel.project {
        left.push(Span::raw(format!(" {project}")));
        if let Some(env) = &app.sel.env {
            left.push(Span::styled(" / ", theme::dim()));
            left.push(Span::raw(env.clone()));
        }
    }
    let mode = match app.session.mode {
        Mode::Keychain => "keychain",
        Mode::Passphrase => "passphrase",
    };
    let count = app.secrets().len();
    let right = format!("{mode} · {count} {} ", if count == 1 { "secret" } else { "secrets" });
    frame.render_widget(Paragraph::new(Line::from(left)), area);
    frame.render_widget(Paragraph::new(Line::styled(right, theme::dim())).alignment(Alignment::Right), area);
}

fn draw_pane(frame: &mut Frame, app: &mut App, pane: Pane, area: Rect) {
    let focused = app.pane == pane;
    let title = match pane {
        Pane::Projects => " Projects ".to_string(),
        Pane::Envs => " Envs ".to_string(),
        Pane::Secrets => match &app.find {
            Some(find) => format!(" Secrets · /{find} "),
            None => " Secrets ".to_string(),
        },
    };
    let title_style = if focused { theme::accent().patch(theme::bold()) } else { theme::dim() };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(Span::styled(title, title_style));
    let inner = block.inner(area);
    app.hits.panes.push((pane, area, inner));

    let names = app.names(pane);
    if names.is_empty() {
        let hint = match (pane, &app.find) {
            (Pane::Secrets, Some(find)) if !find.is_empty() => format!("Nothing matches /{find}"),
            (Pane::Projects, _) => "Empty. Press a to add a secret".to_string(),
            _ => String::new(),
        };
        frame.render_widget(Paragraph::new(Line::styled(hint, theme::dim())).block(block), area);
        return;
    }

    let selected = app.selected_index(pane);
    let name_width = names.iter().map(|n| n.chars().count()).max().unwrap_or(0).clamp(8, 28);
    let revealed = app.revealed_value().map(|v| v.replace('\n', "↵"));
    let items: Vec<ListItem> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let is_selected = selected == Some(index);
            let marker = match (is_selected, focused) {
                (true, true) => Span::styled("● ", theme::accent()),
                (true, false) => Span::styled("● ", theme::dim()),
                _ => Span::raw("  "),
            };
            let name_style = if is_selected { theme::bold() } else { ratatui::style::Style::new() };
            let mut spans = vec![marker, Span::styled(format!("{name:<name_width$}"), name_style)];
            if pane == Pane::Secrets {
                spans.push(Span::raw("  "));
                spans.push(match (&revealed, is_selected) {
                    (Some(value), true) => Span::styled(value.clone(), theme::EXPOSED),
                    _ => Span::styled(MASK, theme::dim()),
                });
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let state = &mut app.lists[pane.index()];
    state.select(selected);
    frame.render_stateful_widget(List::new(items).block(block), area, state);
}

fn status_line(text: &str, tone: &Tone) -> Line<'static> {
    match tone {
        Tone::Ok => Line::styled(format!("✓ {text}"), theme::OK),
        Tone::Error => Line::styled(format!("✗ {text}"), theme::ERROR),
        Tone::Busy => Line::styled(format!("· {text}"), theme::accent()),
    }
}

fn draw_footer(frame: &mut Frame, app: &mut App, area: Rect) {
    app.hits.footer.clear();
    if app.finding {
        let find = app.find.clone().unwrap_or_default();
        let line = Line::from(vec![
            Span::styled(" /", theme::accent()),
            Span::raw(find),
            Span::styled("█", theme::accent()),
            Span::styled("   enter keep · esc clear", theme::dim()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let hints: &[(&str, &str, Command)] = match app.pane {
        Pane::Projects => &[("a", "add", Command::Add), ("r", "rename", Command::Rename), ("d", "delete", Command::Delete)],
        Pane::Envs => &[
            ("a", "add", Command::Add),
            ("c", "copy", Command::Copy),
            ("r", "rename", Command::Rename),
            ("d", "delete", Command::Delete),
        ],
        Pane::Secrets => &[
            ("enter", "show", Command::Reveal),
            ("a", "add", Command::Add),
            ("e", "edit", Command::Edit),
            ("r", "rename", Command::Rename),
            ("d", "delete", Command::Delete),
            ("/", "find", Command::Find),
        ],
    };
    let common: [(&str, &str, Command); 2] = [("?", "help", Command::Help), ("q", "quit", Command::Quit)];
    // Help and quit always show. Pane hints drop off the end when it's tight.
    let width_of = |&(key, label, _): &(&str, &str, Command)| (key.chars().count() + 1 + label.len()) as u16 + 2;
    let mut shown: Vec<_> = hints.iter().chain(common.iter()).copied().collect();
    while shown.len() > common.len() && shown.iter().map(width_of).sum::<u16>() + 1 > area.width {
        shown.remove(shown.len() - common.len() - 1);
    }
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    for (key, label, command) in shown {
        let width = (key.chars().count() + 1 + label.len()) as u16;
        if x + width > area.right() {
            break;
        }
        app.hits.footer.push((Rect { x, y: area.y, width, height: 1 }, command));
        spans.push(Span::styled(key, theme::accent()));
        spans.push(Span::styled(format!(" {label}  "), theme::dim()));
        x += width + 2;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
