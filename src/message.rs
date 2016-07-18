extern crate serde;

extern crate time;
use time::Timespec;

#[derive(Clone)]
pub struct Message {
    pub text: String,
    pub username: String,
    pub timestamp: Timespec
}

impl serde::Serialize for Message {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
            where S: serde::Serializer {
        serializer.serialize_struct("Message", MessageMapVisitor {
            value: self,
            state: 0
        })
    }
}

struct MessageMapVisitor<'a> {
    value: &'a Message,
    state: u8
}

impl<'a> serde::ser::MapVisitor for MessageMapVisitor<'a> {
    fn visit<S>(&mut self, serializer: &mut S) -> Result<Option<()>, S::Error>
            where S: serde::Serializer {
        self.state += 1;
        match self.state {
            1 => Ok(Some(try!(serializer.serialize_struct_elt("type", String::from("message"))))),
            2 => Ok(Some(try!(serializer.serialize_struct_elt("text", &self.value.text)))),
            3 => Ok(Some(try!(serializer.serialize_struct_elt("username", &self.value.username)))),
            4 => Ok(Some(try!(serializer.serialize_struct_elt("timestamp", &self.value.timestamp.sec)))),
            _ => Ok(None)
        }
    }
}
