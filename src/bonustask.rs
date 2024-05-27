use omnipaxos::macros::UniCacheEntry;
use omnipaxos::macros::Entry;
use omnipaxos::messages;
use omnipaxos::unicache::UniCache;
use omnipaxos::unicache::MaybeEncoded;
use omnipaxos::storage::Entry;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use omnipaxos::macros::UniCacheSerdeEntry;

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Hash)]
pub struct AccessLogEntry {
    pub ip: String,
    pub day: u8,
    pub month: String,
    pub year: u16,
    pub time: String,
    pub offset: i16,
    pub method: String,
    pub url: String,
    pub protocol: String,
    pub status_code: u16,
    pub response_size: u16,
}


#[cfg(test)]
pub mod unicache_tests {

    use std::fs::File;
    use std::io::{self, BufRead};
    use std::path::Path;
    use regex::Regex;
    use crate::bonustask::AccessLogEntry;

    use crate::datastore::example_datastore::Tx;
    use crate::tests::tests::print_nodes;
    use crate::utils::{create_runtime, delete_connections, reconnect_nodes, spawn_nodes};
    use omnipaxos::util::NodeId;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref RE: Regex = Regex::new("^(?P<number>\\d+\\.\\d+\\.\\d+\\.\\d+) - - \\[(?P<day>\\d{2})/(?P<month>[a-zA-Z]{3})/(?P<year>\\d{4}):(?P<time>\\d{2}:\\d{2}:\\d{2}) (?P<offset>[+-]\\d{4})\\] \\\"(?P<method>\\w+) (?P<link>.*?) HTTP/1\\.1\\\" (?P<status_code>\\d+) (?P<response_size>-|\\d+)").unwrap();
    }

    // 10.223.157.186 - - [15/Jul/2009:14:58:59 -0700] "GET / HTTP/1.1" 403 202


    //#[derive(Clone, Debug, Entry)]

    // Function to parse a single log line
    fn parse_log_line(line: &str) -> Option<AccessLogEntry> {
        // Regular expression to match the log line format

        if let Some(caps) = RE.captures(line) {
            Some(AccessLogEntry {
                ip: caps["number"].to_string(),
                day: caps["day"].parse::<u8>().unwrap(),
                month: caps["month"].to_string(),
                year: caps["year"].parse::<u16>().unwrap(),
                time: caps["time"].to_string(),
                offset: caps["offset"].parse::<i16>().unwrap(),
                method: caps["method"].to_string(),
                url: caps["link"].to_string(),
                protocol: "HTTP/1.1".to_string(),
                status_code: caps["status_code"].parse::<u16>().unwrap(),
                response_size: caps["response_size"].parse::<u16>().unwrap_or(0),
            })
        } else {
            None
        }
    }

    // Function to parse the log file
    fn parse_log_file(path: &str) -> Result<Vec<AccessLogEntry>, std::io::Error> {
        let file = File::open(path)?;
        let reader = io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            match parse_log_line(line.as_ref().unwrap()) {
                Some(entry) => {
                    // Handle the successful case
                    entries.push(entry);
                },
                None => {
                    println!("Failed to parse log line: {}", line.unwrap());
                }
            }
        }
        Ok(entries)
    }


    #[test]
    fn load_dataset_print_first_entry() -> Result<(), std::io::Error> {
        //let path = "/Users/sennevandeneynde/Downloads/Vakken Stockholm 1/Distributed Systems/OmniSpacetimeDB-main/src/data/bonustask_access_log_smaller.txt";
        let path = "./src/data/bonustask_access_log_smaller.txt";
        let messages = parse_log_file(path)?;
        println!("{:?}", messages[0]);
        Ok(())
    }

    #[test]
    fn load_dataset_check_unique_values() -> Result<(), std::io::Error> {
        //let path = "/Users/sennevandeneynde/Downloads/Vakken Stockholm 1/Distributed Systems/OmniSpacetimeDB-main/src/data/bonustask_access_log_smaller.txt";
        let path = "./src/data/bonustask_access_log_smaller.txt";
        let messages = parse_log_file(path)?;
        // count all unique values of all fields
        let mut unique_ips = std::collections::HashMap::new();
        let mut unique_days = std::collections::HashMap::new();
        let mut unique_months = std::collections::HashMap::new();
        let mut unique_years = std::collections::HashMap::new();
        let mut unique_times = std::collections::HashMap::new();
        let mut unique_offsets = std::collections::HashMap::new();
        let mut unique_methods = std::collections::HashMap::new();
        let mut unique_urls = std::collections::HashMap::new();
        let mut unique_protocolls = std::collections::HashMap::new();
        let mut unique_status_codes = std::collections::HashMap::new();
        let mut unique_response_sizes = std::collections::HashMap::new();


        for message in messages.clone() {
            let entry = unique_ips.entry(message.ip).or_insert(0);
            *entry += 1;
            let entry = unique_days.entry(message.day.to_string()).or_insert(0);
            *entry += 1;
            let entry = unique_months.entry(message.month).or_insert(0);
            *entry += 1;
            let entry = unique_years.entry(message.year.to_string()).or_insert(0);
            *entry += 1;
            let entry = unique_times.entry(message.time).or_insert(0);
            *entry += 1;
            let entry = unique_offsets.entry(message.offset.to_string()).or_insert(0);
            *entry += 1;
            let entry = unique_methods.entry(message.method).or_insert(0);
            *entry += 1;
            let entry = unique_urls.entry(message.url).or_insert(0);
            *entry += 1;
            let entry = unique_protocolls.entry(message.protocol).or_insert(0);
            *entry += 1;
            let entry = unique_status_codes.entry(message.status_code.to_string()).or_insert(0);
            *entry += 1;
            let entry = unique_response_sizes.entry(message.response_size.to_string()).or_insert(0);
            *entry += 1;
        }
        // print the number of unique values in each array
        println!("Unique ips: {}", unique_ips.len());
        println!("Unique days: {}", unique_days.len());
        println!("Unique months: {}", unique_months.len());
        println!("Unique years: {}", unique_years.len());
        println!("Unique times: {}", unique_times.len());
        println!("Unique offsets: {}", unique_offsets.len());
        println!("Unique methods: {}", unique_methods.len());
        println!("Unique urls: {}", unique_urls.len());
        println!("Unique protocolls: {}", unique_protocolls.len());
        println!("Unique status codes: {}", unique_status_codes.len());
        println!("Unique response sizes: {}", unique_response_sizes.len());
        // print the possible compression of each field using reoccuring values
        println!("Compression of ips: {}", messages.len() as f64 / unique_ips.len() as f64);
        println!("Compression of days: {}", messages.len() as f64 / unique_days.len() as f64);
        println!("Compression of months: {}", messages.len() as f64 / unique_months.len() as f64);
        println!("Compression of years: {}", messages.len() as f64 / unique_years.len() as f64);
        println!("Compression of times: {}", messages.len() as f64 / unique_times.len() as f64);
        println!("Compression of offsets: {}", messages.len() as f64 / unique_offsets.len() as f64);
        println!("Compression of methods: {}", messages.len() as f64 / unique_methods.len() as f64);
        println!("Compression of urls: {}", messages.len() as f64 / unique_urls.len() as f64);
        println!("Compression of protocolls: {}", messages.len() as f64 / unique_protocolls.len() as f64);
        println!("Compression of status codes: {}", messages.len() as f64 / unique_status_codes.len() as f64);
        println!("Compression of response sizes: {}", messages.len() as f64 / unique_response_sizes.len() as f64);
        Ok(())
    }

    use crate::datastore::TxOffset;
    use crate::datastore::tx_data::TxData;
    use crate::durability::DurabilityLevel;
    use crate::durability::omnipaxos_durability;
    use crate::durability::DurabilityLayer;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    #[test]
    fn benchmark_compressed() {
        #[cfg(feature = "project_use_unicache")]
        println!("--- USING UNICACHE ---");
        //let path = "/Users/sennevandeneynde/Downloads/Vakken Stockholm 1/Distributed Systems/OmniSpacetimeDB-main/src/data/bonustask_access_log_smaller.txt";
        let path = "./src/data/bonustask_access_log_smaller.txt";
        println!("Parsing log file");
        let messages_result = parse_log_file(path);

        // slice all messages execpt the first 5
       

        println!("Starting benchmark");

        const SERVERS: [NodeId; 5] = [1, 2, 3, 4, 5];
        const WAIT_LEADER_TIMEOUT: Duration = Duration::from_millis(400);
        let mut runtime = create_runtime();
        let nodes = spawn_nodes(&mut runtime, &SERVERS);


        std::thread::sleep(WAIT_LEADER_TIMEOUT);

        print_nodes(&nodes);


        let leader = &nodes.get(&1).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().get_current_leader().unwrap();
        let mut followers: Vec<NodeId> = SERVERS.to_vec();
        followers.retain(|&x| &x != leader);

        // Iterate over all messages and append each to omnipaxos
        match messages_result {
            Ok(mut messages) => {
                //messages = messages[..5].to_vec();
                let messages_length = messages.len();
                println!("Total amount of messages: {}", messages_length);

                let tx_offset = TxOffset(0); // Just to test
                let tx_data = TxData {
                    inserts: Arc::new([]),
                    deletes: Arc::new([]),
                    truncs: Arc::new([]),
                };
                for ale in messages{
                    //nodes.get(&leader).unwrap().0.lock().unwrap().omni_durability.lock().unwrap().append_tx(tx_offset, tx_data.clone(), ale);
                    match nodes.get(&leader).unwrap().0.lock() {
                        Ok(mut node) => {
                            match node.omni_durability.lock() {
                                Ok(mut omni) => {
                                    omni.append_tx(tx_offset, tx_data.clone(), ale);
                                },
                                Err(e) => {
                                    println!("Failed to lock node: {}", e);
                                }
                            }
                        },
                        Err(e) => {
                            println!("Failed to lock node: {}", e);
                        }
                    }
                    // not sure if we need a sleep here
                    std::thread::sleep(Duration::from_millis(1));
                }

                println!("Total message size: {}", nodes.get(&followers[0]).unwrap().0.lock().unwrap().received_messages_size);
                // Print total size of all received messages from random follower
                std::thread::sleep(WAIT_LEADER_TIMEOUT * 2);
                print_nodes(&nodes);
            },
            Err(e) => {
                println!("Failed to parse log file: {}", e);
            }
        }
    }

}
    