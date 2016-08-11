use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use enums::errcode::ErrCode;

use types::vote::*;

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
    pub fn vote(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let votetype = require!(self, int_to_votetype(require!(self,
            get_i32(&json, "votetype"), ErrCode::Malformed)),
            ErrCode::Malformed);

        let id = require!(self, get_i32(&json, "messageid"), ErrCode::Malformed);
        let userid = require!(self, self.userid.clone(), ErrCode::NeedLogin);
        let muserid = rrequire!(self, {
            let lock = self.glavra.lock().unwrap();
            self.get_sender(id, &lock)
        }, ErrCode::Malformed);

        if userid == muserid {
            self.send_error(ErrCode::VoteOwnMessage);
            return Ok(());
        }

        let vote = Vote {
            id: -1,
            messageid: id,
            userid: userid,
            votetype: votetype.clone(),
            timestamp: time::get_time()
        };
        self.send_vote(vote);

        match &votetype {
            &VoteType::Star | &VoteType::Pin => {
                let lock = self.glavra.lock().unwrap();
                try!(self.out.broadcast(self.starboard_json(votetype, &lock)));
            },
            _ => {}
        }
        Ok(())
    }
}
