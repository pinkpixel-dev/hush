//! Dialogs: forms with masked fields, confirmations, and the help list.
//! Field text lives in `Zeroizing` strings, so closing a dialog (or the idle
//! lock dropping it) wipes whatever was typed.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use zeroize::Zeroizing;

use super::theme;

#[derive(Clone)]
pub enum Kind {
    AddSecret,
    EditSecret { key: String },
    RenameSecret { key: String },
    DeleteSecret { key: String },
    RenameEnv,
    CopyEnv,
    DeleteEnv,
    RenameProject,
    DeleteProject,
    Help,
}

pub struct Field {
    pub label: &'static str,
    pub text: Zeroizing<String>,
    masked: bool,
    shown: bool,
}

impl Field {
    pub fn plain(label: &'static str, text: &str) -> Self {
        Self { label, text: Zeroizing::new(text.to_string()), masked: false, shown: false }
    }

    pub fn secret(label: &'static str) -> Self {
        Self { label, text: Zeroizing::new(String::new()), masked: true, shown: false }
    }

    pub fn is_secret(&self) -> bool {
        self.masked
    }
}

pub enum Outcome {
    Continue,
    Cancel,
    Submit,
}

/// Where the dialog's clickable parts were drawn last frame.
#[derive(Default)]
struct Hits {
    fields: Vec<Rect>,
    ok: Rect,
    cancel: Rect,
}

pub struct Dialog {
    pub kind: Kind,
    /// The project and environment the dialog was opened for.
    pub project: String,
    pub env: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
    hits: Hits,
}

const HELP: [(&str, &str, &str, &str); 6] = [
    ("↑↓ jk", "move", "a", "add"),
    ("←→ hl", "switch pane", "e", "edit value"),
    ("tab", "next pane", "r", "rename"),
    ("enter", "show value", "d", "delete"),
    ("/", "find secrets", "c", "copy environment"),
    ("q", "quit", "?", "this help"),
];

impl Dialog {
    pub fn new(kind: Kind, project: &str, env: &str, fields: Vec<Field>, focus: usize) -> Self {
        let project = project.to_string();
        Self { kind, project, env: env.to_string(), fields, focus, error: None, hits: Hits::default() }
    }

    fn is_confirm(&self) -> bool {
        matches!(self.kind, Kind::DeleteSecret { .. })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if matches!(self.kind, Kind::Help) {
            return Outcome::Cancel;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Enter => return Outcome::Submit,
            KeyCode::Char('y') if self.is_confirm() => return Outcome::Submit,
            KeyCode::Char('n') if self.is_confirm() => return Outcome::Cancel,
            KeyCode::Tab | KeyCode::Down if !self.fields.is_empty() => {
                self.focus = (self.focus + 1) % self.fields.len();
            }
            KeyCode::BackTab | KeyCode::Up if !self.fields.is_empty() => {
                self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
            }
            KeyCode::Char('r') if ctrl => {
                if let Some(field) = self.fields.get_mut(self.focus).filter(|f| f.masked) {
                    field.shown = !field.shown;
                }
            }
            KeyCode::Char('u') if ctrl => self.edit(|text| text.clear()),
            KeyCode::Backspace => self.edit(|text| {
                text.pop();
            }),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => self.edit(|text| text.push(c)),
            _ => {}
        }
        Outcome::Continue
    }

    /// Bracketed paste. Secret fields keep inner line breaks (for things like
    /// PEM keys) minus one trailing newline. Name fields drop them all.
    pub fn paste(&mut self, pasted: &str) {
        let Some(field) = self.fields.get_mut(self.focus) else { return };
        if field.masked {
            let trimmed = pasted.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(pasted);
            field.text.push_str(trimmed);
        } else {
            field.text.extend(pasted.chars().filter(|c| !matches!(c, '\n' | '\r')));
        }
        self.error = None;
    }

    pub fn click(&mut self, at: Position) -> Outcome {
        if self.hits.ok.contains(at) {
            return Outcome::Submit;
        }
        if self.hits.cancel.contains(at) {
            return Outcome::Cancel;
        }
        if let Some(index) = self.hits.fields.iter().position(|r| r.contains(at)) {
            self.focus = index;
        }
        Outcome::Continue
    }

