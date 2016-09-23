use std::thread;
use std::thread::JoinHandle;
use std::sync::{Mutex,Arc,RwLock,Weak};


struct AppData{
    module:RwLock<Option<Arc<Module>>>,

}

#[derive(PartialEq, Eq, Copy, Clone)]
enum ModuleState{
    Initialization,
    Processing,
    Finish,
    Error,
}

struct Module{
    appData:Weak<AppData>,
    threadHandle:Mutex<Option<JoinHandle<()>>>,
    state:RwLock<ModuleState>,
}

impl Module{
    pub fn initialize( appData:Arc<AppData> ) -> bool {
        println!("initialization");

        //если ошибка,то return false, мусор удалится raii

        let module=Module{
            appData:Arc::downgrade(&appData),
            threadHandle:Mutex::new(None),
            state:RwLock::new(ModuleState::Initialization),
        };

        let module=Arc::new(module);
        let threadModule=module.clone();//для потока

        let threadHandle=thread::spawn(move||{
            let appData=threadModule.appData.upgrade().unwrap();// часто будет использоваться
            //что-то еще делаем
            *threadModule.state.write().unwrap()=ModuleState::Processing;

            let mut inThreadErrorOccured=false;

            while {*threadModule.state.read().unwrap()}==ModuleState::Processing {
                thread::sleep_ms(100);

                println!("working");

                /*
                //произошла ошибка
                *threadModule.state.write().unwrap()=ModuleState::Error;
                inThreadErrorOccured=true;
                break;
                */
            }

            if inThreadErrorOccured {
                *threadModule.threadHandle.lock().unwrap()=None; //чтобы не было join самого себя
                Module::finish(threadModule);
            }
            //else уже другой поток вызовет Module::finish, а завершения этого будет ждать
        });

        while {*module.state.read().unwrap()}==ModuleState::Initialization {
            thread::sleep_ms(10);
        }

        if *module.state.read().unwrap()==ModuleState::Error {
            return false;
        }

        *module.threadHandle.lock().unwrap()=Some(threadHandle);

        //а теперь добавляем данный модуль
        *appData.module.write().unwrap()=Some(module);

        println!("initialized");

        true
    }

    pub fn finish( module:Arc<Module> ){
        match *module.state.read().unwrap() {
            ModuleState::Processing => println!("finishing function"),
            ModuleState::Error=>println!("error"),
            _=>{},
        }

        *module.state.write().unwrap()=ModuleState::Finish; //остановим поток правда при этом состояние Error затрется

        let appData=module.appData.upgrade().unwrap();

        *appData.module.write().unwrap()=None; //не позволим другим трогать модуль

        match module.threadHandle.lock().unwrap().take(){
            Some(th) => {th.join();},// подождем, тогда останется лишь одна ссылка на модуль - module
            None => {},
        }

        //теперь вызовется drop для module
    }
}

impl AppData{
    pub fn finish( appData:Arc<AppData> ){
        {
            //удаляем существующие модули(можно вызвать из любого потока)
            let module=(*appData.module.read().unwrap()).clone(); //нужно обязательно не блокировать appData.module тк его будет изменять Module::finish

            match module{
                Some ( m ) => {println!("appData::Finish : finish module"); Module::finish(m);},
                None=>{},
            }
        }
    }
}

fn main(){
    let appData=Arc::new(AppData{
        module:RwLock::new(None),
    });

    if !Module::initialize(appData.clone()) {
        AppData::finish(appData);
        return;
    }

    thread::sleep_ms(1000);
    println!("finishing");
    AppData::finish(appData);
    println!("finished");
} 
