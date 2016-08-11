use ws;

use serde_json;
use serde_json::Value;
use serde_json::builder::ObjectBuilder;

use postgres;

use time;
use time::Timespec;

use std::sync::MutexGuard;

use types::message::*;
use types::vote::*;
use enums::errcode::*;
use enums::privtype::*;

use Glavra;
use Server;

impl Server {

    pub fn send_message(&self, message: Message) {
        let mut message = message;
        let lock = self.glavra.lock().unwrap(); // TODO stop the incessant locking
        let edit;
        if message.id == -1 {
            edit = false;
            message.id = lock.conn.query("
                    INSERT INTO messages (roomid, userid, replyid, text, tstamp)
                    VALUES ($1, $2, $3, $4, $5)
                    RETURNING id",
                    &[&self.roomid, &message.userid, &message.replyid,
                        &message.text, &message.timestamp])
                .unwrap().get(0).get(0);
        } else {
            edit = true;
            let oldquery = lock.conn.query("
                    SELECT replyid, text FROM messages WHERE id = $1",
                    &[&message.id]).unwrap();
            let (oldreplyid, oldtext) =
                (oldquery.get(0).get::<usize, Option<i32>>(0),
                 oldquery.get(0).get::<usize, String>(1));
            if oldtext.is_empty() {
                self.send_error(ErrCode::EditDeleted);
            } else {
                lock.conn.execute("INSERT INTO history
                        (messageid, replyid, text, tstamp)
                        VALUES ($1, $2, $3, $4)",
                        &[&message.id, &oldreplyid, &oldtext, &time::get_time()])
                    .unwrap();
                lock.conn.execute("UPDATE messages
                        SET replyid = $1, text = $2 WHERE id = $3",
                        &[&message.replyid, &message.text, &message.id])
                    .unwrap();
            }
        }
        self.out.broadcast(self.message_json(&message, edit, &lock)).unwrap();
    }

