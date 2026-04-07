//! AI conversation overlay for Kaku.
//!
//! Activated via Cmd+Shift+I. Renders a full-pane chat TUI using raw termwiz
//! Change sequences, communicating with the LLM via a background thread and
//! std::sync::mpsc for streaming tokens.

use crate::ai_client::{AiClient, ApiMessage, AssistantConfig};
use mux::pane::PaneId;
use mux::termwiztermtab::TermWizTerminal;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use termwiz::cell::{AttributeChange, CellAttributes};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers, MouseButtons, MouseEvent};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::Terminal;

/// Terminal context captured from the active pane before entering chat mode.
pub struct TerminalContext {
    pub cwd: String,
    pub visible_lines: Vec<String>,
    pub git_branch: Option<String>,
}

// ─── Message model ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Role {
    User,
    Assistant,
}

#[derive(Clone)]
struct Message {
    role: Role,
    content: String,
    /// False while the assistant is still streaming.
    complete: bool,
}

// ─── Streaming tokens ────────────────────────────────────────────────────────

enum StreamMsg {
    Token(String),
    Done,
    Err(String),
}

// ─── App state ───────────────────────────────────────────────────────────────

struct App {
    messages: Vec<Message>,
    input: String,
    input_cursor: usize,
    /// Lines scrolled up from the bottom (0 = show the latest messages).
    scroll_offset: usize,
    is_streaming: bool,
    model: String,
    token_rx: Option<Receiver<StreamMsg>>,
    cols: usize,
    rows: usize,
    error: Option<String>,
    /// Context to include in the first system message.
    context: TerminalContext,
}

impl App {
    fn new(context: TerminalContext, model: String, cols: usize, rows: usize) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            is_streaming: false,
            model,
            token_rx: None,
            cols,
            rows,
            error: None,
            context,
        }
    }

    fn content_width(&self) -> usize {
        self.cols.saturating_sub(4) // 2 border + 2 padding per side
    }

    /// Total visible rows for the message area.
    fn msg_area_height(&self) -> usize {
        self.rows.saturating_sub(4) // top border + separator + input + bottom border
    }

    /// Submit the current input as a user message and kick off a stream.
    fn submit(&mut self, client_cfg: AssistantConfig) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input.clear();
        self.input_cursor = 0;
        self.error = None;
        self.scroll_offset = 0;

        self.messages.push(Message {
            role: Role::User,
            content: text,
            complete: true,
        });
        // Placeholder for the streaming assistant response.
        self.messages.push(Message {
            role: Role::Assistant,
            content: String::new(),
            complete: false,
        });
        self.is_streaming = true;

        let (tx, rx): (Sender<StreamMsg>, Receiver<StreamMsg>) = mpsc::channel();
        self.token_rx = Some(rx);

        let api_messages = self.build_api_messages();
        std::thread::spawn(move || {
            let client = AiClient::new(client_cfg);
            let tx_token = tx.clone();
            let result = client.chat_stream(&api_messages, &mut |token| {
                let _ = tx_token.send(StreamMsg::Token(token.to_string()));
            });
            match result {
                Ok(_) => {
                    let _ = tx.send(StreamMsg::Done);
                }
                Err(e) => {
                    let _ = tx.send(StreamMsg::Err(e.to_string()));
                }
            }
        });
    }

    fn build_api_messages(&self) -> Vec<ApiMessage> {
        let mut out = Vec::new();
        let sys = build_system_prompt(&self.context);
        out.push(ApiMessage::system(sys));
        for msg in &self.messages {
            match msg.role {
                Role::User => out.push(ApiMessage::user(msg.content.clone())),
                Role::Assistant if msg.complete => {
                    out.push(ApiMessage::assistant(msg.content.clone()))
                }
                _ => {}
            }
        }
        out
    }

    /// Drain any pending tokens from the background thread.
    /// Returns true if the UI needs a redraw.
    fn drain_tokens(&mut self) -> bool {
        let rx = match &self.token_rx {
            Some(r) => r,
            None => return false,
        };
        let mut changed = false;
        loop {
            match rx.try_recv() {
                Ok(StreamMsg::Token(t)) => {
                    if let Some(last) = self.messages.last_mut() {
                        last.content.push_str(&t);
                    }
                    changed = true;
                }
                Ok(StreamMsg::Done) => {
                    if let Some(last) = self.messages.last_mut() {
                        last.complete = true;
                    }
                    self.is_streaming = false;
                    self.token_rx = None;
                    changed = true;
                    break;
                }
                Ok(StreamMsg::Err(e)) => {
                    if let Some(last) = self.messages.last_mut() {
                        last.content = format!("[error: {}]", e);
                        last.complete = true;
                    }
                    self.is_streaming = false;
                    self.token_rx = None;
                    changed = true;
                    break;
                }
                Err(_) => break, // empty or disconnected
            }
        }
        changed
    }

    /// Build the flat list of lines to display in the message area.
    fn display_lines(&self) -> Vec<DisplayLine> {
        let w = self.content_width().max(4);
        let mut lines: Vec<DisplayLine> = Vec::new();
        let wrap_opts = textwrap::Options::new(w).break_words(true);

        for msg in &self.messages {
            // Role header
            lines.push(DisplayLine::Header(msg.role.clone()));

            // Wrapped content
            let content_to_wrap = if msg.content.is_empty() && !msg.complete {
                "▋".to_string() // blinking cursor placeholder
            } else {
                msg.content.clone()
            };

            for raw_line in content_to_wrap.lines() {
                for wrapped in textwrap::wrap(raw_line, &wrap_opts) {
                    lines.push(DisplayLine::Text {
                        text: wrapped.into_owned(),
                        role: msg.role.clone(),
                    });
                }
            }
            // Blank separator between messages
            lines.push(DisplayLine::Blank);
        }

        lines
    }
}

