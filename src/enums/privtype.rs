#[derive(Copy, Clone)]
pub enum PrivType {
    ReadAccess,
    SendMessage,
    MoveIn,
    MoveOut,
    ModifyRoom,
    ModifyPrivs,
    EditOwn,
    EditOthers,
    DeleteOwn,
    DeleteOthers,
    StarOwn,
    StarOthers,
    PinOwn,
    PinOthers,
    UpvoteOwn,
    UpvoteOthers,
    DownvoteOwn,
    DownvoteOthers
}
