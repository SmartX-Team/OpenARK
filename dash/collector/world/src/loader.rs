use anyhow::Result;
use dash_api::storage::ModelStorageCrd;
use itertools::Itertools;
use kube::{api::ListParams, Api, ResourceExt};
use tracing::{info, instrument, Level};

use crate::ctx::WorldContext;

pub struct StorageLoader<'a> {
    ctx: &'a WorldContext,
}

impl<'a> StorageLoader<'a> {
    pub fn new(ctx: &'a WorldContext) -> Self {
        Self { ctx }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn load(&self) -> Result<()> {
        info!("loading storage info");
        let kube = &*self.ctx.kube;
        let api = Api::<ModelStorageCrd>::all(kube.clone());
        let lp = ListParams::default();
        let crds = api.list(&lp).await?.items;

        let mut plans = Vec::with_capacity(crds.len());
        {
            let mut storage = self.ctx.data.write().await;
            for crd in crds
                .into_iter()
                .sorted_by_key(|crd| crd.creation_timestamp())
            {
                if let Some(plan) = storage.add_storage(crd).await? {
                    plans.push(plan);
                }
            }
        }

        for plan in plans {
            self.ctx.add_plan(plan).await?;
        }
        Ok(())
    }
}
