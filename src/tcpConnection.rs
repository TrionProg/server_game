use std::thread;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};

use byteorder::{ByteOrder, BigEndian};

use mio::*;
use mio::tcp::*;

use std::collections::VecDeque;

use time::Timespec;
use time::get_time;
//use std::time::Duration;

use server::{Server, DisconnectionReason, DisconnectionSource};
use tcpServer::TCPServer;

use packet::{ServerToClientTCPPacket, ClientToServerTCPPacket};

/*
причины disconnect:
Error (detected by TCP)
    помечаем shouldReset и, если есть игрок, уничтожаем UDP connection, отправляя сообщение
Error (detected by UDP)
    помечаем isActive=false, отправляем сообщение
ConnectionLost(detected by TCP)
    помечаем shouldReset и, если есть игрок, уничтожаем UDP connection, не отправляя сообщение
ConnectionLost(detected by UDP)
    помечаем shouldReset, не отправляем сообщение
Kick
    помечаем isActive=false, отправляем сообщение


read может возвратить лишь часть буфера, причем в реальной сети его рубят на куски, поэтому надо юзать отдельный буфер для каждого клиента
take позволяет ограничить кол-во читаемых байт

match self.socket.write(&message[..]) { а не рвет ли он сообщения?! придется тогда делать writeContinuation
*/

const STATE_WAITING_SESSIONID_TIMEOUT: usize = 10;
const STATE_LOADING_PLAYER_DATA_FROM_MASTER_SERVER_TIMEOUT: usize = 10;
const STATE_LOGIN_OR_REGISTER_TIMEOUT: usize = 10;
const STATE_LOGIN_OR_REGISTER_ATTEMPTS_LIMIT: usize = 3;

const STATE_INITIALIZING_UDP_CONNECTION_TIMEOUT: usize = 5;

const MESSAGE_LIMIT_NOT_LOGINED: usize = 16*1024;//256;
const MESSAGE_LIMIT_LOGINED: usize = 16*1024;

const MASTERSERVER_ADDRESS:&'static str = "89.110.48.1:1941";

#[derive(PartialEq, Eq, Clone)]
enum TCPConnectionState{
    Disconnecting( DisconnectionReason ),
    WaitingSessionID(i64),
    LoadingPlayerDataFromMasterServer(i64),
    LoginOrRegister(i64, usize),
    InitializingUDPConnection(i64, usize),
    Logined,
}

enum ReadingState{
    ReadingLength ([u8;4]),
    ReadingMessage (usize),
}

pub struct TCPConnection{
    pub token: Token,
    server: Arc<Server>,
    pub shouldReset: bool,
    pub isActive:bool,
    pub shouldReregister:bool,
    playerID:Option<u16>,

    socket: TcpStream,

    sendQueue: VecDeque<Vec<u8>>,

    needsToRead:usize,
    readingState:ReadingState,
    buffer:Arc<Mutex<Vec<u8>>>,

    writeBuffer:Option<Vec<u8>>,
    needsToWrite:usize,

    state:TCPConnectionState,
}

