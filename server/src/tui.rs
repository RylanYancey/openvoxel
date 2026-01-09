use std::{collections::VecDeque, io};

use bevy::{
    ecs::system::Local,
    log::{LogPlugin, info, tracing},
    prelude::{
        App, AppExit, Commands, IntoScheduleConfigs, Last, MessageWriter, Plugin, PreStartup,
        ResMut, Resource, Update, on_message,
    },
    utils::default,
};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    prelude::*,
    text::Span,
    widgets::{Block, Paragraph},
};

pub struct TuiPlugin;

impl Plugin for TuiPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .insert_resource(Terminal::default())
            .add_plugins(
                LogPlugin {
                    custom_layer: move |app| {
                        Some(Box::new(LogCapture {
                            tx: app.main()
                                .world()
                                .get_resource::<Terminal>()
                                .unwrap()
                                .logs.tx.clone()
                        }))
                    },
                    fmt_layer: move |_| None,
                    ..default()
                }
            )
            .add_systems(Update, render_tui)
            .add_systems(Last, cleanup_terminal.run_if(on_message::<AppExit>))
        ;
    }
}

fn render_tui(
    mut terminal: ResMut<Terminal>,
    mut exit: MessageWriter<AppExit>,
    mut not_first: Local<bool>,
) {
    let mut needs_redraw = false;
    if !*not_first {
        needs_redraw = true;
        *not_first = true;
    }

    while event::poll(std::time::Duration::ZERO).unwrap() {
        needs_redraw = true;
        if let Event::Key(key) = event::read().unwrap() {
            match key.code {
                _ if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    exit.write(AppExit::Success);
                }
                _ if key.code == KeyCode::Char('d')
                    && key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    terminal.prompt.clear();
                }
                KeyCode::Enter => {
                    if let Some(text) = terminal.prompt.submit() {
                        info!("Submitted: {text}");
                    }
                }
                KeyCode::Backspace => terminal.prompt.backspace(),
                KeyCode::Delete => terminal.prompt.delete(),
                KeyCode::Tab => terminal.prompt.autofill(),
                KeyCode::Right => terminal.prompt.cursor_next(),
                KeyCode::Left => terminal.prompt.cursor_prev(),
                KeyCode::Up => terminal.prompt.history_up(),
                KeyCode::Down => terminal.prompt.history_down(),
                _ => {
                    if let Some(c) = key.code.as_char() {
                        terminal.prompt.insert_at_cursor(c);
                    }
                }
            }
        }
    }

    if terminal.logs.recv() != 0 {
        needs_redraw = true;
    }

    if needs_redraw {
        terminal.draw().unwrap();
    }
}

fn cleanup_terminal() {
    ratatui::restore()
}

#[derive(Resource)]
struct Terminal {
    ctx: DefaultTerminal,
    prompt: Prompt,
    logs: Logs,
}

impl Terminal {
    fn draw(&mut self) -> Result<(), io::Error> {
        self.ctx.draw(|frame: &mut Frame| {
            let vertical = Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]);
            let [content_area, input_area] = vertical.areas(frame.area());
            let horizon =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]);
            let [content_area, logs_area] = horizon.areas(content_area);

            frame.render_widget(Paragraph::new("hi"), content_area);

            self.logs.draw(frame, logs_area);
            self.prompt.draw(frame, input_area);
        })?;

        Ok(())
    }
}

impl Default for Terminal {
    fn default() -> Self {
        color_eyre::install().unwrap();
        Self {
            ctx: ratatui::init(),
            prompt: Prompt::default(),
            logs: Logs::default(),
        }
    }
}

struct Prompt {
    /// index of currently selected character in input.
    cursor: usize,

    /// the users keboard input.
    input: String,

    /// Previously submitted input.
    history: Vec<String>,

    /// Current index in the history.
    history_cursor: usize,

    /// hint text that tries to predict what the user
    /// will input.
    hint: String,
}

impl Prompt {
    #[rustfmt::skip]
    fn draw(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(
                Text::from(Line::from(vec![
                    Span::styled(self.input.clone(), Style::default().fg(Color::White)),
                    Span::styled(self.hint.clone(), Style::default().fg(Color::Gray)),
                ]))
            ).block(
                Block::bordered()
                    .title("Command Prompt")
            ),
            area
        );

