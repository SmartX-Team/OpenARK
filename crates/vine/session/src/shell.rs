use std::{fmt, io::stdout, mem::swap, time::Duration};

use anyhow::{Error, Result};
use avt::Vt;
use chrono::Utc;
use futures::{channel::mpsc, stream::FuturesUnordered, SinkExt, StreamExt};
use kube::{
    api::{AttachParams, TerminalSize},
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
    time::{sleep, Instant},
};
use tracing::{error, info};

use crate::{
    batch::{collect_user_sessions, BatchCommandUsers},
    exec::{Process, SessionExec},
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

        if sessions_filtered.is_empty() {
            info!("no such sessions");
            return Ok(());
        }

        let processes = sessions_filtered
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

        let app = App::new(processes)?;
        app.try_loop_forever().await
    }
}

struct App {
    is_closed: bool,
    session_selected: usize,
    sessions: Vec<Session>,
    timer_alive: Timer,
}

struct Timer {
    instant: Instant,
    interval: Duration,
    is_triggered: bool,
}

impl Timer {
    fn new(interval: Duration) -> Self {
        Self {
            instant: Instant::now(),
            interval,
            is_triggered: false,
        }
    }

    fn trigger(&mut self) {
        self.is_triggered = true
    }

    fn tick(&mut self) -> bool {
        if self.is_triggered || self.instant.elapsed() >= self.interval {
            self.instant = Instant::now();
            self.is_triggered = false;
            true
        } else {
            false
        }
    }
}

impl App {
    fn new(processes: impl Iterator<Item = Process>) -> Result<Self> {
        Ok(Self {
            is_closed: false,
            session_selected: 0,
            sessions: processes
                .filter_map(
                    |Process {
                         mut ap,
                         name,
                         namespace,
                     }| {
                        Some(Session {
                            channel_stdin: Box::new(ap.stdin()?),
                            channel_stdout: Box::new(ap.stdout()?),
                            channel_terminal_size: ap.terminal_size()?,
                            events: Vec::default(),
                            name,
                            namespace,
                            state: SessionState::Running,
                            vt: None,
                        })
                    },
                )
                .collect(),
            timer_alive: Timer::new(Duration::from_secs(1)),
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
        // handle keyboard events
        let mut inputs = String::default();
        while event::poll(std::time::Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                // terminate on completion
                if self.is_closed
                    && key.kind == event::KeyEventKind::Press
                    && key.code == KeyCode::Char('q')
                {
                    return Ok(Some(AppState::Completed));
                }

                // record all key inputs
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

        // handle pre-timers
        if self.timer_alive.tick() && inputs.is_empty() {
            inputs.push('\x00');
        }

        // handle sessions
        self.sessions
            .iter_mut()
            .filter(|session| !session.is_closed())
            .map(|session| session.update(&inputs))
            .collect::<FuturesUnordered<_>>()
            .collect::<()>()
            .await;

        // handle post-timers
        if !inputs.is_empty() {
            self.timer_alive.trigger()
        }

        // handle channels
        if !self.is_closed && self.sessions.iter().all(|session| session.is_closed()) {
            self.is_closed = true;
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
        let [header_area, body_area, footer_area] = layout.areas(area);

        self.render_header(header_area, buf);
        self.render_footer(footer_area, buf);
        self.render_body(body_area, buf);
    }
}

impl App {
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;
        Paragraph::new("OpenARK VINE Interactive Terminal")
            .bold()
            .alignment(Alignment::Center)
            .fg(CUSTOM_LABEL_COLOR)
            .render(area, buf)
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        const CUSTOM_LABEL_COLOR: Color = tailwind::SLATE.c200;

        let text = if self.is_closed {
            "Press \"q\" to exit"
        } else {
            "Press Any key to batch commands"
        };

        Paragraph::new(text)
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

        let session_selected = self.session_selected;
        let text = match self
            .sessions
            .get(session_selected)
            .and_then(|session| session.vt.as_ref())
        {
            // Some(SessionTerminal { vt, .. }) => vt.dump(),
            Some(SessionTerminal { vt, .. }) => vt
                .view()
                .into_iter()
                .map(|line| line.text())
                .collect::<Vec<_>>()
                .join("\n"),
            None => format!("no such session: {session_selected}"),
        };

        Paragraph::new(text).render(area, buf)
    }
}

enum AppState {
    Completed,
}

struct Session {
    channel_stdin: Box<dyn AsyncWrite + Unpin>,
    channel_stdout: Box<dyn AsyncRead + Unpin>,
    channel_terminal_size: mpsc::Sender<TerminalSize>,
    events: Vec<SessionEvent>,
    name: String,
    namespace: Option<String>,
    state: SessionState,
    vt: Option<SessionTerminal>,
}

impl Session {
    const fn is_closed(&self) -> bool {
        matches!(&self.state, SessionState::Completed | SessionState::Error)
    }

    fn name(&self) -> &str {
        self.namespace
            .as_ref()
            .map(|namespace| namespace.as_str())
            .unwrap_or_else(|| self.name.as_str())
    }

    async fn update(&mut self, inputs: &str) {
        match self.try_update(inputs).await {
            Ok(()) => (),
            Err(error) => self.complete_on_error(error),
        }
    }

    async fn try_update(&mut self, inputs: &str) -> Result<()> {
        // handle incoming events
        if !self.events.is_empty() {
            let mut events = Vec::default();
            swap(&mut self.events, &mut events);

            for event in events {
                match event {
                    SessionEvent::UpdateSize { width, height } => {
                        self.channel_terminal_size
                            .send(TerminalSize { width, height })
                            .await?;
                    }
                }
            }
        }

        // handle stdout
        if let Some(SessionTerminal {
            area: _,
            buf,
            len,
            vt,
        }) = self.vt.as_mut()
        {
            loop {
                select! {
                    result = self.channel_stdout.read(&mut buf[*len..]) => match result {
                        Ok(0) => break,
                        Ok(n) => *len += n,
                    Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe =>
                    {
                        self.complete();
                        return Ok(())
                    },
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

        // handle stdin
        if !inputs.is_empty() {
            match self.channel_stdin.write_all(inputs.as_bytes()).await {
                Ok(()) => (),
                Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => {
                    self.complete();
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn complete(&mut self) {
        let name = self.name().to_string();
        let timestamp = Utc::now().to_rfc3339();

        self.state = SessionState::Completed;
        if let Some(SessionTerminal { vt, .. }) = &mut self.vt {
            vt.feed_str(&format!(
                "\n<Session {name:?} closed on Completed at {timestamp}>"
            ));
        }
    }

    fn complete_on_error(&mut self, error: Error) {
        let name = self.name().to_string();
        let timestamp = Utc::now().to_rfc3339();

        self.state = SessionState::Error;
        if let Some(SessionTerminal { vt, .. }) = &mut self.vt {
            vt.feed_str(&format!(
                "\n<Session {name:?} closed on Error at {timestamp}>"
            ));
            vt.feed_str(&error.to_string());
        }
    }
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
    fn new(area: Rect) -> Self {
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
    Error,
}
