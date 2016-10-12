use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io::{self, ErrorKind};
use std::rc::Rc;

use mio::*;
use mio::udp::*;
use slab::Slab;
use std::net::SocketAddr;

use time::{get_time};
use std::time::Duration;

use appData::AppData;
use server::{Server, DisconnectionReason, DisconnectionSource};
use server::ServerState;

use tcpConnection::{TCPConnection, ReadResult, TCPConnectionStage};
use udpConnection::UDPConnection;
use player::Player;

use packet::{ServerToClientUDPPacket, ClientToServerUDPPacket};

use rand::random;

pub const UDP_DATAGRAM_LENGTH_LIMIT:usize = 4*1024;

pub struct UDPSocket{
    pub socket: UdpSocket, //listening socket
    pub token: Token,
}

pub struct UDPServer{
    pub appData:Arc<AppData>,
    pub server:Arc<Server>,
    pub poll: Poll,
    pub events: Events, // a list of events to process
    readBuffer:Vec<u8>,

    pub sessions:Vec<u64>,
    tickTime:i64,
}

impl UDPServer{
    pub fn process(&mut self) -> Result<(), &'static str>{
        try!(self.server.udpSocket.lock().unwrap().register(&mut self.poll).or(Err( "Can not register server poll" ) ) );

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

        self.appData.log.print(format!("[INFO] UDP server is ready"));

        while {*self.server.state.read().unwrap()}==ServerState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err( "Can not get eventsNumber" ) ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or( "Can not get event" ) );

                try!(self.processEvent(event.token(), event.kind()));
            }

            self.disconnectUDPConnectionsFromList();

            //try!(self.server.udpSocket.lock().unwrap().reregister(&mut self.poll));

            self.processTick();
        }

        Ok(())
    }

    fn disconnectUDPConnectionsFromList(&mut self){
        let disconnectUDPConnectionsListGuard=self.server.disconnectUDPConnectionsList.lock().unwrap();
        let udpConnectionsGuard=self.server.udpConnections.read().unwrap();

        for &(sessionID, ref reason) in (*disconnectUDPConnectionsListGuard).iter(){
            match (*udpConnectionsGuard).get( sessionID ) {
                Some( udpConnectionMutex ) => {
                    let udpConnectionGuard=udpConnectionMutex.lock().unwrap();
                    (*udpConnectionGuard)._disconnect(reason.clone());
                },
                None => {},
            }
        }

        (*disconnectUDPConnectionsListGuard).clear();
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
            let udpConnectionsGuard=self.server.udpConnections.read().unwrap();

            for connectionMutex in (*udpConnectionsGuard).iter() {
                let mut connection=connectionMutex.lock().unwrap();

                if (*connection).shouldReset {
                    removeConnections.push(( (*connection).session & 0x0000_0000_0000_FFFF) as usize );
                }
            }
        }

        if removeConnections.len()>0 {
            let mut udpConnectionsGuard=self.server.udpConnections.write().unwrap();

            for sessionID in removeConnections {
                self.sessions[sessionID]=0;
                (*udpConnectionsGuard).remove(sessionID);
            }
        }
    }

    fn processEvent(&mut self, token: Token, event: Ready) -> Result<(), &'static str> {
        println!("event!!");
        if event.is_readable() {
            let result=self.server.udpSocket.lock().unwrap().socket.recv_from( &mut self.readBuffer[..] );

            match result{
                Ok (None) => {}
                Ok (Some(( length, clientAddr ))) => {
                    if length>=16 && length<UDP_DATAGRAM_LENGTH_LIMIT {
                        let session=ClientToServerUDPPacket::unpackSession(&self.readBuffer);

                        if session==0 {
                            match self.processAccept( clientAddr ){
                                Ok ( _ ) => {},
                                Err( e ) => self.appData.log.print( format!("[ERROR] UDP Connection acception error : {}", e) ),
                            }
                        }else{
                            let playerID=(session & 0x0000_0000_0000_FFFF) as usize;

                            if playerID<self.sessions.len() && session==self.sessions[playerID] {
                                let time=ClientToServerUDPPacket::unpackTime(&self.readBuffer);

                                match ClientToServerUDPPacket::unpack(&self.readBuffer) {
                                    Ok ( ref packet ) => {
                                        self.server.getSafePlayerAnd(playerID, |player| player.processDatagram(packet, time));
                                    },
                                    Err( e ) => self.appData.log.print( format!("[ERROR] Player {} : {}", playerID, e) ),
                                }
                            }
                        }
                    }
                },
                Err( e ) => return Err( "UDP Socket read error" ),
            }
        }

        Ok(())
    }

    fn processAccept(&mut self, clientAddr:SocketAddr) -> Result<(), &'static str> {
        //в редкой сетуации, когда TCPConnection есть(прошло более 1 сек), а UDP и Player нет, лучше отказать, пусть попросит еще раз
        //locks tcp/udp/players at the same time - may cause deadlock, особенно со стороны disconnect, от которого и все лочится
        let packet=try!( ClientToServerUDPPacket::unpack(&self.readBuffer) );

        let sessionID=match packet{
            ClientToServerUDPPacket::Initialization( sessionID ) => sessionID,
            _=>return Err("Expected only ClientToServerUDPPacket::Initialization packet"),
        };

        if sessionID<self.sessions.len(){
            return Err("too much sessionID");
        }

        if self.sessions[sessionID]!=0 {//self.sessions и self.server.udpConnections чистятся одним потоком в одной функции, поэтому существование UDPConnection гарантировано
            let udpConnectionsGuard=self.server.udpConnections.read().unwrap();
            let connection=(*udpConnectionsGuard)[sessionID].lock().unwrap();

            if !connection.shouldReset{
                //send package
                return Ok(())
            }else{
                return Err("Inactive UDP Connection still exists");
            }
        }

        {

            //GUARD лочим сперва TCP Connections, чтобы всякие trydisconnect если и вызвались, то записались бы в очереди
            let tcpConnectionsGuard=self.server.tcpConnections.read().unwrap();

            match (*tcpConnectionsGuard).get( Token(sessionID) ) {
                Some( tcpConnectionMutex ) => {
                    let connection=tcpConnectionMutex.lock().unwrap();

                    if !(*connection).isActive {
                        return Err("no active TCP Connection");
                    }
                },
                None => return Err("no active TCP Connection"),
            }

            let tcpConnectionGuard=(*tcpConnectionsGuard)[ Token(sessionID) ].lock().unwrap();

            let (userID, userName) = match (*tcpConnectionGuard).stage{
                TCPConnectionStage::UDPConnectionInitialization ( _, ref userID, ref userName) =>
                    (*userID, userName.clone()),
                _=>
                    return Err("stage is no UDP Initialization"),
            };

            {
                //GUARD
                let playersGuard=self.server.players.read().unwrap();

                if (*playersGuard).contains(sessionID){
                    return Err("Inactive Player still exists");
                }
            }

            (*tcpConnectionGuard).stage=TCPConnectionStage::Playing;

            let randomBytes = (rand::random::<u64>()%0xFFFF_FFFF_FFFE+1)<<16; //>0

            let session=randomBytes+sessionID as u64;

            self.sessions[sessionID]=session;

            {
                let udpConnectionsGuard=self.server.udpConnections.write().unwrap();

                let udpConnection=UDPConnection::new(session, clientAddr);
                (*udpConnectionsGuard).insert_at(sessionID,Mutex::new(udpConnection));
            }

            {
                let playersGuard=self.server.players.write().unwrap();
                let player=Player::new(self.server.clone(), sessionID, userID, userName);
                (*playersGuard).insert_at(sessionID,RwLock::new(player));
            }
        }

        //send package
        return Ok(())
    }

    pub fn onServerShutdown(&mut self, sendAbschiedMessage:bool) -> Result<(),String> {
        self.server.udpSocket.lock().unwrap().deregister(&mut self.poll);

        Ok(())
    }
}