pub enum ReadResult{
    NotReady,
    Ready( Option<u16>, Arc<Mutex<Vec<u8>>> ),
    Error( &'static str ),
    FatalError( &'static str ),
}

impl TCPConnection{
    pub fn new(socket: TcpStream, token: Token, server:Arc<Server>) -> TCPConnection {
        TCPConnection {
            token: token,
            server: server,
            shouldReset: false,
            isActive:true,
            shouldReregister:false,
            playerID:None,

            socket: socket,

            sendQueue: VecDeque::with_capacity(32),

            needsToRead:4,
            readingState:ReadingState::ReadingLength([0;4]),
            buffer:Arc::new(Mutex::new(Vec::with_capacity(1024))),

            writeBuffer:None,
            needsToWrite:0,

            state:TCPConnectionState::WaitingSessionID( get_time().sec+STATE_WAITING_SESSIONID_TIMEOUT as i64 ),
        }
    }

    pub fn readMessage(&mut self) -> ReadResult {
        let sockRef = <TcpStream as Read>::by_ref(&mut self.socket);

        match self.readingState{
            ReadingState::ReadingLength(mut buffer) => {
                let mut needsToRead=self.needsToRead;

                match sockRef.take(needsToRead as u64).read(&mut buffer[4-needsToRead..needsToRead]) {
                    Ok(n) => {
                        needsToRead-=n;

                        if needsToRead==0 {
                            let messageLength = BigEndian::read_u32(buffer.as_ref()) as usize;

                            match self.state {
                                TCPConnectionState::Logined => {
                                    if messageLength>MESSAGE_LIMIT_LOGINED {
                                        return ReadResult::Error("Message length is too large");
                                    }
                                },
                                _=>{
                                    if messageLength>MESSAGE_LIMIT_NOT_LOGINED {
                                        return ReadResult::Error("Message length is too large");
                                    }
                                },
                            }

                            self.readingState=ReadingState::ReadingMessage(messageLength);
                            self.needsToRead=messageLength;
                            //self.buffer.lock().unwrap().clear();
                            unsafe { self.buffer.lock().unwrap().set_len(messageLength); }
                        }else{
                            self.readingState=ReadingState::ReadingLength(buffer);
                            self.needsToRead=needsToRead;

                            return ReadResult::NotReady; // Try to read message length on next event
                        }
                    },
                    Err(e) => {
                        if e.kind() == ErrorKind::WouldBlock { // Try to read message length on next event
                            return ReadResult::NotReady;
                        } else {
                            return ReadResult::FatalError("read message length error")
                        }
                    }
                }
            },
            _=>{},
        }

        match self.readingState{
            ReadingState::ReadingMessage(messageLength) => {
                let mut needsToRead=self.needsToRead;

                let mut bufferGuard=self.buffer.lock().unwrap();

                match sockRef.take(needsToRead as u64).read(&mut (*bufferGuard)[messageLength-needsToRead..messageLength]) {
                    Ok(n) => {
                        println!("got:{} = {}/{}",n,messageLength-needsToRead+n,messageLength);

                        needsToRead-=n;

                        if needsToRead==0 {
                            self.readingState=ReadingState::ReadingLength([0;4]);
                            self.needsToRead=4;

                            ReadResult::Ready( self.playerID, self.buffer.clone() )
                        }else{
                            self.needsToRead=needsToRead;

                            ReadResult::NotReady
                        }
                    },
                    Err(e) => {
                        if e.kind() == ErrorKind::WouldBlock { // Try to read message on next event
                            ReadResult::NotReady
                        } else {
                            ReadResult::FatalError("read message length error")
                        }
                    }
                }
            },
            _=>ReadResult::NotReady,
        }
    }

    pub fn writeMessages(&mut self) -> Result<(), &'static str> {
        /*
        while self.sendQueue.len()>0 {
            let message=self.sendQueue.pop_back().unwrap();

            let length=message.len();
            match self.socket.write(&message[..]) {
                Ok(n) => {},
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        self.sendQueue.push_back(message);

                        break;
                    } else {
                        return Err("write message error")
                    }
                }
            }
        }

        Ok(())
        */

        loop{
            match self.writeBuffer{
                Some(ref buffer) => {
                    match self.socket.write(&buffer[buffer.len()-self.needsToWrite..]) {
                        Ok(n) => {
                            self.needsToWrite-=n;

                            if self.needsToWrite!=0 {
                                break;
                            }
                        },
                        Err(e) => {
                            if e.kind() == ErrorKind::WouldBlock {
                                break;
                            } else {
                                return Err("write message error")
                            }
                        }
                    }
                },
                None => {
                    match self.sendQueue.pop_back() {
                        Some(buffer) => {
                            self.needsToWrite=buffer.len();
                            self.writeBuffer=Some(buffer);
                        },
                        None=> break,
                    }
                },
            }

            if self.needsToWrite==0 {
                self.writeBuffer=None;
            }
        }

        Ok(())
    }

    pub fn sendMessage(&mut self, msg:Vec<u8>){
        println!("send: {}",msg.len());

        self.sendQueue.push_front(msg);

        self.shouldReregister=true;
    }

    pub fn sendAbschiedMessage(&mut self, msg:Vec<u8>){
        self.sendQueue.clear();
        self.sendQueue.push_front(msg);

        self.shouldReregister=true;
    }

    pub fn register(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        let interest=Ready::hup() | Ready::error() | Ready::readable();

        poll.register(
            &self.socket,
            self.token,
            interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not register connection")
        )
    }

    /// Re-register interest in read events with poll.
    pub fn reregister(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        if !self.shouldReregister || self.shouldReset {
            return Ok(());
        }

        self.shouldReregister=false;

        let mut interest=Ready::hup() | Ready::error();

        if self.isActive {
            interest.insert(Ready::readable());
        }else if self.needsToWrite==0 && self.sendQueue.len()==0 { //и отправили прощальное сообщение
            self.shouldReset=true;
            return Ok(());
        }

        if self.needsToWrite>0 || self.sendQueue.len()>0 {
            interest.insert(Ready::writable());
        }

        poll.reregister(
            &self.socket,
            self.token,
            interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not reregister connection")
        )
    }

    pub fn check(&mut self){
        if self.shouldReset {
            return ;
        }

        match self.state {
            TCPConnectionState::WaitingSessionID( timeout ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionSource::TCP, DisconnectionReason::ServerDesire( String::from("Expectation Session ID timeout")) );
                }
            },
            TCPConnectionState::LoadingPlayerDataFromMasterServer( timeout ) => {
                if get_time().sec>=timeout {//не удалось подключиться к главному серверу - предложим зарегаться или залогиниться
                    self.state=TCPConnectionState::LoginOrRegister( get_time().sec + STATE_LOGIN_OR_REGISTER_TIMEOUT as i64, 0 );
                    self.sendMessage( ServerToClientTCPPacket::LoginOrRegister.pack() );
                }
            },
            TCPConnectionState::LoginOrRegister( timeout, attemptsNumber ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionSource::TCP, DisconnectionReason::ServerDesire( String::from("Expectation Login timeout")) );
                }
            },
            TCPConnectionState::InitializingUDPConnection( timeout, _ ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionSource::TCP, DisconnectionReason::ServerDesire( String::from("Expectation UDP Connection timeout")) );
                }
            }
            TCPConnectionState::Logined => {},
            _=>{},
        }

    }

    /*
    причины disconnect:
    Error (detected by TCP)
        помечаем shouldReset и, если есть игрок, уничтожаем UDP connection, отправляя сообщение
    Error (detected by UDP)
        помечаем isActive=false, отправляем сообщение
    ConnectionLost(detected by TCP)
        помечаем shouldReset и, если есть игрок, уничтожаем UDP connection, не отправляя сообщение
    ConnectionLost(detected by UDP)
        помечаем shouldReset, не отправляем сообщение
    Kick
        помечаем isActive=false, отправляем сообщение
    Other

    */

    pub fn disconnect(&mut self, source:DisconnectionSource, reason:DisconnectionReason){
        println!("disconnect!");

        match reason{
            DisconnectionReason::Hup =>
                self.shouldReset=true,
            DisconnectionReason::ServerShutdown =>
                self.sendAbschiedMessage( ServerToClientTCPPacket::ServerShutdown.pack() ),
            DisconnectionReason::FatalError( msg ) => {
                match source{
                    DisconnectionSource::TCP =>
                        self.shouldReset=true,
                    DisconnectionSource::UDP | DisconnectionSource::Player=>
                        self.sendAbschiedMessage( ServerToClientTCPPacket::ServerError( String::from(msg) ).pack() ),
                }
            },
            DisconnectionReason::ClientDesire( ref msg ) =>
                self.shouldReset=true,
            DisconnectionReason::ServerDesire( ref msg ) =>
                self.sendAbschiedMessage( ServerToClientTCPPacket::ServerDesire( msg.clone() ).pack() ),
            DisconnectionReason::ClientError( ref msg ) =>
                self.shouldReset=true,
            DisconnectionReason::ServerError( ref msg ) =>
                self.sendAbschiedMessage( ServerToClientTCPPacket::ServerError( msg.clone() ).pack() ),
        }

        self.server.appData.upgrade().unwrap().log.print(
            match reason{
                DisconnectionReason::Hup =>
                    String::from("[INFO] Disconnecting : hup"),
                DisconnectionReason::ServerShutdown =>
                    String::from("[INFO] Disconnecting : server shutdown"),
                DisconnectionReason::FatalError( ref msg ) =>
                    format!("[ERROR] Disconnecting : fatal error : {}",msg),
                DisconnectionReason::ClientDesire ( ref msg ) =>
                    format!("[INFO] Disconnecting : client desire : {}",msg),
                DisconnectionReason::ServerDesire ( ref msg ) =>
                    format!("[INFO] Disconnecting : server desire : {}",msg),
                DisconnectionReason::ClientError ( ref msg ) =>
                    format!("[ERROR] Disconnecting : client error : {}",msg),
                DisconnectionReason::ServerError( ref msg ) =>
                    format!("[ERROR] Disconnecting : server error : {}",msg),
            }
        );

        match source{
            DisconnectionSource::TCP => {
                match self.playerID {
                    Some( playerID ) =>
                        self.server.getPlayerAnd(playerID, | player | player.disconnect(source.clone(), reason.clone()) ),
                    None => {},
                }
            },
            DisconnectionSource::UDP | DisconnectionSource::Player=> {},
        }

        self.state=TCPConnectionState::Disconnecting( reason.clone() );
        self.isActive=false;
        self.playerID=None;
    }

    pub fn deregister(&mut self, poll: &mut Poll) {
        poll.deregister(
            &self.socket
        );
    }

    pub fn processPacket(&mut self, packet:&ClientToServerTCPPacket) -> Result<(), String> {
        match *packet{
            ClientToServerTCPPacket::SessionID( ref sessionID ) => {
                match self.state {
                    TCPConnectionState::WaitingSessionID(_) => {},
                    _ => return Err( String::from("unexpected ClientToServerTCPPacket::Session") ),
                }

                if sessionID.len()>0 { //точнее = 256 байт в base64
                    self.state=TCPConnectionState::LoadingPlayerDataFromMasterServer( get_time().sec + STATE_LOADING_PLAYER_DATA_FROM_MASTER_SERVER_TIMEOUT as i64 );

                    let appData=self.server.appData.upgrade().unwrap();

                    let request=format!("GET /getUserIDAndName_sessionID={} HTTP/1.1\r\nHost: {}\r\n\r\n", sessionID, MASTERSERVER_ADDRESS).into_bytes();
                    let token=self.token;

                    appData.clone().getHTTPRequesterAnd(move |httpRequester| httpRequester.addRequest(
                        MASTERSERVER_ADDRESS,
                        request,
                        STATE_LOADING_PLAYER_DATA_FROM_MASTER_SERVER_TIMEOUT,

                        move |responseCode:usize, buffer:&[u8] | {

                            if responseCode==200 || responseCode==304  {
                                let response=String::from_utf8_lossy(buffer);

                                if response.starts_with("Error:") {
                                    let server=appData.getServerAnd(move | server | {
                                        server.getSafeTCPConnectionAnd(token, |connection| {
                                            connection.disconnect( DisconnectionSource::TCP, DisconnectionReason::ServerError(
                                                String::from("invalid SessionID, try to connect to server from server list again")
                                            ));
                                        });
                                    });
                                }else{
                                    let server=appData.getServerAnd(move | server | {
                                        server.getSafeTCPConnectionAnd(token, |connection| {
                                            match connection.initializeUDPConnection(&*response) {
                                                Ok ( _ ) => {},
                                                Err( e ) => connection.disconnect( DisconnectionSource::TCP, DisconnectionReason::ServerError(e)),
                                            }
                                        });
                                    });
                                }
                            }else{
                                println!("code: {}",responseCode);
                                let server=appData.getServerAnd(move | server | {
                                    server.getSafeTCPConnectionAnd(token, |connection| {
                                        connection.state=TCPConnectionState::LoginOrRegister( get_time().sec + STATE_LOGIN_OR_REGISTER_TIMEOUT as i64, 0 );
                                        connection.sendMessage( ServerToClientTCPPacket::LoginOrRegister.pack() );
                                    });
                                });
                            }
                        }
                    ));
                }else{
                    return Err( String::from("no sessionID") );

                    //Позволить юзеру зарегаться, если он не указал sessionID - сейчас запрещено
                    //self.state=TCPConnectionState::LoginOrRegister( get_time().sec + STATE_LOGIN_OR_REGISTER_TIMEOUT, 0 );
                    //self.sendMessage( ServerToClientTCPPacket::LoginOrRegister.pack() );
                }
            },
            _ => {},
        }

        Ok(())
    }

    fn initializeUDPConnection(&mut self, response:&str ) -> Result<(), String> {
        use description;

        let userID=try!(
            description::parse( response, |root| {
                root.getStringAs::<usize>("userID")
            })
        );

        self.state=TCPConnectionState::InitializingUDPConnection( get_time().sec + STATE_INITIALIZING_UDP_CONNECTION_TIMEOUT as i64, userID );

        let token:usize=usize::from(self.token);
        self.sendMessage( ServerToClientTCPPacket::InitializeUDPConnection(token).pack() );

        Ok(())
    }


}
