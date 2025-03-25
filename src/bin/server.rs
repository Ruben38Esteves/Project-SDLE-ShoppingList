use std::{collections::HashMap, env, fs};
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use slde::crdt::AWSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct TimeoutTable {
    server_timestamp: HashMap<String, SystemTime>,
}

struct Servers {
    ports: HashMap<String, String>,
    context: zmq::Context,
}

impl TimeoutTable {
    // this function updates the time saved in the table to the now time
    fn update_timestamp(&mut self, server_id: &str) {
        self.server_timestamp
            .insert(server_id.to_string(), SystemTime::now());
    }

    // this function verifies if the server is timed out
    fn is_timed_out(&self, server_id: &str, timeout_duration: Duration) -> bool {
        let timestamp = self.server_timestamp.get(server_id).unwrap_or(&UNIX_EPOCH);
        let elapsed = SystemTime::now()
            .duration_since(*timestamp)
            .unwrap_or_default();
        elapsed <= timeout_duration
    }
}

impl Servers {
    // abstraction to send messages to other workers
    fn send_to_worker(&self, server_id: String, message: String) -> String {
        let requester = self.context.socket(zmq::REQ).unwrap();
        let address = format!("tcp://localhost:{}", self.ports[&server_id]);
        assert!(requester.connect(&address).is_ok());
        requester.send(&message, 0).unwrap();

        let response = match requester.recv_msg(0) {
            Ok(x) => x,
            Err(e) => {
                println!("Failed to send message: {}", e);
                return "".to_string();
            }
        };
        assert!(requester.disconnect(&address).is_ok());
        println!("Got response:{}", response.as_str().unwrap());
        let response_string = match response.as_str() {
            Some(x) => x,
            None => "",
        };
        return response_string.to_string();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: cargo run --bin server <id>")
    }
    let id = &args[1];
    let ports_contents = fs::read_to_string("data/ports.json")?;
    let json: Value = match serde_json::from_str(&ports_contents) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Error parsing ports JSON: {:?}", e);
            panic!();
        }
    };

    let json_clone = json.clone();

    // create the port HashMap
    let ports_hashmap: HashMap<String, String> = match json_clone {
        Value::Object(obj) => obj
            .into_iter()
            .filter_map(|(k, v)| match v {
                Value::String(s) => Some((k, s)),
                _ => None, // Ignore non-string values
            })
            .collect(),
        _ => panic!("Expected a JSON object"),
    };

    let mut shopping_list: HashMap<String, AWSet> = load_shopping_list(id.to_owned());

    // create the Timestamps HashMap
    let mut timestamps: HashMap<String, SystemTime> = HashMap::new();

    let servers = Servers {
        ports: ports_hashmap,
        context: zmq::Context::new(),
    };

    for i in 0..7 {
        let server_id = i.to_string();
        timestamps.insert(server_id, UNIX_EPOCH);
    }

    let mut timeout_table = TimeoutTable {
        server_timestamp: timestamps,
    };

    // connect to proxy
    let context: zmq::Context = zmq::Context::new();
    let proxy_responder: zmq::Socket = context.socket(zmq::REP).unwrap();
    assert!(proxy_responder.connect("tcp://localhost:5560").is_ok());

    // socket to recieve messages from other servers
    let server_responder = context.socket(zmq::REP).unwrap();
    let my_ip = format!("tcp://*:{}", servers.ports[id]);
    println!("my address is: {}", my_ip);
    assert!(server_responder.bind(&my_ip).is_ok());

    let items = &mut [
        proxy_responder.as_poll_item(zmq::POLLIN),
        server_responder.as_poll_item(zmq::POLLIN),
    ];

    loop {
        zmq::poll(items, -1).unwrap();
        // proxy_responder
        // here, messages come from proxy
        if items[0].is_readable() {
            loop {
                let string = match proxy_responder.recv_msg(0) {
                    Ok(x) => x,
                    Err(_e) => {
                        println!("Failed to extract message!");
                        break;
                    }
                };
                let more = if proxy_responder.get_rcvmore().unwrap() {
                    zmq::SNDMORE
                } else {
                    0
                };

                let message = string.as_str().unwrap();
                
                //read message
                let message_parsed = if message.starts_with("READ") {
                    &message[4..]
                } else {
                    // assume its write
                    message
                };

                //case of read message
                if message.starts_with("READ") {
                    let list_owner_id = match get_owner_id(message_parsed.to_string()) {
                        Some(x) => x,
                        None => {
                            println!("Failed to retrieve owner");
                            break;
                        }
                    };
                    if &list_owner_id == id {
                        let _ = dynamo_style_read(&servers, message_parsed, id);
                    } else {
                        println!(
                            "I am not the owner of the list, sending to server reader {}",
                            list_owner_id
                        );
                        let response = servers.send_to_worker(list_owner_id, string.as_str().unwrap().to_string());
                        proxy_responder.send(&response, 0).unwrap();
                    }
                    break;
                }

                // case of write message

                // get the json file
                let json: Value = match serde_json::from_str(string.as_str().unwrap()) {
                    Ok(value) => value,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {:?}", e);
                        panic!();
                    }
                };
                // get the list id
                let mut list_id = String::new();
                if let Some(first_key) = get_first_key(&json) {
                    list_id = first_key;
                } else {
                    println!("Not a Json file");
                }
                // get owner id
                let list_owner_id = match get_owner_id(list_id) {
                    Some(x) => x,
                    None => {
                        println!("Failed to retrieve owner");
                        break;
                    }
                };
                //compare with self
                if &list_owner_id != id {
                    // send message to owner
                    let response =
                        servers.send_to_worker(list_owner_id, string.as_str().unwrap().to_string());
                    proxy_responder.send(&response, 0).unwrap();
                }

                if more == 0 {
                    break;
                }
            }
        }
        // crdt resonder
        // here, messages come from other servers
        if items[1].is_readable() {
            loop {
                let string = match server_responder.recv_msg(0) {
                    Ok(x) => x,
                    Err(_e) => break,
                };

                let more = if server_responder.get_rcvmore().unwrap() {
                    zmq::SNDMORE
                } else {
                    0
                };

                let messagee = string.as_str().unwrap();
                // check type of request
                let rest_of_message = if messagee.starts_with("WRITE") {
                    &messagee[5..]
                } else if messagee.starts_with("READ") {
                    &messagee[4..]
                } else if messagee.starts_with("REROUTE") {
                    &messagee[7..]
                } else {
                    messagee
                };

                if messagee.starts_with("WRITE") {
                    // create awset
                    let mut awset = AWSet::new();
                    let json_value: Value = serde_json::from_str(&rest_of_message).unwrap();
                    awset.from_json(json_value);
                    // save locally
                    shopping_list.insert(awset.id.clone(), awset.clone());
                    //write to local storage
                    let _ = write_shopping_list_to_file(id, &shopping_list);
                    server_responder.send("Received", 0).unwrap();
                    break;

                } else if messagee.starts_with("REROUTE") {
                    // a server was found to be offline, this node is tasked with sending the
                    // write to the offline node once its online

                    let id_send = rest_of_message.chars().next().unwrap();
                    let send_message = &rest_of_message[1..];
                    let response =
                        servers.send_to_worker(id_send.to_string(), send_message.to_string());
                    server_responder.send(&response, 0).unwrap();
                    break;

                } else if messagee.starts_with("READ") {
                    let list_owner_id = match get_owner_id(rest_of_message.to_string()) {
                        Some(x) => x,
                        None => {
                            println!("Failed to retrieve owner");
                            break;
                        }
                    };
                    // if node is the owner, send to other nodes 
                    if list_owner_id == *id {
                        let result: String = match dynamo_style_read(&servers, rest_of_message, id)
                        {
                            Ok(out) => out,
                            Err(e) => e.to_string(),
                        };
                        server_responder.send(result.as_str(), 0).unwrap();
                        break;
                    }
                    //else, read locally and respond
                    let id_list = rest_of_message.trim();
                    let shopping_lists = &shopping_list;

                    if let Some(list) = shopping_lists.get(id_list) {
                        let response = list.to_json().to_string();
                        server_responder.send(&response, 0).unwrap();
                    } else {
                        // Handle case where the list ID does not exist
                        server_responder.send("List not found", 0).unwrap();
                    }
                    break;
                }

                // case where message is a write

                let json: Value = serde_json::from_str(rest_of_message).unwrap();

                let mut key = String::new();
                if let Some(first_key) = get_first_key(&json) {
                    key = first_key;
                } else {
                    println!("Not a Json file");
                }
                let list_id = &key;
                //get the owner
                let list_owner_id = match get_owner_id(list_id.to_string()) {
                    Some(x) => x,
                    None => {
                        println!("Failed to retrieve owner");
                        break;
                    }
                };

                if &list_owner_id == id {
                    let mut owner_awset = AWSet::new();
                    owner_awset.from_json(json.clone());
                    if let Some(local_awset) = shopping_list.get(&key) {
                        owner_awset.merge(local_awset);
                    }
                    shopping_list.insert(key.clone(), owner_awset.clone());

                    let _ = write_shopping_list_to_file(id, &shopping_list);

                    let result: String = match send_to_other_nodes(&servers, &mut timeout_table, id, &owner_awset){
                            Ok(out) => out,
                            Err(e) => e.to_string(),
                        };
                        server_responder.send(result.as_str(), 0).unwrap();
                }
                if more == 0 {
                    break;
                }
            }
        }
    }
}

