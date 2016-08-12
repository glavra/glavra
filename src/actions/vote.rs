use util::*;

use ws;

use serde_json::{Value, Map};

use time;

use enums::errcode::*;
use enums::privtype::*;

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
        let i_votetype = require!(self, get_i32(&json, "votetype"),
            ErrCode::Malformed);
        let votetype = require!(self, int_to_votetype(i_votetype),
            ErrCode::Malformed);

        let lock = self.glavra.lock().unwrap();
        let id = require!(self, get_i32(&json, "messageid"), ErrCode::Malformed);
        let roomid = require!(self, self.roomid, ErrCode::NoRoomId);
        let userid = require!(self, self.userid.clone(), ErrCode::NeedLogin);
        let muserid = rrequire!(self, self.get_sender(id, &lock), ErrCode::Malformed);
        let own = userid == muserid;

        let privtype = match votetype {
            VoteType::Upvote   => if own { PrivType::UpvoteOwn      }
                                    else { PrivType::UpvoteOthers   },
            VoteType::Downvote => if own { PrivType::DownvoteOwn    }
                                    else { PrivType::DownvoteOthers },
            VoteType::Star     => if own { PrivType::StarOwn        }
                                    else { PrivType::StarOthers     },
            VoteType::Pin      => if own { PrivType::PinOwn         }
                                    else { PrivType::PinOthers      }
        };
        let (threshold, period) = self.get_privilege(roomid, &self.userid,
            privtype, &lock).unwrap();

        if lock.conn.query("
                    SELECT COUNT(*) >= $1
                    FROM votes v
                    INNER JOIN messages m ON m.id = v.messageid
                    WHERE m.roomid = $3
                      AND v.userid = $4
                      AND v.votetype = $5
                      AND v.tstamp BETWEEN now() - (interval '1s') * $2
                                   AND now()",
                &[&threshold, &period, &roomid, &userid, &i_votetype])
                    .unwrap().get(0).get(0) {
            self.send_error(ErrCode::RateLimit);
            return Ok(());
        }

        let vote = Vote {
            id: -1,
            messageid: id,
            userid: userid,
            votetype: votetype.clone(),
            timestamp: time::get_time()
        };
        self.send_vote(vote, &lock);

        match &votetype {
            &VoteType::Star | &VoteType::Pin => {
                try!(self.out.broadcast(self.starboard_json(votetype, &lock)));
            },
            _ => {}
        }
        Ok(())
    }
}
