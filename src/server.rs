use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};

use std::io::{self, ErrorKind};
use std::rc::Rc;

use mio::*;
use mio::tcp::*;
use mio::udp::*;
use slab::Slab;
use std::net::SocketAddr;

use time::get_time;

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


как быть с закрытием извне, по try ошибке в одном из тредов, по еррор ошибке в одном из тредов(udpread tcpaccept)
achtung!! в случае try ошибки не вызовется disconnect у клиентов!
переместить ридбуфер в UDPServer
схема, что такое udpConnection, и как ои отправляют сообщения/disconnect
распознавание юдп коннекшионов в тч к тцпконекшн
надо переделать основную функцию примерно как на клиенте
*/

use appData::AppData;
use player::Player;

use tcpServer::TCPServer;
use tcpConnection::TCPConnection;

use udpServer::{UDPSocket, UDPServer, UDP_DATAGRAM_LENGTH_LIMIT};
use udpConnection::UDPConnection;


#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ServerState{
    Initialization(usize),
    Processing,
    Shutdown,
    TCPError,
    UDPError,
}

#[derive(PartialEq, Eq, Clone)]
pub enum DisconnectionReason{
    Hup,
    ServerShutdown,
    FatalError ( &'static str  ),
    ClientDesire ( String ),
    ServerDesire ( String ),
    ClientError ( String ),
    ServerError( String ),
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

    pub udpSocket:Mutex<UDPSocket>,

    //all have one index - Token
    pub tcpConnections: RwLock<Slab< Mutex<TCPConnection>, Token>>,
    pub udpConnections: RwLock<Slab< Mutex<UDPConnection>, usize>>,
    pub players: RwLock<Slab< RwLock<Player>, usize>>,

    pub playersCount: RwLock<usize>,

    pub disconnectTCPConnectionsList:Mutex<Vec<(usize, DisconnectionReason)>>,
    pub disconnectUDPConnectionsList:Mutex<Vec<(usize, DisconnectionReason)>>,
    pub disconnectPlayersList:Mutex<Vec<(usize, DisconnectionReason)>>,
}

impl Server{
    pub fn start( appData:Arc<AppData> ) -> Result<(), String> {
        appData.log.print(format!("[INFO] Starting TCP and UDP servers"));

        let port=if appData.isEditor {
            appData.serverConfig.server_editorPort
        }else{
            appData.serverConfig.server_gamePort
        };

        let serverAddress=format!("{}:{}", appData.serverConfig.server_address, port);
        let addr = try!((&serverAddress).parse::<SocketAddr>().or( Err(format!("Can not use server address : {}", &serverAddress)) ));

        let udpSocket=try!( UDPSocket::new(addr.clone()) );

        let server=Server{
            appData:Arc::downgrade(&appData),
            state:RwLock::new(ServerState::Initialization(0)),

            tcpServerJoinHandle:Mutex::new(None),
            udpServerJoinHandle:Mutex::new(None),

            udpSocket:Mutex::new(udpSocket),

            tcpConnections:RwLock::new(Slab::with_capacity(appData.serverConfig.server_connectionsLimit)),
            udpConnections:RwLock::new(Slab::with_capacity(appData.serverConfig.server_connectionsLimit)),
            players:RwLock::new(Slab::with_capacity(appData.serverConfig.server_connectionsLimit)),

            playersCount:RwLock::new(0),

            disconnectTCPConnectionsList:Mutex::new(Vec::new()),
            disconnectUDPConnectionsList:Mutex::new(Vec::new()),
            disconnectPlayersList:Mutex::new(Vec::new()),
        };

        let server=Arc::new(server);

        let mut tcpServer=TCPServer{
            appData:appData.clone(),
            server:server.clone(),
            listener:try!(TcpListener::bind(&addr).or( Err(format!("Can not create TCP socket with address : {}", &serverAddress)) )),
            poll:try!(Poll::new().or( Err(format!("Can not create TCP event poll")) )),
            token:Token(10_000_000),
            events:Events::with_capacity(appData.serverConfig.server_connectionsLimit*8),
            tickTime:get_time().sec,
        };


        let mut udpServer=UDPServer{
            appData:appData.clone(),
            server:server.clone(),
            poll:try!(Poll::new().or( Err(format!("Can not create UDP event poll")) )),
            events:Events::with_capacity(appData.serverConfig.server_playersLimit*8),
            readBuffer:Vec::with_capacity(UDP_DATAGRAM_LENGTH_LIMIT),

            sessions:Vec::with_capacity(appData.serverConfig.server_playersLimit),
            tickTime:get_time().sec,
        };

        udpServer.sessions.resize(appData.serverConfig.server_playersLimit, 0);

        let tcpServerJoinHandle=thread::spawn(move||{
            let (sendAbschiedMessage, thisThread)=match tcpServer.process(){
                Err( e ) => {
                    *tcpServer.server.state.write().unwrap()=ServerState::TCPError;
                    tcpServer.appData.log.print( format!("[INFO] TCP server error : {}", e) );
                    (false, true)
                },
                Ok ( _ ) => {
                    let state=*tcpServer.server.state.read().unwrap();

                    match state{
                        ServerState::Shutdown => (true, false),
                        ServerState::UDPError => (false, false),
                        _ => (false,true),
                    }
                }
            };

            tcpServer.onServerShutdown(sendAbschiedMessage);

            tcpServer.appData.log.print(format!("[INFO] TCP server has been stoped"));

            if thisThread {
                *tcpServer.server.tcpServerJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                Server::stop(tcpServer.server);
            }
        });

        let udpServerJoinHandle=thread::spawn(move||{
            let (sendAbschiedMessage, thisThread)=match udpServer.process(){
                Err( e ) => {
                    *udpServer.server.state.write().unwrap()=ServerState::UDPError;
                    udpServer.appData.log.print( format!("[INFO] UDP server error : {}", e) );
                    (false, true)
                },
                Ok ( _ ) => {
                    let state=*udpServer.server.state.read().unwrap();

                    match state{
                        ServerState::Shutdown => (true, false),
                        ServerState::TCPError => (false, false),
                        _ => (false,true),
                    }
                }
            };

            udpServer.onServerShutdown(sendAbschiedMessage);

            udpServer.appData.log.print(format!("[INFO] UDP server has been stoped"));

            if thisThread {
                *udpServer.server.udpServerJoinHandle.lock().unwrap()=None; //чтобы не было join самого себя
                Server::stop(udpServer.server);
            }
        });

        while match *server.state.read().unwrap() { ServerState::Initialization(_) => true, _=>false} {
            thread::sleep_ms(10);
        }

        match *server.state.read().unwrap() {
            ServerState::Processing => {},
            _=> return Err( String::from("Error occured") ),
        }

        *server.tcpServerJoinHandle.lock().unwrap()=Some(tcpServerJoinHandle);
        *server.udpServerJoinHandle.lock().unwrap()=Some(udpServerJoinHandle);

        //а теперь добавляем данный модуль
        *appData.server.write().unwrap()=Some(server);

        Ok(())
    }