#[derive(Clone)]
enum DisplayLine {
    Header(Role),
    Text { text: String, role: Role },
    Blank,
}

// ─── Rendering ───────────────────────────────────────────────────────────────

fn render(term: &mut TermWizTerminal, app: &App) -> termwiz::Result<()> {
    let cols = app.cols;
    let rows = app.rows;
    let inner_w = cols.saturating_sub(2); // inside left and right borders

    let mut changes: Vec<Change> = Vec::with_capacity(rows * 4);

    // 1. Clear screen with dark background.
    changes.push(Change::AllAttributes(plain_attrs()));
    changes.push(Change::ClearScreen(ColorAttribute::from(AnsiColor::Black)));

    // 2. Top border.
    let title = format!(" Kaku AI  {}  ESC to exit ", app.model);
    let border_fill = inner_w.saturating_sub(title.len());
    let top_line = format!("╭─{}{}─╮", title, "─".repeat(border_fill.saturating_sub(2)));
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(0),
    });
    changes.push(Change::AllAttributes(accent()));
    changes.push(Change::Text(truncate(&top_line, cols)));

    // 3. Message area.
    let msg_area_h = app.msg_area_height();
    let all_lines = app.display_lines();
    let total = all_lines.len();

    // Determine the slice to show, accounting for scroll.
    let visible_start = if total <= msg_area_h {
        0
    } else {
        (total - msg_area_h).saturating_sub(app.scroll_offset)
    };
    let visible = &all_lines[visible_start..total.min(visible_start + msg_area_h)];

    for (i, line) in visible.iter().enumerate() {
        let row = i + 1; // row 0 is top border
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(row),
        });
        changes.push(Change::AllAttributes(border_dim()));
        changes.push(Change::Text("│".to_string()));

        let (attrs, text) = match line {
            DisplayLine::Header(Role::User) => (user_header_attrs(), "  You".to_string()),
            DisplayLine::Header(Role::Assistant) => (ai_header_attrs(), "  AI".to_string()),
            DisplayLine::Text {
                text,
                role: Role::User,
            } => (user_text_attrs(), format!("  {}", text)),
            DisplayLine::Text {
                text,
                role: Role::Assistant,
            } => (ai_text_attrs(), format!("  {}", text)),
            DisplayLine::Blank => (plain_attrs(), String::new()),
        };

        // Pad to inner_w and render
        let padded = format!("{:<width$}", text, width = inner_w.saturating_sub(1));
        changes.push(Change::AllAttributes(attrs));
        changes.push(Change::Text(truncate(&padded, inner_w.saturating_sub(1))));
        changes.push(Change::AllAttributes(border_dim()));
        changes.push(Change::Text("│".to_string()));
    }

    // Fill remaining rows in message area with empty lines.
    for i in visible.len()..msg_area_h {
        let row = i + 1;
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(row),
        });
        changes.push(Change::AllAttributes(border_dim()));
        changes.push(Change::Text("│".to_string()));
        changes.push(Change::AllAttributes(plain_attrs()));
        changes.push(Change::Text(format!(
            "{:<width$}",
            "",
            width = inner_w.saturating_sub(1)
        )));
        changes.push(Change::AllAttributes(border_dim()));
        changes.push(Change::Text("│".to_string()));
    }

    // 4. Separator row.
    let sep_row = rows.saturating_sub(3);
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(sep_row),
    });
    changes.push(Change::AllAttributes(border_dim()));
    changes.push(Change::Text(format!(
        "├{}┤",
        "─".repeat(inner_w.saturating_sub(0))
    )));

    // 5. Input row.
    let input_row = rows.saturating_sub(2);
    let prompt = if app.is_streaming { "  ⏳ " } else { "  > " };
    let input_display = format!("{}{}", prompt, app.input);
    let input_padded = format!(
        "{:<width$}",
        input_display,
        width = inner_w.saturating_sub(1)
    );
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(input_row),
    });
    changes.push(Change::AllAttributes(border_dim()));
    changes.push(Change::Text("│".to_string()));
    changes.push(Change::AllAttributes(input_attrs()));
    changes.push(Change::Text(truncate(
        &input_padded,
        inner_w.saturating_sub(1),
    )));
    changes.push(Change::AllAttributes(border_dim()));
    changes.push(Change::Text("│".to_string()));

    // 6. Bottom border.
    let bot_row = rows.saturating_sub(1);
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(bot_row),
    });
    changes.push(Change::AllAttributes(accent()));
    changes.push(Change::Text(format!(
        "╰{}╯",
        "─".repeat(inner_w.saturating_sub(0))
    )));

    // 7. Position cursor at the end of the input text.
    let cursor_col = (prompt.len() + app.input.len() + 1).min(cols.saturating_sub(2));
    changes.push(Change::CursorPosition {
        x: Position::Absolute(cursor_col),
        y: Position::Absolute(input_row),
    });
    changes.push(Change::CursorVisibility(CursorVisibility::Visible));

    term.render(&changes)
}

