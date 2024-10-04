use async_trait::async_trait;

use crate::{Node, PacketInfo, StatTrackerNode};

pub(crate) struct OwnerNode {
    name: String,
    next: Option<Box<dyn Node>>,
}

impl OwnerNode {
    pub(crate) fn new(name: &str, next: Option<Box<dyn Node>>) -> Self {
        Self {
            name: name.to_owned(),
            next,
        }
    }
}

#[async_trait]
impl Node for OwnerNode {
    async fn process_packet(&mut self, mut packet_info: PacketInfo) {
        packet_info.add_event(&format!("received by {}", self.name));
        if let Some(ref mut next) = self.next {
            packet_info.add_event(&format!("sent by {}", self.name));
            next.process_packet(packet_info).await;
        }
    }
}

pub(crate) fn create_owner_node_pipeline(pipeline_id: u32, num_nodes: usize) -> Box<dyn Node> {
    let mut prev_node: Option<Box<dyn Node>> = Some(Box::new(StatTrackerNode::default()));
    for i in (0..num_nodes).rev() {
        let node = Box::new(OwnerNode::new(
            &format!("pipeline_{pipeline_id}_node_{i}"),
            prev_node.take(),
        ));
        prev_node = Some(node);
    }

    prev_node.unwrap()
}
