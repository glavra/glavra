use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use enums::errcode::*;
use enums::privtype::*;

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
        let text = require!(self, get_string(&json, "text"), ErrCode::Malformed);

        if text.is_empty() {
            self.send_error(ErrCode::EmptyMsg);
        } else {
            let roomid = require!(self, self.roomid, ErrCode::NoRoomId);
            let userid = require!(self, self.userid.clone(), ErrCode::NeedLogin);

            let lock = self.glavra.lock().unwrap();
            let (threshold, period) = self.get_privilege(roomid, &self.userid,
                PrivType::SendMessage, &lock).unwrap();

            if lock.conn.query("
                        SELECT COUNT(*) >= $1
                        FROM messages
                        WHERE roomid = $3
                          AND userid = $4
                          AND tstamp BETWEEN now() - (interval '1s') * $2
                                     AND now()",
                    &[&threshold, &period, &roomid, &userid])
                        .unwrap().get(0).get(0) {
                self.send_error(ErrCode::RateLimit);
                return Ok(());
            }

            let message = Message {
                id: -1,
                roomid: roomid,
                userid: userid,
                replyid: get_i32(&json, "replyid"),
                text: text,
                timestamp: time::get_time()
            };
            self.send_message(message, &lock);
        }
        Ok(())
    }
}