// ─── Input handling ──────────────────────────────────────────────────────────

enum Action {
    Continue,
    Quit,
}

fn handle_key(key: &KeyEvent, app: &mut App, client_cfg: &AssistantConfig) -> Action {
    match (&key.key, key.modifiers) {
        // Exit
        (KeyCode::Escape, _) | (KeyCode::Char('C'), Modifiers::CTRL) => Action::Quit,

        // Submit
        (KeyCode::Enter, Modifiers::NONE) if !app.is_streaming => {
            app.submit(client_cfg.clone());
            Action::Continue
        }
        (KeyCode::Enter, _) => Action::Continue,

        // Backspace
        (KeyCode::Backspace, _) => {
            if app.input_cursor > 0 {
                let byte_pos = char_to_byte_pos(&app.input, app.input_cursor - 1);
                let next_pos = char_to_byte_pos(&app.input, app.input_cursor);
                app.input.drain(byte_pos..next_pos);
                app.input_cursor -= 1;
            }
            Action::Continue
        }

        // Clear line
        (KeyCode::Char('U'), Modifiers::CTRL) => {
            app.input.clear();
            app.input_cursor = 0;
            Action::Continue
        }

        // Scroll up/down in message history
        (KeyCode::UpArrow, _) | (KeyCode::PageUp, _) => {
            app.scroll_offset = app.scroll_offset.saturating_add(3);
            Action::Continue
        }
        (KeyCode::DownArrow, _) | (KeyCode::PageDown, _) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(3);
            Action::Continue
        }

        // Left / Right cursor movement
        (KeyCode::LeftArrow, _) => {
            if app.input_cursor > 0 {
                app.input_cursor -= 1;
            }
            Action::Continue
        }
        (KeyCode::RightArrow, _) => {
            let len = app.input.chars().count();
            if app.input_cursor < len {
                app.input_cursor += 1;
            }
            Action::Continue
        }

        // Regular character input
        (KeyCode::Char(c), Modifiers::NONE) | (KeyCode::Char(c), Modifiers::SHIFT) => {
            if !app.is_streaming {
                let byte_pos = char_to_byte_pos(&app.input, app.input_cursor);
                app.input.insert(byte_pos, *c);
                app.input_cursor += 1;
            }
            Action::Continue
        }

        _ => Action::Continue,
    }
}

