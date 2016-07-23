use util::*;

use ws;

use serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

extern crate rand;
use self::rand::{Rng, OsRng};

use time;

use message::*;

use strings;

use std::io::Write;

use Server;

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

impl Server {
    pub fn register(&mut self, json: Map<String, Value>) -> ws::Result<()> {
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
                roomid: self.roomid,
                userid: -1,
                replyid: None,
                text: format!("{} has connected", username),
                timestamp: time::get_time()
            };
            self.send_message(message);
        }

        Ok(())
    }
}
