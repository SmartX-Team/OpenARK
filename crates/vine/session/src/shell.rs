use std::{fmt, io::stdout, mem::swap, time::Duration};

use anyhow::{Error, Result};
use avt::Vt;
use futures::{channel::mpsc, stream::FuturesUnordered, Future, SinkExt, StreamExt};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use kube::{
    api::{AttachParams, AttachedProcess, TerminalSize},
    Client,
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    layout::{Alignment, Constraint, Layout, Rect},
    style::{palette::tailwind, Color, Stylize},
    widgets::{Paragraph, Widget},
    Terminal,
};
use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    select,
    task::yield_now,
    time::sleep,
};
use tracing::error;

use crate::{
    batch::{collect_user_sessions, BatchCommandUsers},
    exec::SessionExec,
};

pub struct BatchShellArgs<C, U> {
    pub command: C,
    pub users: BatchCommandUsers<U>,
}

impl<C, U> BatchShellArgs<C, U> {
    pub async fn exec(&self, kube: &Client) -> Result<()>
    where
        C: 'static + Send + Sync + Clone + fmt::Debug + Into<String>,
        U: AsRef<str>,
    {
        let Self { command, users } = self;

        let sessions_all = collect_user_sessions(kube).await?;
        let sessions_filtered = users.filter(sessions_all)?;

        let sessions = sessions_filtered
            .into_iter()
            .map(|session| {
                let kube = kube.clone();
                let ap = AttachParams::interactive_tty();
                let command = [command.clone()];
                async move { session.exec(kube, ap, command).await }
            })
            .collect::<FuturesUnordered<_>>()
            .filter_map(|result| async move {
                match result {
                    Ok(process) => Some(process),
                    Err(error) => {
                        error!("{error}");
                        None
                    }
                }
            })
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flatten();

        let app = App::new(sessions)?;
        app.try_loop_forever().await
    }
}

struct App {
    sessions: Vec<Session>,
}

impl App {
    fn new(processes: impl Iterator<Item = AttachedProcess>) -> Result<Self> {
        Ok(Self {
            sessions: processes
                .filter_map(|mut process| {
                    Some(Session {
                        channel_status: Box::new(process.take_status()?),
                        channel_stdin: Box::new(process.stdin()?),
                        channel_stdout: Box::new(process.stdout()?),
                        channel_terminal_size: process.terminal_size()?,
                        events: Vec::default(),
                        process,
                        state: SessionState::Running,
                        vt: None,
                    })
                })
                .collect(),
        })
    }

    async fn try_loop_forever(mut self) -> Result<()> {
        self.enter()?;

        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

        let state = loop {
            terminal.draw(|f| f.render_widget(&mut self, f.size()))?;

            match self.handle_events().await {
                Ok(None) => yield_now().await,
                Ok(Some(value)) => break Ok(value),
                Err(error) => break Err(error),
            }
        };

        self.exit()?;
        state.map(|AppState::Completed| ())
    }

    async fn handle_events(&mut self) -> Result<Option<AppState>> {
        let mut inputs = String::default();
        while event::poll(std::time::Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('~') {
                    return Ok(Some(AppState::Completed));
                }

                if matches!(key.kind, event::KeyEventKind::Press) {
                    match key.code {
                        KeyCode::Backspace => inputs.push('\r'),
                        KeyCode::Enter => inputs.push('\n'),
                        // KeyCode::Left => inputs.push_str("\xE0\x72"),
                        // KeyCode::Right => inputs.push('\x27'),
                        // KeyCode::Up => inputs.push('\x26'),
                        // KeyCode::Down => inputs.push('\x28'),
                        // KeyCode::Home => todo!(),
                        // KeyCode::End => todo!(),
                        // KeyCode::PageUp => todo!(),
                        // KeyCode::PageDown => todo!(),
                        // KeyCode::Tab => todo!(),
                        // KeyCode::BackTab => todo!(),
                        // KeyCode::Delete => todo!(),
                        // KeyCode::Insert => todo!(),
                        // KeyCode::F(_) => todo!(),
                        KeyCode::Char(ch) => inputs.push(ch),
                        // KeyCode::Null => todo!(),
                        // KeyCode::Esc => todo!(),
                        // KeyCode::CapsLock => todo!(),
                        // KeyCode::ScrollLock => todo!(),
                        // KeyCode::NumLock => todo!(),
                        // KeyCode::PrintScreen => todo!(),
                        // KeyCode::Pause => todo!(),
                        // KeyCode::Menu => todo!(),
                        // KeyCode::KeypadBegin => todo!(),
                        // KeyCode::Media(_) => todo!(),
                        // KeyCode::Modifier(_) => todo!(),
                        _ => (),
                    }
                }
            }
        }

