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

    pub fn disconnect(&self, source:DisconnectionSource, reason:DisconnectionReason){
        match reason{
            DisconnectionReason::Hup =>
                println!("hup"),
            DisconnectionReason::ServerShutdown =>
                println!("server shutdown"),
            DisconnectionReason::FatalError( ref msg ) =>
                println!("fatal error {}",msg),
            DisconnectionReason::ClientDesire( ref msg ) =>
                println!("client desire {}",msg),
            DisconnectionReason::ServerDesire( ref msg ) =>
                println!("server desire {}",msg),
            DisconnectionReason::ClientError( ref msg ) =>
                println!("client error {}",msg ),
            DisconnectionReason::ServerError( ref msg ) =>
                println!("server error {}",msg ),
        }

        let mut tcpConnectionGuard=self.tcpConnection.write().unwrap();
        //let mut udpConnectionGuard=self.udpConnection.write().unwrap();

        match source{
            DisconnectionSource::TCP => {
                /*
                match *udpConnectionGuard {
                    Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect( source.clone(), reason.clone() )),
                    None => {},
                }
                */
            },
            DisconnectionSource::UDP => {
                match *tcpConnectionGuard {
                    Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect( source.clone(), reason.clone() )),
                    None => {},
                }
            },
            DisconnectionSource::Player => {
                /*
                match *udpConnectionGuard {
                    Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect( source.clone(), reason.clone() )),
                    None => {},
                }
                */

                match *tcpConnectionGuard {
                    Some( token ) => self.server.getTCPConnectionAnd(token, | connection | connection.disconnect( source.clone(), reason.clone() )),
                    None => {},
                }
            },
        }

        *tcpConnectionGuard=None;
        //*udpConnectionGuard=None;
    }
}
