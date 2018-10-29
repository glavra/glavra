use serde_json::{Value, Map};

extern crate crypto;
use self::crypto::bcrypt;

use std::io::Write;

pub fn get_string(json: &Map<String, Value>, key: &str) -> Option<String> {
    match json.get(key) {
        Some(&Value::String(ref s)) => Some(s.clone()),
        _ => None
    }
}

pub fn get_i32(json: &Map<String, Value>, key: &str) -> Option<i32> {
    match json.get(key) {
        Some(&Value::Number(ref i)) => i.as_i64().map(|x| x as i32),
        _ => None
    }
}

// adapted from https://www.reddit.com/r/rust/comments/2sipzj/is_there_an_easy_way_to_hash_passwords_in_rust/cnptvs6
pub fn hash_pwd(salt: [u8; 16], password: &String) -> Vec<u8> {
    let mut result = [0u8; 24];
    let password: &[u8] = &password.as_bytes().into_iter().cloned().take(72)
        .collect::<Vec<u8>>()[..];
    let empty_password = &[0];
    bcrypt::bcrypt(10, &salt, if password.is_empty() { empty_password }
                   else { password }, &mut result);
    let mut v = Vec::with_capacity(24);
    v.write(&result).unwrap();
    v
}
