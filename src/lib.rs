extern crate ws;

extern crate serde;
extern crate serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

use std::sync::{Arc, Mutex};

mod message;
use message::*;

extern crate time;

#[derive(Clone)]
pub struct Glavra {
    messages: Vec<Message>
}

struct Server {
    glavra: Arc<Mutex<Glavra>>,
    out: ws::Sender,
    username: Option<String>
}

impl Glavra {

    pub fn start(address: &str) {
        let glavra = Glavra {
            messages: vec![]
        };
        let arc = Arc::new(Mutex::new(glavra));

        ws::listen(address, |out| {
            Server {
                glavra: arc.clone(),
                out: out,
                username: None
            }
        }).unwrap();
    }

}

impl ws::Handler for Server {

    fn on_open(&mut self, _: ws::Handshake) -> ws::Result<()> {
        println!("client connected");
        Ok(())
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        let data = try!(msg.into_text());
        let json: Map<String, Value> = serde_json::from_str(&data[..]).unwrap();
        println!("got message: {:?}", json);

        let msg_type = get_string(&json, "type").unwrap();
        match &msg_type[..] {
            "auth" => {
                self.username = Some(get_string(&json, "username").unwrap());
                let auth_response = ObjectBuilder::new()
                    .insert("type", "authResponse")
                    .insert("success", true)
                    .unwrap();
                self.out.send(serde_json::to_string(&auth_response).unwrap()).unwrap();
                let message = Message {
                    text: format!("{} has connected", self.username.clone().unwrap()),
                    username: String::new(),
                    timestamp: get_timestamp()
                };
                self.send_message(message);
            },
            "message" => {
                let message = Message {
                    text: get_string(&json, "text").unwrap(),
                    username: self.username.clone().unwrap(),
                    timestamp: get_timestamp()
                };
                self.send_message(message);
            },
            _ => panic!()
        }

        Ok(())
    }

    fn on_close(&mut self, _: ws::CloseCode, _: &str) {
        println!("client disconnected");
        if self.username.is_some() {
            let message = Message {
                text: format!("{} has disconnected", self.username.clone().unwrap()),
                username: String::new(),
                timestamp: get_timestamp()
            };
            self.send_message(message);
        }
    }

}

impl Server {

    fn send_message(&mut self, message: Message) {
        self.out.broadcast(serde_json::to_string(&message).unwrap()).unwrap();
        self.glavra.lock().unwrap().messages.push(message);
    }

}

fn get_string(json: &Map<String, Value>, key: &str) -> Result<String, ()> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Ok(s.clone()),
        _ => Err(())
    }
}

fn get_timestamp() -> u64 {
    let now = time::get_time();
    (now.sec as u64) * 1000 + (now.nsec as u64) / 1000000
}
