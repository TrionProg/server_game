use bincode::rustc_serialize::{encode_into, decode};
use bincode::SizeLimit;
use byteorder::{ByteOrder, BigEndian};

const MESSAGE_TO_SERVER_LIMIT: u64 = 16*1024;
const MESSAGE_TO_CLIENT_LIMIT: u64 = 64*1024;

#[derive(RustcEncodable, RustcDecodable)]
pub enum ClientToServerTCPPacket{
    ClientError( String ),
    ClientDesire( String ),

    SessionID( String ),
}

impl ClientToServerTCPPacket{
    pub fn pack(&self) -> Vec<u8>{
        let bufferLength=match *self{
            ClientToServerTCPPacket::ClientError( _ ) => 64,
            ClientToServerTCPPacket::ClientDesire( _ ) => 64,

            ClientToServerTCPPacket::SessionID ( _ ) => 64,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(4); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(MESSAGE_TO_CLIENT_LIMIT) ){
            Ok ( _ ) =>{
                let packetLength=buffer.len() as u32 - 4;
                BigEndian::write_u32(&mut buffer[0..4], packetLength);
            },
            Err( _ ) =>{
                BigEndian::write_u32(&mut buffer[0..4], 0);
                println!("err");
            }

        }

        buffer
    }

    pub fn unpack(message:&Vec<u8>) -> Result<ClientToServerTCPPacket, &'static str>{
        match decode(&message[..]){
            Ok ( p ) => Ok ( p ),
            Err( e ) => Err("deserialization error"),
        }
    }
}



#[derive(RustcEncodable, RustcDecodable)]
pub enum ServerToClientTCPPacket{
    ServerShutdown,
    ServerError( String ),
    ServerDesire( String ),

    LoginOrRegister,
    InitializeUDPConnection( usize ),
}

impl ServerToClientTCPPacket{
    pub fn pack(&self) -> Vec<u8>{
        let bufferLength=match *self{
            ServerToClientTCPPacket::ServerShutdown => 16,
            ServerToClientTCPPacket::ServerError( _ ) => 64,
            ServerToClientTCPPacket::ServerDesire( _ ) => 64,

            ServerToClientTCPPacket::LoginOrRegister => 16,
            ServerToClientTCPPacket::InitializeUDPConnection ( _ ) => 16,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(4); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(MESSAGE_TO_SERVER_LIMIT) ){
            Ok ( _ ) =>{
                let packetLength=buffer.len() as u32 - 4;
                BigEndian::write_u32(&mut buffer[0..4], packetLength);
            },
            Err( _ ) =>
                BigEndian::write_u32(&mut buffer[0..4], 0),
        }

        buffer
    }

    pub fn unpack(message:&Vec<u8>) -> Result<ServerToClientTCPPacket, String>{
        match decode(&message[..]){
            Ok ( p ) => Ok ( p ),
            Err( e ) => Err( String::from("deserialization error") ),
        }
    }
}

#[derive(RustcEncodable, RustcDecodable)]
pub enum ClientToServerUDPPacket{
    Initialization(usize),
}

impl ClientToServerUDPPacket{
    pub fn pack(&self) -> Result< Vec<u8>, String>{
        let bufferLength=match *self{
            ClientToServerUDPPacket::Initialization( _ ) => 32,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(16); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(MESSAGE_TO_CLIENT_LIMIT-16) ){
            Ok ( _ ) =>Ok(buffer),
            Err( e ) =>Err( format!("Packet serialization error : {:?}, (maybe it's length is more than {}?)", e, MESSAGE_TO_CLIENT_LIMIT) ),
        }
    }

    pub fn unpack(message:&Vec<u8>) -> Result<ClientToServerUDPPacket, &'static str>{
        match decode(&message[16..]){
            Ok ( p ) => Ok ( p ),
            Err( e ) => Err("deserialization error"),
        }
    }

    pub fn unpackSession(message:&Vec<u8>) -> u64 {
        BigEndian::read_u64(&message[0..8])
    }

    pub fn unpackTime(message:&Vec<u8>) -> u64 {
        BigEndian::read_u64(&message[8..16])
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ClientToServerUDPPacketAcception{
    Initialization,
}

#[derive(RustcEncodable, RustcDecodable)]
pub enum ServerToClientUDPPacket{
}

/*

#[derive(RustcEncodable, RustcDecodable)]
pub enum ServerToClientUDPPacket{
    ServerShutdown,
    ServerError( String ),
    ServerDesire( String ),

}

impl ServerToClientUDPPacket{
    pub fn pack(&self, token:u16, code:u64) -> Vec<u8>{
        let bufferLength=match *self{
            ServerToClientTCPPacket::ServerShutdown => 16,
            ServerToClientTCPPacket::ServerError( _ ) => 64,
            ServerToClientTCPPacket::ServerDesire( _ ) => 64,

            ServerToClientTCPPacket::LoginOrRegister => 16,
            ServerToClientTCPPacket::InitializeUDPConnection ( _ ) => 16,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(4); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(MESSAGE_TO_SERVER_LIMIT) ){
            Ok ( _ ) =>{
                let packetLength=buffer.len() as u32 - 4;
                BigEndian::write_u32(&mut buffer[0..4], packetLength);
            },
            Err( _ ) =>
                BigEndian::write_u32(&mut buffer[0..4], 0),
        }

        buffer
    }

    pub fn unpack(message:&Vec<u8>) -> Result<ServerToClientTCPPacket, String>{
        match decode(&message[..]){
            Ok ( p ) => Ok ( p ),
            Err( e ) => Err( String::from("deserialization error") ),
        }
    }
}
*/
