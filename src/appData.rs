use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

//use adminServer::AdminServer;
use log::Log;
use serverConfig::ServerConfig;
use gameState::GameState;
use storage::Storage;
use httpRequester::HTTPRequester;
use server::Server;
use map::Map;


pub struct AppData{
    pub log:Log,
    pub serverConfig:ServerConfig,
    pub isEditor:bool,
    pub gameState:RwLock<GameState>,

    pub storage:RwLock<Option<Arc<Storage>>>,
    pub httpRequester:RwLock<Option<Arc<HTTPRequester>>>,
    pub server: RwLock<Option<Arc<Server>>>,
    pub map:    RwLock<Option<Arc<Map>>>,

    //pub adminServer:RwLock< Option< Arc<AdminServer> > >,
    //pub shouldStop:RwLock<bool>,
}

impl AppData{
    pub fn initialize( serverConfig:ServerConfig, log:Log, isEditor:bool ) -> Arc<AppData> {
        let appData=AppData{
            log:log,
            serverConfig:serverConfig,
            isEditor:isEditor,
            gameState:RwLock::new(GameState::Initializing),

            storage:RwLock::new(None),
            httpRequester:RwLock::new(None),
            server: RwLock::new(None),
            map:    RwLock::new(None),

            //adminServer:RwLock::new(None),
            //shouldStop:RwLock::new(false),
        };

        Arc::new(appData)
    }

    pub fn destroy( appData:Arc<AppData> ) {
        //==================Stop the server==================
        let server=(*appData.server.read().unwrap()).clone();

        match server{
            Some ( s ) => Server::stop(s),
            None=>{},
        }

        //==================Stop the httpRequester==================
        let httpRequester=(*appData.httpRequester.read().unwrap()).clone();

        match httpRequester{
            Some ( r ) => HTTPRequester::destroy(r),
            None=>{},
        }

        //==================Destroy storage==================
        let storage=(*appData.storage.read().unwrap()).clone();

        match storage{
            Some ( m ) => Storage::destroy(m),
            None=>{},
        }
    }

    pub fn getHTTPRequesterAnd<T,F>(&self, f:F) -> T where F:FnOnce(&HTTPRequester) -> T {
        match *self.httpRequester.read().unwrap(){
            Some( ref httpRequester) => {
                f( httpRequester )
            },
            None=>panic!("No web httpRequester"),
        }
    }
}
