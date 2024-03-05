use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use kube::Client;
use tracing::{info, instrument, Level};
use vine_api::{user::UserSpec, user_auth::UserSessionResponse};

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Command {
    Login(LoginArgs),
    Logout(LogoutArgs),
}

impl Command {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn run(self) -> Result<()> {
        let kube = Client::try_default()
            .await
            .map_err(|error| anyhow!("failed to load kubernetes config: {error}"))?;

        let response = match self {
            Self::Login(command) => command
                .run(kube)
                .await
                .map_err(|error| anyhow!("failed to login: {error}"))?,
            Self::Logout(command) => command
                .run(kube)
                .await
                .map_err(|error| anyhow!("failed to logout: {error}"))?,
        };

        match response {
            UserSessionResponse::Accept {
                box_quota: _,
                user:
                    UserSpec {
                        name,
                        contact: _,
                        detail: _,
                    },
            } => {
                info!("Ok ({name})");
                Ok(())
            }
            UserSessionResponse::Error(error) => Err(error.into()),
        }
    }
}

#[derive(Clone, Debug, Parser)]
pub(crate) struct LoginArgs {
    #[arg(long, env = "VINE_SESSION_BOX", value_name = "NAME")]
    r#box: String,

    #[arg(long, env = "VINE_SESSION_USER", value_name = "NAME")]
    user: String,
}

impl LoginArgs {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn run(self, kube: Client) -> Result<UserSessionResponse> {
        let Self {
            r#box: box_name,
            user: user_name,
        } = &self;

        ::vine_rbac::login::execute(&kube, box_name, user_name).await
    }
}

#[derive(Clone, Debug, Parser)]
pub(crate) struct LogoutArgs {
    #[arg(long, env = "VINE_SESSION_BOX", value_name = "NAME")]
    r#box: String,

    #[arg(long, env = "VINE_SESSION_USER", value_name = "NAME")]
    user: String,
}

impl LogoutArgs {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn run(self, kube: Client) -> Result<UserSessionResponse> {
        let Self {
            r#box: box_name,
            user: user_name,
        } = &self;

        ::vine_rbac::logout::execute(&kube, box_name, user_name).await
    }
}
