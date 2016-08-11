use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use enums::errcode::ErrCode;

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
    pub fn edit(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let text = require!(self, get_string(&json, "text"),
            ErrCode::Malformed);
        if text.is_empty() {
            self.send_error(ErrCode::EmptyMsg);
        } else {
            let message = Message {
                id: require!(self, get_i32(&json, "id"), ErrCode::Malformed),
                roomid: self.roomid,
                userid: require!(self, self.userid.clone(),
                    ErrCode::NeedLogin),
                replyid: get_i32(&json, "replyid"),
                text: require!(self, get_string(&json, "text"),
                    ErrCode::Malformed),
                timestamp: time::get_time()
            };
            self.send_message(message, &self.glavra.lock().unwrap());
        }
        Ok(())
    }
}
