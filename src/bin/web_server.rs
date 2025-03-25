use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use actix_cors::Cors;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slde::crdt::AWSet;
use uuid::Uuid;
use std::fs::{self, OpenOptions};
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct Change {
    r#type: String,
    list_id: String,
    item_name: String,
    target: Option<u64>,
    bought: Option<u64>,
    replica: String
}



#[derive(Serialize, Deserialize)]
struct Changes {
    changes: Vec<Change>,
}

#[get("/generate_id")]
async fn generate_id() -> impl Responder {
    let user_id = Uuid::new_v4().to_string();
    user_id
}

#[get("/list.json/{id}")]
async fn get_list(id: web::Path<String>) -> impl Responder {
    println!("Looking for the list");
    // Read the contents of the JSON file
    let contents = match fs::read_to_string("public/list.json") {
        Ok(x) => x,
        Err(_e) => {
            read_from_servers(id.to_string())
        }
    };
    println!("{}",contents);

    

    // Parse the JSON contents
    let json: Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Error parsing JSON: {}", e));
        }
    };

    let correct_list = match json.get(id.to_string()){
        Some(_x) => true,
        None => false
    };

    if correct_list{
        return HttpResponse::Ok().json(json);
    }else{
        println!("incorrect list");
        read_from_servers(id.to_string());
        let contents = fs::read_to_string("public/list.json").unwrap();

        let json: Value = match serde_json::from_str(&contents) {
            Ok(value) => value,
            Err(e) => {
                return HttpResponse::InternalServerError().body(format!("Error parsing JSON: {}", e));
            }
        };
        return HttpResponse::Ok().json(json)
    }

    // Return an error if the list ID is not found or "s" is not available
    //HttpResponse::NotFound().body("No list associated with this ID.")
}

#[post("/changes")]
async fn add_change(change: web::Json<Change>) -> impl Responder {

    let contents = match fs::read_to_string("public/list.json"){
        Ok(x) => {
            println!("{}",x);
            x
        },
        Err(e) => {
            panic!("{}",e)
        }
    };
    let json: Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Error parsing ports JSON: {:?}", e);
            panic!();
        }
    };

    

    let mut shopping_list = AWSet::new();
    shopping_list.from_json(json.clone());

    match change.r#type.as_str(){
        "add" =>{
            println!("recieved an add request");
            shopping_list.add(&change.item_name, change.target.unwrap(), change.bought.unwrap(), &change.replica, false);
        },
        "remove" =>{
            println!("recieved a remove request");
            shopping_list.remove(&change.item_name, &change.replica);
        },
        "update" =>{
            println!("recieved an update request");
            shopping_list.update_item_amounts(&change.item_name, change.target.unwrap(), change.bought.unwrap(), &change.replica);

        }, 
        _=> println!("invalid change type")
    }

    let changed_json = shopping_list.to_json();
    let json_as_string = serde_json::to_string_pretty(&changed_json).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open("public/list.json").unwrap();
    file.write_all(json_as_string.as_bytes()).unwrap();
    let result = write_to_servers();
    println!("DID IT CHANGE THE SERVER? {}", result);
    "Change added successfully"
}

fn read_from_servers(list_id: String)->String{
    let context = zmq::Context::new();

    let requester = context.socket(zmq::REQ).unwrap();
    assert!(requester.connect("tcp://localhost:5559").is_ok());

    let contents = format!("READ{}",list_id);
    println!("requesting: {}", contents);
    requester.send(contents.as_str(), 0).unwrap();
    println!("Request sent!");

    let response_string = requester.recv_string(0).unwrap().unwrap();
    println!("Response:\n{}",response_string);

    if response_string =="NONE" {
        // list doesnt exist on server
        println!("list doesnt exist on server");
        let json_data = r#"
{
    "list_id": {
        "s": [
        ],
        "c": [
        ]
    }
}
        "#;
        let final_json_data = json_data.replace("list_id", &list_id);
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open("public/list.json").unwrap();
        file.write_all(final_json_data.clone().as_bytes()).unwrap();
        return final_json_data;
    }else{
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open("public/list.json").unwrap();
        file.write_all(response_string.as_bytes()).unwrap();
        return response_string.to_string();
    }
}

fn write_to_servers() -> String{
    let context = zmq::Context::new();

    let requester = context.socket(zmq::REQ).unwrap();
    assert!(requester.connect("tcp://localhost:5559").is_ok());

    let contents = fs::read_to_string("public/list.json").unwrap();
    requester.send(contents.as_str(), 0).unwrap();
    println!("Request sent!");
    let string = requester.recv_msg(0).unwrap();
    return string.as_str().unwrap().to_string()
}


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(Cors::permissive())
            .service(get_list)
            .service(add_change)
            .service(generate_id)
    })
    .bind("127.0.0.1:5000")?
    .run()
    .await
}