    pub fn stop( server:Arc<Server> ){
        if {*server.state.read().unwrap()}==ServerState::Processing {
            server.appData.upgrade().unwrap().log.print( String::from("[INFO] Server shutdown") );
            *server.state.write().unwrap()=ServerState::Shutdown;
        }

        let appData=server.appData.upgrade().unwrap();

        *appData.server.write().unwrap()=None; //не позволим другим трогать сервер

        match server.tcpServerJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на сервер - server
            None => {},
        }

        match server.udpServerJoinHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на сервер - server
            None => {},
        }

        //теперь вызовется drop для server
    }

    pub fn getTCPConnectionAnd<T,F>(&self, token:Token, mut f:F) -> T where F:FnMut(&mut TCPConnection) -> T {
        let tcpConnectionsGuard=self.tcpConnections.read().unwrap();

        let mut connection=(*tcpConnectionsGuard)[token].lock().unwrap();

        f( &mut (*connection) )
    }

    pub fn getSafeTCPConnectionAnd<T,F>(&self, token:Token, mut f:F) -> Option<T> where F:FnMut(&mut TCPConnection) -> T {
        let tcpConnectionsGuard=self.tcpConnections.read().unwrap();

        match (*tcpConnectionsGuard).get(token){
            Some( connectionMutex ) =>
                Some( f( &mut (*connectionMutex).lock().unwrap() ) ),
            None =>
                None,
        }
    }

    pub fn getSafeUDPConnectionAnd<T,F>(&self, sessionID:usize, mut f:F) -> Option<T> where F:FnMut(&mut UDPConnection) -> T {
        let udpConnectionsGuard=self.udpConnections.read().unwrap();

        match (*udpConnectionsGuard).get(sessionID){
            Some( connection ) =>
                Some( f( &mut (*connection).lock().unwrap() ) ),
            None =>
                None,
        }
    }

    pub fn getSafePlayerAnd<T,F>(&self, sessionID:usize, mut f:F) -> Option<T> where F:FnMut(&mut Player) -> T {
        let playersGuard=self.players.read().unwrap();

        match (*playersGuard).get(sessionID){
            Some( player ) =>
                Some( f( &mut (*player).write().unwrap() ) ),
            None =>
                None,
        }
    }

    pub fn isTCPConnectionActive(&self, token:Token ) -> bool {
        let tcpConnectionsGuard=self.tcpConnections.read().unwrap();

        match (*tcpConnectionsGuard).get(token){
            Some( connectionMutex ) =>
                (*connectionMutex.lock().unwrap()).isActive,
            None =>
                false
        }
    }

    pub fn tryDisconnectUDPConnection(&self, sessionID:usize, reason:DisconnectionReason) {
        match self.udpConnections.try_read(){
            Ok( udpConnectionsGuard ) => {
                match (*udpConnectionsGuard).get( sessionID ){
                    Some( udpConnectionMutex ) => {
                        match udpConnectionMutex.try_lock(){
                            Ok( udpConnectionGuard ) =>
                                (*udpConnectionGuard)._disconnect(reason),
                            Err( _ ) =>
                                self.disconnectUDPConnectionsList.lock().unwrap().push( (sessionID,reason) ),
                        }
                    },
                    None => {},
                }
            },
            Err( _ ) =>
                self.disconnectUDPConnectionsList.lock().unwrap().push( (sessionID,reason) ),
        }
    }

    pub fn tryDisconnectTCPConnection(&self, sessionID:usize, reason:DisconnectionReason) {
        match self.tcpConnections.try_read(){
            Ok( tcpConnectionsGuard ) => {
                match (*tcpConnectionsGuard).get( Token(sessionID) ){
                    Some( tcpConnectionMutex ) => {
                        match tcpConnectionMutex.try_lock(){
                            Ok( tcpConnectionGuard ) =>
                                (*tcpConnectionGuard)._disconnect(reason),
                            Err( _ ) =>
                                self.disconnectTCPConnectionsList.lock().unwrap().push( (sessionID,reason) ),
                        }
                    },
                    None => {},
                }
            },
            Err( _ ) =>
                self.disconnectTCPConnectionsList.lock().unwrap().push( (sessionID,reason) ),
        }
    }

    pub fn tryDisconnectPlayer(&self, sessionID:usize, reason:DisconnectionReason) {
        match self.players.try_read(){
            Ok( playersGuard ) => {
                match (*playersGuard).get( sessionID ){
                    Some( playerMutex ) => {
                        match playerMutex.try_write(){
                            Ok( playerGuard ) =>
                                (*playerGuard)._disconnect(reason),
                            Err( _ ) =>
                                self.disconnectPlayersList.lock().unwrap().push( (sessionID,reason) ),
                        }
                    },
                    None => {},
                }
            },
            Err( _ ) =>
                self.disconnectPlayersList.lock().unwrap().push( (sessionID,reason) ),
        }
    }
}
