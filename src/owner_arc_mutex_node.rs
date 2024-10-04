use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{Node, PacketInfo, StatTrackerNode};

pub(crate) struct OwnerNodeArcMutex {
    name: String,
    next: Option<Arc<Mutex<Box<dyn Node>>>>,
}

impl OwnerNodeArcMutex {
    pub(crate) fn new(name: &str, next: Option<Arc<Mutex<Box<dyn Node>>>>) -> Self {
        Self {
            name: name.to_owned(),
            next,
        }
    }
}

#[async_trait]
impl Node for OwnerNodeArcMutex {
    async fn process_packet(&mut self, mut packet_info: PacketInfo) {
        packet_info.add_event(&format!("received by {}", self.name));
        if let Some(ref mut next) = self.next {
            packet_info.add_event(&format!("sent by {}", self.name));
            next.lock().await.process_packet(packet_info).await;
        }
    }
}

pub(crate) fn create_owner_arc_mutex_node_pipeline(
    pipeline_id: u32,
    num_nodes: usize,
) -> Arc<Mutex<Box<dyn Node>>> {
    let mut prev_node: Option<Arc<Mutex<Box<dyn Node>>>> =
        Some(Arc::new(Mutex::new(Box::new(StatTrackerNode::default()))));
    for i in (0..num_nodes).rev() {
        let node = Box::new(OwnerNodeArcMutex::new(
            &format!("pipeline_{pipeline_id}_node_{i}"),
            prev_node.take(),
        ));
        prev_node = Some(Arc::new(Mutex::new(node)));
    }

    prev_node.unwrap()
}
