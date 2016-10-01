

use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use mio::*;
use mio::tcp::*;
use slab::Slab;
use std::net::SocketAddr;

use std::io;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};

use time::{get_time};
use std::time::Duration;

use std::collections::VecDeque;

use appData::AppData;

const  REQUESTS_LIMIT: usize = 256;
const  REQUEST_SEND_LENGTH: usize = 256;
const  KEEP_ALIVE_TIMEOUT: i64 = 5;

/*
pub trait Handler: FnMut(usize, &[u8]) -> () + Send + Sync + 'static {
    FnMut(usize, &[u8])
    /// Produce a `Response` from a Request, with the possibility of error.
    //fn handle(&self, usize, &[u8]) -> ();
}
*/
//type ProcessCallback = FnMut(usize, &[u8]) -> () + Send + Sync + 'static;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum HTTPRequesterState{
    Initialization,
    Processing,
    Destroy,
    Error,
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum HTTPRequestState{
    WritingRequest,
    ReadingHeader,
    ReadingBody,
    Processing,
    Processed, //may be used again
    Error,
}

struct HTTPRequest{
    pub token: Token,
    pub address:String,
    socket: TcpStream,
    state:HTTPRequestState,
    timeout: i64,

    buffer: Vec<u8>,
    cursor:usize,
    begin:usize,

    responseCode:usize,
    contentLength:usize,

    processCallback:Box<FnMut(usize, &[u8]) -> () + Send + Sync + 'static>,
}

pub struct HTTPRequester{
    pub appData:Weak<AppData>,
    pub state:RwLock<HTTPRequesterState>,

    threadJoinHandle:Mutex<Option<JoinHandle<()>>>,

    addRequests:Mutex<VecDeque<(String, Vec<u8>, usize, Box<FnMut(usize, &[u8]) -> () + Send + Sync + 'static>)>>,
}

struct HTTPRequesterCore{
    appData:Arc<AppData>,
    requester:Arc<HTTPRequester>,
    requests: Slab< HTTPRequest , Token>,
    poll: Poll,
    events: Events,
}

impl HTTPRequester{
    pub fn initialize( appData:Arc<AppData> ) -> Result<(), String> {
        appData.log.print(format!("[INFO] Initializing HTTP Requester"));

        let requester=HTTPRequester{
            appData:Arc::downgrade(&appData),
            state:RwLock::new(HTTPRequesterState::Initialization),

            threadJoinHandle:Mutex::new(None),

            addRequests:Mutex::new(VecDeque::with_capacity(16)),
        };

        let requester=Arc::new(requester);

        let mut httpRequesterCore=HTTPRequesterCore{
            appData:appData.clone(),
            requester:requester.clone(),
            requests:Slab::with_capacity(REQUESTS_LIMIT),
            poll:try!(Poll::new().or( Err(format!("Can not create event poll")) )),
            events:Events::with_capacity(REQUESTS_LIMIT),
        };

        let threadJoinHandle=thread::spawn(move||{
            match httpRequesterCore.process(){
                Ok ( _ ) => { httpRequesterCore.appData.log.print(format!("[INFO] HTTP Requester has been destroyed")); },
                Err( e ) => {
                    httpRequesterCore.appData.log.print( format!("[ERROR] HTTP Requester: {}", e) );

                    *httpRequesterCore.requester.threadJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                    HTTPRequester::destroy(httpRequesterCore.requester);
                }
            }
        });

        while {*requester.state.read().unwrap()}==HTTPRequesterState::Initialization {
            thread::sleep_ms(10);
        }

        if *requester.state.read().unwrap()==HTTPRequesterState::Error {
            return Err( String::from("Error occured") );
        }

        *requester.threadJoinHandle.lock().unwrap()=Some(threadJoinHandle);

        //а теперь добавляем данный модуль
        *appData.httpRequester.write().unwrap()=Some(requester);

        Ok(())
    }

