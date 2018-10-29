mod util;
use util::*;

mod server_util;

extern crate ws;
const UPDATE: ws::util::Token = ws::util::Token(1);

#[macro_use]
extern crate serde_json;
use serde_json::{Value, Map};

extern crate rand;

extern crate postgres;
use postgres::{Connection, TlsMode};

extern crate time;

extern crate url;
use url::Url;

use std::sync::{Arc, Mutex};

use std::ops::Deref;

mod types;
use types::message::*;
use types::vote::*;
mod enums;
use enums::errcode::ErrCode;
mod actions;

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
    roomid: Option<i32>
}

impl Glavra {

    pub fn start(address: &str, reset: bool) {
        let conn = Connection::connect("postgres://glavra@localhost",
            TlsMode::None).unwrap();

        if reset {
            conn.batch_execute("
            DROP TABLE IF EXISTS messages CASCADE;
            DROP TABLE IF EXISTS users CASCADE;
            DROP TABLE IF EXISTS tokens CASCADE;
            DROP TABLE IF EXISTS votes CASCADE;
            DROP TABLE IF EXISTS history CASCADE;
            DROP TABLE IF EXISTS privileges CASCADE;
            DROP TABLE IF EXISTS rooms CASCADE;

            CREATE TABLE rooms (
            id          SERIAL PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL
            );

            CREATE TABLE messages (
            id          SERIAL PRIMARY KEY,
            roomid      INT NOT NULL,
            userid      INT NOT NULL,
            replyid     INT,
            text        TEXT NOT NULL,
            tstamp      TIMESTAMP NOT NULL
            );

            CREATE TABLE users (
            id          SERIAL PRIMARY KEY,
            username    TEXT NOT NULL UNIQUE,
            salt        BYTEA NOT NULL,
            hash        BYTEA NOT NULL,
            theme       TEXT NOT NULL
            );

            CREATE TABLE tokens (
            userid      INT NOT NULL,
            token       TEXT NOT NULL
            );

            CREATE TABLE votes (
            id          SERIAL PRIMARY KEY,
            messageid   INT NOT NULL,
            userid      INT NOT NULL,
            votetype    INT NOT NULL,
            tstamp      TIMESTAMP NOT NULL
            );

            CREATE TABLE history (
            id          SERIAL PRIMARY KEY,
            messageid   INT NOT NULL,
            replyid     INT,
            text        TEXT NOT NULL,
            tstamp      TIMESTAMP NOT NULL
            );

            CREATE TABLE privileges (
            id          SERIAL PRIMARY KEY,
            roomid      INT NOT NULL,
            userid      INT,
            privtype    INT NOT NULL,
            threshold   INT NOT NULL,
            period      INTERVAL NOT NULL
            );

            INSERT INTO rooms (name, description)
            VALUES ('Glavra', 'Glavra chatroom');

            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 1, 5, '5s'); -- SendMessage
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 6, 5, '5s'); -- EditOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 7, 0, '0s'); -- EditOthers
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 8, 5, '5s'); -- DeleteOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 9, 0, '0s'); -- DeleteOthers
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 10, 0, '0s'); -- UpvoteOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 11, 5, '5s'); -- UpvoteOthers
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 12, 0, '0s'); -- DownvoteOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 13, 5, '5s'); -- DownvoteOthers
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 14, 0, '0s'); -- StarOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 15, 3, '1d'); -- StarOthers
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 16, 0, '0s'); -- PinOwn
            INSERT INTO privileges (roomid, userid, privtype, threshold, period)
            VALUES (1, NULL, 17, 0, '0s'); -- PinOthers
            ").unwrap();
        }

        let glavra = Glavra {
            conn: conn
        };
        let arc = Arc::new(Mutex::new(glavra));

        ws::listen(address, |out| {
            Server {
                glavra: arc.clone(),
                out: out,
                userid: None,
                roomid: None
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
            self.error_close(ErrCode::BadReqUrl);
            return Ok(());
        };

        let lock = self.glavra.lock().unwrap();

        let mut username = None;  // TODO ugh this is really bad
        if let Some((_, token)) = url.query_pairs()
                .find(|&(ref k, _)| k == "token") {
            let auth_query = lock.conn.query("
                    SELECT t.userid, u.username
                    FROM tokens t
                    INNER JOIN users u ON u.id = t.userid
                    WHERE token = $1", &[&token.deref()]).unwrap();
            if auth_query.is_empty() {
                // the token does not exist
                // I guess we'll just fail silently then? (TODO)
            } else {
                let row = auth_query.get(0);
                self.userid = Some(row.get(0));
                username = Some(row.get::<usize, String>(1));
                // TODO this is The Wrong Way(tm) of doing things
                // (code duplication and whatnot)
                try!(self.out.send(serde_json::to_string(&json!({
                    "type": "auth",
                    "success": true,
                    "username": row.get::<usize, String>(1)
                })).unwrap()));
            }
        }

        let pref_query = lock.conn.query("
                SELECT theme
                FROM users
                WHERE id = $1", &[&self.userid])
            .unwrap();
        if !pref_query.is_empty() {
            let prefs = pref_query.get(0);
            try!(self.out.send(serde_json::to_string(&json!({
                "type": "preferences",
                "theme": prefs.get::<usize, String>(0)
            })).unwrap()));
        }

        if let Some((_, room)) = url.query_pairs()
                .find(|&(ref k, _)| k == "room") {
            let room = match room.parse() {
                Ok(parsed_room) => parsed_room,
                Err(_) => {
                    self.error_close(ErrCode::InvalidRoomId);
                    return Ok(());
                }
            };
            self.roomid = Some(room);

            if let Some(username) = username {
                // TODO AHHHHHHHHHHHHH
                self.system_message(format!("{} has connected", username),
                    &lock);
            }

            let room_query = lock.conn.query("
                    SELECT name, description
                    FROM rooms
                    WHERE id = $1", &[&room])
                .unwrap();
            if room_query.is_empty() {
                self.error_close(ErrCode::RoomNotExist);
                return Ok(());
            }

            try!(self.out.send(serde_json::to_string(&json!({
                "type": "roominfo",
                "name": room_query.get(0).get::<usize, String>(0),
                "desc": room_query.get(0).get::<usize, String>(1)
            })).unwrap()));

            for row in lock.conn.query("
                    SELECT * FROM (
                      SELECT id, userid, replyid, text, tstamp
                      FROM messages
                      WHERE roomid = $1
                      ORDER BY id DESC
                      LIMIT 100
                    ) AS _
                    ORDER BY id ASC", &[&room]).unwrap().iter() {
                let message = Message {
                    id: row.get(0),
                    roomid: room,
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

            return Ok(());
        };

        if url.query_pairs().any(|(ref k, _)| k == "queryrooms") {
            for row in lock.conn.query("
                        SELECT id, name, description
                        FROM rooms
                        ORDER BY id DESC", &[])
                    .unwrap().iter() {
                try!(self.out.send(serde_json::to_string(&json!({
                    "type": "roomlist",
                    "id": row.get::<usize, i32>(0),
                    "name": row.get::<usize, String>(1),
                    "desc": row.get::<usize, String>(2)
                })).unwrap()));
            }

            return Ok(());
        }

        if url.query_pairs().any(|(ref k, _)| k == "queryusers") {
            for row in lock.conn.query("
                        SELECT id, username
                        FROM users
                        ORDER BY id DESC", &[])
                    .unwrap().iter() {
                try!(self.out.send(serde_json::to_string(&json!({
                    "type": "userlist",
                    "id": row.get::<usize, i32>(0),
                    "username": row.get::<usize, String>(1)
                })).unwrap()));
            }
        }

        if let Some((_, quser)) = url.query_pairs()
                .find(|&(ref k, _)| k == "queryuser") {
            let quser: i32 = match quser.parse() {
                Ok(parsed_quser) => parsed_quser,
                Err(_) => {
                    self.error_close(ErrCode::InvalidUserId);
                    return Ok(());
                }
            };

            let quser_query = lock.conn.query("
                    SELECT username
                    FROM users
                    WHERE id = $1", &[&quser]).unwrap();
            if quser_query.is_empty() {
                self.error_close(ErrCode::UserNotExist);
                return Ok(());
            }
            let ruser = quser_query.get(0);

            try!(self.out.send(serde_json::to_string(&json!({
                "type": "userinfo",
                "id": quser,
                "username": ruser.get::<usize, String>(0)
            })).unwrap()));
        }

        Ok(())
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        let data = rrequire!(self, msg.into_text(), ErrCode::Malformed);

        let json: Map<String, Value> = rrequire!(self,
            serde_json::from_str(&data[..]), ErrCode::Malformed);
        println!("got message: {:?}", json);

        let msg_type = require!(self, get_string(&json, "type"),
            ErrCode::Malformed);

        match &msg_type[..] {
            "auth"     => self.auth(json),
            "register" => self.register(json),
            "message"  => self.message(json),
            "edit"     => self.edit(json),
            "delete"   => self.delete(json),
            "vote"     => self.vote(json),
            "history"  => self.history(json),
            "room"     => self.room(json),
            _ => {
                self.send_error(ErrCode::Malformed);
                Ok(())
            }
        }
    }

    fn on_close(&mut self, _: ws::CloseCode, _: &str) {
        println!("client disconnected");
        if self.userid.is_some() && self.roomid.is_some() {
            let lock = self.glavra.lock().unwrap();
            self.system_message(format!("{} has disconnected",
                self.get_username(self.userid.clone().unwrap(), &lock)
                    .unwrap()), &lock);
        }
    }

    fn on_timeout(&mut self, token: ws::util::Token) -> ws::Result<()> {
        if token == UPDATE && self.roomid.is_some() {
            let lock = self.glavra.lock().unwrap();
            try!(self.out.send(self.starboard_json(VoteType::Star, &lock)));
            try!(self.out.send(self.starboard_json(VoteType::Pin, &lock)));
            try!(self.out.timeout(60 * 1000, UPDATE));
        }

        Ok(())
    }

}
