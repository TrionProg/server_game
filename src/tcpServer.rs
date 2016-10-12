use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io::{self, ErrorKind};
use std::rc::Rc;

use mio::*;
use mio::tcp::*;
use slab::Slab;
use std::net::SocketAddr;

use time::{get_time};
use std::time::Duration;

use appData::AppData;
use server::{Server, DisconnectionReason, DisconnectionSource};
use server::ServerState;

use tcpConnection::{TCPConnection, ReadResult};

use packet::{ServerToClientTCPPacket, ClientToServerTCPPacket};

const  ACTIVITY_CONNECTION_LOST_DELAY: i64 = 10;
const  ACTIVITY_DISCONNECT_DELAY: i64 = 2;

pub struct TCPServer{
    pub appData:Arc<AppData>,
    pub server:Arc<Server>,
    pub listener: TcpListener, //listening socket
    pub poll: Poll,
    pub token: Token, // token of our server. we keep track of it here instead of doing `const SERVER = Token(0)`.
    pub events: Events, // a list of events to process
    pub tickTime: i64,
}

impl TCPServer{
    pub fn process(&mut self) -> Result<(), &'static str>{
        try!(self.register().or(Err( "Can not register server poll" ) ) );

        {
            let mut serverStateGuard=self.server.state.write().unwrap();

            if *serverStateGuard==ServerState::Initialization(0) {
                (*serverStateGuard)=ServerState::Initialization(1);
            }else if *serverStateGuard==ServerState::Initialization(1) {
                *serverStateGuard=ServerState::Processing;
            }else if *serverStateGuard!=ServerState::Processing {//Disconnect кем-то другим
                return Ok(());
            }
        }

        while match *self.server.state.read().unwrap() { ServerState::Initialization(_) => true, _=>false} {
            thread::sleep_ms(10);
        }

        self.appData.log.print(format!("[INFO] TCP server is ready"));

