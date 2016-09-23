use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

//use adminServer::AdminServer;
use log::Log;
use serverConfig::ServerConfig;
use gameState::GameState;
use storage::Storage;
use server::Server;
use map::Map;


pub struct AppData{
    pub log:Log,
    pub serverConfig:ServerConfig,
    pub isEditor:bool,
    pub gameState:RwLock<GameState>,

    pub storage:RwLock<Option<Arc<Storage>>>,
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
            server: RwLock::new(None),
            map:    RwLock::new(None),

            //adminServer:RwLock::new(None),
            //shouldStop:RwLock::new(false),
        };

        Arc::new(appData)
    }

    pub fn destroy( appData:Arc<AppData> ) {
        let storage=(*appData.storage.read().unwrap()).clone();

        match storage{
            Some ( m ) => Storage::destroy(m),
            None=>{},
        }
    }
}
