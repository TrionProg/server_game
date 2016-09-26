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

use server::{Server,DisconnectionReason,DisconnectionSource};
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
*/

const STATE_WAITSESSIONID_TIMEOUT: i64 = 10;
const STATE_LOGIN_OR_REGISTER_TIMEOUT: i64 = 10;
const STATE_LOGIN_OR_REGISTER_ATTEMPTS_LIMIT: usize = 3;

const MESSAGE_LIMIT_NOT_LOGINED: usize = 16*1024;//256;
const MESSAGE_LIMIT_LOGINED: usize = 16*1024;

enum TCPConnectionState{
    WaitSessionID(i64),
    LoadingPlayerDataFromMasterServer(i64),
    LoginOrRegister(i64, usize),
    CreatingUDPConnection(i64),
    Logined,
}

enum ReadingState{
    ReadingLength ([u8;4]),
    ReadingMessage (usize),
}

pub struct TCPConnection{
    pub token: Token,
    server: Arc<Server>,
    shouldReset: bool,
    isActive:bool,
    pub shouldReregister:bool,
    playerID:Option<u16>,

    socket: TcpStream,

    sendQueue: VecDeque<Vec<u8>>,

    needsToRead:usize,
    readingState:ReadingState,
    buffer:Arc<Mutex<Vec<u8>>>,

    state:TCPConnectionState,
}

pub enum ReadResult{
    NotReady,
    Ready( Option<u16>, Arc<Mutex<Vec<u8>>> ),
    Error( &'static str ),
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

            state:TCPConnectionState::WaitSessionID( get_time().sec+STATE_WAITSESSIONID_TIMEOUT ),
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
                            return ReadResult::Error("read message length error")
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
                            ReadResult::Error("read message length error")
                        }
                    }
                }
            },
            _=>ReadResult::NotReady,
        }
    }

    pub fn writeMessages(&mut self) -> Result<(), &'static str> {
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
    }

    pub fn sendMessage(&mut self, msg:Vec<u8>){
        println!("send: {}",msg.len());

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

        if self.isActive() {
            interest.insert(Ready::readable());
        }else if self.sendQueue.len()==0 { //и отправили прощальное сообщение
            self.shouldReset=true;
            return Ok(());
        }

        if self.sendQueue.len()>0 {
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

    pub fn isActive(&self) -> bool{
        self.isActive
    }

    pub fn shouldReset(&self) -> bool {
        self.shouldReset
    }

    pub fn check(&mut self){
        if self.shouldReset() {
            return ;
        }

        match self.state {
            TCPConnectionState::WaitSessionID( timeout ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionReason::Kick( DisconnectionSource::TCP, String::from("Expectation Session ID timeout")) );
                }
            },
            TCPConnectionState::LoadingPlayerDataFromMasterServer( timeout ) => {
                if get_time().sec>=timeout {//не удалось подключиться к главному серверу - предложим зарегаться или залогиниться
                    //send message
                    self.state=TCPConnectionState::LoginOrRegister( get_time().sec + STATE_LOGIN_OR_REGISTER_TIMEOUT, 0 );
                }
            },
            TCPConnectionState::LoginOrRegister( timeout, attemptsNumber ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionReason::Kick( DisconnectionSource::TCP, String::from("Expectation Login timeout")) );
                }
            },
            TCPConnectionState::CreatingUDPConnection( timeout ) => {
                if get_time().sec>=timeout {
                    self.disconnect( DisconnectionReason::Kick( DisconnectionSource::TCP, String::from("Expectation UDP Connection timeout")) );
                }
            }
            TCPConnectionState::Logined => {},
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

    pub fn disconnect(&mut self, reason:DisconnectionReason){
        println!("disconnect!");

        match reason{
            DisconnectionReason::Error( ref source, ref msg ) => {
                println!("{}",msg);
                match *source{
                    DisconnectionSource::TCP => {
                        self.shouldReset=true;

                        match self.playerID {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {
                        self.sendMessage( ServerToClientTCPPacket::Disconnect{ reason:String::from("Connection error") }.pack() );
                    },
                }
            },
            DisconnectionReason::ConnectionLost( ref source ) => {
                self.shouldReset=true;
                match *source{
                    DisconnectionSource::TCP => {
                        match self.playerID {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {},
                }
            },
            DisconnectionReason::Kick( ref source, ref message ) => {//только от игрока??
                match *source{
                    DisconnectionSource::TCP => {
                        match self.playerID {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {},
                }

                self.sendMessage( ServerToClientTCPPacket::Disconnect{ reason:message.clone() }.pack() );
            },
            DisconnectionReason::ServerShutdown => {
                match self.playerID {
                    Some( playerID ) =>
                        self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                    None => {},
                }

                self.sendMessage( ServerToClientTCPPacket::Disconnect{ reason:String::from("Server shutdown") }.pack() );
            },
            DisconnectionReason::Hup( ref source ) => {
                self.shouldReset=true;

                match *source{
                    DisconnectionSource::TCP => {
                        match self.playerID {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {},
                }
            },
        }

        self.isActive=false;
        self.playerID=None;
    }

    pub fn deregister(&mut self, poll: &mut Poll) {
        poll.deregister(
            &self.socket
        );
    }
}