    pub fn destroy( requester:Arc<HTTPRequester> ){
        if {*requester.state.read().unwrap()}==HTTPRequesterState::Processing {
            requester.appData.upgrade().unwrap().log.print( String::from("[INFO] Destroying HTTP Requester") );
        }

        *requester.state.write().unwrap()=HTTPRequesterState::Destroy;

        let appData=requester.appData.upgrade().unwrap();

        *appData.httpRequester.write().unwrap()=None; //не позволим другим трогать HTTP Requester

        match requester.threadJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на HTTP Requester - requester
            None => {},
        }

        //теперь вызовется drop для requester
    }

    pub fn addRequest<PC:FnMut(usize, &[u8]) -> () + Send + Sync + 'static>(&self, address:&str, request:Vec<u8>, timeout:usize, processCallback:PC) {
        self.addRequests.lock().unwrap().push_front( (String::from(address), request, timeout, Box::new(processCallback) ) );
    }
}

impl HTTPRequesterCore{
    fn process( &mut self ) -> Result<(),String>{
        *self.requester.state.write().unwrap()=HTTPRequesterState::Processing;

        while {*self.requester.state.read().unwrap()}==HTTPRequesterState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );

            for i in 0..eventsNumber {
                let event = try!(self.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEvent(event.token(), event.kind());
            }

            self.checkRequests();

            self.addRequests();
        }

        self.onHttpRequesterShutdown();

        Ok(())
    }

    fn processEvent(&mut self, token: Token, event: Ready) {
        //Error with request has been occured
        if event.is_error() {
            self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} error", token));
            self.requests[token].state=HTTPRequestState::Error;

            return;
        }

        //Request has been closed -- response is ready
        if event.is_hup() {
            self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} : connection has been refused", token));
            self.requests[token].state=HTTPRequestState::Error;

            return;
        }

        //Write request
        if event.is_writable() {
            match self.requests[token].writeRequest(&mut self.poll) {
                Ok ( _ ) => {},
                Err( e ) => self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} : {}", token, e)),
            }
        }

        //Read response
        if event.is_readable() {
            match self.requests[token].readResponse(&mut self.poll) {
                Ok ( _ ) => {},
                Err( e ) => self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} : {}", token, e)),
            }
        }
    }

    fn checkRequests(&mut self){
        let mut removeRequests = Vec::new();

        for request in self.requests.iter() {
            let removeRequest=match request.state{
                HTTPRequestState::Error => true,
                _ => get_time().sec >= request.timeout,
            };

            if removeRequest {
                removeRequests.push(request.token);
                request.deregister(&mut self.poll);
            }
        }

        for token in removeRequests {
            self.requests.remove(token);
        }
    }


    fn addRequests(&mut self) {
        //prepare list of processed requests
        let mut vacantRequests=Vec::with_capacity(16);

        for request in self.requests.iter() {
            match request.state{
                HTTPRequestState::Processed => vacantRequests.push( (request.address.clone(), request.token) ),
                _ => {},
            }
        }

        //add requests

        let mut addRequestsGuard=self.requester.addRequests.lock().unwrap();

        for (address, request, timeout, processCallback) in (*addRequestsGuard).pop_back() {
            let mut vacantRequestIndex=None;

            for (index, &( ref reqAddress, ref reqToken )) in vacantRequests.iter().enumerate(){
                if address==*reqAddress {
                    vacantRequestIndex=Some(index);
                    break;
                }
            }

            match vacantRequestIndex {
                Some(index) => {
                    let reqToken=vacantRequests[index].1;

                    match self.requests[reqToken].nextRequest( &mut self.poll, address, request, timeout, processCallback ){
                        Ok ( _ ) => {},
                        Err( e ) => self.appData.log.print( format!("[ERROR] HTTPRequester: Failed to use keep alive request {:?} : {}", reqToken, e) ),
                    }

                    vacantRequests.remove(index);
                },
                None => {
                    let token=match self.requests.vacant_entry() {
                        Some(entry) => {
                            match HTTPRequest::new(entry.index(), address, request, timeout, processCallback ){
                                Ok (httpRequest) => entry.insert( httpRequest ).index(),
                                Err(e) => {
                                    self.appData.log.print( format!("[ERROR] HTTPRequester: {}",e) );
                                    continue;
                                },
                            }
                        },
                        None => {
                            self.appData.log.print( String::from("[WARNING] HTTPRequester: Failed to insert request into slab, I will try later") );
                            (*addRequestsGuard).push_back((address,request,timeout,processCallback));
                            return;
                        }
                    };

                    match self.requests[token].register(&mut self.poll) {
                        Ok(_) => {},
                        Err(e) => {
                            self.appData.log.print( format!("[ERROR] HTTPRequester: Failed to register request {:?} with poll : {:?}", token, e) );
                            self.requests.remove(token);
                        }
                    }
                }
            }

        }
    }

    fn onHttpRequesterShutdown(&mut self){
        for request in self.requests.iter() {
            request.deregister(&mut self.poll);
        }

        self.requests.clear();
    }
}

