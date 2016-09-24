
use mio::*;
use mio::tcp::*;
use slab::Slab; //не slab, а slabCash

use std::collections::VecDeque;

const  REQUESTS_LIMIT: i64 = 256;
const  REQUEST_SEND_LENGTH: i64 = 256;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum HTTPRequesterState{
    Initialization,
    Processing,
    Stop,
    Error,
}

struct HTTPRequest{
    pub token: Token,
    socket: TcpStream,
    timeout: i64,
    buffer: Vec<u8>,
    request: Option<String, usize>,
    interest: Ready,
    remove: bool,
}

pub struct HTTPRequester{
    pub appData:Weak<AppData>,
    pub state:RwLock<ServerState>,

    threadJoinHandle:Mutex<Option<JoinHandle<()>>>,

    addRequests:Mutex<VecDeque<(String, Vec<u8>, usize)>>,
}

struct HTTPRequesterCore{
    appData:Arc<AppData>,
    requester::Arc<HTTPRequester>,
    requests: Slab< HTTPRequest , Token>,
    poll: Poll,
    events: Events,
}

impl HTTPRequester{
    pub fn initialize( appData:Arc<AppData> ) -> Result<(), String> {
        appData.log.print(format!("[INFO] Initializing HTTP Requester"));

        let requester=Requester{
            appData:Arc::downgrade(&appData),
            state:RwLock::new(HTTPRequesterState::Initialization),

            threadJoinHandle:Mutex::new(None),

            addRequests:Mutex::new(Vec::with_capacity(16)),
        };

        let requester=Arc::new(requester);

        let mut httpRequesterCore=HTTPRequesterCore{
            appData:appData.clone(),
            requester:requester.clone(),
            requests:Slab::with_capacity(REQUESTS_LIMIT),
            poll:try!(Poll::new().or( Err(format!("Can not create event poll")) )),
            events:Events::with_capacity(REQUESTS_LIMIT),
        };

        let requester=Arc::new(requester);

        let threadJoinHandle=thread::spawn(move||{
            match httpRequesterCore.process(){
                Ok ( _ ) => { httpRequesterCore.appData.log.print(format!("[INFO] HTTP Requester has been stoped")); },
                Err( e ) => {
                    httpRequesterCore.appData.log.print( format!("[ERROR] HTTP Requester: {}", e) );

                    *httpRequesterCore.requester.threadJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                    HTTPRequester::stop(httpRequesterCore.requester);
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
        *appData.httpRequester.write().unwrap()=Some(server);

        Ok(())
    }

    pub fn stop( requester:Arc<HTTPRequester> ){
        if {*requester.state.read().unwrap()}==HTTPRequesterState::Processing {
            requester.appData.upgrade().unwrap().log.print( String::from("[INFO] Stoping HTTP Requester") );
        }

        *requester.state.write().unwrap()=HTTPRequesterState::Stop;

        let appData=requester.appData.upgrade().unwrap();

        *appData.httpRequester.write().unwrap()=None; //не позволим другим трогать HTTP Requester

        match requester.threadJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на HTTP Requester - requester
            None => {},
        }

        //теперь вызовется drop для requester
    }

    pub fn addRequest(&self, address:String, request:Vec<u8>, timeout:u64) -> Result<(), String>{
        addRequests.lock().unwrap().push( (address,request,timeout) );
    }
}

impl HTTPRequesterCore{
    fn process( &mut self ) -> Result<(),String>{
        self.state.write().unwrap()=HTTPRequesterState::Processing;

        self.appData.log.print(format!("[INFO] HTTP Requester server is ready"));

        let mut reregisterList=Vec::with_capacity(REQUESTS_LIMIT);

        while {*self.state.read().unwrap()}==HTTPRequesterState::Processing {
            let eventsNumber=try!(self.poll.poll(&mut self.events, Some(Duration::new(0,100_000_000)) ).or(Err(String::from("Can not get eventsNumber")) ) );

            for i in 0..eventsNumber {
                let event = try!(requster.events.get(i).ok_or(String::from("Can not get event") ) );

                self.processEvent(event.token(), event.kind(), &mut reregisterList);
            }

            self.reregisterRequests( &mut reregisterList );

            self.checkRequests();

            self.addRequests();
        }
    }

    fn processEvent(&mut self, token: Token, event: Ready, reregisterList: &mut Vec<Token>) {
        //Error with request has been occured
        if event.is_error() {
            self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} error", token));
            self.requests[token].remove=true;

            return;
        }

        //Request has been closed -- response is ready
        if event.is_hup() {
            println!("closure!!");
            self.requests[token].remove=true;

            return;
        }

        //Write request
        if event.is_writable() {
            let request=&mut self.requests[token];

            match request.request {
                Some( buffer, ref cursor ) {
                    let length=if buffer.len()-cursor > REQUEST_SEND_LENGTH { REQUEST_SEND_LENGTH } else { buffer.len()-cursor };

                    match request.socket.write(&buffer[cursor..cursor+length]) {
                        Ok(n) => {
                            cursor+=length;

                            if buffer.len() == cursor {
                                request.request=None;
                                request.interest.remove(Ready::writable());
                            }
                        },
                        Err(e) => {
                            if e.kind() != ErrorKind::WouldBlock {
                                self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} write error", token));
                                self.requests[token].remove=true;

                                return;
                            }
                        }
                    }
                },
                None =>
                    request.interest.remove(Ready::writable()),
            }
        }

        //Read response
        if event.is_readable() {
            match request.read_to_end(&mut request.buffer) {
                Ok ( n ) => {},
                Err( e ) => {
                    if e.kind() != ErrorKind::WouldBlock { // Try to read message on next event
                        self.appData.log.print(format!("[ERROR] HTTP Requester : request {:?} read error", token));
                        self.requests[token].remove=true;

                        return;
                    }
                },
            }
        }

        reregisterList.push(token);
    }

    fn reregisterConnections(&mut self, reregisterList:&mut Vec<Token>){
        for requestToken in reregisterList.iter(){
            let request=&mut request[Token];

            if !request.remove {
                request.reregister(&mut self.poll).unwrap_or_else(|e| {
                    self.appData.log.print(format!("[ERROR] HTTP Requester : can not reregiter request {:?}", requestToken));
                    request.remove=true;
                });
            }
        }

        reregisterList.clear();
    }

    fn checkRequests(&mut self){
        let mut removeRequests = Vec::new();

        for request in self.requests.iter() {
            if request.remove || get_time().sec >= request.timeout  {
                removeRequests.push(request.token);
                request.deregister(&mut self.poll);
            }
        }

        for token in removeRequests {
            self.requests.remove(token);
        }
    }


    fn addRequests(&mut self) {
        let addRequestsGuard=self.requester.addRequests.lock().unwrap();

        for (address, request, timeout) in (*addRequestsGuard).pop_back() {
            let token=match self.requests.vacant_entry() {
                Some(entry) => {
                    match HTTPRequest::new(entry.index(), address, request, timeout){
                        Ok (httpRequest) => entry.insert( httpRequest ),
                        Err(e) => {
                            self.appData.log.print( format!("[ERROR] HTTPRequester: {}",e) );
                            continue;
                        },
                    }

                    entry.index()
                },
                None => {
                    self.appData.log.print( String::from("[WARNING] HTTPRequester: Failed to insert request into slab, I will try later") );
                    (*addRequestsGuard).push_back((address,request,timeout));
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

impl HTTPRequest{
    fn new(token:Token, address:String, request:Vec<u8>, timeout:usize) -> Result<HTTPRequest, String> {
        let addr = try!((&address).parse::<SocketAddr>().or( Err(format!("Can not use server address : {}", &address)) ));
        let socket=try!(TcpStream::connect(&addr).or( Err(format!("Can not create tcp socket for address : {}", &address)) ));

        Ok(HTTPRequest{
            token: token,
            socket: socket,
            timeout: get_time().sec+timeout as i64,
            buffer: Vec::with_capacity(128),
            request: Option<request, 0>,
            interest: Ready::all(),
            remove: false,
        })
    }

    pub fn register(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        poll.register(
            &self.socket,
            self.token,
            Ready::all(),//self.interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not register connection")
        )
    }

    pub fn reregister(&mut self, poll: &mut Poll) -> Result<(), &'static str> {
        poll.reregister(
            &self.socket,
            self.token,
            Ready::all(),//self.interest,
            PollOpt::edge() | PollOpt::oneshot()
        ).or_else(|e|
            Err("Can not reregister connection")
        )
    }

    pub fn deregister(&self, poll: &mut Poll) {
        poll.deregister(
            &self.socket
        );
    }
}
