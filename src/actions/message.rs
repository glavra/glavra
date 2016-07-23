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
    pub fn message(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let text = require!(self, get_string(&json, "text"),
            strings::MALFORMED);
        if text.is_empty() {
            self.send_error(strings::EMPTY_MSG);
        } else {
            let message = Message {
                id: -1,
                roomid: self.roomid,
                userid: require!(self, self.userid.clone(),
                    strings::NEED_LOGIN),
                replyid: get_i32(&json, "replyid"),
                text: text,
                timestamp: time::get_time()
            };
            self.send_message(message);
        }
        Ok(())
    }
}