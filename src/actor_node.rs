use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{PacketInfo, StatTrackerNode};

#[async_trait]
trait Node: Send {
    async fn process_packet(&mut self, packet_info: PacketInfo);
}

#[async_trait]
impl Node for tokio::sync::mpsc::Sender<PacketInfo> {
    async fn process_packet(&mut self, packet_info: PacketInfo) {
        let _ = self.send(packet_info).await;
    }
}

#[async_trait]
impl Node for StatTrackerNode {
    async fn process_packet(&mut self, packet_info: PacketInfo) {
        let transmit_time = packet_info.transit_time();
        self.jitter_calculator.add_value(transmit_time);
        self.latency.update(transmit_time.as_micros() as f64);
    }
}

struct StatTrackerWrapper {
    inner: StatTrackerNode,
    receiver: tokio::sync::mpsc::Receiver<PacketInfo>,
}

impl StatTrackerWrapper {
    async fn run(mut self) {
        while let Some(p) = self.receiver.recv().await {
            // if p.index % 10000 == 0 {
            //     p.dump_timeline();
            // }
            self.inner.process_packet(p).await;
        }
        self.inner.dump_stats();
    }
}

struct ActorNode {
    name: String,
    // Hack: make this an option so we can hand it out to whoever needs it but drop it in 'run' q   // there won't be a dangling reference to the channel sender and we can detect when to shut
    // down correctly
    incoming_tx: Option<mpsc::Sender<PacketInfo>>,
    incoming_rx: mpsc::Receiver<PacketInfo>,
    next: Option<Box<dyn Node>>,
}

impl ActorNode {
    fn new(name: &str) -> Self {
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::channel(1000);
        Self {
            name: name.to_owned(),
            incoming_tx: Some(incoming_tx),
            incoming_rx,
            next: None,
        }
    }

    fn get_sender(&self) -> mpsc::Sender<PacketInfo> {
        self.incoming_tx.as_ref().unwrap().clone()
    }

    async fn run(mut self) {
        self.incoming_tx.take();
        while let Some(mut packet) = self.incoming_rx.recv().await {
            // if true
            // /*packet.index % 1000 == 0 */
            // {
            //     println!("actor {} processing packet {}", self.name, packet.index);
            // }
            packet.add_event(&format!("received by {}", self.name));
            if let Some(ref mut next) = self.next {
                packet.add_event(&format!("sent by {}", self.name));
                next.process_packet(packet).await;
            }
        }
    }
}

fn create_actor_node_pipeline(
    pipeline_id: u32,
    num_nodes: usize,
    endpoint: Box<dyn Node>,
) -> Box<dyn Node> {
    let mut nodes = vec![];
    let first_node = ActorNode::new(&format!("pipeline_{pipeline_id}_node_0"));
    nodes.push(first_node);
    for i in 1..num_nodes {
        let node = ActorNode::new(&format!("pipeline_{pipeline_id}_node_{i}"));
        nodes[i - 1].next = Some(Box::new(node.get_sender()));
        nodes.push(node);
    }
    nodes[num_nodes - 1].next = Some(endpoint);
    let pipeline_start = Box::new(nodes[0].get_sender());
    for node in nodes {
        tokio::spawn(node.run());
    }
    pipeline_start
}

fn create_actor_node_pipelines<T: Node + Clone + 'static>(
    num_pipelines: usize,
    num_nodes: usize,
    endpoint: T,
) -> Vec<Box<dyn Node>> {
    let mut pipelines = vec![];
    for i in 0..num_pipelines {
        let pipeline = create_actor_node_pipeline(i as u32, num_nodes, Box::new(endpoint.clone()));
        pipelines.push(pipeline);
    }

    pipelines
}

pub(crate) async fn run_test(
    num_pipelines: usize,
    num_nodes: usize,
    num_packets: usize,
) -> Duration {
    let (tx, rx) = tokio::sync::mpsc::channel(1000);
    let stat_tracker = StatTrackerWrapper {
        inner: StatTrackerNode::default(),
        receiver: rx,
    };
    let stats = tokio::spawn(stat_tracker.run());
    let mut pipelines = create_actor_node_pipelines(num_pipelines, num_nodes, tx);
    let start = Instant::now();
    for i in 1..=num_packets {
        for pipeline in &mut pipelines {
            let packet = PacketInfo::new(i as u32, "packet received", i == num_packets);
            pipeline.process_packet(packet).await;
        }
    }
    pipelines.clear();
    let _ = stats.await;
    Instant::now() - start
}
