extern crate time;
use time::Timespec;

#[derive(Clone)]
pub struct Message {
    pub id: i32,
    pub userid: i32,
    pub replyid: Option<i32>,
    pub text: String,
    pub timestamp: Timespec
}
