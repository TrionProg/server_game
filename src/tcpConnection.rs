use std::thread;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};
use std::rc::Rc;

use byteorder::{ByteOrder, BigEndian};

use mio::*;
use mio::tcp::*;

use std::collections::VecDeque;

use time::Timespec;
use time::get_time;
//use std::time::Duration;

use server::{Server,DisconnectionReason,DisconnectionSource};
use tcpServer::TCPServer;

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
*/



pub struct TCPConnection{
    pub token: Token,
    server: Arc<Server>,
    shouldReset: Mutex<bool>,
    isActive:RwLock<bool>,
    playerID:RwLock<Option<u16>>,

    socket: Mutex<TcpStream>,
    interest: Mutex< Ready >,
    sendQueue: Mutex< VecDeque<Vec<u8>> >,
    readContinuation:Mutex<Option<u32>>,

    pub timeoutBegin:Mutex<Option<Timespec>>, //вообще для тцп он не нужен(не нужно подтверждать соединение), только для прощального пакета
}


impl TCPConnection{
    pub fn new(socket: TcpStream, token: Token, server:Arc<Server>) -> TCPConnection {
        TCPConnection {
            token: token,
            server: server,
            shouldReset: Mutex::new(false),
            isActive:RwLock::new(true),
            playerID:RwLock::new(None),

            socket: Mutex::new(socket),
            interest: Mutex::new( Ready::hup() ),
            sendQueue: Mutex::new( VecDeque::with_capacity(32) ),
            readContinuation: Mutex::new(None),

            timeoutBegin: Mutex::new( None ),
        }
    }

    pub fn readMessage(&self, buffer:&mut Vec<u8>) -> Result<Option<u16>, &'static str> {
        let messageLength = match try!(self.readMessageLength()) {
            Some(ml) => ml,
            None => { return Ok(None); },
        };

        if messageLength == 0 {
            return Ok(None);
        }

        if messageLength > 16*1024 {
            return Err("Too much length of message max is 16 kbytes");
        }

        unsafe { buffer.set_len(messageLength as usize); }

        let mut socketGuard=self.socket.lock().unwrap();
        let sockRef = <TcpStream as Read>::by_ref(&mut *socketGuard);

        match sockRef.take(messageLength as u64).read(buffer) {
            Ok(n) => {
                if n < messageLength as usize {
                    return Err("Did not read enough bytes");
                }

                *self.readContinuation.lock().unwrap() = None;

                Ok(*self.playerID.read().unwrap())
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock { // Try to read message on next event
                    println!("continue");
                    *self.readContinuation.lock().unwrap() = Some(messageLength);
                    Ok(None)
                } else {
                    Err("read message error")
                }
            }
        }
    }


    fn readMessageLength(&self) -> Result<Option<u32>, &'static str> {
        {
            let readContinuationGuard=self.readContinuation.lock().unwrap();

            match *readContinuationGuard {
                Some(messageLength)  => return Ok(Some(messageLength)),
                None => {},
            }
        }

        let mut socketGuard=self.socket.lock().unwrap();

        let mut buf=[0u8;4];

        let bytes = match (*socketGuard).read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    return Ok(None);
                } else {
                    return Err("Read message length error");
                }
            }
        };

        if bytes!=4 {
            return Err("Invalid message length");
        }

        let messageLength = BigEndian::read_u32(buf.as_ref());
        Ok(Some(messageLength))
    }

    pub fn writeMessages(&self) -> Result<(), &'static str> {
        let mut sendQueueGuard=self.sendQueue.lock().unwrap();
        let mut socketGuard=self.socket.lock().unwrap();

        while sendQueueGuard.len()>0 {
            let message=(*sendQueueGuard).pop_back().unwrap();

            let length=message.len();
            match (*socketGuard).write(&message[..]) {
                Ok(n) => {},
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        (*sendQueueGuard).push_back(message);

                        break;
                    } else {
                        return Err("write message error")
                    }
                }
            }
        }

        if sendQueueGuard.len()==0 {
            self.interest.lock().unwrap().remove(Ready::writable());
        }

        Ok(())
    }

    pub fn sendMessage(&self, msg:Vec<u8>){
        self.sendQueue.lock().unwrap().push_front(msg);

        let mut interestGuard=self.interest.lock().unwrap();

        if !(*interestGuard).is_writable() {
            (*interestGuard).insert(Ready::writable());
        }
    }

    pub fn register(&self, poll: &mut Poll) -> Result<(), &'static str> {
        self.interest.lock().unwrap().insert(Ready::readable());

        poll.register(
            &(*self.socket.lock().unwrap()),
            self.token,
            *self.interest.lock().unwrap(),
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not register connection")
        )
    }

    /// Re-register interest in read events with poll.
    pub fn reregister(&self, poll: &mut Poll) -> Result<(), &'static str> {
        poll.reregister(
            &(*self.socket.lock().unwrap()),
            self.token,
            *self.interest.lock().unwrap(),
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not reregister connection")
        )
    }

    pub fn isActive(&self) -> bool{
        *self.isActive.read().unwrap()
    }

    pub fn shouldReset(&self) -> bool {
        *self.shouldReset.lock().unwrap()
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

    pub fn disconnect(&self, reason:DisconnectionReason){
        println!("disconnect");
        let mut playerGuard=self.playerID.write().unwrap();

        match reason{
            DisconnectionReason::Error( ref source, ref msg ) => {
                match *source{
                    DisconnectionSource::TCP => {
                        *self.shouldReset.lock().unwrap()=true;

                        match *playerGuard {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {
                        //sendMessage
                    },
                }
            },
            DisconnectionReason::ConnectionLost( ref source ) => {
                *self.shouldReset.lock().unwrap()=true;
                match *source{
                    DisconnectionSource::TCP => {
                        match *playerGuard {
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
                        match *playerGuard {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {},
                }

                //send message
            },
            DisconnectionReason::ServerShutdown => {
                match *playerGuard {
                    Some( playerID ) =>
                        self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                    None => {},
                }

                //send message
            },
            DisconnectionReason::Hup( ref source ) => {
                *self.shouldReset.lock().unwrap()=true;

                match *source{
                    DisconnectionSource::TCP => {
                        match *playerGuard {
                            Some( playerID ) =>
                                self.server.getPlayerAnd(playerID, | player | player.disconnect(reason.clone())),
                            None => {},
                        }
                    },
                    DisconnectionSource::UDP | DisconnectionSource::Player=> {},
                }
            },
        }

        *self.isActive.write().unwrap()=false;
        *playerGuard=None;
    }

    pub fn deregister(&self, poll: &mut Poll) {
        poll.deregister(
            &(*self.socket.lock().unwrap())
        );
    }
}
