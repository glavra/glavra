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
use message::*;

mod vote;
use vote::*;

mod strings;

macro_rules! require {
    ($self_: expr, $e:expr, $err:expr) => (match $e {
        Some(x) => x,
        None => { $self_.send_error($err); return Ok(()); }
    })
}

macro_rules! rrequire {
    ($self_: expr, $e:expr, $err:expr) => (match $e {
        Ok(x) => x,
        Err(_) => { $self_.send_error($err); return Ok(()); }
    })
}

pub struct Glavra {
    conn: Connection
}

struct Server {
    glavra: Arc<Mutex<Glavra>>,
    out: ws::Sender,
    userid: Option<i64>
}

impl Glavra {

    pub fn start(address: &str) {
        let conn = Connection::open("data.db").unwrap();

        conn.create_scalar_function("POW", 2, true, |ctx|
                Ok(ctx.get::<f64>(0).unwrap().powf(ctx.get::<f64>(1).unwrap()))
            ).unwrap();

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
                            CREATE TABLE IF NOT EXISTS votes (
                            id          INTEGER PRIMARY KEY,
                            messageid   INTEGER NOT NULL,
                            userid      INTEGER NOT NULL,
                            votetype    INTEGER NOT NULL,
                            timestamp   TEXT NOT NULL
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
                      ORDER BY id LIMIT 100
                      OFFSET (SELECT COUNT(*) FROM messages) - 100").unwrap();
        let mut vote_query = lock.conn
            .prepare("SELECT id, userid, votetype, timestamp FROM votes
                      WHERE messageid = ?").unwrap();

        for message in backlog_query.query_map(&[], |row| {
                    Message {
                        id: row.get(0),
                        userid: row.get(1),
                        text: row.get(2),
                        timestamp: row.get(3)
                    }
                }).unwrap() {
            let message = message.unwrap();
            try!(self.out.send(self.message_json(&message, false, &lock)));
            for vote in vote_query.query_map(&[&message.id], |row| {
                        Vote {
                            id: row.get(0),
                            messageid: message.id,
                            userid: row.get(1),
                            votetype: int_to_votetype(row.get(2)).unwrap(),
                            timestamp: row.get(3)
                        }
                    }).unwrap() {
                let vote = vote.unwrap();
                try!(self.out.send(self.vote_json(&vote, false)));
            }
        }

        try!(self.out.send(self.starboard_json(&lock)));

        Ok(())
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        let data = rrequire!(self, msg.into_text(), strings::MALFORMED);

        let json: Map<String, Value> = rrequire!(self,
            serde_json::from_str(&data[..]), strings::MALFORMED);
        println!("got message: {:?}", json);

        let msg_type = require!(self, get_string(&json, "type"),
            strings::MALFORMED);

        match &msg_type[..] {

            "auth" => {
                let (username, password, mut userid) =
                    (require!(self, get_string(&json, "username"),
                        strings::MALFORMED),
                     require!(self, get_string(&json, "password"),
                        strings::MALFORMED),
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
                    (require!(self, get_string(&json, "username"),
                        strings::MALFORMED),
                     require!(self, get_string(&json, "password"),
                        strings::MALFORMED));
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
                let text = require!(self, get_string(&json, "text"),
                    strings::MALFORMED);
                if !text.is_empty() {
                    let message = Message {
                        id: -1,
                        userid: require!(self, self.userid.clone(),
                            strings::NEED_LOGIN),
                        text: text,
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "edit" => {
                let message = Message {
                    id: require!(self, get_i64(&json, "id"),
                        strings::MALFORMED),
                    userid: require!(self, self.userid.clone(),
                        strings::NEED_LOGIN),
                    text: require!(self, get_string(&json, "text"),
                        strings::MALFORMED),
                    timestamp: time::get_time()
                };
                self.send_message(message);
            },

            "vote" => {
                let votetype = require!(self, int_to_votetype(require!(self,
                    get_i64(&json, "votetype"), strings::MALFORMED)),
                    strings::MALFORMED);

                let vote = Vote {
                    id: -1,
                    messageid: require!(self, get_i64(&json, "messageid"),
                        strings::MALFORMED),
                    userid: require!(self, self.userid.clone(),
                        strings::NEED_LOGIN),
                    votetype: votetype.clone(),
                    timestamp: time::get_time()
                };
                self.send_vote(vote);

                match votetype {
                    VoteType::Star => {
                        let lock = self.glavra.lock().unwrap();
                        try!(self.out.broadcast(self.starboard_json(&lock)));
                    },
                    _ => {}
                }
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
        let mut message = message;
        let lock = self.glavra.lock().unwrap();
        let edit;
        if message.id == -1 {
            edit = false;
            lock.conn.execute("INSERT INTO messages
                    (userid, text, timestamp) VALUES ($1, $2, $3)",
                    &[&message.userid, &message.text, &message.timestamp])
                .unwrap();
            message.id = lock.conn.query_row("SELECT last_insert_rowid()", &[],
                |row| row.get(0)).unwrap();
        } else {
            edit = true;
            lock.conn.execute("UPDATE messages
                    SET text = $1 WHERE id = $2",
                    &[&message.text, &message.id])
                .unwrap();
        }
        self.out.broadcast(self.message_json(&message, edit, &lock)).unwrap();
    }

    fn message_json(&self, message: &Message, edit: bool,
            lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", if edit { "edit" } else { "message" })
            .insert("id", message.id)
            .insert("userid", message.userid)
            .insert("username", self.get_username(message.userid, lock).unwrap())
            .insert("text", &message.text)
            .insert("timestamp", message.timestamp.sec)
            .unwrap()).unwrap()
    }

    fn send_vote(&mut self, vote: Vote) {
        let lock = self.glavra.lock().unwrap();
        let voteid: Result<i64, rusqlite::Error> = lock.conn
            .query_row("SELECT id FROM votes
                        WHERE messageid = ? AND userid = ? AND votetype = ?",
            &[&vote.messageid, &vote.userid, &votetype_to_int(&vote.votetype)],
            |row| row.get(0));
        let undo;
        if let Ok(voteid) = voteid {
            undo = true;
            lock.conn.execute("DELETE FROM votes WHERE id = $1", &[&voteid])
                .unwrap();
        } else {
            undo = false;
            lock.conn.execute("INSERT INTO votes
                    (messageid, userid, votetype, timestamp)
                    VALUES ($1, $2, $3, $4)",
                    &[&vote.messageid, &vote.userid,
                    &votetype_to_int(&vote.votetype), &vote.timestamp])
                .unwrap();
        }
        self.out.broadcast(self.vote_json(&vote, undo)).unwrap();
    }

    fn vote_json(&self, vote: &Vote, undo: bool) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", if undo { "undovote" } else { "vote" })
            .insert("messageid", vote.messageid)
            .insert("userid", vote.userid)
            .insert("votetype", votetype_to_int(&vote.votetype))
            .unwrap()).unwrap()
    }

    fn send_error(&self, err: &str) {
        self.out.send(serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "error")
            .insert("text", err)
            .unwrap()).unwrap()).unwrap();
    }

    fn get_username(&self, userid: i64, lock: &MutexGuard<Glavra>)
            -> Result<String, rusqlite::Error> {
        if userid == -1 { return Ok(String::new()); }
        lock.conn.query_row("SELECT username FROM users
            WHERE id = ?", &[&userid], |row| { row.get(0) })
    }

    fn starboard_json(&self, lock: &MutexGuard<Glavra>) -> String {
        let mut starboard_query = lock.conn
            .prepare("SELECT m.id, m.text, m.timestamp,
                             u.id, u.username,
                             COUNT(v.userid) as starcount
                      FROM votes v
                        INNER JOIN messages m ON v.messageid = m.id
                        LEFT JOIN users u ON m.userid = u.id
                      WHERE v.votetype = 3
                      GROUP BY m.id
                      ORDER BY (starcount * POW(
                        (STRFTIME('%s', 'NOW') - STRFTIME('%s', m.timestamp))
                          / 60.0,
                        -1.5))
                      LIMIT ?").unwrap();
        let mut starboard_result = starboard_query.query(&[&10]).unwrap();
        let mut starboard: Vec<Value> = Vec::new();

        while let Some(row) = starboard_result.next() {
            let row = row.unwrap();
            starboard.push(ObjectBuilder::new()
                .insert("id", row.get::<i32, i64>(0))
                .insert("text", row.get::<i32, String>(1))
                .insert("timestamp", row.get::<i32, time::Timespec>(2).sec)
                .insert("userid", row.get::<i32, Option<i64>>(3).unwrap_or(-1))
                .insert("username", row.get::<i32, Option<String>>(4).unwrap_or_else(|| String::new()))
                .insert("starcount", row.get::<i32, i32>(5))
                .unwrap());
        }

        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "starboard")
            .insert("messages", starboard)
            .unwrap()).unwrap()
    }
}

fn get_string(json: &Map<String, Value>, key: &str) -> Option<String> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Some(s.clone()),
        _ => None
    }
}

fn get_i64(json: &Map<String, Value>, key: &str) -> Option<i64> {
    match json.get(key) {
        Some(&Value::I64(i)) => Some(i),
        Some(&Value::U64(i)) => Some(i as i64),
        _ => None
    }
}
