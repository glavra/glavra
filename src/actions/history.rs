use util::*;

use ws;

use serde_json::{Value, Map};

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
    pub fn history(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let id = require!(self, get_i32(&json, "id"), strings::MALFORMED);
        let lock = self.glavra.lock().unwrap();
        try!(self.out.send(self.history_json(id, &lock)));
        Ok(())
    }
}
