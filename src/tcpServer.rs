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
use server::{Server,DisconnectionReason,DisconnectionSource};
use server::ServerState;

use tcpConnection::TCPConnection;

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

    pub buffer: Vec<u8>,
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

        let mut reregisterList=Vec::with_capacity(self.appData.serverConfig.server_connectionsLimit);

        while {*self.server.state.read().unwrap()}==ServerState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );
            //периодически выходить по таймауту

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEvent(event.token(), event.kind(), &mut reregisterList);
            }

            self.reregisterConnections( &mut reregisterList );

            self.processTick();
        }

        self.onServerShutdown()
    }

    fn reregisterConnections(&mut self, reregisterList:&mut Vec<Token>){
        let connectionsGuard=self.server.tcpConnections.read().unwrap();

        for connectionToken in reregisterList.iter(){
            let connection=& (*connectionsGuard)[*connectionToken];

            if connection.isActive() {
                connection.reregister(&mut self.poll).unwrap_or_else(|e| { connection.disconnect(DisconnectionReason::Error(DisconnectionSource::TCP, e)); });
            }
        }

        reregisterList.clear();
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

            for connection in (*tcpConnectionsGuard).iter() {
                let removeConnection=if connection.shouldReset() {
                    true
                /*
                }else{
                    //match connection.
                     if !connection.isActive() && (*connection.activityTime.lock().unwrap()).sec+ACTIVITY_DISCONNECT_DELAY < get_time().sec {
                    true
                }else if (*connection.activityTime.lock().unwrap()).sec+ACTIVITY_CONNECTION_LOST_DELAY < get_time().sec {
                    connection.disconnect(DisconnectionReason::ConnectionLost( DisconnectionSource::TCP ) );
                    true
                */
                }else{
                    false
                };

                if removeConnection {
                    removeConnections.push(connection.token);
                    connection.deregister(&mut self.poll);
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

    fn processEvent(&mut self, token: Token, event: Ready, reregisterConnections: &mut Vec<Token>) {
        //Error with connection has been occured
        if event.is_error() {
            println!("error!!");
            self.server.getTCPConnectionAnd(token, |connection| { connection.disconnect(DisconnectionReason::Error(DisconnectionSource::TCP, "socket error") ); });

            return;
        }

        //Connection has been closed
        if event.is_hup() {
            println!("hup!!");
            self.server.getTCPConnectionAnd(token, |connection| { connection.disconnect(DisconnectionReason::Hup(DisconnectionSource::TCP)); });

            return;
        }

        //Listener has no writeable event, but connections have
        if event.is_writable() {
            println!("write!!");
            self.server.getTCPConnectionAnd(token, |connection| {
                if !connection.shouldReset() { //isActive пропускается, чтобы отправить прощальное сообщение
                    connection.writeMessages().unwrap_or_else(|e| { connection.disconnect(DisconnectionReason::Error(DisconnectionSource::TCP, e) ); });
                }
            });
        }

        //Read event for Listener means we need to accept new connection, else mean read event for connection
        if event.is_readable() {
            if self.token == token {    //accept new connection
                self.processAccept();
            } else {     //process read event for connection[token]
                let buffer=&mut self.buffer;

                let packetResult=self.server.getTCPConnectionAnd(token, |connection| {
                    if connection.isActive() {
                        match connection.readMessage(buffer){
                            Ok ( playerID ) => Ok(playerID),
                            Err( e ) => { connection.disconnect(DisconnectionReason::Error(DisconnectionSource::TCP, e) ); Err(()) },//.unwrap_or_else(|e| { connection.onError(); /*return*/});
                        }
                    }else{
                        Err(())
                    }
                });

                match packetResult{
                    Ok ( None ) => {},
                    Ok ( Some(playerID) ) => {
                        //get player and...
                    },
                    Err( _ ) => {},
                }
            }
        }

        //reregister
        if self.token!=token {
            reregisterConnections.push(token);
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
                    let connection = TCPConnection::new(socket, entry.index(), self.server.clone());
                    entry.insert(connection).index()
                },
                None => {
                    self.appData.log.print( String::from("[ERROR] Server: Failed to insert tcp connection into slab(maybe connectionsLimit has been exceeded") );
                    return;
                }
            };

            match (*connectionsGuard)[token].register(&mut self.poll) {
                Ok(_) => {},
                Err(e) => {
                    self.appData.log.print( format!("[ERROR] Server: Failed to register tcp conenction {:?} connection with poll, {:?}", token, e) );
                    (*connectionsGuard).remove(token);
                }
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

    fn processEventOnServerShutdown(&mut self, token: Token, event: Ready) {
        if event.is_writable() {
            println!("write!!");
            self.server.getTCPConnectionAnd(token, |connection| {
                if !connection.shouldReset() { //isActive пропускается, чтобы отправить прощальное сообщение
                    connection.writeMessages().unwrap_or_else(|e| { connection.disconnect(DisconnectionReason::Error(DisconnectionSource::TCP, e) ); });
                }
            });
        }
    }

    fn onServerShutdown(&mut self) -> Result<(),String> {
        //Disconnect by reason ServerShutdown
        {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for connection in (*tcpConnectionsGuard).iter() {
                if connection.isActive() {
                    connection.disconnect(DisconnectionReason::ServerShutdown);
                }
            }

            (*tcpConnectionsGuard).clear();
        }

        let waitTimeBegin=get_time();

        while waitTimeBegin.sec + ACTIVITY_DISCONNECT_DELAY > get_time().sec {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );
            //периодически выходить по таймауту

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEventOnServerShutdown(event.token(), event.kind());
            }
        }

        {
            let mut tcpConnectionsGuard=self.server.tcpConnections.write().unwrap();

            for connection in (*tcpConnectionsGuard).iter() {
                connection.deregister(&mut self.poll);
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
