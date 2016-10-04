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
    pub checkerCounter: usize,
}

impl TCPServer{
    pub fn process(&mut self) -> Result<(),String>{
        try!(self.register().or(Err(String::from("Can not register server poll")) ) );

        {
            let mut serverStateGuard=self.server.state.write().unwrap();

            /*
            if *serverStateGuard==ServerState::Initialization(0) {
                (*serverStateGuard)=ServerState::Initialization(1);
            }else if *serverStateGuard==ServerState::Initialization(1) {
                *serverStateGuard=ServerState::Processing;
            }else{
                return Ok(());//do not call error functions in thread!
            }
            */
            *serverStateGuard=ServerState::Processing;

        }

        self.appData.log.print(format!("[INFO] TCP server is ready"));

        while {*self.server.state.read().unwrap()}==ServerState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEvent(event.token(), event.kind());
            }

            self.reregisterConnections();

            self.processTick();
        }

        self.onServerShutdown()
    }

    fn reregisterConnections(&mut self){
        let connectionsGuard=self.server.tcpConnections.read().unwrap();

        for connectionMutex in (*connectionsGuard).iter() {
            let mut connection=connectionMutex.lock().unwrap();
            (*connection).reregister(&mut self.poll).unwrap_or_else(|e| { (*connection).disconnect(DisconnectionSource::TCP, DisconnectionReason::FatalError(e)); });
        }
    }

    fn processTick(&mut self) {
        self.checkerCounter+=1;

        if self.checkerCounter==10 {
            self.checkerCounter=0;

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

    fn processEvent(&mut self, token: Token, event: Ready) {
        //Error with connection has been occured
        if event.is_error() {
            println!("error!!");
            self.server.getTCPConnectionAnd(token, |connection| {
                if connection.isActive {
                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::FatalError("socket error") );
                }
            });

            return;
        }

        //Connection has been closed
        if event.is_hup() {
            println!("hup!!");
            self.server.getTCPConnectionAnd(token, |connection| {
                if connection.isActive {
                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::Hup);
                }
            });


            return;
        }

        //Listener has no writeable event, but connections have
        if event.is_writable() {
            println!("write!!");
            self.server.getTCPConnectionAnd(token, |connection| {
                if !connection.shouldReset {
                    match connection.writeMessages(){
                        Ok ( _ ) => connection.shouldReregister=true,
                        Err( e ) => connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::FatalError(e) ),
                    }
                }
            });
        }

        //Read event for Listener means we need to accept new connection, else mean read event for connection
        if event.is_readable() {
            if self.token == token {    //accept new connection
                self.processAccept();
            } else {     //process read event for connection[token]
                let readResult={
                    let connectionsGuard=self.server.tcpConnections.read().unwrap();

                    let mut connection=(*connectionsGuard)[token].lock().unwrap();

                    if connection.isActive {
                        let readResult=connection.readMessage();

                        match readResult{
                            ReadResult::FatalError( e ) =>
                                connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::FatalError(e) ),
                            ReadResult::Error( e ) =>
                                connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::ServerError( String::from(e) ) ),
                            _=>
                                connection.shouldReregister=true,
                        }

                        Some(readResult)
                    }else{
                        None
                    }
                };

                match readResult{
                    Some(ReadResult::Ready( playerID, buffer )) => {
                        let bufferGuard=buffer.lock().unwrap();

                        match self.processMessage(token, playerID, &(*bufferGuard)) {
                            Ok ( _ ) => {},
                            Err( e ) => {
                                self.server.getTCPConnectionAnd(token, |connection| {
                                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::ServerError( e.clone() ) )
                                });
                            },
                        }
                    },
                    _=>{},
                }
            }
        }
    }

    fn processMessage(&self, token:Token, playerID:Option<u16>, buffer:&Vec<u8>) -> Result<(), String> {
        let packet=try!(ClientToServerTCPPacket::unpack(buffer));

        let process=match packet{
            ClientToServerTCPPacket::ClientDesire( ref msg ) => {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::ClientDesire(msg.clone()) );
                });

                false
            },
            ClientToServerTCPPacket::ClientError( ref msg ) => {
                self.server.getTCPConnectionAnd(token, |connection| {
                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::ClientError(msg.clone()) );
                });

                false
            },
            _=>
                true,
        };

        if process{
            match playerID {
                Some(playerID) => {
                    //get player and..
                    Ok(())
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
            Ready::readable(),
            PollOpt::edge()
        ).or_else(|e| {
            Err(e)
        })
    }

    fn onServerShutdown(&mut self) -> Result<(),String> {
        //Disconnect by reason ServerShutdown
        {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for connectionMutex in (*tcpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();

                if connection.isActive {
                    connection.disconnect(DisconnectionSource::TCP, DisconnectionReason::ServerShutdown );
                    connection.reregister(&mut self.poll);
                }
            }
        }

        let waitTimeBegin=get_time();

        while waitTimeBegin.sec + ACTIVITY_DISCONNECT_DELAY > get_time().sec {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEvent(event.token(), event.kind());
            }

            self.reregisterConnections();
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

        Ok(())
    }

    fn deregister(&mut self) {
        self.poll.deregister(
            &self.listener,
        );
    }
}
