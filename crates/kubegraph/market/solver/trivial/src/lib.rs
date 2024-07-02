use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use kubegraph_api::market::{
    price::{Direction, PriceHistogram, PriceItem},
    product::ProductSpec,
    transaction::TransactionTemplate,
    BaseModel,
};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Default)]
pub struct MarketSolver {}

#[async_trait]
impl ::kubegraph_market_solver_api::MarketSolver for MarketSolver {
    #[instrument(level = Level::INFO, skip(self, _product, histogram))]
    async fn solve(
        &self,
        _product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TransactionTemplate>> {
        let mut pubs = histogram
            .iter()
            .filter(|item| matches!(item.direction, Direction::Pub))
            .filter(|item| item.cost >= 0 && item.count > 0)
            .copied()
            .sorted_by_key(|item| (item.cost, item.timestamp));
        let mut subs = histogram
            .iter()
            .filter(|item| matches!(item.direction, Direction::Sub))
            .filter(|item| item.cost >= 0 && item.count > 0)
            .copied()
            .sorted_by_key(|item| (-item.cost, item.timestamp));

        let mut r#pub = pubs.next();
        let mut sub = subs.next();
        let mut transactions = Vec::default();
        while let Some((
            PriceItem {
                id: pub_id,
                cost: pub_cost,
                count: pub_count,
                ..
            },
            PriceItem {
                id: sub_id,
                cost: sub_cost,
                count: sub_count,
                ..
            },
        )) = r#pub.as_mut().zip(sub.as_mut())
        {
            if pub_cost > sub_cost {
                break;
            }

            let pub_id = *pub_id;
            let sub_id = *sub_id;

            fn withdraw(
                mut queue: impl Iterator<Item = PriceItem>,
                item: &mut Option<PriceItem>,
                count: <ProductSpec as BaseModel>::Count,
            ) {
                if let Some(PriceItem {
                    count: item_price, ..
                }) = item.as_mut()
                {
                    if *item_price == count {
                        *item = queue.next();
                    } else {
                        *item_price -= count;
                    }
                }
            }

            let cost = *sub_cost;
            let count = (*pub_count).min(*sub_count).max(1);
            withdraw(&mut pubs, &mut r#pub, count);
            withdraw(&mut subs, &mut sub, count);

            let template = TransactionTemplate {
                r#pub: pub_id,
                sub: sub_id,
                cost,
                count,
            };
            transactions.push(template);
        }
        Ok(transactions)
    }
}
