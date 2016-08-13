use util::*;

use ws;

use serde_json;
use serde_json::{Value, Map};
use serde_json::builder::ObjectBuilder;

use enums::errcode::ErrCode;

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
            (require!(self, get_string(&json, "username"), ErrCode::Malformed),
             require!(self, get_string(&json, "password"), ErrCode::Malformed),
             -1);

        let lock = self.glavra.lock().unwrap();
        let auth_success = {
            let auth_query = lock.conn.query("
                SELECT id, salt, hash
                FROM users
                WHERE username = $1", &[&username]).unwrap();
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

        let builder = ObjectBuilder::new()
            .insert("type", "auth")
            .insert("success", auth_success);
        let builder = if auth_success {
            builder.insert("token", self.get_auth_token(userid, &lock))
        } else { builder };
        try!(self.out.send(serde_json::to_string(&builder.unwrap()).unwrap()));

        if auth_success {
            self.userid = Some(userid);
            self.system_message(format!("{} has connected", username), &lock);
        }

        Ok(())
    }

}
