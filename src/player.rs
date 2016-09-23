use appData::AppData;

use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io::{self, ErrorKind};
use std::rc::Rc;

use mio::*;
use mio::tcp::*;
use slab::Slab;
use std::net::SocketAddr;

use server::{Server,DisconnectionReason,DisconnectionSource};

pub struct Player{
    //name:String,
    server:Arc<Server>,

    tcpConnection:RwLock< Option< Token > >,
    //udpConnection:RwLock< Option< u16 > >,
}


impl Player{
    pub fn new(server:Arc<Server>, tcpConnectionToken:Token) -> Player {
        Player{
            //name:
            server:server,
            tcpConnection:RwLock::new(Some(tcpConnectionToken)),
            //udpConnection:RwLock::new(Some(udpConnectionIndex)),
        }
    }

    pub fn disconnect(&self, reason:DisconnectionReason){
        let mut tcpConnectionGuard=self.tcpConnection.write().unwrap();
        //let mut udpConnectionGuard=self.udpConnection.write().unwrap();

        match reason{
            DisconnectionReason::Error( ref source, ref msg ) => {
                match *source{
                    DisconnectionSource::Player => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }

                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::TCP => {
                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::UDP => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                }
            },
            DisconnectionReason::ConnectionLost( ref source ) => {
                match *source{
                    DisconnectionSource::TCP => {
                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::UDP => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::Player=> {},
                }
            },
            DisconnectionReason::Kick( ref source, ref message ) => {//только от игрока??
                match *source{
                    DisconnectionSource::Player => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }

                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::TCP => {
                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::UDP => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                }
            },
            DisconnectionReason::ServerShutdown => {},
            DisconnectionReason::Hup( ref source ) => {
                match *source{
                    DisconnectionSource::TCP => {
                        /*
                        match *udpConnectionGuard {
                            Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                        */
                    },
                    DisconnectionSource::UDP => {
                        match *tcpConnectionGuard {
                            Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::Player=> {},
                }
            },
        }

        *tcpConnectionGuard=None;
        //*udpConnectionGuard=None;
    }
}
