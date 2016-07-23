use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use strings;

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
            get_i32(&json, "votetype"), strings::MALFORMED)),
            strings::MALFORMED);

        let vote = Vote {
            id: -1,
            messageid: require!(self, get_i32(&json, "messageid"),
                strings::MALFORMED),
            userid: require!(self, self.userid.clone(), strings::NEED_LOGIN),
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