fn get_first_key(json: &Value) -> Option<String> {
    if let Value::Object(obj) = json {
        return obj.keys().next().map(|k| k.to_string());
    }
    None
}

// if we want to change the way to calculate the owner, we only need to change this function
fn get_owner_id(list_id: String) -> Option<String> {
    let list_id_int = match list_id.parse::<u32>() {
        Ok(x) => x,
        Err(e) => {
            println!("List id not a number: {}", e);
            return None;
        }
    };
    let owner_int = list_id_int % 6;
    let owner_id = format!("{}", owner_int);
    Some(owner_id)
}

fn write_shopping_list_to_file(
    server_id: &str,
    shopping_list: &HashMap<String, AWSet>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = format!("public/data_{}.json", server_id);
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&file_path)?;

    let mut root = serde_json::Map::new();
    for (key, awset) in shopping_list {
        let json_value = awset.to_json();
        root.insert(key.clone(), json_value[key].clone());
    }

    let json_data = serde_json::Value::Object(root);
    file.write_all(serde_json::to_string_pretty(&json_data)?.as_bytes())?;
    Ok(())
}

fn send_to_other_nodes(
    servers: &Servers,
    timeout_table: &mut TimeoutTable,
    server_id: &String,
    awset: &AWSet,
) -> Result<String, &'static str> {
    let server_count = servers.ports.len() as i32;

    let id: i32 = server_id.parse().unwrap();

    let json_string = awset.to_json().to_string();

    let sent_message = "WRITE".to_owned() + &json_string;

    let mut timeout_list = vec![];

    let n: i32 = 3;

    let mut number: i32 = id + 1;

    let timeout = Duration::from_secs(100);

    let mut success_send: i32 = 0;
    let server_list = (0..server_count)
        .cycle()
        .skip((id + 1) as usize)
        .take((n - 1) as usize)
        .collect::<Vec<_>>();
        

    while success_send < (n - 1) && (number) != id {
        if number >= servers.ports.len() as i32 {
            number = 0;
        }
        // println!("NUMBER: {}", number);
        // println!("ID, {}", id);
        // println!("SUCC, {}", success_send);
        if server_list.contains(&number)
            && !timeout_table.is_timed_out(&number.to_string(), timeout){   
            let result = servers.send_to_worker(number.to_string(), sent_message.clone());
            println!("SENT TO, {}", result.as_str());
            let match_result = result.as_str();

            match match_result {
                "Received" => { success_send += 1; }
                _ => {
                    println!("Error sending to worker, getting in timeout");
                    timeout_table.update_timestamp(&number.to_string());
                }
            }
        } else if server_list.contains(&number) {
            timeout_list.push(number);
        } else {
            if !timeout_table.is_timed_out(&number.to_string(), timeout) {
                if !timeout_list.is_empty() {
                    let real_node = timeout_list.remove(0);

                    let message = format!("REROUTE{}", real_node.to_string());

                    let final_message = message + &sent_message;

                    let result = servers.send_to_worker(number.to_string(), final_message);

                    let match_result = result.as_str();

                    match match_result {
                        "Received" => {
                            success_send += 1;
                        }
                        _ => {
                            println!("Error sending to worker, getting in timeout");
                            timeout_table.update_timestamp(&number.to_string());
                        }
                    }
                }
            }
        }
        number += 1;
    }

    if success_send < 2 {
        println!("Error no servers available");
        return Err("Not enough successes");
    }
    Ok("Success".to_string())
}

