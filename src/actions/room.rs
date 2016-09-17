use util::*;

use ws;

use serde_json::{Value, Map};

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
    pub fn room(&mut self, json: Map<String, Value>) -> ws::Result<()> {
        let name = require!(self, get_string(&json, "name"), ErrCode::Malformed);
        let desc = require!(self, get_string(&json, "desc"), ErrCode::Malformed);
        let lock = self.glavra.lock().unwrap();
        let id: i32 = lock.conn.query("
        INSERT INTO rooms (name, description) VALUES ($1, $2)
        RETURNING id", &[&name, &desc]).unwrap().get(0).get(0);
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 1, 5, '5s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 6, 5, '5s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 7, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 8, 5, '5s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 9, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 10, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 11, 5, '5s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 12, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 13, 5, '5s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 14, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 15, 3, '1d')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 16, 0, '0s')", &[&id]).unwrap();
        lock.conn.execute("
        INSERT INTO privileges (roomid, userid, privtype, threshold, period)
        VALUES ($1, NULL, 17, 0, '0s')", &[&id]).unwrap();
        Ok(())
    }
}
