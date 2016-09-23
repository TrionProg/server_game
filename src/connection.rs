use std::thread;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};
use std::rc::Rc;

//use byteorder::{ByteOrder, BigEndian};

use mio::*;
use mio::tcp::*;

use std::collections::VecDeque;

use time::Timespec;

/*
могут приходить обрубки
смысл write: берем сообщение из очереди и пытаемся отправить, сначала длину, потом сообщение, если происходит ошибка, то останавливаемя на
2х состояниях:
не удалось отправить длину. в этом случае потом отправляем сообщение
удалось отправить длину, но не удалось само сообщение. в этом случае надо сохранить сообщение в буфере и отправить позже

writeContinuation:enum{All(все отправили), Length(только длину, но надо сообщение) } - Option<None(не надо продолжать, Some(надо сообщение отправить)

и так пытаемся отправить все(или все же по-одному тк может еще событие read прити)

read:
пытаемся прочесть сначала длину, потом сообщение, если прочиталось не все сообщение, то оно еще остаётся в буфере, если же целиком, то выполняем событие

Очередь сообщений представляем собой enum, тк чанки не надо, например, сохранять в RAM, их надо прочесть - отдать - забыть, то же касается и истории, ее строки
можно хранить в виде Arc, который будет жить, пока не станет слишком старым и при этом будучи всем отправленным или те коннекшионы обрублены.
недоотправленные сообщения и так хранятся в озу, а чанки пусть пока в буфере где-то полежат
для отправляемых сообщений должен быть спец буфер или все же так или иначе выделать память тк того хочет сериализация?

нормальные пакеты видимо будут представлять собой общий кусок памяти, а чанки, история, будут собираться. Те Packet::Chunk(chunk::load)

склоняюсь пока хранить сами вектора, а не писать их в буфер так как это ооочень все усложняет, а previous optimization is the root of evil,
а скорее всего сами пакеты(но все равно придется vec получатт, что плохо). хз-хз пока.

send(MessageType::Package(Package::Teleport{x:6,z:7}.serialize()));
сам пакет может содержать множество строк, векторов и подобой дряни(в тч ссылки), которую выгодно сразу же перевести в массив(а может сразу и длину, сжатие?)
для read юзается весьма небольшой буфер, но в случае необходимости аллокается.
если чанк не удалось отправить, топушим просто Data с буфером (инфа чанка). при этом помечается, дескать чанк мы отправили и юзер будет получать обновления

MessageType{
Bytes:Vec<>,
Chunk(x,y),
History(Arc),
}
*/

enum Message{
    Buffer(Vec<u8>),//includes length
    Chunk{x:i16,z:i16},
    //History<Arc<HistoryRow>>,
}

pub struct Connection{
    pub token: Token,
    pub shouldReset: Mutex<bool>,
    pub isActive:RwLock<bool>,

    pub tcp:TCPConnection

    //udp send queue and interest


    pub activityTime:Mutex<Timespec>,
}

struct TCPConnection{
    socket: TcpStream,
    interest: Mutex< Ready >,
    sendQueue: Mutex< VecDeque<Message> >,
}

struct UDPConnection{


impl Connection{
    pub fn new(sock: TcpStream, token: Token) -> Connection {
        Connection {
            token: token,
            shouldReset: false,
            isActive:RwLock::new(false),


            tcpSocket: sock,
            interest: Mutex::new( Ready::hup() ),
            sendQueue: Mutex::new( VecDeque::with_capacity(32) ),
            //send_queue: Vec::new(),
            shouldReset: false,
            isActive:RwLock::new(false),
            //read_continuation: None,
            //write_continuation: false,
        }
    }

    pub fn processWritable(&mut self) -> io::Result<()> {
        let sendQueueGuard=self.sendQueue.lock().unwrap();

        while sendQueueGuard.len()>0 {
            let message=(*sendQueueGuard).pop_back();

            match message {
                Message::Buffer( buffer ) => {
                    let length=buffer.len();
                    match self.sock.write(&buffer[..]) {
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
                },
                _=>{},
            }
        }

        if sendQueueGuard.len()==0 {
            self.interest.lock().unwrap().remove(Ready::writable());
        }

        Ok(())
    }

    pub fn sendMessage(&mut self, message: Message) {
        self.sendQueue.lock().unwrap().push_front(message);

        let interestGuard=self.interest.lock.unwrap();

        if !(*interestGuard).is_writable() {
            (*interestGuard).insert(Ready::writable());
        }
    }

    pub fn disconnect(&self){
        self.isActive.write().unwrap() = false;

        sendMessage
    }


    pub fn register(&self, poll: &mut Poll) -> io::Result<()> {
        self.interest.insert(Ready::readable());

        poll.register(
            &self.sock,
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
            &self.sock,
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

    pub fn markShouldReset(&self) {
        *self.shouldReset.lock().unwrap()=true;
        *self.isActive.write().unwrap()=false;
    }

    pub fn isActive(&self) -> bool{
        *self.isActive.read().unwrap()
    }
}
