pub mod actor_node;
mod owner_arc_mutex_node;
pub mod owner_node;

use std::time::{Duration, Instant};

use meansd::MeanSD;

#[allow(dead_code)]
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

#[allow(dead_code)]
pub(crate) struct PacketInfo {
    index: u32,
    timeline: Vec<TimelineEvent>,
    last_packet: bool,
}

impl PacketInfo {
    pub fn new(index: u32, initial_event: &str, last_packet: bool) -> Self {
        let timeline = vec![TimelineEvent::new(Instant::now(), initial_event)];
        Self {
            index,
            timeline,
            last_packet,
        }
    }

    fn add_event(&mut self, event: &str) {
        let time = Instant::now();
        self.timeline.push(TimelineEvent::new(time, event));
    }

    #[allow(dead_code)]
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

#[derive(Default)]
pub(crate) struct JitterCalculator {
    prev_transit_time: Duration,
    current_jitter: f32,
}

impl JitterCalculator {
    fn add_value(&mut self, current_transit_time: Duration) {
        let d = current_transit_time.abs_diff(self.prev_transit_time);
        self.prev_transit_time = current_transit_time;
        self.current_jitter += (1f32 / 16f32) * (d.as_micros() as f32 - self.current_jitter);
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
            "jitter: {}us, mean latency: {}us, stddev latency: {}us across {} samples",
            self.jitter_calculator.current_jitter(),
            self.latency.mean(),
            self.latency.sstdev(),
            self.latency.size(),
        );
    }
}

#[tokio::main]
// #[tokio::main(flavor = "current_thread")]
async fn main() {
    let num_packets = 1000000;
    let num_nodes = 10;
    let num_pipelines = 10;
    // console_subscriber::init();
    println!("Running {num_pipelines} pipelines with {num_nodes} nodes per pipeline and processing {num_packets} packets");

    println!("==> Running owner node test");
    let owner_node_test_time = owner_node::run_test(num_pipelines, num_nodes, num_packets);
    println!("Owner node took {}ms", owner_node_test_time.as_millis());

    println!("==> Running owner arc mutex node test");
    let owner_arc_mutex_node_test_time =
        owner_arc_mutex_node::run_test(num_pipelines, num_nodes, num_packets);
    println!(
        "Owner arc mutex node took {}ms",
        owner_arc_mutex_node_test_time.as_millis()
    );

    println!("==> Running actor node test");
    let actor_node_test_time = actor_node::run_test(num_pipelines, num_nodes, num_packets).await;
    println!("Actor node took {}ms", actor_node_test_time.as_millis());
}
