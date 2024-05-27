use crate::datastore::error::DatastoreError;
use crate::datastore::example_datastore::ExampleDatastore;
use crate::datastore::tx_data::TxResult;
use crate::datastore::{self, *};
use crate::durability::omnipaxos_durability::{DatabaseLogEntry, OmniPaxosDurability};
use crate::durability::{DurabilityLayer, DurabilityLevel};
use core::time::Duration;
use omnipaxos::messages::sequence_paxos::{PaxosMessage, PaxosMsg};
use omnipaxos::messages::*;
use omnipaxos::storage::Entry;
use omnipaxos::util::NodeId;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::mem::size_of_val;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time;

use crate::datastore::{tx_data::TxData, TxOffset};
use omnipaxos::macros::Entry;

use self::example_datastore::{MutTx, Tx};

const TICK_PERIOD: Duration = Duration::from_millis(10);
const MSG_HANDLING_PERIOD: Duration = Duration::from_millis(1);

pub struct NodeRunner {
    pub node: Arc<Mutex<Node>>,
    // The receiver is in the node runner since it handles the incoming messages
    pub receiver: mpsc::Receiver<Message<DatabaseLogEntry>>,
}

impl NodeRunner {
    pub fn new(
        node: Arc<Mutex<Node>>,
        receiver: mpsc::Receiver<Message<DatabaseLogEntry>>,
    ) -> Self {
        NodeRunner { node, receiver }
    }

    async fn send_outgoing_msgs(&mut self) {
        let out_messages = self
            .node
            .lock()
            .unwrap()
            .omni_durability
            .lock()
            .unwrap()
            .omnipaxos
            .outgoing_messages();
        for msg in out_messages {
            let receiver = msg.get_receiver();
            let mut connections = self.node.lock().unwrap().connections.clone();
            if (!connections.contains_key(&receiver)) {
                // if the receiver is not connected to the node we skip the message
                continue;
            }
            let channel = connections
                .get_mut(&receiver)
                .expect("No channel for receiver");
            let _ = channel.send(msg).await;
        }
    }

    pub async fn run(&mut self) {
        println!("Node {} is running", self.node.lock().unwrap().node_id);
        let mut tick_interval = time::interval(TICK_PERIOD);
        let mut msg_handling_interval = time::interval(MSG_HANDLING_PERIOD); // From omnipaxos examples

        while (self.node.lock().unwrap().is_running) {
            // takes the branch that finished first while maintaining order, so every 10ms tick_interval, every 1ms msg handling and if there is a message in the receiver
            tokio::select! {
                biased;
                _ = tick_interval.tick() => { self.node.lock().unwrap().omni_durability.lock().unwrap().tick(); },
                _ = msg_handling_interval.tick() => {
                    self.node.lock().unwrap().update_leader();
                    self.node.lock().unwrap().apply_replicated_txns();
                    self.send_outgoing_msgs().await;

                }
                Some(in_msg) = self.receiver.recv() => {
                    let serialized = serde_json::to_string(&in_msg).expect("serialization failed");
                    let size_string = serialized.len();
                    self.node.lock().unwrap().received_messages_size += size_string as u64;
                    let in_msg_checking = in_msg.clone();

                    self.node.lock().unwrap().omni_durability.lock().unwrap().handle_incoming(in_msg);
                },
                else => { }
            }
        }
        println!(
            "Node {} is shutting down",
            self.node.lock().unwrap().node_id
        );
    }
}

pub struct Node {
    node_id: NodeId, // Unique identifier for the node
    datastore: Arc<Mutex<ExampleDatastore>>,
    pub omni_durability: Arc<Mutex<OmniPaxosDurability>>,
    last_applied_index: Option<TxOffset>, // Keeps track of how many entries have been applied to the datastore
    pub connections: HashMap<NodeId, mpsc::Sender<Message<DatabaseLogEntry>>>, // need connections here to manipulate them after initialization
    pub disconnected_connections: HashMap<NodeId, mpsc::Sender<Message<DatabaseLogEntry>>>, // disconnected connections are kept to establish reconnection
    is_running: bool, // used for testing 
    pub current_leader: Option<NodeId>,
    pub received_messages_size: u64, // stores te size of the received messages in bytes
    pub got_promoted_leader: bool, // used to check if the node got promoted to leader
}

