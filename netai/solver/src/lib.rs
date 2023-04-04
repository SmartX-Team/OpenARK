mod io;
mod models;
mod primitive;
mod role;
pub mod session;
mod tensor;

type BoxSolver = Box<dyn Solver + Send + Sync>;

#[::ipis::async_trait::async_trait(?Send)]
trait Solver {
    async fn solve(
        &self,
        session: &crate::session::Session,
        request: crate::io::Request,
    ) -> ::ipis::core::anyhow::Result<crate::io::Response>;
}
