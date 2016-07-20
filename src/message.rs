extern crate time;
use time::Timespec;

#[derive(Clone)]
pub struct Message {
    pub id: i64,
    pub userid: i64,
    pub replyid: Option<i64>,
    pub text: String,
    pub timestamp: Timespec
}
