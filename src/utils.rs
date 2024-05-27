use crate::durability::omnipaxos_durability::DatabaseLogEntry;
use crate::durability::omnipaxos_durability::OmniPaxosDurability;
use crate::node::Node;
use crate::node::NodeRunner;

use omnipaxos::messages::Message;
use omnipaxos::util::NodeId;
use omnipaxos::ClusterConfig;
use omnipaxos::OmniPaxos;
use omnipaxos::OmniPaxosConfig;
use omnipaxos::ServerConfig;
use omnipaxos_storage::memory_storage::MemoryStorage;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const MSG_BUFFER_SIZE: usize = 10000;

pub fn spawn_nodes(
    runtime: &mut Runtime,
    servers: &[NodeId],
) -> HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)> {
    let mut nodes = HashMap::new();
    let (mut sender_channels, mut receiver_channels) = initialise_channels(servers);
    for pid in servers {
        // spawn nodes
        let mut connections = HashMap::new();
        connections.extend(sender_channels.clone());
        // remove connection to itself
        connections.remove(pid);
        let receiver = receiver_channels.remove(&pid).unwrap();
        let node = Node::new((*pid).clone(), servers, connections);
        let node = Arc::new(Mutex::new(node));
        let node_clone = Arc::clone(&node);
        let handle: JoinHandle<()> = runtime.spawn(async move {
            let mut node_runner = NodeRunner::new(node_clone, receiver);
            node_runner.run().await;
        });
        // save the node and its handle to be able to access it later
        nodes.insert((*pid).clone(), (node, handle));
    }
    nodes
}

pub fn create_runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(10)
        .enable_all()
        .build()
        .unwrap()
}

fn initialise_channels(
    servers: &[NodeId],
) -> (
    HashMap<NodeId, mpsc::Sender<Message<DatabaseLogEntry>>>,
    HashMap<NodeId, mpsc::Receiver<Message<DatabaseLogEntry>>>,
) {
    let mut senders = HashMap::new();
    let mut receivers = HashMap::new();
    for pid in servers {
        // create channels
        let (sender, receiver) = mpsc::channel(MSG_BUFFER_SIZE);
        senders.insert((*pid).clone(), sender);
        receivers.insert((*pid).clone(), receiver);
    }
    (senders, receivers)
}

/// We use this to simulate the partial connectivity scenarios
pub fn delete_connections(
    node_a: NodeId,
    node_b: NodeId,
    nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>,
) {
    let connection_a = nodes
        .get(&node_a)
        .unwrap()
        .0
        .lock()
        .unwrap()
        .connections
        .remove(&node_b);
    let connection_b = nodes
        .get(&node_b)
        .unwrap()
        .0
        .lock()
        .unwrap()
        .connections
        .remove(&node_a);
    // save the disconnected connections to be able to reconnect later
    if (connection_a.is_some()) {
        nodes
            .get(&node_a)
            .unwrap()
            .0
            .lock()
            .unwrap()
            .disconnected_connections
            .insert(node_b, connection_a.unwrap());
    }

    if (connection_b.is_some()) {
        nodes
            .get(&node_b)
            .unwrap()
            .0
            .lock()
            .unwrap()
            .disconnected_connections
            .insert(node_a, connection_b.unwrap());
    }
}

/// Reconnects all the nodes that were disconnected
pub fn reconnect_nodes(nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>) {
    for (node_id, (node, _)) in nodes {
        let disconnected_connections = node.lock().unwrap().disconnected_connections.clone();
        for (disconnected_node_id, connection) in disconnected_connections.iter() {
            node.lock()
                .unwrap()
                .connections
                .insert(disconnected_node_id.clone(), connection.clone());
        }
        node.lock().unwrap().disconnected_connections.clear();
    }
}
