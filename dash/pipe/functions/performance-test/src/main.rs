use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byte_unit::{Byte, UnitType};
use clap::{ArgAction, Parser};
use dash_pipe_provider::{
    storage::{StorageIO, StorageSet},
    FunctionContext, MessengerType, PipeArgs, PipeMessage, PipeMessages, PipePayload,
};
use derivative::Derivative;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::spawn;
use tracing::info;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_PERFORMANCE_TEST_DATA_SIZE", value_name = "SIZE")]
    data_size: Byte,

    #[arg(long, env = "PIPE_PERFORMANCE_TEST_PAYLOAD_SIZE", value_name = "SIZE")]
    payload_size: Option<Byte>,

    #[arg(long, env = "PIPE_PERFORMANCE_TEST_QUIET", action = ArgAction::SetTrue)]
    quiet: bool,

    #[arg(long, env = "PIPE_PERFORMANCE_SAVE_METRICS", action = ArgAction::SetTrue)]
    save_metrics: bool,

    #[arg(
        long,
        env = "PIPE_PERFORMANCE_TEST_TOTAL_MESSAGES",
        value_name = "COUNT",
        default_value = "100K"
    )]
    total_messages: Byte,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    ctx: FunctionContext,
    metric: MetricData,
    next_stdout: Duration,
    save_metrics: bool,
    #[derivative(Debug = "ignore")]
    storage: Arc<StorageSet>,
    timestamp: Option<Instant>,
    verbose: bool,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        FunctionArgs {
            data_size,
            payload_size,
            quiet,
            save_metrics,
            total_messages,
        }: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        let metric = MetricData {
            data_size: data_size
                .as_u128()
                .try_into()
                .map_err(|error| anyhow!("too large data size: {error}"))?,
            messenger_type: ctx.messenger_type(),
            num_sent: 0,
            num_sent_bytes: 0,
            num_sent_payload_bytes: 0,
            payload_size: payload_size
                .map(|size| {
                    size.as_u128()
                        .try_into()
                        .map_err(|error| anyhow!("too large data size: {error}"))
                })
                .transpose()?
                .filter(|&size| size > 0),
            sum_latency: Default::default(),
            total: total_messages.as_u64(),
            total_sent: 0,
            total_sent_bytes: 0,
            total_sent_payload_bytes: 0,
        };

        let verbose = !*quiet;
        if verbose {
            metric.describe();
        }

        Ok(Self {
            ctx: ctx.clone(),
            metric,
            next_stdout: Self::TICK,
            save_metrics: *save_metrics,
            storage: storage.output.clone(),
            timestamp: None,
            verbose,
        })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = Metric;
    type Output = Metric;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        let elapsed = self.elapsed();

        let outputs = match inputs {
            PipeMessages::None => {
                let message = self.create_packet();
                self.metric.update_bytes_counter_one(&message);
                PipeMessages::Single(message)
            }
            PipeMessages::Single(message) => {
                self.metric.update_bytes_counter_one(&message);
                PipeMessages::Single(message)
            }
            PipeMessages::Batch(messages) => {
                self.metric.update_bytes_counter_batch(&messages);
                PipeMessages::Batch(messages)
            }
        };

        if self.metric.is_finished() {
            self.flush_metric();
            self.metric.show_avg(elapsed);
            return self.ctx.terminate_ok::<()>().map(|_| outputs);
        }

        while elapsed >= self.next_stdout {
            self.next_stdout += Self::TICK;
            self.flush_metric();
        }

        Ok(outputs)
    }
}

impl Function {
    const TICK: Duration = Duration::from_secs(1);

    fn elapsed(&mut self) -> Duration {
        if self.timestamp.is_none() {
            self.timestamp = Some(Instant::now());
        }
        self.timestamp.unwrap().elapsed()
    }

    fn create_packet(&self) -> PipeMessage<<Self as ::dash_pipe_provider::Function>::Output> {
        PipeMessage::with_payloads(self.create_payload(), self.create_value())
    }

    fn create_payload(&self) -> Vec<PipePayload> {
        match self.metric.payload_size {
            Some(size) => vec![PipePayload::new(
                format!("{index}", index = self.metric.total_sent),
                Some(create_data(size).into()),
            )],
            None => Default::default(),
        }
    }

    fn create_value(&self) -> <Self as ::dash_pipe_provider::Function>::Output {
        Metric {
            data: Default::default(),
            value: create_data(self.metric.data_size),
        }
    }

