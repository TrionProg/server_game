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

use server::Server;
use tcpServer::TCPServer;


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

    pub activityTime:Mutex<Timespec>,
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

            activityTime: Mutex::new( get_time() ),
        }
    }

    pub fn readMessage(&self, buffer:&mut Vec<u8>) -> io::Result<Option<u16>> {
        let messageLength = match try!(self.readMessageLength()) {
            Some(ml) => ml,
            None => { return Ok(None); },
        };

        if messageLength == 0 {
            return Ok(None);
        }

        if messageLength > 16*1024 {
            return Err(Error::new(ErrorKind::InvalidData, "Too much length of message"));
        }

        unsafe { buffer.set_len(messageLength as usize); }

        let mut socketGuard=self.socket.lock().unwrap();
        let sockRef = <TcpStream as Read>::by_ref(&mut *socketGuard);

        match sockRef.take(messageLength as u64).read(buffer) {
            Ok(n) => {
                if n < messageLength as usize {
                    return Err(Error::new(ErrorKind::InvalidData, "Did not read enough bytes"));
                }

                *self.readContinuation.lock().unwrap() = None;

                *self.activityTime.lock().unwrap()=get_time();
                //update activityTime

                Ok(*self.playerID.read().unwrap())
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock { // Try to read message on next event
                    println!("continue");
                    *self.readContinuation.lock().unwrap() = Some(messageLength);
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }


    fn readMessageLength(&self) -> io::Result<Option<u32>> {
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
                    return Err(e);
                }
            }
        };

        if bytes!=4 {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid message length"));
        }

        let messageLength = BigEndian::read_u32(buf.as_ref());
        Ok(Some(messageLength))
    }

    pub fn writeMessages(&self) -> io::Result<()> {
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
                        return Err(e)
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

    pub fn disconnect(&self){
        *self.isActive.write().unwrap() = false;

        //sendMessage
    }


    pub fn register(&self, poll: &mut Poll) -> io::Result<()> {
        self.interest.lock().unwrap().insert(Ready::readable());

        poll.register(
            &(*self.socket.lock().unwrap()),
            self.token,
            *self.interest.lock().unwrap(),
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err(e)
        )
    }

    /// Re-register interest in read events with poll.
    pub fn reregister(&self, poll: &mut Poll) -> io::Result<()> {
        poll.reregister(
            &(*self.socket.lock().unwrap()),
            self.token,
            *self.interest.lock().unwrap(),
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err(e)
        )
    }

    pub fn shouldReset(&self) -> bool {
        *self.shouldReset.lock().unwrap()
    }

    pub fn onError(&self) {
        *self.shouldReset.lock().unwrap()=true;
        *self.isActive.write().unwrap()=false;

        let mut playerGuard=self.playerID.write().unwrap();

        match *playerGuard {
            Some( playerID ) => {
                let playersGuard=self.server.players.read().unwrap();

                //(*playersGuard)[playerID].onTCPError();
            },
            None => {},
        }

        *playerGuard=None;
    }

    pub fn disconnect_with(&self, msg:&str){
        *self.isActive.write().unwrap()=false;
        *self.playerID.write().unwrap()=None;

        //sendMessage
    }

    pub fn isActive(&self) -> bool{
        *self.isActive.read().unwrap()
    }
}
