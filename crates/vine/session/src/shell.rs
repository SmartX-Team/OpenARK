use std::{fmt, io::stdout};

use anyhow::Result;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::{api::AttachedProcess, Client};
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    widgets::{Block, Paragraph},
    Frame, Terminal,
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

        let mut sessions: Vec<_> = sessions_filtered
            .into_iter()
            .map(|session| {
                let kube = kube.clone();
                let command = [command.clone()];
                async move { session.exec(kube, command).await }
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
            .flatten()
            .collect();

        {
            enable_raw_mode()?;
            stdout().execute(EnterAlternateScreen)?;

            let result = exec_in_terminal(&mut sessions).await;

            disable_raw_mode()?;
            stdout().execute(LeaveAlternateScreen)?;

            result
        }
    }
}

async fn exec_in_terminal(sessions: &mut [AttachedProcess]) -> Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut should_quit = false;
    while !should_quit {
        terminal.draw(ui)?;
        should_quit = handle_events()?;
    }

    fn handle_events() -> std::io::Result<bool> {
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn ui(frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new("Hello World!").block(Block::bordered().title("Greeting")),
            frame.size(),
        );
    }

    Ok(())

    // sessions_exec
    //     .collect::<FuturesUnordered<_>>()
    //     .then(|result| async move {
    //         match result
    //             .map_err(Error::from)
    //             .and_then(|result| result.map_err(Error::from))
    //         {
    //             Ok(processes) => {
    //                 if *wait {
    //                     processes
    //                         .into_iter()
    //                         .map(|process| async move {
    //                             match process.join().await {
    //                                 Ok(()) => (),
    //                                 Err(error) => {
    //                                     error!("failed to execute: {error}");
    //                                 }
    //                             }
    //                         })
    //                         .collect::<FuturesUnordered<_>>()
    //                         .collect::<()>()
    //                         .await;
    //                 }
    //             }
    //             Err(error) => {
    //                 warn!("failed to command: {error}");
    //             }
    //         }
    //     })
    //     .collect::<()>()
    //     .await;
}
