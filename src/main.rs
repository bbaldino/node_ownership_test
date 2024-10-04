pub mod actor_node;
pub mod owner_node;

use std::{fmt::Display, time::Instant};

use actor_node::{create_actor_node_pipeline, ActorNode};
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
    initial_time: Instant,
    timeline: Vec<TimelineEvent>,
}

impl PacketInfo {
    fn new(index: u32, initial_event: &str) -> Self {
        let initial_time = Instant::now();
        let timeline = vec![TimelineEvent::new(initial_time, initial_event)];
        Self {
            index,
            initial_time,
            timeline,
        }
    }

    fn add_event(&mut self, event: &str) {
        let time = Instant::now();
        self.timeline
            .push(TimelineEvent::new(time, format!("[{:?}]: {event}", time)));
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
}

trait Node: Send {
    fn process_packet(&mut self, packet_info: PacketInfo);
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
                pipeline.process_packet(packet);
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
    let start = Instant::now();
    println!("Running {num_pipelines} pipelines with {num_nodes} nodes per pipeline and processing {num_packets} packets");
    owner_node_test(num_pipelines, num_nodes, num_packets).await;
    let end = Instant::now();
    println!("Owner node took {}ms", (end - start).as_millis());
    let start = Instant::now();
    actor_node_test(num_pipelines, num_nodes, num_packets).await;
    let end = Instant::now();
    println!("Actor node took {}ms", (end - start).as_millis());
}
