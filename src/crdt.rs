use std::{collections::{HashMap, HashSet}, hash::{Hash, Hasher}};
use serde_json::{json, Value};
use serde::{Serialize, Deserialize};
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Item {
    item_name: String,
    target: u64,
    bought: u64,
    replica: String,
    timestamp: u64,
    deleted: bool,
}

impl Hash for Item {
    fn hash<H: Hasher>(&self, state: &mut H){
        self.item_name.hash(state);
    }
}

impl PartialEq for Item {
    fn eq(&self, other: &Item)->bool{
        self.item_name == other.item_name
    }
}

impl Item{
    fn to_json(&self)->serde_json::Value{
        let json = json!({
            "item_name": self.item_name,
            "target": self.target,
            "bought": self.bought,
            "replica": self.replica,
            "timestamp": self.timestamp,
            "deleted": self.deleted
        });
        json
    }
}

impl PartialEq for AWSet {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.s == other.s
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AWSet {
    #[serde(skip)]
    pub id: String,
    s: HashSet<Item>, // The set of items
    c: HashMap<String, u64>, // Causal context, mapping replica to max timestamp
}

impl AWSet {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            s: HashSet::new(),
            c: HashMap::new(),
        }
    }

    pub fn set_id(&mut self, id: String){
        self.id = id;
    }
          
    pub fn from_json(&mut self, json: serde_json::Value ){
        let (first_key, first_value) = if let Some(obj) = json.as_object() {
            obj.iter().next().map(|(key, value)| (key, value))
        } else {
            None
        }.expect("JSON object is empty or invalid");
        self.id = first_key.to_owned();
        let s_array = first_value["s"].as_array().unwrap();
        for item in s_array{
            let new_item = Item{
                item_name: item["item_name"].as_str().unwrap().to_string(),
                target: item["target"].as_u64().unwrap(),
                bought: item["bought"].as_u64().unwrap(),
                replica: item["replica"].as_str().unwrap().to_string(),
                timestamp: item["timestamp"].as_u64().unwrap(),
                deleted: item["deleted"].as_bool().unwrap()
            };
            self.s.insert(new_item);
        }
        let c_array = first_value["c"].as_array().unwrap();
        for context in c_array{
            self.c.insert(context["replica"].as_str().unwrap().to_string(), context["timestamp"].as_u64().unwrap());
        }

    }
    

    pub fn to_json(&self)->Value{
        let mut s_array: Vec<Value> = Vec::new();
        for item in &self.s{
            s_array.push(item.to_json());
        }
        let mut c_array: Vec<Value> = Vec::new();
        for item in &self.c{
            let context = json!({"replica":item.0,"timestamp":item.1});
            c_array.push(context)
        }
        let final_json = json!({&self.id:{"s":s_array,"c":c_array}});
        final_json
    }

    pub fn add(&mut self, item_name: &str, target: u64, bought: u64, replica: &str, deleted: bool) {
        let next_timestamp = self.c.get(replica).cloned().unwrap_or(0) + 1;
        let item = Item {
            item_name: item_name.to_string(),
            target: target,
            bought: bought,
            replica: replica.to_string(),
            timestamp: next_timestamp,
            deleted: deleted
        };
        self.s.insert(item);
        self.c.insert(replica.to_string(), next_timestamp);
    }

    pub fn remove(&mut self, item_name: &str, replica: &str) {
        let current_timestamp = self.c.get(replica).copied().unwrap_or(0);

        let item_to_remove = Item {
            item_name: item_name.to_string(),
            target: 0,
            bought: 0,
            replica: "".to_string(),
            timestamp: 0,
            deleted: false,
        };

        let removed_item = self.s.take(&item_to_remove).unwrap();

        let new_item = Item{
            item_name: item_name.to_string(),
            target: removed_item.target,
            bought: removed_item.bought,
            replica: replica.to_string(),
            timestamp: current_timestamp +1,
            deleted: true
        };

        self.s.insert(new_item);

        self.c.insert(replica.to_string(), current_timestamp + 1);
    }
    

    pub fn update_item_amounts(&mut self, item_name: &str, new_target: u64, new_bought: u64, replica: &str) {
        // Get the current timestamp for the given replica, or default to 0
        let current_timestamp = self.c.get(replica).copied().unwrap_or(0);

        let item_to_remove = Item {
            item_name: item_name.to_string(),
            target: 0,
            bought: 0,
            replica: "".to_string(),
            timestamp: 0,
            deleted: false,
        };

        let removed_item = self.s.take(&item_to_remove).unwrap();

        let new_item = Item{
            item_name: item_name.to_string(),
            target: new_target,
            bought: new_bought,
            replica: replica.to_string(),
            timestamp: current_timestamp +1,
            deleted: removed_item.deleted
        };

        self.s.insert(new_item);

        self.c.insert(replica.to_string(), current_timestamp + 1);
    }

    pub fn contains(&self, item_name: &str) -> bool {
        self.s.iter().any(|item| item.item_name == item_name)
    }


    pub fn merge(&mut self, other: &AWSet) {
        let mut new_s = HashSet::new();
        println!("causal context in c:{}", other.c.len());

        // keep items from the current set not known by the other causal context
        for item in &self.s {
            if !other.c.contains_key(&item.replica)|| other.c[&item.replica] < item.timestamp {
                new_s.insert(item.clone());
                println!("added an item from self");
            }
        
        }

        // keep items from the other set not known by the current causal context
        for item in &other.s {
            if !self.c.contains_key(&item.replica) || self.c[&item.replica] < item.timestamp {
                new_s.insert(item.clone());
                println!("added an item from other");
            }
        }

        // common items differing in relpica or timestamp
        for item in &self.s {
            if let Some(other_item) = other.s.iter().find(|&i| i.item_name == item.item_name) {
                // if the items recplica is in the others context but not in self, save from other
                if !self.c.contains_key(&other_item.replica) {
                    new_s.insert(other_item.clone());
                    println!("kept item from other (replica not in self's causal context)");
                } else { // if its in both
                    // chose the more recent one
                    if item.timestamp > other_item.timestamp {
                        new_s.insert(item.clone()); // Keep the item from self
                        println!("kept item from self (timestamp: {})", item.timestamp);
                    } else {
                        new_s.insert(other_item.clone()); // Keep the item from other
                        println!("kept item from other (timestamp: {})", other_item.timestamp);
                    }
                }
            }
        }

        // Update causal context by taking the max timestamps
        for (replica, timestamp) in &other.c {
            self.c
                .entry(replica.clone())
                .and_modify(|t| *t = (*t).max(*timestamp))
                .or_insert(*timestamp);
        }

        // Update the set
        self.s = new_s;
    }

    pub fn elements(&self) -> Vec<&Item> {
        self.s.iter().collect()
    }
}
