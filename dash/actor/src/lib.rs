use dash_api::function::FunctionActorSpec;
use ipis::core::anyhow::Result;
use tera::Tera;

pub struct FunctionActor {
    spec: FunctionActorSpec,
    tera: Tera,
}

impl FunctionActor {
    pub async fn try_new(spec: FunctionActorSpec) -> Result<Self> {
        todo!()
        // Ok(Self { spec })
    }
}
