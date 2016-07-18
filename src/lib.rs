extern crate ws;

extern crate serde;
extern crate serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

extern crate rusqlite;
use rusqlite::Connection;

extern crate time;

use std::sync::{Arc, Mutex, MutexGuard};

mod message;
use message::Message;

pub struct Glavra {
    conn: Connection
}

struct Server {
    glavra: Arc<Mutex<Glavra>>,
    out: ws::Sender,
    userid: Option<i32>
}

impl Glavra {

    pub fn start(address: &str) {
        let conn = Connection::open("data.db").unwrap();

        conn.execute_batch("BEGIN;
                            CREATE TABLE IF NOT EXISTS messages (
                            id          INTEGER PRIMARY KEY,
                            userid      INTEGER NOT NULL,
                            text        TEXT NOT NULL,
                            timestamp   TEXT NOT NULL
                            );
                            CREATE TABLE IF NOT EXISTS users (
                            id          INTEGER PRIMARY KEY,
                            username    TEXT NOT NULL UNIQUE,
                            password    TEXT NOT NULL
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
                userid: None
            }
        }).unwrap();
    }

}

impl ws::Handler for Server {

    fn on_open(&mut self, _: ws::Handshake) -> ws::Result<()> {
        println!("client connected");

        let lock = self.glavra.lock().unwrap();
        let mut backlog_query = lock.conn
            .prepare("SELECT id, userid, text, timestamp FROM messages
                      ORDER BY id LIMIT 50").unwrap();

        for message in backlog_query.query_map(&[], |row| {
                    Message {
                        id: row.get(0),
                        userid: row.get(1),
                        text: row.get(2),
                        timestamp: row.get(3)
                    }
                }).unwrap() {
            self.out.send(self.message_json(&message.unwrap(), &lock)).unwrap();
        }

        Ok(())
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        println!("on_message called");
        let data = try!(msg.into_text());
        let json: Map<String, Value> = serde_json::from_str(&data[..]).unwrap();
        println!("got message: {:?}", json);

        let msg_type = get_string(&json, "type").unwrap();
        match &msg_type[..] {

            "auth" => {
                let (username, password, mut userid) =
                    (get_string(&json, "username").unwrap(),
                     get_string(&json, "password").unwrap(),
                     -1);
                let auth_success = self.glavra.lock().unwrap().conn
                    .query_row("SELECT id, password FROM users WHERE username = ?",
                               &[&username], |row| {
                                   userid = row.get(0);
                                   let correct_password: String = row.get(1);
                                   password == correct_password
                               }).unwrap_or(false);  // username doesn't exist

                let auth_response = ObjectBuilder::new()
                    .insert("type", "auth")
                    .insert("success", auth_success)
                    .unwrap();
                try!(self.out.send(serde_json::to_string(&auth_response).unwrap()));

                if auth_success {
                    self.userid = Some(userid);
                    let message = Message {
                        id: -1,
                        userid: -1,
                        text: format!("{} has connected", username),
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "register" => {
                let (username, password) =
                    (get_string(&json, "username").unwrap(),
                     get_string(&json, "password").unwrap());
                let success;

                {
                    let lock = self.glavra.lock().unwrap();
                    success = lock.conn
                        .execute("INSERT INTO users (username, password)
                                  VALUES ($1, $2)", &[&username, &password])
                        .is_ok();
                    if success {
                        self.userid = Some(lock.conn
                            .query_row("SELECT last_insert_rowid()", &[],
                            |row| row.get(0)).unwrap());
                    }
                }

                try!(self.out.send(serde_json::to_string(&ObjectBuilder::new()
                    .insert("type", "register")
                    .insert("success", success)
                    .unwrap()).unwrap()));

                if success {
                    let message = Message {
                        id: -1,
                        userid: -1,
                        text: format!("{} has connected", username),
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "message" => {
                let message = Message {
                    id: -1,
                    userid: self.userid.clone().unwrap(),
                    text: get_string(&json, "text").unwrap(),
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
        if self.userid.is_some() {
            let message = Message {
                id: -1,
                userid: -1,
                text: format!("{} has disconnected",
                    self.get_username(self.userid.clone().unwrap(),
                        &self.glavra.lock().unwrap()).unwrap()),
                timestamp: time::get_time()
            };
            self.send_message(message);
        }
    }

}

impl Server {

    fn send_message(&mut self, message: Message) {
        let lock = self.glavra.lock().unwrap();
        self.out.broadcast(self.message_json(&message, &lock)).unwrap();
        lock.conn.execute("INSERT INTO messages
                (userid, text, timestamp) VALUES ($1, $2, $3)",
                &[&message.userid, &message.text, &message.timestamp])
            .unwrap();
    }

    fn get_username(&self, userid: i32, lock: &MutexGuard<Glavra>)
            -> Result<String, rusqlite::Error> {
        if userid == -1 { return Ok(String::new()); }
        lock.conn.query_row("SELECT username FROM users
            WHERE id = ?", &[&userid], |row| { row.get(0) })
    }

    fn message_json(&self, message: &Message, lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "message")
            .insert("username", self.get_username(message.userid, lock).unwrap())
            .insert("text", &message.text)
            .insert("timestamp", message.timestamp.sec)
            .unwrap()).unwrap()
    }

}

fn get_string(json: &Map<String, Value>, key: &str) -> Result<String, ()> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Ok(s.clone()),
        _ => Err(())
    }
}