impl Node {
    pub fn new(
        node_id: NodeId,
        servers: &[NodeId],
        connections: HashMap<NodeId, mpsc::Sender<Message<DatabaseLogEntry>>>,
    ) -> Self {
        Node {
            node_id,
            datastore: Arc::new(Mutex::new(ExampleDatastore::new())),
            omni_durability: Arc::new(Mutex::new(OmniPaxosDurability::new(node_id, servers))),
            last_applied_index: None,
            connections,
            disconnected_connections: HashMap::new(),
            is_running: true,
            current_leader: None,
            received_messages_size: 0,
            got_promoted_leader: false,
        }
    }

    pub fn to_string(&self) -> String {
        let omnipaxos_offset = self.omni_durability.lock().unwrap().get_durable_tx_offset();
        let omnipaxos_string = omnipaxos_offset.0;
        // Return a string representation of the node with all fields
        format!("NodeID: {}, isRunning {}, CurrentLeader: {} ReplicatedDurability: {}, MemoryDurability: {}, OmniPaxosDecided: {} Connections: {:?}, ReceivedMessagesSize: {} KB", 
        self.node_id, self.is_running, self.omni_durability.lock().unwrap().get_current_leader().unwrap_or(99), if self.last_applied_index.is_some() { self.last_applied_index.unwrap().0.to_string() } else { "None".to_string() }, self.datastore.lock().unwrap().get_cur_offset().unwrap_or(TxOffset(0)).0, omnipaxos_string, self.connections.keys(), self.received_messages_size / 1000)
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    /// update who is the current leader. If a follower becomes the leader,
    /// it needs to apply any unapplied txns to its datastore.
    /// If a node loses leadership, it needs to rollback the txns committed in
    /// memory that have not been replicated yet.
    pub fn update_leader(&mut self) {
        let current_leader = self.omni_durability.lock().unwrap().get_current_leader();
        let old_leader = self.current_leader;
        if (old_leader.is_some()
            && current_leader.is_some()
            && old_leader.unwrap() != current_leader.unwrap())
        {
            println!(
                "Handle leader change at node {} from {:?} to {:?}",
                self.node_id, old_leader, current_leader
            );
            // handle leadership change
            // leader lost leadership
            if old_leader.is_some() && &old_leader.unwrap() == self.node_id() {
                println!("Node {} lost leadership", self.node_id);
                if let Err(e) = self.rollback_unreplicated_txns() {
                    eprintln!("Failed to rollback unreplicated transactions: {}", e);
                }
            // follower got promoted to leader
            } else if (&current_leader.unwrap() == self.node_id()) {
                println!("Node {} got promoted to leader", self.node_id);
                // If the current node is the leader, apply any unapplied transactions
                self.got_promoted_leader = true;
                self.apply_replicated_txns(); // unapplied txns are dictated by omnipaxos log
            } else {
            }

            self.current_leader = current_leader;
        }
        if (old_leader.is_none() && current_leader.is_some()) {
            self.current_leader = current_leader;
        }
    }

    fn rollback_unreplicated_txns(&mut self) -> Result<(), DatastoreError> {
        self.datastore
            .lock()
            .unwrap()
            .rollback_to_replicated_durability_offset()
    }

    /// Apply the transactions that have been decided in OmniPaxos to the Datastore.
    /// We need to be careful with which nodes should do this according to desired
    /// behavior in the Datastore as defined by the application.
    fn apply_replicated_txns(&mut self) {
        let mut omnipaxos_decided_index = self
            .omni_durability
            .lock()
            .unwrap()
            .get_durable_tx_offset_optional();
        let mut replicated_offset = self.last_applied_index;
        if (omnipaxos_decided_index.is_none()
            || (omnipaxos_decided_index.is_some()
                && replicated_offset.is_some()
                && omnipaxos_decided_index.unwrap() == replicated_offset.unwrap()))
        {
            return; // No need to do anything, regardless of role
        }
        // If durability_offset > replicated_offset (replicated lags behind)
        if (self.omni_durability.lock().unwrap().is_leader() && (!self.got_promoted_leader || self.datastore.lock().unwrap().get_cur_offset().unwrap() == omnipaxos_decided_index.unwrap())) {
            // If leader, let omnipaxos advance replicated offset: those in memory will get applied to replicated state
            println!("Node {} is advancing replicated durability offset", self.node_id);
            self.last_applied_index = omnipaxos_decided_index;
            match self.advance_replicated_durability_offset() {
                Ok(_) => (),
                Err(e) => println!("Failed to advance replicated durability offset: {}", e),
            }
        } else {
            println!("Node {} is applying replicated transactions", self.node_id);
            // Follower should replay any new replicated transactions (if there are any)
            let transactions = self
                .omni_durability
                .lock()
                .unwrap()
                .iter_starting_from_offset(replicated_offset.unwrap_or(TxOffset(0)));
            for transaction in transactions {
                let (offset, data) = transaction;
                match self.datastore.lock().unwrap().replay_transaction(&data) {
                    Ok(_) => (),
                    Err(e) => println!("Failed to replay transaction: {}", e),
                } // Replay transaction in datastore

                self.last_applied_index = Some(offset);
                self.datastore
                    .lock()
                    .unwrap()
                    .advance_replicated_durability_offset(offset); // So transaction is no longer considered by this node in the future
            }
        }
        self.got_promoted_leader = false;
        // we want to run the leader log sync only once
    }

    pub fn begin_tx(
        &self,
        durability_level: DurabilityLevel,
    ) -> <ExampleDatastore as Datastore<String, String>>::Tx {
        self.datastore.lock().unwrap().begin_tx(durability_level)
    }

    pub fn release_tx(&self, tx: <ExampleDatastore as Datastore<String, String>>::Tx) {
        self.datastore.lock().unwrap().release_tx(tx); // Release the transaction from the datastore
    }

    /// Begins a mutable transaction. Only the leader is allowed to do so.
    pub fn begin_mut_tx(
        &self,
    ) -> Result<<ExampleDatastore as Datastore<String, String>>::MutTx, DatastoreError> {
        if self.omni_durability.lock().unwrap().is_leader() {
            // If the current node is the leader, begin a mutable transaction using datastore
            Ok(self.datastore.lock().unwrap().begin_mut_tx())
        } else {
            Err(DatastoreError::NotLeader)
        }
    }

    /// Commits a mutable transaction. Only the leader is allowed to do so.
    pub fn commit_mut_tx(
        &mut self,
        tx: <ExampleDatastore as Datastore<String, String>>::MutTx,
    ) -> Result<TxResult, DatastoreError> {
        if self.omni_durability.lock().unwrap().is_leader() {
            // If the current node is the leader, commit the transaction
            let data_commit = self.datastore.lock().unwrap().commit_mut_tx(tx);
            // If transaction is committed successfully, need to append to durability layer
            // Since: "transactions might be committed locally but later get rolled back" (project description)
            // Even if disconnection happens, need to be able to roll back
            // So replicate transaction over omnipaxos:
            self.omni_durability.lock().unwrap().append_tx(
                data_commit.as_ref().unwrap().tx_offset,
                data_commit.as_ref().unwrap().tx_data.clone(),
            );
            data_commit
        } else {
            Err(DatastoreError::NotLeader)
        }
    }

    fn advance_replicated_durability_offset(
        &self,
    ) -> Result<(), crate::datastore::error::DatastoreError> {
        self.datastore
            .lock()
            .unwrap()
            .advance_replicated_durability_offset(self.last_applied_index.unwrap())
    }
}