        for session in &mut self.sessions {
            if !session.events.is_empty() {
                let mut events = Vec::default();
                swap(&mut session.events, &mut events);

                for event in events {
                    match event {
                        SessionEvent::UpdateSize { width, height } => {
                            session
                                .channel_terminal_size
                                .send(TerminalSize { width, height })
                                .await?;
                        }
                    }
                }
            }

            if let Some(SessionTerminal {
                area: _,
                buf,
                len,
                vt,
            }) = session.vt.as_mut()
            {
                loop {
                    select! {
                        result = session.channel_stdout.read(&mut buf[*len..]) => match result {
                            Ok(0) => break,
                            Ok(n) => *len += n,
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                            Err(e) => return Err(e.into()),
                        },
                        () = sleep(Duration::from_millis(10)) => break,
                    }
                }
                if *len > 0 {
                    let buf = &buf[..*len];
                    if let Some(text) = ::std::str::from_utf8(buf).ok() {
                        vt.feed_str(text);
                        *len = 0;
                    }
                }
            }

            if !inputs.is_empty() {
                session.channel_stdin.write_all(inputs.as_bytes()).await?;
            }
        }
        Ok(None)
    }

    fn enter(&self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        Ok(())
    }

    fn exit(&self) -> Result<()> {
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;
        Ok(())
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ]);
        let [header_area, gauge_area, footer_area] = layout.areas(area);

        let layout = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]);
        let [body_area, input_area] = layout.areas(gauge_area);

        self.render_header(header_area, buf);
        self.render_footer(footer_area, buf);

        self.render_body(body_area, buf);
        self.render_input(input_area, buf);
    }
}

impl App {
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;
        Paragraph::new("Ratatui Gauge Example")
            .bold()
            .alignment(Alignment::Center)
            .fg(CUSTOM_LABEL_COLOR)
            .render(area, buf)
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;
        Paragraph::new("Press ENTER to start")
            .alignment(Alignment::Center)
            .fg(CUSTOM_LABEL_COLOR)
            .bold()
            .render(area, buf)
    }

    fn render_body(&mut self, area: Rect, buf: &mut Buffer) {
        for session in &mut self.sessions {
            match session.vt.as_mut() {
                Some(vt) => {
                    if area.width != vt.area.width || area.height != vt.area.height {
                        session.events.push(SessionEvent::UpdateSize {
                            width: area.width,
                            height: area.height,
                        });
                        vt.area = area;
                        vt.vt.feed_str(&format!(
                            "\x1b[8;{rows};{cols};t",
                            rows = vt.area.height,
                            cols = vt.area.width,
                        ));
                    }
                }
                None => {
                    session.events.push(SessionEvent::UpdateSize {
                        width: area.width,
                        height: area.height,
                    });
                    session.vt = Some(SessionTerminal::new(area));
                }
            }
        }

        let mut selected_session = 0;
        let text = match self
            .sessions
            .get(selected_session)
            .and_then(|session| session.vt.as_ref())
        {
            // Some(SessionTerminal { vt, .. }) => vt.dump(),
            Some(SessionTerminal { vt, .. }) => vt
                .view()
                .into_iter()
                .map(|line| line.text())
                .collect::<Vec<_>>()
                .join("\n"),
            None => format!("no such session: {selected_session}"),
        };

        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;
        Paragraph::new(text).render(area, buf)
    }

    fn render_input(&self, area: Rect, buf: &mut Buffer) {
        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;
        Paragraph::new("Press ENTER to start")
            .alignment(Alignment::Center)
            .fg(CUSTOM_LABEL_COLOR)
            .bold()
            .render(area, buf)
    }
}

enum AppState {
    Completed,
}

struct Session {
    channel_status: Box<dyn Future<Output = Option<Status>>>,
    channel_stdin: Box<dyn AsyncWrite + Unpin>,
    channel_stdout: Box<dyn AsyncRead + Unpin>,
    channel_terminal_size: mpsc::Sender<TerminalSize>,
    events: Vec<SessionEvent>,
    process: AttachedProcess,
    state: SessionState,
    vt: Option<SessionTerminal>,
}

#[derive(Debug)]
enum SessionEvent {
    UpdateSize { width: u16, height: u16 },
}

struct SessionTerminal {
    area: Rect,
    buf: Vec<u8>,
    len: usize,
    vt: Vt,
}

impl SessionTerminal {
    pub fn new(area: Rect) -> Self {
        let cols = area.width as usize;
        let rows = area.height as usize;

        Self {
            area,
            buf: vec![0; cols * rows],
            len: 0,
            vt: Vt::builder().size(cols, rows).resizable(true).build(),
        }
    }
}

enum SessionState {
    Running,
    Completed,
    Error(Error),
}
