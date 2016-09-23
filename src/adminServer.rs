
/*
use std::thread;
use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use std::process::{Command, Stdio};

use std::io::{Write,Read, ErrorKind};
use nanomsg::{Socket, Protocol, Endpoint};

use appData::AppData;

struct Channel{
    socket:Socket,
    endpoint:Endpoint,
}

impl Channel{
    pub fn newPull( fileName:&str ) -> Result<Channel, String>{
        let mut socket = match Socket::new(Protocol::Pull){
            Ok(  s )=>s,
            Err( e )=>return Err( format!("Can not create socket \"{}\" : {}", fileName, e.description()) ),
        };

        socket.set_receive_timeout(5000);

        let mut endpoint = match socket.bind( fileName ){
            Ok(  s )=>s,
            Err( e )=>return Err( format!("Can not create endpoint \"{}\" : {}", fileName, e.description()) ),
        };

        Ok(
            Channel{
                socket:socket,
                endpoint:endpoint,
            }
        )
    }

    pub fn newPush( fileName:&str ) -> Result<Channel, String> {
        let mut socket = match Socket::new(Protocol::Push){
            Ok(  s )=>s,
            Err( e )=>return Err(format!("Can not create socket \"{}\" : {}", fileName, e.description())),
        };

        socket.set_send_timeout(200);

        let mut endpoint = match socket.connect( fileName ){
            Ok(  s )=>s,
            Err( e )=>return Err(format!("Can not create endpoint \"{}\" : {}", fileName, e.description())),
        };

        Ok(
            Channel{
                socket:socket,
                endpoint:endpoint,
            }
        )
    }
}

impl Drop for Channel{
    fn drop(&mut self){
        self.endpoint.shutdown();
    }
}

pub struct AdminServer{
    appData:        Weak<AppData>,
    fromGS:         Mutex<Channel>,
    toGSFileName:   String,
    shouldClose:    Mutex<bool>,
    isRunning:      Mutex<bool>,
}

impl AdminServer{
    pub fn connect( appData:Arc<AppData> ) -> Result<(),String> {
        let toGSFileName=format!("ipc:///tmp/ToGS_{}.ipc",appData.port);
        let mut toGS=try!(Channel::newPull( &toGSFileName ));

        //==========================FromGS====================
        let fromGSFileName=format!("ipc:///tmp/FromGS_{}.ipc",appData.port);
        let mut fromGS=try!(Channel::newPush( &fromGSFileName ));

        fromGS.socket.set_send_timeout(5000);
        match fromGS.socket.write(b"answer:FromGS is ready"){
            Ok ( _ ) => {},
            Err( e ) => return Err( format!("Can not open FromGS: {}", e.description()) ),
        }

        //==========================ToGS======================
        let mut msg=String::new();
        toGS.socket.set_receive_timeout(2000);

        match toGS.socket.read_to_string(&mut msg){
            Ok( _ ) => {
                if msg.as_str()!="answer:ToGS is ready" {
                    return Err( format!("Can not open ToGS: {}", msg) );
                }
            },
            Err( e ) => return Err( format!("Can not open ToGS: {}", e.description()) ),
        }

        fromGS.socket.set_send_timeout(200);
        match fromGS.socket.write(b"answer:IPC is ready"){
            Ok ( _ ) => {},
            Err( e ) => return Err( format!("Can not create IPC: {}", e.description()) ),
        }

        //===========================AdminServer===============

        let adminServer=Arc::new(
            AdminServer{
                appData:Arc::downgrade(&appData),
                fromGS:Mutex::new(fromGS),
                toGSFileName:toGSFileName,
                shouldClose:Mutex::new(false),
                isRunning:Mutex::new(true),
            }
        );

        *appData.adminServer.write().unwrap()=Some(adminServer.clone());

        AdminServer::runThread( appData.clone(), adminServer.clone(), toGS );

        Ok(())
    }

    fn runThread(
        appData:Arc<AppData>, //не даст другим потокам разрушить AppData
        adminServer:Arc<AdminServer>, //не даст другим потокам разрушить AdminServer
        mut toGS:Channel
    ) {
        thread::spawn(move || {
            let mut msg=String::with_capacity(1024);
            toGS.socket.set_receive_timeout(500);

            let thread_adminServer=adminServer.clone();
            let threadJoin=thread::spawn(move || {
                loop{
                    for i in 0..10 {
                        thread::sleep_ms(50);

                        if {*thread_adminServer.shouldClose.lock().unwrap()} {
                            return;
                        }
                    }

                    thread_adminServer.fromGS.lock().unwrap().socket.write(b"online:");
                }
            });

            while !{*adminServer.shouldClose.lock().unwrap()} {
                match toGS.socket.read_to_string(&mut msg){
                    Ok( _ ) => {
                        if msg.as_str()=="close:" {
                            break;
                        }

                        let v: Vec<&str> = msg.splitn(2, ':').collect();

                        if v.len()==2{
                            AdminServer::processToGSCommand( &appData, v[0], v[1] );
                        }else{
                            appData.log.print( format!("[ERROR]ToGS: \"{}\" is no command", msg.as_str()) );
                        }
                    },
                    Err( e ) => {
                        match e.kind() {
                            ErrorKind::TimedOut => {},
                            _=> {
                                appData.log.print( format!("[ERROR]ToGS read error : {}", e.description()) );
                                break;
                            }
                        }
                    },
                }

                msg.clear();
            }

            *adminServer.shouldClose.lock().unwrap()=true;
            threadJoin.join();

            *adminServer.isRunning.lock().unwrap()=false;

            //Выжидает, когда gameServer-ом никто не пользуется, и делает недоступным его использование
            *appData.adminServer.write().unwrap()=None;
            appData.log.print(format!("[INFO]Admin server connection has been closed"));
            //AdminServer разрушается автоматически
        });
    }

    fn processToGSCommand( appData:&Arc<AppData>, commandType:&str, args:&str ){
        match commandType {
            //"answer" => *answer.lock().unwrap()=Some(String::from(v[1])),
            //"print" => appData.log.print( String::from(args) ),
            "cmd" => {
                match args{
                    "stop" => *appData.shouldStop.write().unwrap()=true,
                    _=>{},
                }
            },
            _=>appData.log.print( format!("[ERROR]ToGS: unknown command\"{}\"", args) ),
        }
    }

    fn close(&self){
        *self.shouldClose.lock().unwrap()=true;
        let mut toGSTerminator_socket = Socket::new(Protocol::Push).unwrap();
        toGSTerminator_socket.set_send_timeout(2000);
        let mut toGSTerminator_endpoint = toGSTerminator_socket.connect(&self.toGSFileName).unwrap();
        toGSTerminator_socket.write(b"close:").unwrap();

        while {*self.isRunning.lock().unwrap()} {
            thread::sleep_ms(100);
        }
    }

    pub fn send(&self, commandType:&str, msg:&str ) -> Result<(),String>{
        let msg=format!("{}:{}",commandType,msg);

        match {self.fromGS.lock().unwrap().socket.write( msg.as_bytes() )} {
            Ok ( _ ) => Ok(()),
            Err( e ) => {
                let errorMessage=format!("[ERROR]FromGS Write error : {}",e.description());
                //self.appData.upgrade().unwrap().log.print(errorMessage.clone());
                //not to admin

                self.close();

                Err( errorMessage )
            },
        }
    }

    pub fn stop(&self){
        let appData=self.appData.upgrade().unwrap();

        appData.log.print(format!("[INFO]Stopping game server"));

        self.send("close","");

        self.close();

        appData.log.print(format!("[INFO]Game server has been stoped"));
    }
}
*/


/*
use std::thread;
use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use std::process::{Command, Stdio};

use std::io::{Write,Read};

use toGS::ToGS;
use fromGS::FromGS;
use appData::AppData;

pub struct AdminServer{
    appData:    Weak<AppData>,
    toGS:       ToGS,
    fromGS:     FromGS,
}

impl AdminServer{
    pub fn run( appData:Arc<AppData> ) -> Result<(),String> {
        //appData.log.print(format!("Running game server"))
        let toGS=try!(ToGS::new( appData.clone() ));

        let fromGS=try!(FromGS::new( appData.clone() ));

        try!(fromGS.send("answer", "FromGS is ready"));

        println!("1");

        try!(toGS.waitAnswer("ToGS is ready"));

        println!("2");

        try!(fromGS.send("answer", "IPC is ready"));

        println!("yeah");

        thread::sleep_ms( 10000 );

        let adminServer=Arc::new(
            AdminServer{
                appData:Arc::downgrade(&appData),
                fromGS:fromGS,
                toGS:toGS,
            }
        );

        *appData.adminServer.write().unwrap()=Some(adminServer);

        Ok(())
    }
}
*/
