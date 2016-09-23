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

use server::Server;

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

    pub fn onTCPError(&self){
        /*
        let mut udpConnectionGuard=self.udpConnection.write().unwrap();

        match *udpConnectionGuard {
            Some( connectionIndex ) => {
                let udpConnectionsGuard=self.server.udpConnections.read().unwrap();

                (*udpConnectionsGuard)[connectionIndex].disconnect();
            },
            None => {},
        }

        *udpConnectionGuard=None;
        */
        *self.tcpConnection.write().unwrap()=None;

        //push to delete player list
    }

    pub fn onUDPError(&self){
        let mut tcpConnectionGuard=self.tcpConnection.write().unwrap();

        match *tcpConnectionGuard {
            Some( connectionIndex ) => {
                let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

                (*tcpConnectionsGuard)[connectionIndex].disconnect_with("UDP error");
            },
            None => {},
        }

        *tcpConnectionGuard=None;
        //*self.udpConnection.write().unwrap()=None;

        //push to delete player list
    }

    pub fn disconnect(&self, msg:&str){
        {
            let mut tcpConnectionGuard=self.tcpConnection.write().unwrap();

            match *tcpConnectionGuard {
                Some( connectionToken ) => {
                    let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

                    (*tcpConnectionsGuard)[connectionToken].disconnect_with(msg);
                },
                None => {},
            }

            *tcpConnectionGuard=None;
        }
        /*
        {
            let mut udpConnectionGuard=self.udpConnection.write().unwrap();

            match *udpConnectionGuard {
                Some( connectionIndex ) => {
                    let udpConnectionsGuard=self.server.udpConnections.read().unwrap();

                    (*udpConnectionsGuard)[connectionIndex].disconnect();
                },
                None => {},
            }

            *udpConnectionGuard=None;
        }
        */
        //push to delete player list
    }
}
