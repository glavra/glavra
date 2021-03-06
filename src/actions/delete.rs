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
    pub fn delete(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let lock = self.glavra.lock().unwrap();
        let id = require!(self, get_i32(&json, "id"), ErrCode::Malformed);
        let roomid = require!(self, self.roomid, ErrCode::NoRoomId);
        let userid = require!(self, self.userid.clone(), ErrCode::NeedLogin);
        let muserid = rrequire!(self, self.get_sender(id, &lock), ErrCode::Malformed);
        let own = userid == muserid;

        let (threshold, period) = self.get_privilege(roomid, &self.userid,
            if own { PrivType::DeleteOwn } else { PrivType::DeleteOthers },
            &lock).unwrap();

        if lock.conn.query("
                    SELECT COUNT(DISTINCT h.messageid) >= $1
                    FROM history h
                    INNER JOIN messages m ON m.id = h.messageid
                    WHERE m.roomid = $3
                      AND m.userid = $4
                      AND m.text = ''
                      AND h.tstamp BETWEEN now() - (interval '1s') * $2
                                   AND now()",
                &[&threshold, &period, &roomid, &userid])
                    .unwrap().get(0).get(0) {
            self.send_error(ErrCode::RateLimit);
            return Ok(());
        }

        let message = Message {
            id: require!(self, get_i32(&json, "id"), ErrCode::Malformed),
            roomid: roomid,
            userid: require!(self, self.userid.clone(), ErrCode::NeedLogin),
            replyid: None,
            text: String::new(),
            timestamp: time::get_time()
        };
        self.send_message(message, &lock);
        Ok(())
    }
}
