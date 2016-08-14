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

use std::thread;
use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

mod appData;
mod adminServer;

use appData::AppData;
use adminServer::AdminServer;

fn main() {
    let appData=Arc::new( AppData::new() );

    appData.log.print(format!("[INFO]Connecting admin server"));

    match AdminServer::connect( appData.clone() ) {
        Ok ( _ ) => {
            appData.log.print(format!("[INFO]Connected to admin server"));
            thread::sleep_ms(5000);

            match *appData.adminServer.read().unwrap(){
                Some( ref adminServer) => adminServer.stop(),
                None=>{},
            }
        },
        Err( e ) => {appData.log.print(format!("[ERROR]Can not connect to admin : {}", e)); return;}
    }
}
