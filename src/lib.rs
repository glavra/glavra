extern crate ws;

extern crate serde;
extern crate serde_json;
use serde_json::{Value, Map};

use std::sync::{Arc, Mutex};

mod message;
use message::*;

#[derive(Clone)]
pub struct Glavra {
    messages: Vec<Message>
}

struct Server {
    glavra: Arc<Mutex<Glavra>>,
    out: ws::Sender
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
                out: out
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

        let msg_type = get_string(json, "type");
        match &msg_type[..] {
            "message" => {
                let message = Message { text: get_string(json, "text").unwrap() };
                self.out.broadcast(serde_json::to_string(&message).unwrap()).unwrap();
                self.glavra.lock().unwrap().messages.push(message);
            },
            _ => panic!()
        }

        Ok(())
    }

    fn on_close(&mut self, _: ws::CloseCode, _: &str) {
        println!("client disconnected");
    }

}

fn get_string(json: Map<String, Value>, key: &str) -> Result<String, ()> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Ok(s.clone()),
        _ => Err(())
    }
}
