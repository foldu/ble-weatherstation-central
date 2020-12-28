use bytes::{BufMut, BytesMut};
use mqtt::{control::FixedHeader, packet::VariablePacket, Decodable, Encodable};
use std::{convert::TryFrom, io};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Copy, Clone)]
pub(crate) enum MqttDecoder {
    NeedFixedHeader,
    NeedBytes { nbytes: usize },
}

impl Default for MqttDecoder {
    fn default() -> Self {
        Self::NeedFixedHeader
    }
}

impl MqttDecoder {
    fn fixed_header(&self, src: &BytesMut) -> Result<FixedHeader, io::Error> {
        FixedHeader::decode(&mut io::Cursor::new(&src[..]))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn consume_full_packet(
        &mut self,
        src: &mut BytesMut,
        nbytes: usize,
    ) -> Result<Option<VariablePacket>, io::Error> {
        let packet = src.split_to(nbytes + 1);
        log::trace!("{:#?}", packet);
        let ret = VariablePacket::decode(&mut io::Cursor::new(&packet))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        *self = MqttDecoder::NeedFixedHeader;
        Ok(Some(ret))
    }
}

impl Decoder for MqttDecoder {
    type Error = io::Error;

    type Item = VariablePacket;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self {
            MqttDecoder::NeedFixedHeader => {
                if src.len() > 0 {
                    let fixed_header = self.fixed_header(src)?;
                    let nbytes = usize::try_from(fixed_header.remaining_length)
                        .ok()
                        .and_then(|len| len.checked_add(1))
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Packet too big for this system",
                            )
                        })?;

                    if src.len() >= nbytes {
                        self.consume_full_packet(src, nbytes)
                    } else {
                        src.reserve(nbytes);
                        *self = Self::NeedBytes { nbytes };
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            MqttDecoder::NeedBytes { nbytes } => {
                let nbytes = *nbytes;
                if nbytes >= src.len() {
                    self.consume_full_packet(src, nbytes)
                } else {
                    Ok(None)
                }
            }
        }
    }
}

pub(crate) struct MqttEncoder;

impl<T> Encoder<T> for MqttEncoder
where
    T: Encodable,
{
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let nbytes = usize::try_from(item.encoded_length()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Packet too big for this system")
        })?;

        dst.reserve(nbytes);

        item.encode(&mut dst.writer())
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed encoding package"))?;

        Ok(())
    }
}