impl HTTPRequest{
    fn new(token:Token, address:String, request:Vec<u8>, timeout:usize, processCallback:Box<FnMut(usize, &[u8]) -> () + Send + Sync + 'static> ) -> Result<HTTPRequest, String> {
        let addr = try!((&address).parse::<SocketAddr>().or( Err(format!("Can not determine server address : {}", &address)) ));
        let socket=try!(TcpStream::connect(&addr).or( Err(format!("Can not create tcp socket for address : {}", &address)) ));

        Ok(HTTPRequest{
            token: token,
            address: address,
            socket: socket,
            state:HTTPRequestState::WritingRequest,
            timeout: get_time().sec+timeout as i64,

            buffer: request,
            begin:0,
            cursor:0,

            responseCode:0,
            contentLength:0,

            processCallback:processCallback,
        })
    }

    fn nextRequest(&mut self, poll: &mut Poll, address:String, request:Vec<u8>, timeout:usize, processCallback:Box<FnMut(usize, &[u8]) -> () + Send + Sync + 'static> ) -> Result<(), String> {
        let addr = try!((&address).parse::<SocketAddr>().or( Err(format!("Can not determine server address : {}", &address)) ));
        let socket=try!(TcpStream::connect(&addr).or( Err(format!("Can not create tcp socket for address : {}", &address)) ));

        self.address=address;
        self.state=HTTPRequestState::WritingRequest;
        self.timeout=get_time().sec+timeout as i64;
        self.buffer=request;
        self.begin=0;
        self.cursor=0;
        self.responseCode=0;
        self.contentLength=0;

        self.processCallback=processCallback;

        match self.reregister(poll) {
            Ok ( _ ) => Ok(()),
            Err( e ) => Err(String::from(e)),
        }
    }

    fn writeRequest(&mut self, poll: &mut Poll) -> Result<(), &'static str>{
        match self.state {
            HTTPRequestState::WritingRequest => {
                let length=if self.buffer.len()-self.cursor > REQUEST_SEND_LENGTH { REQUEST_SEND_LENGTH } else { self.buffer.len()-self.cursor };

                match self.socket.write(&self.buffer[self.cursor..self.cursor+length]) {
                    Ok(n) => {
                        self.cursor+=length;

                        if self.buffer.len() == self.cursor {
                            self.state=HTTPRequestState::ReadingHeader;

                            self.buffer.clear();
                            self.begin=0;
                            self.cursor=0;

                            self.responseCode=0;
                            self.contentLength=0;
                        }
                    },
                    Err(e) => {
                        if e.kind() != ErrorKind::WouldBlock {
                            self.state=HTTPRequestState::Error;

                            return Err("write error");
                        }
                    }
                }
            },
            _=>{},
        };

        self.reregister( poll );

