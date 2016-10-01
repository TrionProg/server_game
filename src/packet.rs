use bincode::rustc_serialize::{encode_into, decode};
use bincode::SizeLimit;
use byteorder::{ByteOrder, BigEndian};

#[derive(RustcEncodable, RustcDecodable)]
pub enum ClientToServerTCPPacket{
    ClientError( String ),
    ClientDesire( String ),

    SessionID{ sessionID:String },
}

impl ClientToServerTCPPacket{
    pub fn pack(&self) -> Vec<u8>{
        let bufferLength=match *self{
            ClientToServerTCPPacket::ClientError( _ ) => 64,
            ClientToServerTCPPacket::ClientDesire( _ ) => 64,

            ClientToServerTCPPacket::SessionID { ref sessionID } => 64,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(4); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(bufferLength as u64 - 4)){
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
}

impl ServerToClientTCPPacket{
    pub fn pack(&self) -> Vec<u8>{
        let bufferLength=match *self{
            ServerToClientTCPPacket::ServerShutdown => 16,
            ServerToClientTCPPacket::ServerError( _ ) => 64,
            ServerToClientTCPPacket::ServerDesire( _ ) => 64,

            ServerToClientTCPPacket::LoginOrRegister => 16,
        };

        let mut buffer:Vec<u8>=Vec::with_capacity(bufferLength);

        unsafe { buffer.set_len(4); }

        match encode_into(self, &mut buffer, SizeLimit::Bounded(bufferLength as u64 - 4)){
            Ok ( _ ) =>{
                let packetLength=buffer.len() as u32 - 4;
                BigEndian::write_u32(&mut buffer[0..4], packetLength);
            },
            Err( _ ) =>
                BigEndian::write_u32(&mut buffer[0..4], 0),
        }

        buffer
    }

    pub fn unpack(message:&Vec<u8>) -> Result<ServerToClientTCPPacket, &'static str>{
        match decode(&message[..]){
            Ok ( p ) => Ok ( p ),
            Err( e ) => Err("deserialization error"),
        }
    }
}
