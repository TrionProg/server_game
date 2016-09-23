use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use appData::AppData;

pub struct Storage{
    pub appData:Weak<AppData>,
}

impl Storage{
    pub fn initialize( appData:Arc<AppData> ) -> bool{
        appData.log.print( format!("[INFO] Initializing Storage") );

        let storage=Storage{
            appData:Arc::downgrade(&appData),
        };

        let storage=Arc::new(storage);

        *appData.storage.write().unwrap()=Some(storage);

        appData.log.print( format!("[INFO] Storage has been initialized successfully") );
        true
    }

    pub fn destroy( storage:Arc<Storage> ){
        let appData=storage.appData.upgrade().unwrap();
        appData.log.print( format!("[INFO] Destroying storage") );
        *appData.storage.write().unwrap()=None;
        appData.log.print( format!("[INFO] Storage has been destroyed") );
    }
}