        Ok(())
    }

    fn readResponse(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        let mut buf=[0u8;256];

        loop{
            match self.socket.read(&mut buf) {
                Ok ( n ) => {
                    if n==0 {
                        break;
                    }

                    self.buffer.extend_from_slice(&buf[0..n])
                },
                Err( e ) => {
                    if e.kind() != ErrorKind::WouldBlock { // Try to read message on next event
                        self.state=HTTPRequestState::Error;

                        return Err("read error");
                    }

                    break;
                },
            }
        }

        loop{
            match self.state {
                HTTPRequestState::ReadingHeader => {
                    let mut lineEnd=None;
                    let mut cur=self.cursor;

                    while cur<self.buffer.len() {
                        if self.buffer[cur]==b'\r' {
                            lineEnd=Some(cur);
                            break;
                        }

                        cur+=1;
                    }

                    match lineEnd {
                        Some( endLinePos ) => {
                            self.parseLine( endLinePos );
                            self.begin=endLinePos+2;
                            self.cursor=endLinePos+2;
                        },
                        None => self.cursor=cur,
                    }

                    if self.cursor>=self.buffer.len() {
                        break;
                    }
                },
                HTTPRequestState::ReadingBody => {
                    let mut cur=self.cursor;

                    while cur<self.buffer.len() {
                        cur+=1;

                        if cur-self.begin==self.contentLength {
                            self.state=HTTPRequestState::Processing;
                            break;
                        }
                    }

                    self.cursor=cur;

                    if self.cursor>=self.buffer.len() {
                        break;
                    }
                },
                _=>break,
            }
        }

        match self.state {
            HTTPRequestState::Processing => {
                (self.processCallback)( self.responseCode, &self.buffer[self.begin..self.cursor] );

                self.state=HTTPRequestState::Processed;
                self.timeout=get_time().sec+KEEP_ALIVE_TIMEOUT;
            },
            HTTPRequestState::Processed | HTTPRequestState::Error => {},
            _=>{
                match self.reregister(poll) {
                    Ok ( _ ) => {},
                    Err( e ) => return Err(e),
                }
            },
        }

        Ok(())
    }

    fn parseLine(&mut self, lineEnd:usize){
        let lineBegin=self.begin;

        if lineBegin==lineEnd {
            if self.contentLength>0 {
                self.state=HTTPRequestState::ReadingBody;
            }else{
                self.state=HTTPRequestState::Processing;
            }

            return;
        }

        let mut cur=lineBegin;

        while self.buffer[cur]==b' ' { //ends on letter or \r
            cur+=1;
        }

        match self.buffer[cur]{
            b'H' => {
                let caption=b"HTTP/1.1 ";

                for i in 0..caption.len(){
                    if self.buffer[cur+i]!=caption[i] {
                        return;
                    }
                }

                cur+=caption.len();

                while self.buffer[cur]==b' ' { cur+=1; }

                let mut responseCode=0;

                for i in 0..3 {
                    responseCode*=10;
                    responseCode+=(self.buffer[cur+i]-b'0') as usize;
                }

                self.responseCode=responseCode;
            },
            b'C' => {
                let caption=b"Content-Length:";

                for i in 0..caption.len(){
                    if self.buffer[cur+i]!=caption[i] {
                        return;
                    }
                }

                cur+=caption.len();

                while self.buffer[cur]==b' ' { cur+=1; }

                let mut contentLength=0;

                while self.buffer[cur]>=b'0' && self.buffer[cur]<=b'9' {
                    contentLength*=10;
                    contentLength+=(self.buffer[cur]-b'0') as usize;

                    cur+=1;
                }

                self.contentLength=contentLength;
            },
            _=> {},
        }
    }

    fn register(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        let interest=Ready::hup() | Ready::error() | Ready::writable();

        poll.register(
            &self.socket,
            self.token,
            interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not register HTTP Request")
        )
    }

    fn reregister(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        match self.state {
            HTTPRequestState::Processing | HTTPRequestState::Processed | HTTPRequestState::Error => return Ok(()),
            _ => {},
        }

        let mut interest=Ready::hup() | Ready::error();

        match self.state {
            HTTPRequestState::WritingRequest => interest.insert(Ready::writable()),
            HTTPRequestState::ReadingHeader | HTTPRequestState::ReadingBody => interest.insert(Ready::readable()),
            _ => {},
        }

        poll.reregister(
            &self.socket,
            self.token,
            interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not reregister HTTP Request")
        )
    }

    pub fn deregister(&self, poll: &mut Poll) {
        poll.deregister(
            &self.socket
        );
    }
}
