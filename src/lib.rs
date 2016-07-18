extern crate ws;

extern crate serde;
extern crate serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

extern crate rusqlite;
use rusqlite::Connection;

extern crate time;

use std::sync::{Arc, Mutex};

mod message;
use message::Message;

pub struct Glavra {
    conn: Connection
}

struct Server {
    glavra: Arc<Mutex<Glavra>>,
    out: ws::Sender,
    username: Option<String>
}

impl Glavra {

    pub fn start(address: &str) {
        let conn = Connection::open("data.db").unwrap();

        conn.execute_batch("BEGIN;
                            CREATE TABLE IF NOT EXISTS messages (
                            id          INTEGER PRIMARY KEY,
                            text        TEXT NOT NULL,
                            username    TEXT NOT NULL,
                            timestamp   TEXT NOT NULL
                            );
                            CREATE TABLE IF NOT EXISTS users (
                            id          INTEGER PRIMARY KEY
                            );
                            COMMIT;").unwrap();

        let glavra = Glavra {
            conn: conn
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

        let lock = self.glavra.lock().unwrap();
        let mut backlog_query = lock.conn
                .prepare("SELECT text, username, timestamp FROM messages
                          ORDER BY id LIMIT 50").unwrap();

        for message in backlog_query.query_map(&[], |row| {
                    Message {
                        text: row.get(0),
                        username: row.get(1),
                        timestamp: row.get(2)
                    }
                }).unwrap() {
            self.out.send(serde_json::to_string(&message.unwrap()).unwrap()).unwrap();
        }

        Ok(())
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        let data = try!(msg.into_text());
        let json: Map<String, Value> = serde_json::from_str(&data[..]).unwrap();
        println!("got message: {:?}", json);

        let msg_type = get_string(&json, "type").unwrap();
        match &msg_type[..] {
            "auth" => {
                let (username, password) =
                    (get_string(&json, "username").unwrap(),
                     get_string(&json, "password").unwrap());
                if username == password {
                    self.username = Some(username);
                    let auth_response = ObjectBuilder::new()
                        .insert("type", "authResponse")
                        .insert("success", true)
                        .unwrap();
                    self.out.send(serde_json::to_string(&auth_response).unwrap()).unwrap();
                    let message = Message {
                        text: format!("{} has connected", self.username.clone().unwrap()),
                        username: String::new(),
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                } else {
                    let auth_response = ObjectBuilder::new()
                        .insert("type", "authResponse")
                        .insert("success", false)
                        .unwrap();
                    self.out.send(serde_json::to_string(&auth_response).unwrap()).unwrap();
                }
            },
            "message" => {
                let message = Message {
                    text: get_string(&json, "text").unwrap(),
                    username: self.username.clone().unwrap(),
                    timestamp: time::get_time()
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
                timestamp: time::get_time()
            };
            self.send_message(message);
        }
    }

}

impl Server {

    fn send_message(&mut self, message: Message) {
        self.out.broadcast(serde_json::to_string(&message).unwrap()).unwrap();
        self.glavra.lock().unwrap().conn.execute("INSERT INTO messages
                (text, username, timestamp) VALUES ($1, $2, $3)",
                &[&message.text, &message.username, &message.timestamp])
            .unwrap();
    }

}

fn get_string(json: &Map<String, Value>, key: &str) -> Result<String, ()> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Ok(s.clone()),
        _ => Err(())
    }
}
