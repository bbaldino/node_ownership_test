pub mod actor_node;
pub mod owner_arc_mutex_node;
pub mod owner_node;

use std::time::{Duration, Instant};

use actor_node::{create_actor_node_pipeline, ActorNode};
use async_trait::async_trait;
use meansd::MeanSD;
use owner_arc_mutex_node::create_owner_arc_mutex_node_pipeline;
use owner_node::create_owner_node_pipeline;
use tokio::task::JoinSet;

const TIMELINE_PRINT_INTERVAL: u32 = 1000000;

struct TimelineEvent {
    timestamp: Instant,
    event: String,
}

impl TimelineEvent {
    fn new<T: Into<String>>(timestamp: Instant, event: T) -> Self {
        Self {
            timestamp,
            event: event.into(),
        }
    }
}

pub(crate) struct PacketInfo {
    index: u32,
    timeline: Vec<TimelineEvent>,
}

impl PacketInfo {
    fn new(index: u32, initial_event: &str) -> Self {
        let timeline = vec![TimelineEvent::new(Instant::now(), initial_event)];
        Self { index, timeline }
    }

    fn add_event(&mut self, event: &str) {
        let time = Instant::now();
        self.timeline.push(TimelineEvent::new(time, event));
    }

    fn dump_timeline(&self) {
        let Some(first_event) = self.timeline.first() else {
            return;
        };
        let base_time = first_event.timestamp;
        for event in &self.timeline {
            println!("[{:?}]: {}", (event.timestamp - base_time), event.event);
        }
    }

    fn transit_time(&self) -> Duration {
        self.timeline.last().unwrap().timestamp - self.timeline.first().unwrap().timestamp
    }
}

#[async_trait]
trait Node: Send {
    async fn process_packet(&mut self, packet_info: PacketInfo);
}

#[derive(Default)]
struct JitterCalculator {
    prev_transit_time: Duration,
    current_jitter: f32,
}

impl JitterCalculator {
    fn add_value(&mut self, current_transit_time: Duration) {
        let d = current_transit_time.abs_diff(self.prev_transit_time);
        self.prev_transit_time = current_transit_time;
        self.current_jitter += (1f32 / 16f32) * (d.as_millis() as f32 - self.current_jitter);
    }

    fn current_jitter(&self) -> f32 {
        self.current_jitter
    }
}

#[derive(Default)]
struct StatTrackerNode {
    jitter_calculator: JitterCalculator,
    latency: MeanSD,
}

impl StatTrackerNode {
    fn dump_stats(&self) {
        println!(
            "jitter: {}, mean latency: {}ms, stddev latency: {}ms",
            self.jitter_calculator.current_jitter(),
            self.latency.mean(),
            self.latency.sstdev()
        );
    }
}

#[async_trait]
impl Node for StatTrackerNode {
    async fn process_packet(&mut self, packet_info: PacketInfo) {
        let transmit_time = packet_info.transit_time();
        self.jitter_calculator.add_value(transmit_time);
        self.latency.update(transmit_time.as_millis() as f64);

        if packet_info.index == TIMELINE_PRINT_INTERVAL {
            self.dump_stats();
            packet_info.dump_timeline();
        }
    }
}

async fn owner_node_test(num_pipelines: usize, num_nodes_per_pipeline: usize, num_packets: u32) {
    let mut pipelines = vec![];
    for i in 0..num_pipelines {
        let pipeline = create_owner_node_pipeline(i as u32, num_nodes_per_pipeline);
        pipelines.push(pipeline);
    }
    let mut join_set = JoinSet::new();
    let mut senders = vec![];
    for mut pipeline in pipelines {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PacketInfo>(100);
        senders.push(tx);
        join_set.spawn(async move {
            while let Some(mut packet) = rx.recv().await {
                packet.add_event("enters pipeline");
                pipeline.process_packet(packet).await;
            }
        });
    }
    join_set.spawn(async move {
        for i in 1..=num_packets {
            for sender in &mut senders {
                let _ = sender.send(PacketInfo::new(i, "sent to pipeline")).await;
            }
        }
    });
    join_set.join_all().await;
}

async fn owner_arc_mutex_node_test(
    num_pipelines: usize,
    num_nodes_per_pipeline: usize,
    num_packets: u32,
) {
    let mut pipelines = vec![];
    for i in 0..num_pipelines {
        let pipeline = create_owner_arc_mutex_node_pipeline(i as u32, num_nodes_per_pipeline);
        pipelines.push(pipeline);
    }
    let mut join_set = JoinSet::new();
    let mut senders = vec![];
    for mut pipeline in pipelines {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PacketInfo>(100);
        senders.push(tx);
        join_set.spawn(async move {
            while let Some(mut packet) = rx.recv().await {
                packet.add_event("enters pipeline");
                pipeline.lock().await.process_packet(packet).await;
            }
        });
    }
    for sender in senders {
        join_set.spawn(async move {
            for i in 1..=num_packets {
                let _ = sender.send(PacketInfo::new(i, "sent to pipeline")).await;
            }
        });
    }
    join_set.join_all().await;
}

async fn actor_node_test(num_pipelines: usize, num_nodes: usize, num_packets: u32) {
    let mut pipelines: Vec<Vec<ActorNode>> = vec![];
    for i in 0..num_pipelines {
        let pipeline = create_actor_node_pipeline(i as u32, num_nodes);
        pipelines.push(pipeline);
    }
    let mut join_set = JoinSet::new();

    // Grab the entry points for each pipeline and start the tasks for every node of every
    // pipeline
    let mut pipeline_entry_points = vec![];
    for pipeline in pipelines {
        pipeline_entry_points.push(pipeline[0].get_sender());
        for node in pipeline {
            join_set.spawn(node.run());
        }
    }

    for pipeline_entry_point in pipeline_entry_points {
        join_set.spawn(async move {
            for i in 1..=num_packets {
                let _ = pipeline_entry_point
                    .send(PacketInfo::new(i, "enters pipeline"))
                    .await;
            }
        });
    }

    join_set.join_all().await;
}

#[tokio::main]
// #[tokio::main(flavor = "current_thread")]
async fn main() {
    let num_packets = TIMELINE_PRINT_INTERVAL;
    let num_nodes = 10;
    let num_pipelines = 10;
    println!("Running {num_pipelines} pipelines with {num_nodes} nodes per pipeline and processing {num_packets} packets");
    let start = Instant::now();
    owner_node_test(num_pipelines, num_nodes, num_packets).await;
    let end = Instant::now();
    println!("Owner node took {}ms", (end - start).as_millis());
    let start = Instant::now();
    owner_arc_mutex_node_test(num_pipelines, num_nodes, num_packets).await;
    let end = Instant::now();
    println!("Owner arc mutex node took {}ms", (end - start).as_millis());
    let start = Instant::now();
    actor_node_test(num_pipelines, num_nodes, num_packets).await;
    let end = Instant::now();
    println!("Actor node took {}ms", (end - start).as_millis());
}
