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

use packet::{ClientToServerTCPPacket, ClientToServerUDPPacket, ServerToClientTCPPacket, ServerToClientUDPPacket};

pub struct Player{
    isActive:bool,
    server:Arc<Server>,

    playerID:usize,
    userID:usize,
    userName:String,
}


impl Player{
    pub fn new(server:Arc<Server>, playerID:usize, userID:usize, userName:String) -> Player {
        Player{
            isActive:true,
            server:server,

            playerID:playerID,
            userID:userID,
            userName:userName,
        }
    }

    pub fn _disconnect(&mut self, reason:DisconnectionReason){
        self.isActive=false;
    }

    pub fn disconnect(&mut self, reason:DisconnectionReason){
        self._disconnect(reason.clone());

        self.server.tryDisconnectTCPConnection( self.playerID, reason.clone() );
        self.server.tryDisconnectUDPConnection( self.playerID, reason.clone() );
    }



        //self.server.disconnectPlayersList.lock().push((self.playerID, DisconnectionSource::Player, reason));
        /*
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

        match source{
            DisconnectionSource::TCP => {
                //только try_lock
                server.getUDPConnectionAnd(playerID, | connection | connection.disconnect( source.clone(), reason.clone() )),
            },
            DisconnectionSource::UDP => {
                server.getTCPConnectionAnd(Token(token), | connection | connection.disconnect( source.clone(), reason.clone() )),
            },
            DisconnectionSource::Player => {
                /*
                match *udpConnectionGuard {
                    Some( token ) => self.server.getUDPConnectionAnd(token, | connection | connection.disconnect( source.clone(), reason.clone() )),
                    None => {},
                }
                */

                server.getTCPConnectionAnd(Token(token), | connection | connection.disconnect( source.clone(), reason.clone() )),
            },
        }

        let mut playersGuard=server.players.write().unwrap();
        */

    pub fn sendDatagram(playerID:usize, datagram:&Vec<u8>){
        //не паникует, если не находит udpConnection
    }

    pub fn sendMessage(playerID:usize, message:Vec<u8>){
        //не паникует, если не находит tcpConnection
    }

    pub fn processMessage(&mut self, packet:&ClientToServerTCPPacket) -> Result<(), String> {
        println!("process TCP packet");

        Ok(())
    }

    pub fn processDatagram(&mut self, packet:&ClientToServerUDPPacket, time:u64) {
        println!("process UDP packet {}", time);
    }
}
