use std::net::SocketAddr;
use server::DisconnectionReason;

pub struct UDPConnection{
    pub session:u64,
    clientAddr:SocketAddr,

    pub shouldReset:bool,
}

impl UDPConnection {
    pub fn new(session:u64, clientAddr:SocketAddr) -> UDPConnection{
        UDPConnection{
            session:session,
            clientAddr:clientAddr,

            shouldReset:false,
        }
    }

    pub fn _disconnect(&mut self, reason:DisconnectionReason){
        self.shouldReset=true;
    }
}
