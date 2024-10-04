use crate::{Node, PacketInfo};

impl Node for tokio::sync::mpsc::Sender<PacketInfo> {
    fn process_packet(&mut self, packet_info: PacketInfo) {
        self.try_send(packet_info).unwrap();
    }
}

pub(crate) struct ActorNode {
    name: String,
    // Hack: make this an option so we can hand it out to whoever needs it but drop it in 'run' so
    // there won't be a dangling reference to the channel sender and we can detect when to shut
    // down correctly
    incoming_tx: Option<tokio::sync::mpsc::Sender<PacketInfo>>,
    incoming_rx: tokio::sync::mpsc::Receiver<PacketInfo>,
    next: Option<tokio::sync::mpsc::Sender<PacketInfo>>,
}

impl ActorNode {
    pub(crate) fn new(name: &str) -> Self {
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::channel(100);
        Self {
            name: name.to_owned(),
            incoming_tx: Some(incoming_tx),
            incoming_rx,
            next: None,
        }
    }

    pub(crate) fn get_sender(&self) -> tokio::sync::mpsc::Sender<PacketInfo> {
        self.incoming_tx.as_ref().unwrap().clone()
    }

    pub(crate) async fn run(mut self) {
        self.incoming_tx.take();
        while let Some(mut packet) = self.incoming_rx.recv().await {
            packet.add_event(&format!("received by {}", self.name));
            if let Some(ref next) = self.next {
                packet.add_event(&format!("sent by {}", self.name));
                let _ = next.send(packet).await;
            }
        }
    }
}

pub(crate) fn create_actor_node_pipeline(pipeline_id: u32, num_nodes: usize) -> Vec<ActorNode> {
    let mut nodes = vec![];
    let first_node = ActorNode::new("node_0");
    nodes.push(first_node);
    for i in 1..num_nodes {
        let node = ActorNode::new(&format!("pipeline_{pipeline_id}_node_{i}"));
        nodes[i - 1].next = Some(node.get_sender());
        nodes.push(node);
    }
    nodes
}
