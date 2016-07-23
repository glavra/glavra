use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use strings;

use types::message::*;

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
    pub fn delete(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let message = Message {
            id: require!(self, get_i32(&json, "id"),
                strings::MALFORMED),
            roomid: self.roomid,
            userid: require!(self, self.userid.clone(),
                strings::NEED_LOGIN),
            replyid: None,
            text: String::new(),
            timestamp: time::get_time()
        };
        self.send_message(message);
        Ok(())
    }
}
