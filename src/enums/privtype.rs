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
    UpvoteOwn,
    UpvoteOthers,
    DownvoteOwn,
    DownvoteOthers,
    StarOwn,
    StarOthers,
    PinOwn,
    PinOthers
}