fn load_shopping_list(my_id: String) -> HashMap<String, AWSet> {
    let data_location = format!("public/data_{}.json", my_id);
    let data_content = match fs::read_to_string(data_location) {
        Ok(x) => x,
        Err(e) => {
            println!("{}", e);
            panic!()
        }
    };

    let json: Value = match serde_json::from_str(&data_content) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Error parsing ports JSON: {:?}", e);
            panic!();
        }
    };
    let mut shopping_lists: HashMap<String, AWSet> = HashMap::new();
    let lists = match json.as_object() {
        Some(x) => x,
        None => panic!("Oh no"),
    };
    for list_id in lists.keys() {
        let mut new_awset = AWSet::new();
        let new_json = json!({list_id:json[list_id]});
        new_awset.from_json(new_json.clone());
        shopping_lists.insert(list_id.to_owned(), new_awset);
    }
    shopping_lists
}

fn dynamo_style_read(servers: &Servers, key: &str, server_id: &String) -> Result<String, &'static str> {
    let mut responses: Vec<AWSet> = Vec::new();
    let server_count = servers.ports.len() as i32;
    let mut repair_list: Vec<i32> = Vec::new();
    let id: i32 = server_id.parse().unwrap();
    let worker_lists = load_shopping_list(server_id.to_string());
    let mut worker_list = match worker_lists.get(key){
        Some(x) => x.clone(),
        None => return Ok("NONE".to_string())
    };
    let quorum: i32 = 2;
    let n: i32 = 3;
    let replicas = (0..server_count)
        .cycle()
        .skip((id + 1) as usize)
        .take((n - 1) as usize)
        .collect::<Vec<_>>();
    let mut successful_reads = 0;
    for replica in replicas {
        let read_message = format!("READ{}", key);
        match servers.send_to_worker(replica.to_string().clone(), read_message) {
            response if !response.is_empty() => {
                // Parse the response as JSON
                if let Ok(json) = serde_json::from_str::<Value>(&response) {
                    let mut awset = AWSet::new();
                    awset.from_json(json);
                    responses.push(awset.clone());
                    repair_list.push(replica);
                    worker_list.merge(&awset);
                    successful_reads += 1;

                    // Stop early if quorum is met
                    if successful_reads >= quorum {
                        break;
                    }
                } else {
                    println!("Error parsing JSON from replica {}", replica);
                }
            }
            _ => {
                println!("Failed to read from replica {}", replica);
            }
        }
    }

    // Update the local shopping list with the merged result
    let mut shopping_lists = load_shopping_list(server_id.to_string());
    shopping_lists.insert(key.to_string(), worker_list.clone());
    let _ = write_shopping_list_to_file(server_id, &shopping_lists);

    // Repair replicas if needed
    for i in 0..repair_list.len() {
        if worker_list != responses[i] {
            let json_string = worker_list.to_json().to_string();
            let write_message = format!("WRITE{}", json_string);
            let response = servers.send_to_worker(repair_list[i].to_string(), write_message);
            if response != "Received" {
                println!("Failed to repair replica {}", repair_list[i]);
            }
        }
    }
    let string_aw = worker_list.to_json().to_string();
    // Check quorum
    if successful_reads >= quorum {
        return Ok(string_aw); // Return the entire shopping list as a string
    } else {
        println!("reads succ {}", successful_reads);
        return Err("Not enough successful responses");
    }
}
