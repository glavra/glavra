use util::*;

use ws;

use serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

use time;

use message::*;

use strings;

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

    pub fn auth(&mut self, json: Map<String, Value>) -> ws::Result<()> {
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
