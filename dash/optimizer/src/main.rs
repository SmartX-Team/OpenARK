mod model;
// mod router;
mod storage;

use std::process::exit;

use anyhow::Result;
use ark_core::tracer;
use dash_collector_world::{ctx::WorldContext, loader::StorageLoader, service::Service};
use opentelemetry::global;
use tokio::try_join;
use tracing::{error, info, instrument, Level};

#[instrument(level = Level::INFO, skip_all, err(Display))]
async fn try_main() -> Result<()> {
    // init world context
    let (ctx, plan_rx) = WorldContext::try_new("optimizer-metric".into())
        .await
        .unwrap();

    // load optimizer data
    let loader = StorageLoader::new(&ctx);
    loader.load().await.unwrap();

    // spawn services
    try_join!(
        ::dash_collector_converter::trace::Reader::new(ctx.clone()).loop_forever(),
        ::dash_collector_world::plan::PlanRunner::new(ctx.clone(), plan_rx).loop_forever(),
        ::dash_collector_world::syncer::MetricSyncer::new(ctx.clone()).loop_forever(),
        self::model::Service::new(ctx.clone()).loop_forever(),
        self::storage::Service::new(ctx).loop_forever(),
    )?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracer::init_once();

    match try_main().await {
        Ok(()) => {
            info!("Terminated.");
            global::shutdown_tracer_provider();
        }
        Err(error) => {
            error!("{error}");
            global::shutdown_tracer_provider();
            exit(1)
        }
    }

    // use ndarray::Array2;

    // let mut router = self::router::Router::default();
    // router
    //     .add_dimension(
    //         "Distance".into(),
    //         Array2::from_shape_vec(
    //             (17, 17),
    //             [
    //                 [
    //                     0, 548, 776, 696, 582, 274, 502, 194, 308, 194, 536, 502, 388, 354, 468,
    //                     776, 662,
    //                 ],
    //                 [
    //                     548, 0, 684, 308, 194, 502, 730, 354, 696, 742, 1084, 594, 480, 674, 1016,
    //                     868, 1210,
    //                 ],
    //                 [
    //                     776, 684, 0, 992, 878, 502, 274, 810, 468, 742, 400, 1278, 1164, 1130, 788,
    //                     1552, 754,
    //                 ],
    //                 [
    //                     696, 308, 992, 0, 114, 650, 878, 502, 844, 890, 1232, 514, 628, 822, 1164,
    //                     560, 1358,
    //                 ],
    //                 [
    //                     582, 194, 878, 114, 0, 536, 764, 388, 730, 776, 1118, 400, 514, 708, 1050,
    //                     674, 1244,
    //                 ],
    //                 [
    //                     274, 502, 502, 650, 536, 0, 228, 308, 194, 240, 582, 776, 662, 628, 514,
    //                     1050, 708,
    //                 ],
    //                 [
    //                     502, 730, 274, 878, 764, 228, 0, 536, 194, 468, 354, 1004, 890, 856, 514,
    //                     1278, 480,
    //                 ],
    //                 [
    //                     194, 354, 810, 502, 388, 308, 536, 0, 342, 388, 730, 468, 354, 320, 662,
    //                     742, 856,
    //                 ],
    //                 [
    //                     308, 696, 468, 844, 730, 194, 194, 342, 0, 274, 388, 810, 696, 662, 320,
    //                     1084, 514,
    //                 ],
    //                 [
    //                     194, 742, 742, 890, 776, 240, 468, 388, 274, 0, 342, 536, 422, 388, 274,
    //                     810, 468,
    //                 ],
    //                 [
    //                     536, 1084, 400, 1232, 1118, 582, 354, 730, 388, 342, 0, 878, 764, 730, 388,
    //                     1152, 354,
    //                 ],
    //                 [
    //                     502, 594, 1278, 514, 400, 776, 1004, 468, 810, 536, 878, 0, 114, 308, 650,
    //                     274, 844,
    //                 ],
    //                 [
    //                     388, 480, 1164, 628, 514, 662, 890, 354, 696, 422, 764, 114, 0, 194, 536,
    //                     388, 730,
    //                 ],
    //                 [
    //                     354, 674, 1130, 822, 708, 628, 856, 320, 662, 388, 730, 308, 194, 0, 342,
    //                     422, 536,
    //                 ],
    //                 [
    //                     468, 1016, 788, 1164, 1050, 514, 514, 662, 320, 274, 388, 650, 536, 342, 0,
    //                     764, 194,
    //                 ],
    //                 [
    //                     776, 868, 1552, 560, 674, 1050, 1278, 742, 1084, 810, 1152, 274, 388, 422,
    //                     764, 0, 798,
    //                 ],
    //                 [
    //                     662, 1210, 754, 1358, 1244, 708, 480, 856, 514, 468, 354, 844, 730, 536,
    //                     194, 798, 0,
    //                 ],
    //             ]
    //             .into_iter()
    //             .flatten()
    //             .collect::<Vec<_>>(),
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    // router.get_fastest_route(1, 6).unwrap();
}