    pub fn message_json(&self, message: &Message, edit: bool,
            lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", if edit { "edit" } else { "message" })
            .insert("id", message.id)
            .insert("userid", message.userid)
            .insert("replyid", message.replyid)
            .insert("username", self.get_username(message.userid, lock).unwrap())
            .insert("text", &message.text)
            .insert("timestamp", message.timestamp.sec)
            .unwrap()).unwrap()
    }

    pub fn system_message(&mut self, text: String) {
        let message = Message {
            id: -1,
            roomid: self.roomid,
            userid: -1,
            replyid: None,
            text: text,
            timestamp: time::get_time()
        };
        self.send_message(message);
    }

    pub fn send_vote(&mut self, vote: Vote) {
        let lock = self.glavra.lock().unwrap();
        let voteid = lock.conn
            .query("SELECT id FROM votes
                    WHERE messageid = $1 AND userid = $2 AND votetype = $3",
                    &[&vote.messageid, &vote.userid,
                        &votetype_to_int(&vote.votetype)])
            .unwrap();
        let undo;
        if voteid.is_empty() {
            undo = false;
            lock.conn.execute("INSERT INTO votes
                    (messageid, userid, votetype, tstamp)
                    VALUES ($1, $2, $3, $4)",
                    &[&vote.messageid, &vote.userid,
                    &votetype_to_int(&vote.votetype), &vote.timestamp])
                .unwrap();
        } else {
            undo = true;
            lock.conn.execute("DELETE FROM votes WHERE id = $1",
                &[&voteid.get(0).get::<usize, i32>(0)]).unwrap();
        }
        self.out.broadcast(self.vote_json(&vote, undo)).unwrap();
    }

    pub fn vote_json(&self, vote: &Vote, undo: bool) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", if undo { "undovote" } else { "vote" })
            .insert("messageid", vote.messageid)
            .insert("userid", vote.userid)
            .insert("votetype", votetype_to_int(&vote.votetype))
            .unwrap()).unwrap()
    }

    pub fn send_error(&self, err: ErrCode) {
        self.out.send(serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "error")
            .insert("code", err as i32)
            .unwrap()).unwrap()).unwrap();
    }

    pub fn error_close(&self, err: ErrCode) {
        self.send_error(err);
        self.out.close(ws::CloseCode::Unsupported).unwrap();
    }

    pub fn get_username(&self, userid: i32, lock: &MutexGuard<Glavra>)
            -> Result<String, postgres::error::Error> {
        if userid == -1 { return Ok(String::new()); }
        lock.conn.query("SELECT username FROM users
            WHERE id = $1", &[&userid]).map(|rows|
                rows.get(0).get::<usize, String>(0))
    }

    pub fn get_sender(&self, messageid: i32, lock: &MutexGuard<Glavra>)
            -> Result<i32, postgres::error::Error> {
        lock.conn.query("SELECT userid FROM messages
            WHERE id = $1", &[&messageid]).map(|rows|
                rows.get(0).get::<usize, i32>(0))
    }

    pub fn get_privilege(&self, roomid: i32, userid: &Option<i32>,
                         privtype: PrivType, lock: &MutexGuard<Glavra>)
            -> Result<(i64, f64), postgres::error::Error> {
        let privtype = privtype as i32;
        lock.conn.query("
                SELECT threshold, EXTRACT(EPOCH FROM period)::REAL
                FROM privileges
                WHERE roomid = $1
                  AND (userid = $2 OR userid IS NULL)
                  AND privtype = $3
                ORDER BY userid", &[&roomid, userid, &privtype])
            .map(|rows| {
                 let row = rows.get(0);
                 (row.get::<usize, i32>(0) as i64,
                  row.get::<usize, f32>(1) as f64)
            })
    }

    pub fn starboard_json(&self, votetype: VoteType, lock: &MutexGuard<Glavra>)
            -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "starboard")
            .insert("votetype", votetype_to_int(&votetype))
            .insert("messages", lock.conn.query(match votetype {
                VoteType::Star =>
                    "SELECT m.id, m.text, m.tstamp,
                           MIN(u.id), MIN(u.username),
                           COUNT(v.userid)
                    FROM votes v
                      INNER JOIN messages m ON v.messageid = m.id
                      LEFT JOIN users u ON m.userid = u.id
                    WHERE v.votetype = 3 AND m.roomid = $1
                    GROUP BY m.id
                    ORDER BY (COUNT(v.userid) * POW(
                      EXTRACT(EPOCH FROM (NOW() - m.tstamp)) / 60,
                      -1.5))
                    LIMIT 10",
                VoteType::Pin =>
                    "SELECT m.id, m.text, m.tstamp,
                           MIN(u.id), MIN(u.username),
                           COUNT(v.userid)
                    FROM votes v
                      INNER JOIN messages m ON v.messageid = m.id
                      LEFT JOIN users u ON m.userid = u.id
                    WHERE v.votetype = 4 AND m.roomid = $1
                    GROUP BY m.id
                    ORDER BY COUNT(v.userid)",
                _ => panic!("weird votetype in starboard_json")
            }, &[&self.roomid]).unwrap().iter().map(|row|
                ObjectBuilder::new()
                    .insert("id", row.get::<usize, i32>(0))
                    .insert("text", row.get::<usize, String>(1))
                    .insert("timestamp", row.get::<usize, Timespec>(2).sec)
                    .insert("userid", row.get::<usize, Option<i32>>(3).unwrap_or(-1))
                    .insert("username", row.get::<usize, Option<String>>(4).unwrap_or_else(|| String::new()))
                    .insert("votecount", row.get::<usize, i64>(5))
                    .unwrap()
                ).collect::<Vec<Value>>())
            .unwrap()).unwrap()
    }

    pub fn history_json(&self, id: i32, lock: &MutexGuard<Glavra>) -> String {
        serde_json::to_string(&ObjectBuilder::new()
            .insert("type", "history")
            .insert("revisions", lock.conn.query("
                SELECT replyid, text, tstamp
                FROM history
                WHERE messageid = $1
                ORDER BY id", &[&id]).unwrap().iter().map(|row|
                ObjectBuilder::new()
                    .insert("replyid", row.get::<usize, Option<i32>>(0))
                    .insert("text", row.get::<usize, String>(1))
                    .insert("timestamp", row.get::<usize, Timespec>(2).sec)
                    .unwrap()).collect::<Vec<Value>>())
            .unwrap()).unwrap()
    }
}
