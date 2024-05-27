use omnipaxos::macros::Entry;
use omnipaxos::messages::Message as OPMessage;
use omnipaxos::util::{LogEntry, NodeId};
use omnipaxos::OmniPaxos;
use omnipaxos::*;
use omnipaxos_storage::memory_storage::MemoryStorage;
use std::io::{Read, Write};

use super::*;
use crate::{
    datastore::{tx_data::TxData, TxOffset},
    node::Node,
};

use crate::durability::omnipaxos_durability::storage::Entry;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use omnipaxos::unicache::MaybeEncoded;
use omnipaxos::macros::UniCacheSerdeEntry;
use omnipaxos::unicache::*;

use omnipaxos::unicache::UniCache;
use omnipaxos::macros::UniCacheEntry;
use crate::bonustask::AccessLogEntry;


//#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, UniCacheSerdeEntry)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, UniCacheSerdeEntry)]
pub struct DatabaseLogEntry {
    tx_offset: TxOffset,
    tx_data: TxData,
    #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u8), cache(lfu), size(10000)))] // Apply conditionally
    ip: String,
    #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u8), cache(lfu), size(10000)))]
    date: String,
    // #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u16), cache(lru)))]
    time: String,
    // #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u8), cache(lru)))]
    offset: i16,
    #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u8), cache(lfu), size(10000)))]
    url: String,
    #[cfg_attr(feature = "project_use_unicache", unicache(encoding(u8), cache(lfu), size(10000)))]
    method_protocol_status: String,
    response_size: u16
}

/// OmniPaxosDurability is a OmniPaxos node that should provide the replicated
/// implementation of the DurabilityLayer trait required by the Datastore.
pub struct OmniPaxosDurability {
    pub omnipaxos: OmniPaxos<DatabaseLogEntry, MemoryStorage<DatabaseLogEntry>>,
    node_id: NodeId,
}

impl OmniPaxosDurability {
    pub fn new(node_id: NodeId, node_array: &[NodeId]) -> Self {
        let mut omni_paxos: OmniPaxos<DatabaseLogEntry, MemoryStorage<DatabaseLogEntry>> =
            Self::omnipaxos_setup(node_id, node_array);
        Self {
            omnipaxos: omni_paxos,
            node_id,
        }
    }

    //below code for omnipaxos_setup was copied from omnipaxos.com
    fn omnipaxos_setup(
        node_id: NodeId,
        node_array: &[NodeId],
    ) -> OmniPaxos<DatabaseLogEntry, MemoryStorage<DatabaseLogEntry>> {
        let cluster_config = ClusterConfig {
            configuration_id: 1,
            nodes: node_array.to_vec(),
            ..Default::default()
        };

        // create the replica node_id in this cluster
        let server_config = ServerConfig {
            pid: node_id,
            election_tick_timeout: 5,
            ..Default::default()
        };

        // Combined OmniPaxos config with both clsuter-wide and server specific configurations
        let omnipaxos_config = OmniPaxosConfig {
            cluster_config,
            server_config,
        };

        let storage = MemoryStorage::default();
        let mut omni_paxos: OmniPaxos<DatabaseLogEntry, MemoryStorage<DatabaseLogEntry>> =
            omnipaxos_config.build(storage).unwrap();
        omni_paxos
    }

    /// Omnipaxos tickting periodically
    pub fn tick(&mut self) {
        self.omnipaxos.tick();
    }

    //handles all incoming messages as omnipaxos messages
    pub fn handle_incoming(&mut self, msg: OPMessage<DatabaseLogEntry>) {
        //println!("Handling incoming message from {:?} to {:?}", msg.get_sender(), msg.get_receiver());
        //println!("{:?}", msg);
        self.omnipaxos.handle_incoming(msg);
    }

    pub fn get_current_leader(&self) -> Option<NodeId> {
        return self.omnipaxos.get_current_leader();
    }

    pub fn is_leader(&self) -> bool {
        return self.omnipaxos.get_current_leader() == Some(self.node_id);
    }

    //only returns Decided entries
    pub fn read_decided_at_position(&self, pos: u64) -> Option<(TxOffset, TxData)> {
        let log_entry = self.omnipaxos.read(pos.try_into().unwrap()).unwrap();
        match log_entry {
            LogEntry::Decided(item) => Some((item.tx_offset, item.tx_data)),
            _ => None,
        }
    }

    // returns the index of the last decided entry or None if no entry is decided
    pub fn get_durable_tx_offset_optional(&self) -> Option<TxOffset> {
        let mut offset = self.omnipaxos.get_decided_idx();
        if (offset > 0) {
            offset -= 1;
        }
        match self.omnipaxos.read(offset) {
            Some(LogEntry::Decided(entry)) => Some(entry.tx_offset),
            _ => None,
        }
    }
}
impl DurabilityLayer for OmniPaxosDurability {

    fn iter(&self) -> Box<dyn Iterator<Item = (TxOffset, TxData)>> {
        self.iter_starting_from_offset(TxOffset(0))
    }

    fn iter_starting_from_offset(
        &self,
        offset: TxOffset,
    ) -> Box<dyn Iterator<Item = (TxOffset, TxData)>> {
        let mut offset = offset.0;
        let mut decided_entries = Vec::new();
        // if idx is 0, then there is only one entry in the log
        let mut omnipaxos_decided_idx = self.omnipaxos.get_decided_idx();
        if (omnipaxos_decided_idx == 0) {
            decided_entries = vec![self.omnipaxos.read(offset.try_into().unwrap()).unwrap()]
        } else {
            for i in offset + 1..(omnipaxos_decided_idx as u64) + 1 {
                let log_entry = self.omnipaxos.read(i.try_into().unwrap());
                match log_entry {
                    Some(entry) => {
                        decided_entries.push(entry)
                    }
                    _ => {}
                }
            }
        }
        Box::new(decided_entries.into_iter().filter_map(|log_entry| {
            // not optimal syntax but not sure how to do this propperly in rust
            if let LogEntry::Decided(entry) = log_entry {
                Some((entry.tx_offset, entry.tx_data))
            } else {
                println!("Decided entry should be decided but it was not. This should not happen.");
                None
            }
        }))
    }

    /// Append a new transaction to the omnipaxos log
    fn append_tx(&mut self, tx_offset: TxOffset, tx_data: TxData, ale: AccessLogEntry) {
        self.omnipaxos
            .append(DatabaseLogEntry {
                tx_offset: tx_offset,
                tx_data: tx_data,
                ip: ale.ip,
                date: ale.day.to_string() + " - " + &ale.month + " - " + &ale.year.to_string(),
                time: ale.time,
                offset: ale.offset,
                url: ale.url,
                method_protocol_status: ale.method + " - " + &ale.protocol.to_string() + " - " + &ale.status_code.to_string(),
                response_size: ale.response_size,
            })
            .expect("append failed");
    }

    /// Returns the offset of the last decided transaction, returns 0 if no or one transaction is decided
    fn get_durable_tx_offset(&self) -> TxOffset {
        let mut offset = self.omnipaxos.get_decided_idx();
        if offset > 0 {
            offset -= 1;
        }
        match self.omnipaxos.read(offset) {
            Some(LogEntry::Decided(entry)) => entry.tx_offset,
            _ => TxOffset(0),
        };
        TxOffset(offset as u64)
    }
}
