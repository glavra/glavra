#[derive(Copy, Clone)]
pub enum ErrCode {
    NeedLogin,
    Malformed,
    EmptyMsg,
    EditDeleted,
    BadReqUrl,
    NoRoomId,
    InvalidRoomId,
    RoomNotExist,
    UsernameTooLong,
    RateLimit
}