fn handle_mouse(event: &MouseEvent, app: &mut App) {
    // Scroll wheel support
    if event.mouse_buttons.contains(MouseButtons::VERT_WHEEL) {
        if event.mouse_buttons.contains(MouseButtons::WHEEL_POSITIVE) {
            app.scroll_offset = app.scroll_offset.saturating_add(2);
        } else {
            app.scroll_offset = app.scroll_offset.saturating_sub(2);
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn ai_chat_overlay(
    _pane_id: PaneId,
    mut term: TermWizTerminal,
    context: TerminalContext,
) -> anyhow::Result<()> {
    term.set_raw_mode()?;

    let size = term.get_screen_size()?;
    let cols = size.cols;
    let rows = size.rows;

    let client_cfg = match AssistantConfig::load() {
        Ok(c) => c,
        Err(e) => {
            // Show error briefly and exit
            term.render(&[
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(0),
                },
                Change::Text(format!("Kaku AI: {}", e)),
            ])?;
            std::thread::sleep(Duration::from_secs(3));
            return Ok(());
        }
    };

    let model = client_cfg.model.clone();
    let mut app = App::new(context, model, cols, rows);
    let mut needs_redraw = true;

    // Welcome message
    app.messages.push(Message {
        role: Role::Assistant,
        content: "Hello! I'm your Kaku AI assistant. How can I help you today?".to_string(),
        complete: true,
    });

    loop {
        // Drain any streaming tokens first.
        if app.drain_tokens() {
            needs_redraw = true;
        }

        if needs_redraw {
            render(&mut term, &app)?;
            needs_redraw = false;
        }

        // Poll with a short timeout so we can check the token channel regularly.
        let timeout = if app.is_streaming {
            Some(Duration::from_millis(30))
        } else {
            Some(Duration::from_millis(100))
        };

        match term.poll_input(timeout)? {
            Some(InputEvent::Key(key)) => {
                match handle_key(&key, &mut app, &client_cfg) {
                    Action::Quit => break,
                    Action::Continue => {}
                }
                needs_redraw = true;
            }
            Some(InputEvent::Mouse(mouse)) => {
                handle_mouse(&mouse, &mut app);
                needs_redraw = true;
            }
            Some(InputEvent::Resized { cols, rows }) => {
                app.cols = cols;
                app.rows = rows;
                needs_redraw = true;
            }
            Some(_) => {}
            None => {
                // Timeout: if streaming, trigger a redraw to show new tokens.
                if app.is_streaming {
                    needs_redraw = true;
                }
            }
        }
    }

    // Clear screen before handing control back to the terminal.
    term.render(&[
        Change::AllAttributes(CellAttributes::default()),
        Change::ClearScreen(ColorAttribute::Default),
    ])?;

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_system_prompt(ctx: &TerminalContext) -> String {
    let mut s = String::from(
        "You are Kaku AI, a helpful assistant integrated into the Kaku terminal emulator. \
         You help users with terminal tasks, code, and system operations. \
         Keep responses concise and practical.\n\n",
    );
    if !ctx.cwd.is_empty() {
        s.push_str(&format!("Current directory: {}\n", ctx.cwd));
    }
    if let Some(branch) = &ctx.git_branch {
        s.push_str(&format!("Git branch: {}\n", branch));
    }
    if !ctx.visible_lines.is_empty() {
        let snippet: String = ctx
            .visible_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if !snippet.is_empty() {
            s.push_str(&format!(
                "\nVisible terminal content:\n```\n{}\n```\n",
                snippet
            ));
        }
    }
    s
}

/// Convert a character index into a byte offset in `s`.
fn char_to_byte_pos(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn truncate(s: &str, max_cols: usize) -> String {
    let count = s.chars().count();
    if count <= max_cols {
        s.to_string()
    } else {
        s.chars().take(max_cols).collect()
    }
}

// ─── Style helpers ───────────────────────────────────────────────────────────

fn make_attrs(fg: AnsiColor, bg: AnsiColor) -> CellAttributes {
    let mut a = CellAttributes::default();
    a.set_foreground(ColorAttribute::from(fg));
    a.set_background(ColorAttribute::from(bg));
    a
}

fn make_attrs_bold(fg: AnsiColor, bg: AnsiColor) -> CellAttributes {
    let mut a = make_attrs(fg, bg);
    a.apply_change(&AttributeChange::Intensity(termwiz::cell::Intensity::Bold));
    a
}

fn accent() -> CellAttributes {
    make_attrs(AnsiColor::Aqua, AnsiColor::Black)
}

fn border_dim() -> CellAttributes {
    make_attrs(AnsiColor::Grey, AnsiColor::Black)
}

fn plain_attrs() -> CellAttributes {
    make_attrs(AnsiColor::White, AnsiColor::Black)
}

fn user_header_attrs() -> CellAttributes {
    make_attrs_bold(AnsiColor::Yellow, AnsiColor::Black)
}

fn user_text_attrs() -> CellAttributes {
    make_attrs(AnsiColor::Silver, AnsiColor::Black)
}

fn ai_header_attrs() -> CellAttributes {
    make_attrs_bold(AnsiColor::Aqua, AnsiColor::Black)
}

fn ai_text_attrs() -> CellAttributes {
    make_attrs(AnsiColor::White, AnsiColor::Black)
}

fn input_attrs() -> CellAttributes {
    make_attrs(AnsiColor::White, AnsiColor::Black)
}
