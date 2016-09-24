use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io::{self, ErrorKind};
use std::rc::Rc;

use mio::*;
use mio::tcp::*;
use slab::Slab;
use std::net::SocketAddr;

//use connection::Connection;

/*
//Cleaner будет находить неотвечающие или неадекватные соединения, при этом будут ставиться флаги:
//isReset, чтобы другой поток не регистрировал
//byCleaner, который говорит, что

а можно и без Cleaner
при обходе соединений, а их, как известно, не много(однако мутекс может замедлять) можно сразу проверить время, адекватность, например каждые 10 проверок,
и так же каждые 10 проверок удялять, разрегистировать и тд. Это будет весьма надежным, правда нужно избежать того, что вызовется event уже когда
connection был удален

псевдокод

<обрабатываем события и ставим флаги shouldReset, shouldReregister>

{
connections =read.unwrap()

пробегаем по connections
если !shouldReset и shouldReregister перерегистрируем

раз в 10 тиков проверяем адекватность
и при этом
неадекватных заносим в список (Token,shouldUnregister=true)
а просто shouldReset в список (Token,shouldUnregister=false)

}

раз в 10 тиков
удаеляем и разрегистрируем соединения из списка


следует обратить внимание, что connection.read часто вызовет и connection.send, а значит, произойдет deadlock

*/

use appData::AppData;
use player::Player;

use tcpServer::TCPServer;
use tcpConnection::TCPConnection;

//use udpServer::UDPServer;
//use udpConnections::UDPConnection;


#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ServerState{
    Initialization(usize),
    Processing,
    Stop,
    Error,
}

#[derive(PartialEq, Eq, Clone)]
pub enum DisconnectionReason{
    Error (DisconnectionSource, &'static str),
    ConnectionLost(DisconnectionSource),
    Kick ( DisconnectionSource, String),
    ServerShutdown,
    Hup ( DisconnectionSource ),
}

#[derive(PartialEq, Eq, Clone)]
pub enum DisconnectionSource{
    TCP,
    UDP,
    Player,
}

pub struct Server{
    pub appData:Weak<AppData>,
    pub state:RwLock<ServerState>,

    pub tcpServerJoinHandle:Mutex<Option<JoinHandle<()>>>,
    pub udpServerJoinHandle:Mutex<Option<JoinHandle<()>>>,

    pub tcpConnections: RwLock<Slab< TCPConnection , Token>>,
    //udpConnections: RwLock<Slab< UDPConnection , u32>>,
    pub players: RwLock<Slab< Player , usize>>,

    pub reregisterTCPConnections:Mutex<Vec<Token>>,
}

impl Server{
    pub fn start( appData:Arc<AppData> ) -> Result<(), String> {
        appData.log.print(format!("[INFO] Starting TCP and UDP servers"));

        let serverAddress=if appData.isEditor {
            format!("{}:{}", appData.serverConfig.server_address, appData.serverConfig.server_editorPort)
        }else{
            format!("{}:{}", appData.serverConfig.server_address, appData.serverConfig.server_gamePort)
        };

        let server=Server{
            appData:Arc::downgrade(&appData),
            state:RwLock::new(ServerState::Initialization(0)),

            tcpServerJoinHandle:Mutex::new(None),
            udpServerJoinHandle:Mutex::new(None),

            tcpConnections:RwLock::new(Slab::with_capacity(appData.serverConfig.server_connectionsLimit)),
            //udpConnections: RwLock<Slab< UDPConnection , u32>>,
            players:RwLock::new(Slab::with_capacity(appData.serverConfig.server_playersLimit)),

            reregisterTCPConnections:Mutex::new(Vec::with_capacity(appData.serverConfig.server_connectionsLimit)),
        };

        let server=Arc::new(server);

        let addr = try!((&serverAddress).parse::<SocketAddr>().or( Err(format!("Can not use server address : {}", &serverAddress)) ));

        let mut tcpServer=TCPServer{
            appData:appData.clone(),
            server:server.clone(),
            listener:try!(TcpListener::bind(&addr).or( Err(format!("Can not create socket with address : {}", &serverAddress)) )),
            poll:try!(Poll::new().or( Err(format!("Can not create event poll")) )),
            token:Token(10_000_000),
            events:Events::with_capacity(appData.serverConfig.server_connectionsLimit*8),
            checkerCounter:0,
            buffer:Vec::with_capacity(16*1024),
        };

        /*
        let mut udpServer=UDPServer{

        }
        */

        let tcpServerJoinHandle=thread::spawn(move||{
            match tcpServer.process(){
                Ok ( _ ) => { tcpServer.appData.log.print(format!("[INFO] TCP server has been stoped")); },
                Err( e ) => {
                    tcpServer.appData.log.print( format!("[ERROR] TCP Server: {}", e) );

                    *tcpServer.server.tcpServerJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                    Server::stop(tcpServer.server);
                }
            }
        });

        /*
        let udpServerJoinHandle=thread::spawn(move||{
            match udpServer.process(){
                Ok ( _ ) => {},
                Err( e ) => {
                    listener.appData.log.print( format!("[ERROR] Server: {}", e) );

                    *udpServer.server.udpServerJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                    Server::stop(udpServer.server);
                }
            }
        });
        */

        while match *server.state.read().unwrap() { ServerState::Initialization(_) => true, _=>false} {
            thread::sleep_ms(10);
        }

        if *server.state.read().unwrap()==ServerState::Error {
            return Err( String::from("Error occured") );
        }

        *server.tcpServerJoinHandle.lock().unwrap()=Some(tcpServerJoinHandle);
        //*server.udpServerJoinHandle.lock().unwrap()=Some(udpServerJoinHandle);

        //а теперь добавляем данный модуль
        *appData.server.write().unwrap()=Some(server);

        Ok(())
    }

    pub fn stop( server:Arc<Server> ){
        if {*server.state.read().unwrap()}==ServerState::Processing {
            server.appData.upgrade().unwrap().log.print( String::from("[INFO] Stoping the server") );
        }

        *server.state.write().unwrap()=ServerState::Stop;

        let appData=server.appData.upgrade().unwrap();

        *appData.server.write().unwrap()=None; //не позволим другим трогать сервер

        match server.tcpServerJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на сервер - server
            None => {},
        }

        /*
        match server.udpServerJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на сервер - server
            None => {},
        }
        */

        //теперь вызовется drop для server
    }

    pub fn getTCPConnectionAnd<T,F>(&self, token:Token, f:F) -> T where F:FnOnce(&TCPConnection) -> T {
        let tcpConnectionsGuard=self.tcpConnections.read().unwrap();

        f(& (*tcpConnectionsGuard)[token])
    }

    /*
    pub fn getUDPConnectionAnd<T,F>(&self, index:u16, f:F) -> T where F:FnOnce(&mut UDPConnection) -> T {
        let udpConnectionsGuard=self.udpConnections.read().unwrap();

        f(&mut (*udpConnectionsGuard)[token])
    }
    */

    pub fn getPlayerAnd<T,F>(&self, playerID:u16, f:F) -> T where F:FnOnce(&Player) -> T {
        let playersGuard=self.players.read().unwrap();

        f(& (*playersGuard)[playerID as usize])
    }

}
