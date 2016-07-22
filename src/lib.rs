extern crate ws;
const UPDATE: ws::util::Token = ws::util::Token(1);

extern crate serde;
extern crate serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

extern crate postgres;
use postgres::{Connection, SslMode};

extern crate crypto;
use crypto::bcrypt;

extern crate rand;
use rand::{Rng, OsRng};

extern crate time;
use time::Timespec;

extern crate url;
use url::Url;

use std::sync::{Arc, Mutex, MutexGuard};

mod message;
use message::*;

mod vote;
use vote::*;

mod strings;

use std::io::Write;

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
    userid: Option<i32>,
    roomid: i32
}

impl Glavra {

    pub fn start(address: &str) {
        let conn = Connection::connect("postgres://glavra@localhost",
            SslMode::None).unwrap();

        conn.batch_execute("CREATE TABLE IF NOT EXISTS messages (
                            id          SERIAL PRIMARY KEY,
                            userid      INT NOT NULL,
                            replyid     INT,
                            text        TEXT NOT NULL,
                            tstamp      TIMESTAMP NOT NULL
                            );
                            CREATE TABLE IF NOT EXISTS users (
                            id          SERIAL PRIMARY KEY,
                            username    TEXT NOT NULL UNIQUE,
                            salt        BYTEA NOT NULL,
                            hash        BYTEA NOT NULL
                            );
                            CREATE TABLE IF NOT EXISTS votes (
                            id          SERIAL PRIMARY KEY,
                            messageid   INT NOT NULL,
                            userid      INT NOT NULL,
                            votetype    INT NOT NULL,
                            tstamp      TIMESTAMP NOT NULL
                            );
                            CREATE TABLE IF NOT EXISTS history (
                            id          SERIAL PRIMARY KEY,
                            messageid   INT NOT NULL,
                            replyid     INT,
                            text        TEXT NOT NULL,
                            tstamp      TIMESTAMP NOT NULL
                            );").unwrap();

        let glavra = Glavra {
            conn: conn
        };
        let arc = Arc::new(Mutex::new(glavra));

        ws::listen(address, |out| {
            Server {
                glavra: arc.clone(),
                out: out,
                userid: None,
                roomid: 0  // dummy value, will be replaced
            }
        }).unwrap();
    }

}

impl ws::Handler for Server {