        while {*self.server.state.read().unwrap()}==ServerState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err( "Can not get eventsNumber" ) ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or( "Can not get event" ) );

                try!(self.processEvent(event.token(), event.kind()));
            }

            self.disconnectTCPConnectionsFromList();

            self.reregisterConnections();

            self.processTick();
        }

        Ok(())
    }

    fn disconnectTCPConnectionsFromList(&mut self){
        let disconnectTCPConnectionsListGuard=self.server.disconnectTCPConnectionsList.lock().unwrap();
        let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

        for &(sessionID, ref reason) in (*disconnectTCPConnectionsListGuard).iter(){
            match (*tcpConnectionsGuard).get( Token(sessionID) ) {
                Some( tcpConnectionMutex ) => {
                    let tcpConnectionGuard=tcpConnectionMutex.lock().unwrap();
                    (*tcpConnectionGuard)._disconnect(reason.clone());
                },
                None => {},
            }
        }

        (*disconnectTCPConnectionsListGuard).clear();
    }

    fn reregisterConnections(&mut self){
        let connectionsGuard=self.server.tcpConnections.read().unwrap();

        for connectionMutex in (*connectionsGuard).iter() {
            let mut connection=connectionMutex.lock().unwrap();
            (*connection).reregister(&mut self.poll).unwrap_or_else(|e| { (*connection).disconnect( DisconnectionReason::FatalError(e)); });
        }
    }

    fn processTick(&mut self) {
        if get_time().sec-self.tickTime>1 {
            self.tickTime=get_time().sec;

            self.checkConnections();
        }
    }

    fn checkConnections(&mut self){
        let mut removeConnections = Vec::new();

        //create list of connections we need to remove
        {
            let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

            for connectionMutex in (*tcpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();
                let removeConnection=(*connection).check();

                if (*connection).shouldReset {
                    removeConnections.push((*connection).token);
                    (*connection).deregister(&mut self.poll);
                }
            }
        }

        if removeConnections.len()>0 {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for token in removeConnections {
                (*tcpConnectionsGuard).remove(token);
            }
        }
    }

    fn processEvent(&mut self, token: Token, event: Ready) -> Result<(), &'static str>{
        if self.token == token {
            if event.is_error() {
                return Err( "socket error" );
            }

            if event.is_readable() {
                self.processAccept();
            }

            Ok(())
        }else{
            if event.is_error() {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect( DisconnectionReason::FatalError("socket error") );
                });

                return Ok(())
            }

            if event.is_hup() {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect( DisconnectionReason::Hup );
                });

                return Ok(())
            }

            if event.is_writable() {
                self.server.getTCPConnectionAnd(token, |connection| {
                    if !connection.shouldReset {
                        match connection.writeMessages(){
                            Ok ( _ ) => connection.shouldReregister=true,
                            Err( e ) => connection.disconnect( DisconnectionReason::FatalError(e) ),
                        }
                    }
                });
            }

            if event.is_readable() {
                let readResult=self.server.getTCPConnectionAnd(token, | connection |{
                    if connection.isActive {
                        let readResult=connection.readMessage();

                        match readResult{
                            ReadResult::FatalError( e ) =>
                                connection.disconnect( DisconnectionReason::FatalError(e) ),
                            ReadResult::Error( e ) =>
                                connection.disconnect( DisconnectionReason::ServerError( String::from(e) ) ),
                            _=>
                                connection.shouldReregister=true,
                        }

                        Some(readResult)
                    }else{
                        None
                    }
                });

                match readResult{
                    Some(ReadResult::Ready( playerID, buffer )) => {
                        let bufferGuard=buffer.lock().unwrap();

                        match self.processMessage(token, playerID, &(*bufferGuard)) {
                            Ok ( _ ) => {},
                            Err( e ) => {
                                self.server.getTCPConnectionAnd(token, |connection| {
                                    connection.disconnect( DisconnectionReason::ServerError( e.clone() ) );
                                });
                            }
                        };
                    },
                    _=>{},
                }
            }

            Ok(())
        }
    }

    fn processMessage(&self, token:Token, playerID:Option<usize>, buffer:&Vec<u8>) -> Result<(), String> {
        let packet=try!(ClientToServerTCPPacket::unpack(buffer));

        let process=match packet{
            ClientToServerTCPPacket::ClientDesire( ref msg ) => {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect( DisconnectionReason::ClientDesire(msg.clone()) );
                });

                false
            },
            ClientToServerTCPPacket::ClientError( ref msg ) => {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect( DisconnectionReason::ClientError(msg.clone()) );
                });

                false
            },
            _=>
                true,
        };

        if process{
            match playerID {
                Some(playerID) => {
                    match self.server.getSafePlayerAnd(playerID, | player | { player.processMessage(&packet) }){
                        Some( r ) => r,
                        None => Ok(()),
                    }
                },
                None => {
                    self.server.getTCPConnectionAnd(token, |connection| {
                        connection.processPacket(&packet)
                    })
                },
            }
        }else{
            Ok(())
        }
    }

    fn processAccept(&mut self) {
        loop {
            let socket = match self.listener.accept() {
                Ok((socket, _)) => socket,
                Err(e) => {
                    if e.kind() != ErrorKind::WouldBlock {
                        self.appData.log.print( String::from("[ERROR] Server: Accept tcp socket error") );
                    }

                    return;
                }
            };

            let mut connectionsGuard=self.server.tcpConnections.write().unwrap();

            let token=match (*connectionsGuard).vacant_entry() {
                Some(entry) => {
                    let connection = Mutex::new( TCPConnection::new(socket, entry.index(), self.server.clone()) );
                    entry.insert(connection).index()
                },
                None => {
                    self.appData.log.print( String::from("[ERROR] Server: Failed to insert tcp connection into slab(maybe connectionsLimit has been exceeded)") );
                    return;
                }
            };

            let registerFail=match (*connectionsGuard)[token].lock().unwrap().register(&mut self.poll) {
                Ok(_) => false,
                Err(e) => {
                    self.appData.log.print( format!("[ERROR] Server: Failed to register tcp conenction {:?} with poll : {:?}", token, e) );
                    true
                }
            };

            if registerFail {
                (*connectionsGuard).remove(token);
            }
        }
    }

    fn register(&mut self) -> io::Result<()> {
        self.poll.register(
            &self.listener,
            self.token,
            Ready::readable() | Ready::error(),
            PollOpt::edge()
        ).or_else(|e| {
            Err(e)
        })
    }

    fn sendAbschiedMessages(&mut self) -> Result<(), &'static str> {
        self.appData.log.print(format!("[INFO] Stoping TCP server"));

        {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for connectionMutex in (*tcpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();

                if connection.isActive {
                    connection.disconnect( DisconnectionReason::ServerShutdown );
                    connection.reregister(&mut self.poll);
                }
            }
        }

        let waitTimeBegin=get_time();

        while waitTimeBegin.sec + ACTIVITY_DISCONNECT_DELAY > get_time().sec {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err("Can not get eventsNumber") ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or("Can not get event" ) );

                self.processEvent(event.token(), event.kind());
            }

            self.disconnectTCPConnectionsFromList();

            self.reregisterConnections();
        }

        Ok(())
    }


    pub fn onServerShutdown(&mut self, sendAbschiedMessage:bool) {
        if sendAbschiedMessage {
            match self.sendAbschiedMessages(){
                Ok ( _ ) => {},
                Err( e ) => self.appData.log.print( format!("[ERROR] Can not send abschiedMessages : {}", e) ),
            }
        }else{
            let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

            for connectionMutex in (*tcpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();

                if connection.isActive {
                    connection.disconnect( DisconnectionReason::FatalError("server error") );
                }
            }
        }

        {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for connectionMutex in (*tcpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();

                (*connection).deregister(&mut self.poll);
            }

            (*tcpConnectionsGuard).clear();
        }

        self.deregister();
    }

    fn deregister(&mut self) {
        self.poll.deregister(
            &self.listener,
        );
    }
}
