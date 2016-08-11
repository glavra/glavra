mod util;
use util::*;

mod server_util;

extern crate ws;
const UPDATE: ws::util::Token = ws::util::Token(1);

extern crate serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

extern crate postgres;
use postgres::{Connection, SslMode};

extern crate time;

extern crate url;
use url::Url;

use std::sync::{Arc, Mutex};

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
    roomid: i32
}

impl Glavra {

    pub fn start(address: &str) {
        let conn = Connection::connect("postgres://glavra@localhost",
            SslMode::None).unwrap();

        conn.batch_execute("
        CREATE TABLE IF NOT EXISTS rooms (
        id          SERIAL PRIMARY KEY,
        name        TEXT NOT NULL,
        description TEXT NOT NULL
        );

        INSERT INTO rooms (id, name, description)
        VALUES (1, 'Glavra', 'Glavra chatroom')
        ON CONFLICT DO NOTHING;

        CREATE TABLE IF NOT EXISTS messages (
        id          SERIAL PRIMARY KEY,
        roomid      INT NOT NULL,
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
        );

        CREATE TABLE IF NOT EXISTS privileges (
        id          SERIAL PRIMARY KEY,
        roomid      INT NOT NULL,
        userid      INT,
        privtype    INT NOT NULL,
        threshold   INT NOT NULL,
        period      INTERVAL NOT NULL
        );

        INSERT INTO privileges
        VALUES (1, 1, NULL, 1, 5, '5s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (2, 1, NULL, 6, 5, '5s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (3, 1, NULL, 7, 0, '0s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (4, 1, NULL, 10, 0, '0s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (5, 1, NULL, 11, 5, '5s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (6, 1, NULL, 12, 0, '0s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (7, 1, NULL, 13, 5, '5s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (8, 1, NULL, 14, 0, '0s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (9, 1, NULL, 15, 3, '1d')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (10, 1, NULL, 16, 0, '0s')
        ON CONFLICT DO NOTHING;

        INSERT INTO privileges
        VALUES (11, 1, NULL, 17, 0, '0s')
        ON CONFLICT DO NOTHING;
        ").unwrap();

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
            self.error_close(ErrCode::BadReqUrl);
            return Ok(());
        };

        if let Some((_, room)) = url.query_pairs()
                .find(|&(ref k, _)| k == "room") {
            match room.parse() {
                Ok(parsed_room) => self.roomid = parsed_room,
                Err(_) => {
                    self.error_close(ErrCode::InvalidRoomId);
                    return Ok(());
                }
            }
        } else {
            self.error_close(ErrCode::NoRoomId);
            return Ok(());
        };

        let lock = self.glavra.lock().unwrap();

        let room_query = lock.conn.query("
                SELECT name, description
                FROM rooms
                WHERE id = $1", &[&self.roomid])
            .unwrap();
        if room_query.is_empty() {
            self.error_close(ErrCode::RoomNotExist);
            return Ok(());
        }

        try!(self.out.send(serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "roominfo")
            .insert("name", room_query.get(0).get::<usize, String>(0))
            .insert("desc", room_query.get(0).get::<usize, String>(1))
            .unwrap()).unwrap()));

        for row in lock.conn.query("
                SELECT * FROM (
                  SELECT id, userid, replyid, text, tstamp
                  FROM messages
                  WHERE roomid = $1
                  ORDER BY id DESC
                  LIMIT 100
                ) AS _
                ORDER BY id ASC", &[&self.roomid]).unwrap().iter() {
            let message = Message {
                id: row.get(0),
                roomid: self.roomid,
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
            _ => {
                self.send_error(ErrCode::Malformed);
                Ok(())
            }
        }
    }

    fn on_close(&mut self, _: ws::CloseCode, _: &str) {
        println!("client disconnected");
        if self.userid.is_some() {
            let lock = self.glavra.lock().unwrap();
            self.system_message(format!("{} has disconnected",
                self.get_username(self.userid.clone().unwrap(), &lock)
                    .unwrap()), &lock);
        }
    }

    fn on_timeout(&mut self, _: ws::util::Token) -> ws::Result<()> {
        let lock = self.glavra.lock().unwrap();
        try!(self.out.send(self.starboard_json(VoteType::Star, &lock)));
        try!(self.out.send(self.starboard_json(VoteType::Pin, &lock)));
        self.out.timeout(60 * 1000, UPDATE)
    }

}