impl UDPSocket{
    pub fn new(addr:SocketAddr) -> Result<UDPSocket, String>{
        let socketAddress=String::from("0.0.0.0:1945");
        let socketAddr=try!((&socketAddress).parse::<SocketAddr>().or( Err(format!("Can bind udp socket address : {}", &socketAddress)) ));
        //можно и сокет сервера

        let udpSocket = match UdpSocket::bind( &socketAddr) {
            Ok ( socket ) => socket,
            Err( e ) => return Err( format!("UDP Socket error : {:?}",e) ),
        };

        Ok(
            UDPSocket{
                socket:udpSocket,
                token:Token(20_000_000),
            }
        )
    }

    /*
    //вообще на стороне клиента должно приниматься сообщение целиком
    pub fn send(&mut self, msg:&Vec<u8>, important:bool) -> Result<bool,String> {
        let sent=match self.socket.send_to(&msg[..], &self.serverAddr){
            Ok ( s ) => {
                match s {
                    None => false,
                    Some( n ) => n==msg.len(),
                }
            },
            Err( e ) => return Err(format!("{:?}",e)),
        };

        if sent {
            return Ok(true);
        }

        println!("not sent");

        if important {
            self.importantMessages.push_front(msg.clone());
        }

        Ok(false)
    }
    */

    pub fn register(&mut self, poll:&mut Poll) -> Result<(), &'static str> {
        let interest=Ready::readable();//Ready::readable();

        poll.register(
            &self.socket,
            self.token,
            interest,
            PollOpt::edge()
        ).or_else(|e|
            Err("Can not register UDP Server")
        )
    }

    pub fn deregister(&self, poll:&mut Poll ) {
        poll.deregister(
            &self.socket
        );
    }
}
