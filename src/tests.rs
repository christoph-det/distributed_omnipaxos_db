/// Your test cases should spawn up multiple nodes in tokio and cover the following:
/// 1. Find the leader and commit a transaction. Show that the transaction is really *chosen* (according to our definition in Paxos) among the nodes.
/// 2. Find the leader and commit a transaction. Kill the leader and show that another node will be elected and that the replicated state is still correct.
/// 3. Find the leader and commit a transaction. Disconnect the leader from the other nodes and continue to commit transactions before the OmniPaxos election timeout.
/// Verify that the transaction was first committed in memory but later rolled back.
/// 4. Simulate the 3 partial connectivity scenarios from the OmniPaxos liveness lecture. Does the system recover? *NOTE* for this test you may need to modify the messaging logic.
///
/// A few helper functions to help structure your tests have been defined that you are welcome to use.
#[cfg(test)]
pub mod tests {
    use crate::durability::DurabilityLevel;
    use crate::node::*;
    use crate::utils::{create_runtime, delete_connections, reconnect_nodes, spawn_nodes};
    use omnipaxos::messages::Message;
    use omnipaxos::util::NodeId;
    use std::collections::HashMap;
    use std::fmt::format;
    use std::os::unix::thread;
    use std::sync::{Arc, Mutex};
    use std::thread::current;
    use std::vec;
    use tokio::runtime::{Builder, Runtime};
    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;
    use std::time::{Duration, SystemTime};

    const SERVERS: [NodeId; 5] = [1, 2, 3, 4, 5];
    const WAIT_LEADER_TIMEOUT: Duration = Duration::from_millis(400);
    const WAIT_REPLICATION_TIMEOUT: Duration = Duration::from_millis(800);
    const WAIT_COMMIT_TIMEOUT: Duration = Duration::from_millis(20);

