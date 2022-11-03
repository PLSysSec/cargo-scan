/*
    Toy example of a Url client
    (Note: this code doesn't work for many reasons)
*/

use super::url::Url;
use std::net::TcpListener;

pub fn ping_example_com() {
    let url = Url::parse("https://www.example.com").unwrap();
    // I know this isn't valid, we need to use ToSocketAddrs etc. to
    // get a socket, just doing this as an example
    let listener = TcpListener::bind(url.path).unwrap();
    for stream in listener.incoming() {
        println!("{:?}", stream.unwrap());
    }
}
