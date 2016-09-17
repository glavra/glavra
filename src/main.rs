extern crate glavra;
use glavra::*;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    Glavra::start("0.0.0.0:3012", args.contains(&String::from("-r")));
}