    /// Tests basic node spawning
    #[test]
    fn start_nodes() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        assert_eq!(nodes.len(), SERVERS.len());
    }

    /// Tests basic leader election
    #[test]
    fn test_one_leader_should_be_elected() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        assert_eq!(nodes.len(), SERVERS.len());

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        let leaders = find_leaders(&nodes);

        assert_eq!(leaders.len(), 1, "There should be exactly one leader");

        // all nodes should have the same leader
        for pid in SERVERS {
            assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().get_current_leader().unwrap(), leaders[0], "All nodes should have the same leader");
        }
    }

    /// 1. Find the leader and commit a transaction. Show that the transaction is really *chosen* (according to our definition in Paxos) among the nodes.
    #[test]
    fn test_one_tx_is_chosen() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        assert_eq!(nodes.len(), SERVERS.len());

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        // find leader
        let leader = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());

        println!("Startup completed.");
        print_nodes(&nodes);

        let mut followers: Vec<NodeId> = SERVERS.to_vec();
        followers.retain(|&x| x != leader);

        // create a transaction
        let mut tx = nodes.get(&leader).unwrap().0.lock().unwrap().begin_mut_tx().unwrap();
        tx.set("Test".to_string(), "Hello".to_string());

        println!("Transaction created.");

        // commit the transaction
        let result = nodes.get(&leader).unwrap().0.lock().unwrap().commit_mut_tx(tx);
        assert_eq!(result.is_ok(), true);

        println!("Transaction commited.");
        println!("Waiting for replication...");
        println!();

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        // check if the transaction was replicated i.e. choosen
        for pid in SERVERS {
            let mut tx = nodes.get(&pid).unwrap().0.lock().unwrap().begin_tx(DurabilityLevel::Replicated);
            let tx_data = tx.get(&"Test".to_string());
            match tx_data.clone() {
                Some(data) => assert_eq!(data, "Hello"),
                None => assert!(false, "Data should be replicated now at node {}", pid)
            }
            match tx_data {
                Some(data) => {
                    println!("Replicated data at node {} is {:?}", pid, data);
                }
                None => println!("No data was replicated yet at node {}", pid)
                
            }
            nodes.get(&pid).unwrap().0.lock().unwrap().release_tx(tx);
        }

        println!("Transaction was chosen among nodes and correctly replicated.");
        println!();
        print_nodes(&nodes);

    }

    #[test]
    fn test_msg_size_is_recorded() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        assert_eq!(nodes.len(), SERVERS.len());

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        // find leader
        let leader = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());

        println!("Startup completed.");
        print_nodes(&nodes);

        let mut followers: Vec<NodeId> = SERVERS.to_vec();
        followers.retain(|&x| x != leader);

        // create a transaction
        let mut tx = nodes.get(&leader).unwrap().0.lock().unwrap().begin_mut_tx().unwrap();
        tx.set("Test".to_string(), "Hello".to_string());

        println!("Transaction created.");

        // commit the transaction
        let result = nodes.get(&leader).unwrap().0.lock().unwrap().commit_mut_tx(tx);
        assert_eq!(result.is_ok(), true);

        println!("Transaction commited.");
        println!("Waiting for replication...");
        println!();

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        
        print_nodes(&nodes);


        // create a larger transaction
        let mut tx = nodes.get(&leader).unwrap().0.lock().unwrap().begin_mut_tx().unwrap();
        let mut data = String::new();
        for i in 0..100000 {
            data += "Hallo";
        }
        tx.set("Test".to_string(), data.to_string());

        println!("Transaction created.");

        // commit the transaction
        let result = nodes.get(&leader).unwrap().0.lock().unwrap().commit_mut_tx(tx);
        assert_eq!(result.is_ok(), true);

        println!("Transaction commited.");
        println!("Waiting for replication...");
        println!();

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        
        print_nodes(&nodes);

    }

    #[test]
    fn test_many_tx_throughput() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        let mut transactions_ids_data: HashMap<String, String> = HashMap::new();

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        let leader: NodeId = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());

        println!();
        print_nodes(&nodes);

        println!("Comitting 100 transaction to current leader.");
        transactions_ids_data.extend(commit_transactions(leader, &nodes, 100));

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT * 2);

        println!();
        print_nodes(&nodes);

        
        check_replication(&nodes, &transactions_ids_data);

        println!("Replicated data is correct at all nodes.");
        println!();
    }

    /// 2. Find the leader and commit a transaction. Kill the leader and show that another node will be elected and that the replicated state is still correct.
    #[test]
    fn test_two_kill_leader_new_leader_is_elected_correct_replication() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        let mut transactions_ids_data: HashMap<String, String> = HashMap::new();

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        let old_leader: NodeId = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());
        let mut followers: Vec<NodeId> = SERVERS.to_vec();
        followers.retain(|&x| x != old_leader);
        
        println!();
        print_nodes(&nodes);

        println!("Comitting one transaction to current leader.");
        transactions_ids_data.extend(commit_transactions(old_leader, &nodes, 1));

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        check_replication(&nodes, &transactions_ids_data);

        println!();        
        print_nodes(&nodes);

        println!("Comitting one transaction to current leader.");

        transactions_ids_data.extend(commit_transactions(old_leader, &nodes, 1));

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        check_replication(&nodes, &transactions_ids_data);

            
        println!();        
        print_nodes(&nodes);
        println!("Two transactions commited, offsets correctly advanced.");    
        println!();        

        // kill the leader
        println!("Killing the leader.");
        nodes.get(&old_leader).unwrap().0.lock().unwrap().stop();

        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        let new_leader = find_leader(&nodes.get(&followers[0]).unwrap().0.lock().unwrap());
        //should not be same leader
        assert_ne!(old_leader, new_leader, "A new leader should be elected");

        println!();        
        print_nodes(&nodes);

        println!("Comitting one transaction to new leader.");
        transactions_ids_data.extend(commit_transactions(new_leader, &nodes, 1));
        assert!(transactions_ids_data.len() == 3, "There should be 3 transactions commited now");

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        println!();        
        print_nodes(&nodes);
        println!("One transaction commited, offset correctly advanced, old leader is not changing anymore.");
        // check if the data is correct
        check_replication(&nodes, &transactions_ids_data);
        println!("Replicated data is correct at all nodes.");
        println!()
    }

    /// 3. Find the leader and commit a transaction. Disconnect the leader from the other nodes and continue to commit transactions before the OmniPaxos election timeout. Verify that the transaction was first committed in memory but later rolled back.
    #[test]
    fn test_three_leader_disconnected_correct_rollback() {
        const SERVERS_CHAIN: [NodeId; 4] = [1, 2, 3, 4];
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS_CHAIN);
        let mut transactions_ids_data: HashMap<String, String> = HashMap::new();
        
        // wait for complete startup
        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        // find the current leader
        let leader = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());

        println!("Startup complete.");
        print_nodes(&nodes);

        let mut followers = SERVERS_CHAIN.to_vec();
        followers.retain(|&x| x != leader);

        // Commit transaction to leader 
        transactions_ids_data.extend(commit_transactions(leader, &nodes, 1));
        let first_transaction = transactions_ids_data.clone(); // Only one transaction
        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        print_nodes(&nodes);

        // Verify that transaction gets replicated
        check_replication(&nodes, &transactions_ids_data);
        println!("Replicated data is correct at all nodes.");

        let keep_connections = vec![(followers[0], followers[1]), (followers[1], followers[2])];
        // Delete all connections to leader
        for pid_a in SERVERS_CHAIN {
            for pid_b in SERVERS_CHAIN {
                if pid_a != pid_b && !keep_connections.contains(&(pid_a, pid_b)) && !keep_connections.contains(&(pid_b, pid_a)){
                    delete_connections(pid_a, pid_b, &nodes);
                }
            }
        }

        println!();
        println!("Disconnected leader.");
        println!();

        print_nodes(&nodes);

        // commit another transaction to the disconnected leader
        let old_leader_tx = commit_transactions(leader, &nodes, 1); 

        // Verify in memory
        let memory_transaction = nodes.get(&leader).unwrap().0.lock().unwrap().begin_tx(DurabilityLevel::Memory); // Last transaction in memory?
        for (tx_key, tx_data) in &old_leader_tx { // Check that the transaction is in the memory layer of the leader
            assert_eq!(memory_transaction.get(&tx_key.to_string()), Some(tx_data.to_string()));
        }
        nodes.get(&leader).unwrap().0.lock().unwrap().release_tx(memory_transaction);

        println!("Transaction is in memory at leader but not yet replicated.");

        // Election timeout
        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        // Verify not replicated -- the first transaction we committed should be the last one we encounter in DurabilityLevel Replicated in the new leader
        let new_leader = find_leader(&nodes.get(&followers[0]).unwrap().0.lock().unwrap());
        let last_replicated_transaction = nodes.get(&new_leader).unwrap().0.lock().unwrap().begin_tx(DurabilityLevel::Replicated);
        for (tx_key, tx_data) in &first_transaction {
            assert_eq!(last_replicated_transaction.get(&tx_key.to_string()), Some(tx_data.to_string()));
        }
        nodes.get(&new_leader).unwrap().0.lock().unwrap().release_tx(last_replicated_transaction);

        print_nodes(&nodes);

        // commit another transaction to the new leader
        transactions_ids_data.extend(commit_transactions(new_leader, &nodes, 1));
        println!("Committing one transaction to new leader.");

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        print_nodes(&nodes);

        println!("Reconnect older leader with inconsistent state.");
        reconnect_nodes(&nodes);

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        print_nodes(&nodes);

        // now we should read the new transaction at all 4 nodes, and the old transaction should be gone
        check_replication(&nodes, &transactions_ids_data);
        println!("Replicated data is correct at all nodes.");

        let memory_transaction = nodes.get(&leader).unwrap().0.lock().unwrap().begin_tx(DurabilityLevel::Memory); // Last transaction in memory?
        for (tx_key, tx_data) in &old_leader_tx { // Check that the transaction is in the memory layer of the leader
            match memory_transaction.get(&tx_key.to_string()) {
                Some(data) => assert!(false, "Data should have been rolled back at old leader when reconnected."),
                None => {}
            }
        }
        nodes.get(&leader).unwrap().0.lock().unwrap().release_tx(memory_transaction);


    }

    /// 4. Simulate the 3 partial connectivity scenarios from the OmniPaxos liveness lecture. Does the system recover? *NOTE* for this test you may need to modify the messaging logic.
    #[test]
    fn test_four_simulate_partial_connectivity_chained() {
        // we only use 3 nodes for this test, as in the scenario on the slides
        const SERVERS_CHAIN: [NodeId; 4] = [1, 2, 3, 4];
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS_CHAIN);
        let mut transactions_ids_data: HashMap<String, String> = HashMap::new();

        // all nodes should have connectionss to all nodes in the beginning
        for pid in SERVERS_CHAIN {
                assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), SERVERS_CHAIN.len() - 1);
        }
        
        // wait for complete startup
        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        print_nodes(&nodes);
        println!();

        // find the current leader
        let leader: u64 = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());
        println!("The first leader is {:?}", leader);

        let mut followers = SERVERS_CHAIN.to_vec();
        followers.retain(|&x| x != leader);

        println!("Comitting one transaction to current leader.");
        transactions_ids_data.extend(commit_transactions(leader, &nodes, 1));
        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        print_nodes(&nodes);
        println!();

        check_replication(&nodes, &transactions_ids_data);

        // chained connection scenario (2 must not be the leader here): 1 -> 2 -> 3 -> 4
        let keep_connections = vec![(leader, followers[0]), (followers[0], followers[1]), (followers[1], followers[2])];
        println!("Disconnecting {:?} and {:?}", followers[1], leader);
        println!("Disconnecting {:?} and {:?}", followers[2], leader);
        println!("Disconnecting {:?} and {:?}", followers[0], followers[2]);
        // delete all other connections
        for pid_a in SERVERS_CHAIN {
            for pid_b in SERVERS_CHAIN {
                if pid_a != pid_b && !keep_connections.contains(&(pid_a, pid_b)) && !keep_connections.contains(&(pid_b, pid_a)){
                    delete_connections(pid_a, pid_b, &nodes);
                }
            }
        }

        // check that the connections are as expected, old leader should only have one, one follower should have 2, the other 1
        for pid in SERVERS_CHAIN {
            if (pid == leader || pid == followers[2]) {
                assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), 1);
                println!("Node {} has 1 connection", pid);
            } else {
                assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), 2);
                println!("Node {} has 2 connections", pid);
            }
        }

        // wait for leader reelection
        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        // check if system makes progress, i.e. new leader is elected and transactions are committed

        println!("The old leader was {:?}", leader);
        let new_leaders = find_leaders(&nodes);
        let new_leader = new_leaders[0];
        println!("The new leader is {:?}", new_leader);
        assert!(new_leaders.len() >= 1, "There should be a leader elected");

        print_nodes(&nodes);
        println!();

        // some leader needs to be elected to make progress, it can continue as a quorum and make progress
        // one node still thinks the old leader is the leader, because it is not connected to the new leader, but it remains silent to
        // allow for progress

        println!("Comitting one transaction to new leader.");
        transactions_ids_data.extend(commit_transactions(new_leader, &nodes, 1));

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        print_nodes(&nodes);
        println!();

        check_replication(&nodes, &transactions_ids_data);

        println!("Replicated data is correct at all nodes.");
        println!();

    }


    #[test]
    fn test_four_simulate_partial_connectivity_constrained() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);
        let mut tx_ids_data: HashMap<String, String> = HashMap::new();
        // all nodes should have 4 connections in the beginning
        for pid in SERVERS {
            assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), SERVERS.len() - 1);
        }

        // wait for complete startup
        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        println!("Startup completed.");
        print_nodes(&nodes);
    
        // find the current leader
        let leader = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());
        println!("The first leader is {:?}", leader);
        let mut followers: Vec<NodeId> = SERVERS.to_vec();
        followers.retain(|&x| x != leader);

        // Commit a normal transaction
        tx_ids_data.extend(commit_transactions(leader, &nodes, 1));
        std::thread::sleep(WAIT_REPLICATION_TIMEOUT); // Leave time for transactions to be replicated

        print_nodes(&nodes);
        check_replication(&nodes, &tx_ids_data);

        println!("Disconnected all nodes except two of the followers and the leader.");

        // Disconnect everything but two nodes and leader to prepere for the next test
        let keep_connections = vec![(leader, followers[0]), (leader, followers[1]), (followers[0], followers[1])];
        // delete all other connections
        for pid_a in SERVERS {
            for pid_b in SERVERS {
                if pid_a != pid_b && !keep_connections.contains(&(pid_a, pid_b)) && !keep_connections.contains(&(pid_b, pid_a)){
                    delete_connections(pid_a, pid_b, &nodes);
                }
            }
        }

        std::thread::sleep(WAIT_LEADER_TIMEOUT);
        print_nodes(&nodes);

        println!("Commit transaction to the quorum.");

        // commit a tx to the quorum, should be replicated also when reconnecting to the others
        tx_ids_data.extend(commit_transactions(leader, &nodes, 1)); 

        std::thread::sleep(WAIT_REPLICATION_TIMEOUT); 

        check_replication(&nodes, &tx_ids_data);

        print_nodes(&nodes);


        println!("Reconnecting nodes to node that lags begind.");

        // Reconnect everyone except leader to follower with outdated log and delete all other connections
        // this follower was previously disconnected from the leader and the other followers so it is not up to date
        let follower_with_outdated_log = followers[3];
        let keep_constrained_connections = vec![(follower_with_outdated_log, followers[0]), (follower_with_outdated_log, followers[1]), (follower_with_outdated_log, followers[2])];

        for pid_a in SERVERS {
            for pid_b in SERVERS {
                if pid_a != pid_b && !keep_constrained_connections.contains(&(pid_a, pid_b)) && !keep_constrained_connections.contains(&(pid_b, pid_a)){
                    delete_connections(pid_a, pid_b, &nodes);
                } else {
                    // reestablish connection
                    let connection_a = nodes.get(&pid_a).unwrap().0.lock().unwrap().disconnected_connections.remove(&pid_b);
                    let connection_b = nodes.get(&pid_b).unwrap().0.lock().unwrap().disconnected_connections.remove(&pid_a);
                    match connection_a {
                        Some(connection) => {
                            nodes.get(&pid_a).unwrap().0.lock().unwrap().connections.insert(pid_b, connection);
                        }
                        None => { }
                    }
                    match connection_b {
                        Some(connection) => {
                            nodes.get(&pid_b).unwrap().0.lock().unwrap().connections.insert(pid_a, connection);
                        }
                        None => { }
                    }
                }
            }
        }

        std::thread::sleep(WAIT_LEADER_TIMEOUT);
        print_nodes(&nodes);

        // Check if system makes progress, i.e. new leader is elected and transactions are committed
        
        let new_leaders = find_leaders(&nodes);

        // Only the QC follower can become the new leader, even if doesn't have most updated log
        let is_leader = nodes.get(&follower_with_outdated_log).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().is_leader();
        assert_eq!(is_leader, true, "Quorum connected follower should be elected as leader now");

        // logs should now be synced, the new leader has to replay the transaction from the more uptodate quorum
        check_replication(&nodes, &tx_ids_data);


        // commit tx to the new leader and check if it gets replicated and if all the old data is there
        tx_ids_data.extend(commit_transactions(follower_with_outdated_log, &nodes, 1)); 


        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);

        check_replication(&nodes, &tx_ids_data);

        print_nodes(&nodes);
        println!();

    }


    #[test]
    fn test_four_simulate_partial_connectivity_quorumloss() {
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);

        let mut transactions_ids_data: HashMap<String, String> = HashMap::new();

        // all nodes should have 4 connections in the beginning
        for pid in SERVERS {
            assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), SERVERS.len() - 1);
        }

        std::thread::sleep(WAIT_LEADER_TIMEOUT);
        println!();

        println!("All nodes are connected to one another");

        print_nodes(&nodes);

        let leader = find_leader(&nodes.get(&1).unwrap().0.lock().unwrap());
        transactions_ids_data.extend(commit_transactions(leader, &nodes, 1));

        println!("Leader (node {:?}) committed a transaction", leader);
        print_nodes(&nodes);

        // select any node that is not a leader
        let mut random_follower_idx = 0;
        let mut follower = SERVERS[random_follower_idx];
        while (nodes.get(&follower).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().is_leader()) {
            random_follower_idx += 1;
            follower = SERVERS[random_follower_idx];
        }
        assert!(nodes.get(&follower).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().is_leader() == false);
        // quorum loss connection scenario: follower -> 2, follower -> 3, follower -> 4, follower -> 5
        let keep_connections = vec![(follower, 1), (follower, 2), (follower, 3), (follower, 4), (follower, 5)];
        // delete all other connections
        for pid_a in SERVERS {
            for pid_b in SERVERS {
                if pid_a != pid_b && !keep_connections.contains(&(pid_a, pid_b)) && !keep_connections.contains(&(pid_b, pid_a)){
                    delete_connections(pid_a, pid_b, &nodes);
                }
            }
        }
        // check that the connections are as expected, first node should have 4, others have 1 connections
        for pid in SERVERS {
            if (pid == follower) {
                assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), 4);
            } else {
                assert_eq!(nodes.get(&pid).unwrap().0.lock().unwrap().connections.len(), 1);
            }
        }
        println!("Established quorumloss scenario:");
        println!("The only connections now are the connections between node {:?} and the others", follower);
        println!();

        // Check if system makes progress, i.e. new leader is elected and transactions are committed
        std::thread::sleep(WAIT_LEADER_TIMEOUT); // Wait for leader to get re-elected
        
        let new_leaders = find_leaders(&nodes);
        let new_leader = new_leaders[0];
        println!("The new leader is {:?}", new_leader);
        assert!(new_leaders.len() >= 1, "There should be a leader elected");

        print_nodes(&nodes);

        // check if QC follower gets elected as leader now
        let is_leader = nodes.get(&follower).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().is_leader();
        assert_eq!(is_leader, true, "Quorum connected follower should be elected as leader now");
        println!("QC node {:?} is elected as leader", follower);

        transactions_ids_data.extend(commit_transactions(follower, &nodes, 1));
        std::thread::sleep(WAIT_REPLICATION_TIMEOUT);
        check_replication(&nodes, &transactions_ids_data);

        println!("Transaction commited by leader {:?} gets replicated correctly", follower);
        println!();

        print_nodes(&nodes);
    }

    fn find_leaders(nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>) -> Vec<NodeId> {
        let mut leaders = vec![];
        for (_, (node, _)) in nodes {
            // check of node is the leader and if it is still running (might think it is leader if it has been stopped)
            if node.lock().unwrap().omni_durability.lock().unwrap().is_leader() {
                leaders.push(*node.lock().unwrap().node_id());
            }
        }
        leaders
    }

    fn find_leader(node: &Node) -> NodeId {
        node.omni_durability.lock().unwrap().get_current_leader().unwrap()
    }

    fn print_nodes(nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>) {
        println!("The nodes are:");
        for (_, (node, _)) in nodes {
            println!("{}", node.lock().unwrap().to_string());
        }
        println!();
    }

    fn commit_transactions(leader: NodeId, nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>, tx_amount: u16) -> HashMap<String, String> {
        // hash map of transactions
        let mut tx_ids_data: HashMap<String, String> = HashMap::new();

        for i in 0..tx_amount {
            // create a transaction
            let tx_key = format!("TxKey{}{}", i, SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("Time should not run backwards").as_nanos());
            let tx_data = format!("TxData{}", i);
            println!("Creating transaction with key: {}", tx_key);
            tx_ids_data.insert(tx_key.clone(), tx_data.clone());
            let mut tx = nodes.get(&leader).unwrap().0.lock().unwrap().begin_mut_tx().unwrap();
            tx.set((tx_key).to_string(), tx_data.to_string());
            
            // commit the transaction
            let result = nodes.get(&leader).unwrap().0.lock().unwrap().commit_mut_tx(tx);
            assert_eq!(result.is_ok(), true);
            std::thread::sleep(WAIT_COMMIT_TIMEOUT);
        }
        tx_ids_data
    }

    fn check_replication(nodes: &HashMap<NodeId, (Arc<Mutex<Node>>, JoinHandle<()>)>, tx_ids_data: &HashMap<String, String>) {
        let mut leaders: Vec<NodeId> = vec![];
        for pid in SERVERS {
            if (!nodes.contains_key(&pid)) {
                continue;
            }
            let node = nodes.get(&pid).unwrap().0.lock().unwrap();
            leaders.push(find_leader(&node));
        }
        // find the most common element in the leaders vec
        let mut leader_counts: HashMap<NodeId, u16> = HashMap::new();
        for leader in leaders {
            let count = leader_counts.entry(leader).or_insert(0);
            *count += 1;
        }
        let (leader, _) = leader_counts.iter().max_by_key(|&(_, count)| count).unwrap();
        for (tx_key , tx_data) in tx_ids_data {
            for pid in SERVERS {
                if (!nodes.contains_key(&pid)) {
                    continue;
                }
                let mut tx = nodes.get(&pid).unwrap().0.lock().unwrap().begin_tx(DurabilityLevel::Replicated);
                let tx_data = tx.get(&tx_key.to_string());
                match tx_data.clone() {
                    Some(data) => assert_eq!(data, tx_data.unwrap_or_default()),
                    None => {
                        if (nodes.get(&pid).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().get_current_leader().unwrap() != *leader) {
                            println!("Node {} is not following the leader, so it is not expected to have the data yet", pid)
                        } else if (nodes.get(&pid).unwrap().0.lock().unwrap().connections.len() == 0) {
                            println!("Node {} does not have any connections so it is not expected to have the data yet", pid)
                        } 
                        else {
                            assert!(!nodes.get(&pid).unwrap().0.lock().unwrap().is_running(), "Data should be replicated now at node {} Data key: {}", pid, tx_key)
                        }
                        
                    }
                }
                nodes.get(&pid).unwrap().0.lock().unwrap().release_tx(tx);  
            }
        }
    }

}