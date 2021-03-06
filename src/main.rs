#![allow(non_snake_case)]

/*
extern crate nanomsg;
use std::io::Read;

use nanomsg::{Socket, Protocol, Error};

/// Creating a new `Pull` socket type. Pull sockets can only receive messages
/// from a `Push` socket type.
fn create_socket() -> Result<(), Error> {
    let mut socket = try!(Socket::new(Protocol::Pull));

    // Create a new endpoint bound to the following protocol string. This returns
    // a new `Endpoint` that lives at-most the lifetime of the original socket.
    let mut endpoint = try!(socket.bind("ipc:///tmp/pipelineToGS_1941.ipc"));

    let mut msg = String::new();
    loop {
        try!(socket.read_to_string(&mut msg));
        println!("We got a message: {}", &*msg);
        msg.clear();
    }

    Ok(())
}

fn main() {
    println!("Hello, world!");

    match create_socket() {
        Ok(_)=>println!("Ok"),
        Err(_)=>println!("Err"),
    }
}

*/

/*
extern crate nanomsg;
use std::io::Write;

use nanomsg::{Socket, Protocol, Error};

fn pusher() -> Result<(), Error> {
    let mut socket = try!(Socket::new(Protocol::Push));
    socket.set_survey_deadline(500);
    let mut endpoint = try!(socket.connect("ipc:///tmp/ToGS_1941.ipc"));

    socket.write(b"answer:ToGS is opened");

    endpoint.shutdown();
    Ok(())
}

fn main() {
    println!("Hello, world!");

    match pusher() {
        Ok(_)=>println!("Ok"),
        Err(_)=>println!("Err"),
    }
}
*/

extern crate nanomsg;
extern crate zip;
extern crate mio;
extern crate slab;
extern crate time;
extern crate byteorder;
extern crate rustc_serialize;
extern crate bincode;
extern crate rand;

use std::env;
use std::thread;
use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

mod log;
mod appData;
//mod adminServer;
mod lexer;
mod description;
mod serverConfig;
mod version;
mod modLoader;

mod gameState;
mod map;
mod storage;
mod server;
mod tcpServer;
mod tcpConnection;
mod udpServer;
mod udpConnection;
mod player;
mod packet;
mod httpRequester;


use appData::AppData;
use log::Log;
use serverConfig::ServerConfig;
use gameState::GameState;
//use adminServer::AdminServer;
use storage::Storage;
use httpRequester::HTTPRequester;
use server::Server;



fn main() {
    let mut isEditorOrUndefined=None;

    for argument in env::args() {
        if argument=="editor" {
            isEditorOrUndefined=Some(true);
        }else if argument=="game" {
            isEditorOrUndefined=Some(false);
        }
    }

    let isEditor=match isEditorOrUndefined {
        Some( v ) => v,
        None => {
            println!("[ERROR] Do not launch server_game directly! Use server_admin for it!");
            return;
        }
    };

    //===================Log===========================
    let log=match Log::new(isEditor){
        Ok( l ) => l,
        Err( msg )=>{
            println!( "[ERROR] Can not create log: {}", msg);
            return;
        },
    };

    //===================ServerConfig==================

    let serverConfig=match ServerConfig::read(){
        Ok( sc )=>{
            log.print(format!("[INFO] Server configurations are loaded"));
            sc
        },
        Err( msg )=>{
            log.print(format!("[ERROR] Can not read server configurations: {}", msg));
            return;
        },
    };

    //===================AppData======================
    let appData=AppData::initialize(serverConfig, log, isEditor);

    //AdminServer

    //===================Storage======================

    if !Storage::initialize (appData.clone()) {
        *appData.gameState.write().unwrap()=GameState::Error;
        //close adminServer
        return;
    }

    //==============HTTP Requester====================

    match HTTPRequester::initialize( appData.clone() ) {
        Ok ( _ ) => appData.log.print(String::from("[INFO] HTTP Requester has been initialized")),
        Err( e ) => {
            appData.log.print(format!("[ERROR] Can not initialize HTTP Requester:{}",e));
            AppData::destroy( appData );
            return;
        }
    }

    //===================Server========================

    match Server::start( appData.clone() ) {
        Ok ( _ ) => appData.log.print(String::from("[INFO] Server has been started")),
        Err( e ) => {
            appData.log.print(format!("[ERROR] Can not start server:{}",e));
            AppData::destroy( appData );
            return;
        }
    }

    /*
    appData.getHTTPRequesterAnd(|httpRequester| httpRequester.addRequest(
        "89.110.48.1:1941",
        String::from("GET /getUserIDAndName_sessionID=123456789 HTTP/1.1\r\nHost: 89.110.48.1:1941\r\n\r\n").into_bytes(),
        10,
        move |responseCode:usize, buffer:&[u8] | {
            let string=&String::from_utf8_lossy(buffer);

            println!("{}",string);
        }
        //move |r: &mut Request|
    ));
    */

    /*

    appData.getHTTPRequesterAnd(|httpRequester| httpRequester.addRequest(
        String::from("72.8.141.90:80"),
        String::from("GET / HTTP/1.1\r\nHost: http://www.rust-lang.org/\r\n\r\n").into_bytes(),
        10,
        move |responseCode:usize, buffer:&[u8] | {
            let string=&String::from_utf8_lossy(buffer);

            println!("{}",string);
        }
    ));

    thread::sleep_ms(3000);

    */

    /*
    appData.getHTTPRequesterAnd(|httpRequester| httpRequester.addRequest(
        String::from("5.255.255.70:80"),
        String::from("GET / HTTP/1.1\r\nHost: https://yandex.ru\r\n\r\n").into_bytes(),
        10,
        move |responseCode:usize, buffer:&[u8] | {
            let string=&String::from_utf8_lossy(buffer);

            println!("{}",string);
        }
    ));
    */

    thread::sleep_ms(20000);

    AppData::destroy( appData );

    //===================Clients======================
    /*
    if !Clients::startListen (appData.clone()) {
        *appData.gameState.write().unwrap()=GameState::Error;
        //close adminServer
        return;
    }
    */


    /*
    let appData=Arc::new( AppData::new() );

    appData.log.print(format!("[INFO]Connecting admin server"));

    match AdminServer::connect( appData.clone() ) {
        Ok ( _ ) => {
            appData.log.print(format!("[INFO]Connected to admin server"));

            let mut t1=0; let mut t2=0;
            while !{*appData.shouldStop.read().unwrap()} {
                t2+=1;
                if t2==10 {
                    t1+=1;
                    t2=0;

                    match *appData.adminServer.read().unwrap(){
                        Some( ref adminServer) => {adminServer.send("print",&format!("Time {}",t1));},
                        None=>{},
                    }
                }

                thread::sleep_ms(100);
            }

            match *appData.adminServer.read().unwrap(){
                Some( ref adminServer) => adminServer.stop(),
                None=>{},
            }
        },
        Err( e ) => {appData.log.print(format!("[ERROR]Can not connect to admin server : {}", e)); return;}
    }
    */


}