        frame.set_cursor_position(Position::new(
            area.x + self.cursor as u16 + 1,
            area.y + 1,
        ));
    }

    fn cursor_next(&mut self) {
        if self.cursor == self.len() {
            self.autofill();
        } else {
            self.cursor += 1;
        }
    }

    fn cursor_prev(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn clear(&mut self) {
        *self = Self {
            history: std::mem::take(&mut self.history),
            ..Default::default()
        };
    }

    fn autofill(&mut self) {
        self.input.push_str(&self.hint);
        self.cursor = self.len();
        self.hint.clear();
    }

    fn backspace(&mut self) {
        if self.cursor != 0 {
            self.input.remove(self.index() - 1);
            self.cursor_prev();
            self.hint.clear();
        }
    }

    fn delete(&mut self) {
        if !self.input.is_empty() && !self.cursor_is_at_end() {
            self.input.remove(self.index());
            self.hint.clear();
        }
    }

    fn submit(&mut self) -> Option<String> {
        if self.input.is_empty() {
            None
        } else {
            self.history.push(self.input.clone());
            self.history_cursor = self.history.len();
            let ret = std::mem::take(&mut self.input);
            self.clear();
            Some(ret)
        }
    }

    fn history_up(&mut self) {
        if self.history_cursor != 0 {
            if self.history_cursor < self.history.len() {
                self.history_cursor -= 1;
                self.input = self.history[self.history_cursor].clone();
            } else if self.history_cursor == self.history.len() {
                if self.input.is_empty() {
                    self.history_cursor -= 1;
                    self.input = self.history[self.history_cursor].clone();
                    self.hint.clear();
                    self.cursor = self.len();
                } else {
                    todo!("Select all text")
                }
            } else {
                unreachable!("History cursor should never be greater than length of history.")
            }
        }
    }

    fn history_down(&mut self) {
        if self.history_cursor < self.history.len() {
            self.history_cursor += 1;
            if self.history_cursor >= self.history.len() {
                self.clear();
            } else {
                self.input = self.history[self.history_cursor].clone();
                self.hint = String::new();
                self.cursor = self.len();
            }
        }
    }

    fn len(&self) -> usize {
        self.input.chars().count()
    }

    fn insert_at_cursor(&mut self, char: char) {
        if self.cursor_is_at_end() {
            if self.hint.chars().next().is_some_and(|c| c != char) {
                self.hint.clear();
            }
        }

        self.input.insert(self.index(), char);
        self.cursor_next();
    }

    fn cursor_is_at_end(&self) -> bool {
        self.cursor == self.input.len()
    }

    /// Byte index of the char at the cursor, just
    /// in case any of it is UTF-16 or UTF-32 encoded.
    fn index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor)
            .unwrap_or(self.input.len())
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            cursor: 0,
            input: String::new(),
            hint: String::new(),
            history: Vec::new(),
            history_cursor: 0,
        }
    }
}

struct Logs {
    buffer: VecDeque<String>,
    tx: Sender<String>,
    rx: Receiver<String>,
}

impl Logs {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        for i in 0..area.height as usize {
            if let Some(s) = self.buffer.get(i) {
                lines.push(Line::from(Span::from(&*s)));
            } else {
                break;
            }
        }

        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("Logs")),
            area,
        );
    }

    fn recv(&mut self) -> usize {
        let mut amt = 0;

        while let Ok(item) = self.rx.try_recv() {
            amt += 1;
            self.buffer.push_back(item);
        }

        while self.buffer.len() > 50 {
            self.buffer.pop_front();
        }

        amt
    }
}

impl Default for Logs {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::bounded(128);
        Self {
            buffer: VecDeque::new(),
            tx,
            rx,
        }
    }
}

struct LogCapture {
    tx: Sender<String>,
}

impl<S> tracing_subscriber::Layer<S> for LogCapture
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        let level = event.metadata().level();
        let target = event.metadata().target();
        let message = format!("[{}] {}: {}", level, target, visitor.message);

        if let Err(TrySendError::Full(_)) = self.tx.try_send(message) {
            println!("LOG BUFFER OVERFLOW. PRINT LESS LOGS");
        }
    }
}

#[derive(Default)]
struct LogVisitor {
    message: String,
}

impl tracing::field::Visit for LogVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}
