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
