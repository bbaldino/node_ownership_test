use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{PacketInfo, StatTrackerNode};

trait Node {
    fn process_packet(&mut self, packet_info: PacketInfo);
}

impl Node for StatTrackerNode {
    fn process_packet(&mut self, packet_info: PacketInfo) {
        let transmit_time = packet_info.transit_time();
        self.jitter_calculator.add_value(transmit_time);
        self.latency.update(transmit_time.as_micros() as f64);
    }
}

struct StatTrackerWrapper {
    inner: StatTrackerNode,
    receiver: std::sync::mpsc::Receiver<PacketInfo>,
}

impl StatTrackerWrapper {
    fn run(mut self) {
        while let Ok(p) = self.receiver.recv() {
            self.inner.process_packet(p);
        }
        self.inner.dump_stats();
    }
}

impl Node for std::sync::mpsc::Sender<PacketInfo> {
    fn process_packet(&mut self, packet_info: PacketInfo) {
        let _ = self.send(packet_info);
    }
}

struct OwnerArcMutexNode {
    name: String,
    next: Option<Arc<Mutex<Box<dyn Node>>>>,
}

impl OwnerArcMutexNode {
    fn new(name: &str, next: Option<Arc<Mutex<Box<dyn Node>>>>) -> Self {
        Self {
            name: name.to_owned(),
            next,
        }
    }
}

impl Node for OwnerArcMutexNode {
    fn process_packet(&mut self, mut packet_info: PacketInfo) {
        // if true /* packet_info.index % 100 == 0 */ {
        //     println!(
        //         "owner {} processing packet {}",
        //         self.name, packet_info.index
        //     );
        // }
        packet_info.add_event(&format!("received by {}", self.name));
        if let Some(ref mut next) = self.next {
            packet_info.add_event(&format!("sent by {}", self.name));
            next.lock().unwrap().process_packet(packet_info);
        }
    }
}

fn create_owner_arc_mutex_node_pipeline(
    pipeline_id: u32,
    num_nodes: usize,
    endpoint: Box<dyn Node>,
) -> Arc<Mutex<Box<dyn Node>>> {
    let mut prev_node: Option<Arc<Mutex<Box<dyn Node>>>> = Some(Arc::new(Mutex::new(endpoint)));
    for i in (0..num_nodes).rev() {
        let node = Arc::new(Mutex::new(Box::new(OwnerArcMutexNode::new(
            &format!("pipeline_{pipeline_id}_node_{i}"),
            prev_node.take(),
        )) as Box<dyn Node>));
        prev_node = Some(node);
    }

    prev_node.unwrap()
}

fn create_owner_arc_mutex_node_pipelines(
    num_pipelines: usize,
    num_nodes: usize,
) -> Vec<Arc<Mutex<Box<dyn Node>>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let stat_tracker = StatTrackerWrapper {
        inner: StatTrackerNode::default(),
        receiver: rx,
    };
    std::thread::spawn(|| stat_tracker.run());
    let mut pipelines = vec![];
    for i in 0..num_pipelines {
        let pipeline =
            create_owner_arc_mutex_node_pipeline(i as u32, num_nodes, Box::new(tx.clone()));
        pipelines.push(pipeline);
    }

    pipelines
}

pub(crate) fn run_test(num_pipelines: usize, num_nodes: usize, num_packets: usize) -> Duration {
    let mut pipelines = create_owner_arc_mutex_node_pipelines(num_pipelines, num_nodes);
    let total_packets = num_packets * num_pipelines;

    let start = Instant::now();
    for i in 1..=total_packets {
        let pipeline = &mut pipelines[i % num_pipelines];
        let packet = PacketInfo::new(i as u32, "packet received", i == total_packets);
        pipeline.lock().unwrap().process_packet(packet);
    }
    Instant::now() - start
}
