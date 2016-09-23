
use std::io::prelude::*;
use std::fs::File;

use std::thread;
use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use std::error::Error;

//use webInterface::WebInterface;

pub struct Log{
    pub logFile:Mutex<File>,
    //adminServer
}

impl Log{
    pub fn new(isEditor:bool) -> Result<Log, String> {
        let fileName=if isEditor {
            format!("Logs/editor.txt")
        }else{
            format!("Logs/game.txt")
        };

        let mut file=match File::create(fileName.clone()){
            Ok( cf ) => cf,
            Err( e ) => return Err(format!("Can not write file {} : {}", fileName, e.description())),
        };

        match file.write_all("[LOG]\n".as_bytes()){
            Ok( cf ) => cf,
            Err( e ) => return Err(format!("Can not write file {} : {}", fileName, e.description())),
        };

        Ok(Log{
            logFile:Mutex::new(file),
            //webInterface:RwLock::new( None ),
        })
    }

    pub fn write(&self, text:&str){
        let mut logFile=self.logFile.lock().unwrap();
        logFile.write_all(text.as_bytes());
        logFile.write("\n".as_bytes());
    }

    pub fn print(&self, text:String){
        {
            let mut logFile=self.logFile.lock().unwrap();
            logFile.write_all(text.as_bytes());
            logFile.write("\n".as_bytes());
        }

        /*

        match *self.webInterface.read().unwrap(){
            Some( ref wi ) => {
                match *wi.adminSession.write().unwrap() {
                    Some( ref mut adminSession ) => {
                        adminSession.news.push_str("log:");
                        adminSession.news.push_str(text.as_str());
                        adminSession.news.push_str(";\n");
                    },
                    None => {},
                }
            },
            None=>{},
        }


        */

        println!("{}",text);
    }
}