    fn edit(&mut self, change: impl FnOnce(&mut String)) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            change(&mut field.text);
            self.error = None;
        }
    }

    fn title(&self) -> String {
        let (p, e) = (&self.project, &self.env);
        match &self.kind {
            Kind::AddSecret => "Add secret".into(),
            Kind::EditSecret { key } => format!("Edit {key} · {p}/{e}"),
            Kind::RenameSecret { key } => format!("Rename {key}"),
            Kind::DeleteSecret { key } => format!("Delete {key}?"),
            Kind::RenameEnv => format!("Rename {p}/{e}"),
            Kind::CopyEnv => format!("Copy {p}/{e}"),
            Kind::DeleteEnv => format!("Delete {p}/{e}"),
            Kind::RenameProject => format!("Rename {p}"),
            Kind::DeleteProject => format!("Delete {p}"),
            Kind::Help => "Keys".into(),
        }
    }

    fn ok_label(&self) -> &'static str {
        match self.kind {
            Kind::AddSecret | Kind::EditSecret { .. } => "Save",
            Kind::RenameSecret { .. } | Kind::RenameEnv | Kind::RenameProject => "Rename",
            Kind::CopyEnv => "Copy",
            Kind::DeleteSecret { .. } | Kind::DeleteEnv | Kind::DeleteProject => "Delete",
            Kind::Help => "Close",
        }
    }

    /// A line of explanation above the fields, where one is needed.
    fn message(&self) -> Option<String> {
        match &self.kind {
            Kind::DeleteSecret { key } => Some(format!("Remove {key} from {}/{}?", self.project, self.env)),
            Kind::DeleteEnv => Some(format!("Every secret in {}/{} goes too.", self.project, self.env)),
            Kind::DeleteProject => Some(format!("Every environment in {} goes too.", self.project)),
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let help = matches!(self.kind, Kind::Help);
        let body = if help { HELP.len() + 2 } else { self.fields.len() + usize::from(self.message().is_some()) };
        // Body, then a blank line, the error line, and the buttons, plus borders and margin.
        let height = (body + 3 + 4) as u16;
        let width = if help { 60 } else { 58 };
        let area = frame.area().centered(Constraint::Max(width), Constraint::Length(height));
        frame.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::accent())
            .title(Span::styled(format!(" {} ", self.title()), theme::bold()));
        let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 1));
        frame.render_widget(block, area);

        let mut rows = Rows { area: inner, y: inner.y };
        if help {
            for (k1, d1, k2, d2) in HELP {
                rows.put(frame, Line::from(vec![
                    Span::styled(format!("{k1:<9}"), theme::accent()),
                    Span::raw(format!("{d1:<16}")),
                    Span::styled(format!("{k2:<4}"), theme::accent()),
                    Span::raw(d2),
                ]));
            }
            rows.put(frame, Line::raw(""));
            rows.put(frame, Line::styled("Mouse: click to select, click again to show a value", theme::dim()));
        }
        if let Some(message) = self.message() {
            rows.put(frame, Line::raw(message));
        }
        self.hits.fields.clear();
        for (index, field) in self.fields.iter().enumerate() {
            self.hits.fields.push(rows.next_rect());
            rows.put(frame, field_line(field, index == self.focus, inner.width));
        }
        rows.put(frame, Line::raw(""));
        rows.put(frame, match &self.error {
            Some(error) => Line::styled(format!("✗ {error}"), theme::ERROR),
            None => Line::raw(""),
        });

        let ok = format!("[ {} ]", self.ok_label());
        let cancel = "[ Cancel ]";
        let line = rows.next_rect();
        self.hits.ok = Rect { width: ok.chars().count() as u16, ..line };
        self.hits.cancel = if help {
            Rect::default()
        } else {
            Rect { x: line.x + self.hits.ok.width + 2, width: cancel.len() as u16, ..line }
        };
        let mut buttons = vec![Span::styled(ok, theme::accent().patch(theme::bold()))];
        if !help {
            buttons.push(Span::raw("  "));
            buttons.push(Span::styled(cancel, theme::dim()));
        }
        if self.fields.get(self.focus).is_some_and(|f| f.masked) {
            buttons.push(Span::styled("   ctrl+r show", theme::dim()));
        }
        rows.put(frame, Line::from(buttons));
    }
}

/// Draws lines one under another inside an area.
struct Rows {
    area: Rect,
    y: u16,
}

impl Rows {
    fn next_rect(&self) -> Rect {
        Rect { y: self.y, height: 1, ..self.area }
    }

    fn put(&mut self, frame: &mut Frame, line: Line) {
        if self.y < self.area.bottom() {
            frame.render_widget(Paragraph::new(line), self.next_rect());
        }
        self.y += 1;
    }
}

fn field_line(field: &Field, focused: bool, width: u16) -> Line<'static> {
    let label_style = if focused { theme::accent() } else { theme::dim() };
    let shown: String = if field.masked && !field.shown {
        "●".repeat(field.text.chars().count())
    } else {
        field.text.chars().map(|c| if c == '\n' { '↵' } else { c }).collect()
    };
    // Keep the end of long text visible, since that's where typing happens.
    let room = (width as usize).saturating_sub(15);
    let skip = shown.chars().count().saturating_sub(room);
    let visible: String = shown.chars().skip(skip).collect();
    let mut spans = vec![Span::styled(format!("{:<13}", field.label), label_style), Span::raw(visible)];
    if focused {
        spans.push(Span::styled("█", theme::accent()));
    }
    Line::from(spans)
}
