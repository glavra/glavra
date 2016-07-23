extern crate time;
use time::Timespec;

#[derive(Clone)]
pub struct Vote {
    pub id: i32,
    pub messageid: i32,
    pub userid: i32,
    pub votetype: VoteType,
    pub timestamp: Timespec
}

#[derive(Clone)]
pub enum VoteType {
    Upvote, Downvote, Star, Pin
}

pub fn votetype_to_int(votetype: &VoteType) -> i32 {
    match votetype {
        &VoteType::Upvote => 1, &VoteType::Downvote => 2,
        &VoteType::Star => 3, &VoteType::Pin => 4
    }
}

pub fn int_to_votetype(votetype: i32) -> Option<VoteType> {
    match votetype {
        1 => Some(VoteType::Upvote), 2 => Some(VoteType::Downvote),
        3 => Some(VoteType::Star), 4 => Some(VoteType::Pin),
        _ => None
    }
}