    fn flush_metric(&mut self) {
        if self.save_metrics {
            self.metric.flush(self.storage.clone(), self.verbose)
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Metric {
    data: MetricData,
    value: Vec<u8>,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MetricData {
    data_size: usize,
    messenger_type: MessengerType,
    num_sent: u64,
    num_sent_bytes: u64,
    num_sent_payload_bytes: u64,
    payload_size: Option<usize>,
    sum_latency: Duration,
    total: u64,
    total_sent: u64,
    total_sent_bytes: u64,
    total_sent_payload_bytes: u64,
}

impl MetricData {
    fn describe(&self) {
        fn describe_option<T>(value: Option<&T>) -> String
        where
            T: ToString,
        {
            value
                .map(ToString::to_string)
                .unwrap_or_else(|| "undefined".into())
        }

        let Self {
            data_size,
            messenger_type,
            num_sent: _,
            num_sent_bytes: _,
            num_sent_payload_bytes: _,
            payload_size,
            sum_latency,
            total: total_messages,
            total_sent: _,
            total_sent_bytes: _,
            total_sent_payload_bytes: _,
        } = self;

        info!("data_size: {data_size}");
        info!("messenger_type: {messenger_type}");
        info!(
            "payload_size: {payload_size}",
            payload_size = describe_option(payload_size.as_ref()),
        );
        info!("sum_latency: {sum_latency:?}");
        info!("total_messages: {total_messages:?}");
    }

    fn is_finished(&self) -> bool {
        self.total_sent >= self.total
    }

    fn update_bytes_counter(
        &mut self,
        message: &PipeMessage<<Function as ::dash_pipe_provider::Function>::Input>,
    ) {
        let bytes = message.value.value.len() as u64;
        self.num_sent_bytes += bytes;
        self.total_sent_bytes += bytes;

        let payload_bytes = message
            .payloads
            .iter()
            .filter_map(|payload| payload.value().map(|value| value.len() as u64))
            .sum::<u64>();
        self.num_sent_payload_bytes += payload_bytes;
        self.total_sent_payload_bytes += payload_bytes;
    }

    fn update_bytes_counter_one(
        &mut self,
        message: &PipeMessage<<Function as ::dash_pipe_provider::Function>::Input>,
    ) {
        let sent = 1;
        self.num_sent += sent;
        self.total_sent += sent;

        self.update_bytes_counter(message)
    }

    fn update_bytes_counter_batch(
        &mut self,
        messages: &[PipeMessage<<Function as ::dash_pipe_provider::Function>::Input>],
    ) {
        let sent = messages.len() as u64;
        self.num_sent += sent;
        self.total_sent += sent;

        messages
            .iter()
            .for_each(|message| self.update_bytes_counter(message))
    }

    fn flush(&mut self, storage: Arc<StorageSet>, verbose: bool) {
        {
            let metric = PipeMessage::with_payloads(
                Default::default(),
                Metric {
                    data: *self,
                    value: Default::default(),
                },
            );
            spawn(async move {
                storage
                    .get_default_metadata()
                    .put_metadata(&[&metric])
                    .await
            });
        }

        let num_sent = self.num_sent;
        self.num_sent = 0;

        let num_sent_bytes = self.num_sent_bytes;
        self.num_sent_bytes = 0;

        let num_sent_payload_bytes = self.num_sent_payload_bytes;
        self.num_sent_payload_bytes = 0;

        if verbose {
            let speed = get_speed_as_bps(num_sent_bytes);
            let speed_payloads = get_speed_as_bps(num_sent_payload_bytes);

            info!("Messages: {num_sent} msgs/sec ~ {speed} / Payloads: {speed_payloads}");
        }
    }

    fn show_avg(&mut self, elapsed: Duration) {
        let elapsed = elapsed.as_millis().max(1) as u64;
        let num_sent = self.total_sent * 1_000 / elapsed;
        let num_sent_bytes = self.total_sent_bytes * 1_000 / elapsed;
        let num_sent_payload_bytes = self.total_sent_payload_bytes * 1_000 / elapsed;

        let speed = get_speed_as_bps(num_sent_bytes);
        let speed_payloads = get_speed_as_bps(num_sent_payload_bytes);

        info!("Avg Messages: {num_sent} msgs/sec ~ {speed} / Avg Payloads: {speed_payloads}");
    }
}

fn create_data(size: usize) -> Vec<u8> {
    vec![98u8; size]
}

fn get_speed_as_bps(speed: u64) -> String {
    let mut speed = Byte::from_u64(8 * speed)
        .get_appropriate_unit(UnitType::Decimal)
        .to_string();
    if speed.ends_with('B') {
        speed.pop();
        speed.push('b');
    }
    speed.push_str("ps");
    speed
}