    fn on_open(&mut self, hs: ws::Handshake) -> ws::Result<()> {
        println!("client connected from {}", hs.request.resource());

        let url = if let Ok(url) = Url::parse("http://localhost").unwrap()
                .join(hs.request.resource()) {
            url
        } else {
            println!("{:?}", url::Url::parse(hs.request.resource()));
            return Err(ws::Error::new(ws::ErrorKind::Internal,
               "failed to parse request resource URL"));
        };

        if let Some((_, room)) = url.query_pairs()
                .find(|&(ref k, _)| k == "room") {
            match room.parse::<i32>() {
                Ok(parsed_room) => self.roomid = parsed_room,
                Err(_) => {
                    return Err(ws::Error::new(ws::ErrorKind::Internal,
                        "invalid room ID specified on WebSocket connection"));
                }
            }
        } else {
            return Err(ws::Error::new(ws::ErrorKind::Internal,
                "no room ID specified on WebSocket connection"));
        };

        let lock = self.glavra.lock().unwrap();

        for row in lock.conn.query("SELECT * FROM (SELECT id, userid, replyid,
                text, tstamp FROM messages ORDER BY id DESC LIMIT 100) AS _
                ORDER BY id ASC", &[]).unwrap().iter() {
            let message = Message {
                id: row.get(0),
                userid: row.get(1),
                replyid: row.get(2),
                text: row.get(3),
                timestamp: row.get(4)
            };
            try!(self.out.send(self.message_json(&message, false, &lock)));
            for row in lock.conn.query("SELECT id, userid, votetype, tstamp
                    FROM votes WHERE messageid = $1", &[&message.id]).unwrap()
                    .iter() {
                let vote = Vote {
                    id: row.get(0),
                    messageid: message.id,
                    userid: row.get(1),
                    votetype: int_to_votetype(row.get(2)).unwrap(),
                    timestamp: row.get(3)
                };
                try!(self.out.send(self.vote_json(&vote, false)));
            }
        }

        // this is bad and I know it
        self.out.timeout(0, UPDATE).unwrap();

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

                let auth_success = {
                    let lock = self.glavra.lock().unwrap();
                    let auth_query = lock.conn.query("SELECT id, salt, hash
                        FROM users WHERE username = $1", &[&username]).unwrap();
                    if auth_query.is_empty() {
                        // the username doesn't exist
                        false
                    } else {
                        let row = auth_query.get(0);
                        userid = row.get(0);
                        let salt: Vec<u8> = row.get(1);
                        let salt = salt.as_slice();
                        // I can't believe I actually have to do this
                        let salt = [salt[0],  salt[1],  salt[2],  salt[3],
                                    salt[4],  salt[5],  salt[6],  salt[7],
                                    salt[8],  salt[9],  salt[10], salt[11],
                                    salt[12], salt[13], salt[14], salt[15]];
                        let hash: Vec<u8> = row.get(2);
                        hash == hash_pwd(salt, &password)
                    }
                };

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
                        replyid: None,
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

                let mut salt = [0u8; 16];
                let mut rng = OsRng::new().unwrap();
                rng.fill_bytes(&mut salt);
                let mut salt_vec = Vec::with_capacity(16);
                salt_vec.write(&salt).unwrap();
                let hash = hash_pwd(salt, &password);

                let success;

                {
                    let lock = self.glavra.lock().unwrap();
                    let register_query = lock.conn
                        .query("INSERT INTO users (username, salt, hash)
                                  VALUES ($1, $2, $3) RETURNING id",
                                  &[&username, &salt_vec, &hash]);
                    success = register_query.is_ok();
                    if success {
                        self.userid = Some(register_query.unwrap().get(0).get(0));
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
                        replyid: None,
                        text: format!("{} has connected", username),
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "message" => {
                let text = require!(self, get_string(&json, "text"),
                    strings::MALFORMED);
                if text.is_empty() {
                    self.send_error(strings::EMPTY_MSG);
                } else {
                    let message = Message {
                        id: -1,
                        userid: require!(self, self.userid.clone(),
                            strings::NEED_LOGIN),
                        replyid: get_i32(&json, "replyid"),
                        text: text,
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "edit" => {
                let text = require!(self, get_string(&json, "text"),
                    strings::MALFORMED);
                if text.is_empty() {
                    self.send_error(strings::EMPTY_MSG);
                } else {
                    let message = Message {
                        id: require!(self, get_i32(&json, "id"),
                            strings::MALFORMED),
                        userid: require!(self, self.userid.clone(),
                            strings::NEED_LOGIN),
                        replyid: get_i32(&json, "replyid"),
                        text: require!(self, get_string(&json, "text"),
                            strings::MALFORMED),
                        timestamp: time::get_time()
                    };
                    self.send_message(message);
                }
            },

            "delete" => {
                let message = Message {
                    id: require!(self, get_i32(&json, "id"),
                        strings::MALFORMED),
                    userid: require!(self, self.userid.clone(),
                        strings::NEED_LOGIN),
                    replyid: None,
                    text: String::new(),
                    timestamp: time::get_time()
                };
                self.send_message(message);
            },

            "vote" => {
                let votetype = require!(self, int_to_votetype(require!(self,
                    get_i32(&json, "votetype"), strings::MALFORMED)),
                    strings::MALFORMED);

                let vote = Vote {
                    id: -1,
                    messageid: require!(self, get_i32(&json, "messageid"),
                        strings::MALFORMED),
                    userid: require!(self, self.userid.clone(),
                        strings::NEED_LOGIN),
                    votetype: votetype.clone(),
                    timestamp: time::get_time()
                };
                self.send_vote(vote);

                match &votetype {
                    &VoteType::Star | &VoteType::Pin => {
                        let lock = self.glavra.lock().unwrap();
                        try!(self.out.broadcast(self.starboard_json(votetype,
                            &lock)));
                    },
                    _ => {}
                }
            },

            "history" => {
                let id = require!(self, get_i32(&json, "id"),
                    strings::MALFORMED);
                let lock = self.glavra.lock().unwrap();
                try!(self.out.send(self.history_json(id, &lock)));
            },

            _ => {
                self.send_error(strings::MALFORMED);
            }
        }

        Ok(())
    }

    fn on_close(&mut self, _: ws::CloseCode, _: &str) {
        println!("client disconnected");
        if self.userid.is_some() {
            let message = Message {
                id: -1,
                userid: -1,
                replyid: None,
                text: format!("{} has disconnected",
                    self.get_username(self.userid.clone().unwrap(),
                        &self.glavra.lock().unwrap()).unwrap()),
                timestamp: time::get_time()
            };
            self.send_message(message);
        }
    }

    fn on_timeout(&mut self, _: ws::util::Token) -> ws::Result<()> {
        let lock = self.glavra.lock().unwrap();
        try!(self.out.send(self.starboard_json(VoteType::Star, &lock)));
        try!(self.out.send(self.starboard_json(VoteType::Pin, &lock)));
        self.out.timeout(60 * 1000, UPDATE)
    }

}

impl Server {

    fn send_message(&mut self, message: Message) {
        let mut message = message;
        let lock = self.glavra.lock().unwrap();
        let edit;
        if message.id == -1 {
            edit = false;
            message.id = lock.conn.query("INSERT INTO messages
                    (userid, replyid, text, tstamp) VALUES ($1, $2, $3, $4)
                    RETURNING id",
                    &[&message.userid, &message.replyid, &message.text,
                        &message.timestamp])
                .unwrap().get(0).get(0);
        } else {
            edit = true;
            let oldquery = lock.conn
                .query("SELECT replyid, text FROM messages
                        WHERE id = $1", &[&message.id]).unwrap();
            let (oldreplyid, oldtext) =
                (oldquery.get(0).get::<usize, Option<i32>>(0),
                 oldquery.get(0).get::<usize, String>(1));
            if oldtext.is_empty() {
                self.send_error(strings::EDIT_DELETED);
            } else {
                lock.conn.execute("INSERT INTO history
                        (messageid, replyid, text, tstamp)
                        VALUES ($1, $2, $3, $4)",
                        &[&message.id, &oldreplyid, &oldtext, &time::get_time()])
                    .unwrap();
                lock.conn.execute("UPDATE messages
                        SET replyid = $1, text = $2 WHERE id = $3",
                        &[&message.replyid, &message.text, &message.id])
                    .unwrap();
            }
        }
        self.out.broadcast(self.message_json(&message, edit, &lock)).unwrap();
    }

    fn message_json(&self, message: &Message, edit: bool,
            lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", if edit { "edit" } else { "message" })
            .insert("id", message.id)
            .insert("userid", message.userid)
            .insert("replyid", message.replyid)
            .insert("username", self.get_username(message.userid, lock).unwrap())
            .insert("text", &message.text)
            .insert("timestamp", message.timestamp.sec)
            .unwrap()).unwrap()
    }

    fn send_vote(&mut self, vote: Vote) {
        let lock = self.glavra.lock().unwrap();
        let voteid = lock.conn
            .query("SELECT id FROM votes
                    WHERE messageid = $1 AND userid = $2 AND votetype = $3",
                    &[&vote.messageid, &vote.userid,
                        &votetype_to_int(&vote.votetype)])
            .unwrap();
        let undo;
        if voteid.is_empty() {
            undo = false;
            lock.conn.execute("INSERT INTO votes
                    (messageid, userid, votetype, tstamp)
                    VALUES ($1, $2, $3, $4)",
                    &[&vote.messageid, &vote.userid,
                    &votetype_to_int(&vote.votetype), &vote.timestamp])
                .unwrap();
        } else {
            undo = true;
            lock.conn.execute("DELETE FROM votes WHERE id = $1",
                &[&voteid.get(0).get::<usize, i32>(0)]).unwrap();
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

    fn get_username(&self, userid: i32, lock: &MutexGuard<Glavra>)
            -> Result<String, postgres::error::Error> {
        if userid == -1 { return Ok(String::new()); }
        lock.conn.query("SELECT username FROM users
            WHERE id = $1", &[&userid]).map(|rows|
                rows.get(0).get::<usize, String>(0))
    }

    fn starboard_json(&self, votetype: VoteType, lock: &MutexGuard<Glavra>)
            -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "starboard")
            .insert("votetype", votetype_to_int(&votetype))
            .insert("messages", lock.conn.query(match votetype {
                VoteType::Star =>
                    "SELECT m.id, m.text, m.tstamp,
                           MIN(u.id), MIN(u.username),
                           COUNT(v.userid)
                    FROM votes v
                      INNER JOIN messages m ON v.messageid = m.id
                      LEFT JOIN users u ON m.userid = u.id
                    WHERE v.votetype = 3
                    GROUP BY m.id
                    ORDER BY (COUNT(v.userid) * POW(
                      EXTRACT(EPOCH FROM (NOW() - m.tstamp)) / 60,
                      -1.5))
                    LIMIT 10",
                VoteType::Pin =>
                    "SELECT m.id, m.text, m.tstamp,
                           MIN(u.id), MIN(u.username),
                           COUNT(v.userid)
                    FROM votes v
                      INNER JOIN messages m ON v.messageid = m.id
                      LEFT JOIN users u ON m.userid = u.id
                    WHERE v.votetype = 4
                    GROUP BY m.id
                    ORDER BY COUNT(v.userid)",
                _ => panic!("weird votetype in starboard_json")
            }, &[]).unwrap().iter().map(|row|
                ObjectBuilder::new()
                    .insert("id", row.get::<usize, i32>(0))
                    .insert("text", row.get::<usize, String>(1))
                    .insert("timestamp", row.get::<usize, Timespec>(2).sec)
                    .insert("userid", row.get::<usize, Option<i32>>(3).unwrap_or(-1))
                    .insert("username", row.get::<usize, Option<String>>(4).unwrap_or_else(|| String::new()))
                    .insert("votecount", row.get::<usize, i64>(5))
                    .unwrap()
                ).collect::<Vec<Value>>())
            .unwrap()).unwrap()
    }

    fn history_json(&self, id: i32, lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "history")
            .insert("revisions", lock.conn.query("
                SELECT replyid, text, tstamp
                FROM history
                WHERE messageid = $1
                ORDER BY id", &[&id]).unwrap().iter().map(|row|
                ObjectBuilder::new()
                    .insert("replyid", row.get::<usize, Option<i32>>(0))
                    .insert("text", row.get::<usize, String>(1))
                    .insert("timestamp", row.get::<usize, Timespec>(2).sec)
                    .unwrap()).collect::<Vec<Value>>())
            .unwrap()).unwrap()
    }
}

fn get_string(json: &Map<String, Value>, key: &str) -> Option<String> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Some(s.clone()),
        _ => None
    }
}

fn get_i32(json: &Map<String, Value>, key: &str) -> Option<i32> {
    match json.get(key) {
        Some(&Value::I64(i)) => Some(i as i32),
        Some(&Value::U64(i)) => Some(i as i32),
        _ => None
    }
}

// adapted from https://www.reddit.com/r/rust/comments/2sipzj/is_there_an_easy_way_to_hash_passwords_in_rust/cnptvs6
fn hash_pwd(salt: [u8; 16], password: &String) -> Vec<u8> {
    let mut result = [0u8; 24];
    let password: String = password.chars().take(72).collect();
    let password = if password.is_empty() { String::from("pls") }
        else { password };
    bcrypt::bcrypt(10, &salt, password.as_bytes(), &mut result);
    let mut v = Vec::with_capacity(24);
    v.write(&result).unwrap();
    v
}
