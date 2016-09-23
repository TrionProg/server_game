use std::error::Error;

use std::io;
use std::io::prelude::*;
use std::fs::File;

use std::sync::{Mutex,RwLock,Arc,Barrier,Weak};

use config;

pub struct ServerConfig{
    pub server_adminPort:u16,
    pub server_gamePort:u16,
    pub server_editorPort:u16,
    pub server_address:String,
    pub server_connectionsLimit:usize,
    pub server_playersLimit:usize,
    pub repositories:RwLock<Vec<String>>,
    pub loadMap:String,
    pub generateMap:String,
}

impl ServerConfig{
    pub fn read() -> Result<ServerConfig, String> {

        let mut configFile=match File::open("serverConfig.cfg") {
            Ok( cf ) => cf,
            Err( e ) => return Err(format!("Can not read file \"serverConfig.cfg\" : {}", e.description())),
        };

        let mut content = String::new();
        match configFile.read_to_string(&mut content){
            Ok( c )  => {},
            Err( e ) => return Err(format!("Can not read file \"serverConfig.cfg\" : {}", e.description())),
        }

        let serverConfig: ServerConfig = match config::parse( &content, |root| {

            Ok(
                ServerConfig{
                    server_adminPort:try!(root.getStringAs::<u16>("server.adminPort")),
                    server_gamePort:try!(root.getStringAs::<u16>("server.gamePort")),
                    server_editorPort:try!(root.getStringAs::<u16>("server.editorPort")),
                    server_address:try!(root.getString("server.address")).clone(),
                    server_connectionsLimit:{
                        let connectionsLimit=try!(root.getStringAs::<usize>("server.connectionsLimit"));

                        if connectionsLimit>=1500 {
                            return Err(format!("Too large number of connections: {}", connectionsLimit));
                        }

                        connectionsLimit
                    },
                    server_playersLimit:{
                        let playersLimit=try!(root.getStringAs::<usize>("server.playersLimit"));

                        if playersLimit>=1000 {
                            return Err(format!("Too large number of players: {}", playersLimit));
                        }

                        playersLimit
                    },
                    repositories:{
                        let repositoriesList=try!(root.getList("repositories"));

                        let mut repositories=Vec::new();

                        for repURL in repositoriesList.iter() {
                            repositories.push(try!(repURL.getString()).clone());
                        }

                        RwLock::new(repositories)
                    },
                    loadMap:try!(root.getString("load map")).clone(),
                    generateMap:try!(root.getString("generate map")).clone(),
                }
            )
        }){
            Ok( sc ) => sc,
            Err( e ) => return Err(format!("Can not parse file \"serverConfig.cfg\" : {}", e)),
        };

        Ok(serverConfig)
    }
}
