// TEMPORARILY disable warnings
#![allow(warnings)]

pub mod datastore;
pub mod durability;
pub mod node;
pub mod tests;
pub mod utils;

use crate::utils::{create_runtime, spawn_nodes};

use omnipaxos::util::NodeId;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;

// define how many nodes to use
const SERVERS: [NodeId; 5] = [1, 2, 3, 4, 5];

/// main is just starting up the nodes, use tests to see functionality
fn main() -> Result<(), Box<dyn Error>> {
    // assignment: Your code must run different nodes on multiple threads
    let mut runtime = create_runtime();
    let nodes = spawn_nodes(&mut runtime, &SERVERS);

    // wait for the nodes to finish
    for (_, (_, handle)) in nodes {
        runtime.block_on(handle)?;
    }

    Ok(())
}
