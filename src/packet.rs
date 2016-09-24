use bincode::rustc_serialize::{encode_into};
use bincode::SizeLimit;
use byteorder::{ByteOrder, BigEndian};

#[derive(RustcEncodable, RustcDecodable)]
pub enum ClientToServerTCPPacket{
    Session{ sessionID:String },
}

#[derive(RustcEncodable, RustcDecodable)]
pub enum ServerToClientTCPPacket{
    Disconnect{ reason:String },

}

impl ServerToClientTCPPacket{
    pub fn pack(&self) -> Vec<u8>{
        let bufferLength=match *self{
            ServerToClientTCPPacket::Disconnect { ref reason } => 256,
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
}
