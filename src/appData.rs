use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use adminServer::AdminServer;


pub struct AppData{
    pub port:u16,
    pub log:Log,
    pub adminServer:RwLock< Option< Arc<AdminServer> > >,
}

pub struct Log{
    s:u32,
}

impl Log{
    pub fn print( &self, msg:String ) {
        println!("{}",msg);
    }
}

impl AppData{
    pub fn new() -> AppData{
        AppData{
            port:1941,
            log:Log{s:67},
            adminServer:RwLock::new(None),
        }
    }
}
