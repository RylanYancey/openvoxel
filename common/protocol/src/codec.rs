use std::{
    io::{self, Read, Write},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use bevy::log::{warn, warn_once};
use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::{
    exit::{ExitCode, ExitStatus},
    packet::{ChannelId, Packet},
    session::Session,
};

pub struct UdpEncoder {
    buffer: Vec<u8>,
    socket: Arc<UdpSocket>,
    address: SocketAddr,
}

impl UdpEncoder {
    pub fn new(session: Session, socket: Arc<UdpSocket>, address: SocketAddr) -> Self {
        let mut buffer = Vec::with_capacity(1200);
        buffer.put_u64_le(session.0);
        Self {
            buffer,
            socket,
            address,
        }
    }

    pub fn set_session(&mut self, session: Session) {
        self.buffer[..8].copy_from_slice(&session.0.to_le_bytes());
    }

    pub fn set_address(&mut self, addr: SocketAddr) {
        self.address = addr;
    }

    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }

    pub fn encode(&mut self, channel: ChannelId, data: &[u8]) -> Option<usize> {
        let ret = if self.buffer.len() + data.len() > 1180 {
            self.flush()
        } else {
            None
        };
        self.buffer.put_u16_le(data.len() as u16);
        self.buffer.put_u16_le(channel.0 as u16);
        self.buffer.put_slice(data);
        ret
    }

    pub fn flush(&mut self) -> Option<usize> {
        if self.buffer.len() > 8 {
            let ret = Some(self.buffer.len());
            match self.socket.send_to(&self.buffer, &self.address) {
                Ok(n) => {
                    if n != self.buffer.len() {
                        warn!("[N482] An outgoing UDP Datagram was truncated.");
                    }
                }
                Err(e) => {
                    if !matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) {
                        warn_once!("[N882] Error while sending UDP datagram: '{e}'");
                    }
                    let _ = self.socket.send_to(&self.buffer, &self.address);
                }
            }
            unsafe { self.buffer.set_len(8) }
            ret
        } else {
            None
        }
    }
}

pub struct UdpDecoder {
    buffer: BytesMut,
    socket: Arc<UdpSocket>,
}

impl UdpDecoder {
    pub fn new(socket: Arc<UdpSocket>) -> Self {
        Self {
            buffer: BytesMut::with_capacity(1200),
            socket,
        }
    }

    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }

    pub fn read(&mut self) -> Option<(SocketAddr, Session)> {
        self.buffer.clear();
        self.buffer.reserve(1200);
        let spare = unsafe {
            let spare = self.buffer.spare_capacity_mut();
            std::slice::from_raw_parts_mut(spare.as_mut_ptr().cast::<u8>(), spare.len())
        };

        loop {
            match self.socket.recv_from(spare) {
                Ok((amt, addr)) => {
                    if amt < 8 {
                        return None;
                    } else {
                        unsafe { self.buffer.set_len(amt) }
                        let session = Session(self.buffer.get_u64_le());
                        return Some((addr, session));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                _ => return None,
            }
        }
    }

    pub fn decode(&mut self) -> Option<(ChannelId, Bytes)> {
        if self.buffer.len() >= 4 {
            let len = self.buffer.get_u16_le() as usize;
            let channel = ChannelId(self.buffer.get_u16_le() as usize);
            if self.buffer.len() >= len {
                return Some((channel, self.buffer.split_to(len).freeze()));
            }
        }

        None
    }
}

pub struct TcpEncoder {
    buffer: BytesMut,
}

impl TcpEncoder {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
        }
    }

    pub fn has_unwritten(&self) -> bool {
        self.buffer.len() != 0
    }

    pub fn encode(&mut self, channel: ChannelId, data: &[u8]) {
        self.buffer.reserve(data.len() + 6);
        self.buffer.put_u32_le(data.len() as u32);
        self.buffer.put_u16_le(channel.0 as u16);
        self.buffer.put_slice(data);
    }

    pub fn encode_exit(&mut self, exit: &ExitCode) {
        let len = exit.encoded_len();
        self.buffer.reserve(len);
        self.buffer.put_u32_le(len as u32);
        self.buffer.put_u16_le(ChannelId::EXIT_CODE.0 as u16);
        exit.into_buf(&mut self.buffer);
    }

    pub fn flush<W: Write>(&mut self, mut writer: W) -> Result<usize, ExitCode> {
        let mut amt = 0;
        while self.buffer.has_remaining() {
            match writer.write(&self.buffer) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err((ExitStatus::NetworkError, e).into()),
                Ok(0) => return Err(ExitCode::DISCONNECTED),
                Ok(n) => {
                    self.buffer.advance(n);
                    amt += n;
                }
            }
        }
        Ok(amt)
    }
}

pub struct TcpDecoder {
    buffer: BytesMut,
    header: Option<(usize, ChannelId)>,
    limit: usize,
}

impl TcpDecoder {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
            header: None,
            limit: 1_000_000,
        }
    }

    pub fn collect<R: Read>(
        &mut self,
        mut reader: R,
        session: Session,
        packets: &mut Vec<Packet>,
    ) -> Result<(), ExitCode> {
        while self.read(&mut reader)? != 0 {
            while let Some((payload, channel)) = self.decode()? {
                packets.push(Packet {
                    session,
                    payload,
                    channel,
                })
            }
        }

        Ok(())
    }

    /// Read some bytes into the Buffer. Returns Ok(0) if no bytes are available to read.
    pub fn read<R: Read>(&mut self, mut reader: R) -> Result<usize, ExitCode> {
        self.buffer.reserve(2048);
        let spare = unsafe {
            let spare = self.buffer.spare_capacity_mut();
            std::slice::from_raw_parts_mut(spare.as_mut_ptr().cast::<u8>(), spare.len())
        };

        loop {
            match reader.read(spare) {
                Ok(0) => return Err(ExitStatus::Disconnected.into()),
                Ok(amt) => {
                    unsafe { self.buffer.set_len(self.buffer.len() + amt) }
                    return Ok(amt);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(0),
                Err(e) => {
                    return Err((ExitStatus::NetworkError, "[N002] (Tcp Read Error)", e).into());
                }
            }
        }
    }

    pub fn decode(&mut self) -> Result<Option<(Bytes, ChannelId)>, ExitCode> {
        let (len, channel) = match self.header {
            Some(hdr) => hdr,
            None => {
                // Check if enough bytes in buffer to read headers.
                if self.buffer.len() < 6 {
                    return Ok(None);
                }

                // read length header and channel.
                let len = self.buffer.get_u32_le() as usize;
                let channel = ChannelId(self.buffer.get_u16_le() as usize);

                // check for exit code
                if channel == ChannelId::EXIT_CODE {
                    return Err(ExitCode::from_bytes(&mut self.buffer));
                }

                // check for protocol violation
                if len > self.limit {
                    return Err(ExitCode {
                        status: ExitStatus::ProtocolViolation,
                        short: Some("[N811] TCP Packet too large".into()),
                        long: None,
                    });
                }

                self.header = Some((len, channel));
                (len, channel)
            }
        };

        if self.buffer.len() < len {
            self.buffer.reserve((len - self.buffer.len()).max(2048));
            Ok(None)
        } else {
            self.header = None;
            Ok(Some((self.buffer.split_to(len).freeze(), channel)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATA_1: u64 = 0x7A38C591;
    const TEST_DATA_2: u64 = 0x9FEB3911;

    #[test]
    fn tcp_encode_decode_vec() {
        let mut encoder = TcpEncoder::new();
        let mut decoder = TcpDecoder::new();

        // write test data
        encoder.encode(ChannelId(0), &(TEST_DATA_1).to_ne_bytes());
        encoder.encode(ChannelId(1), &(TEST_DATA_2).to_ne_bytes());

        // flush encoder to vec
        let mut flush = Vec::new();
        assert_eq!(encoder.flush(&mut flush).unwrap(), 28);

        // write vec to the decoder
        assert_eq!(
            decoder
                .read(std::io::Cursor::new(flush.as_slice()))
                .unwrap(),
            28
        );

        // validate first packet
        let (mut p1, c1) = decoder.decode().unwrap().unwrap();
        assert_eq!(c1, ChannelId(0));
        assert_eq!(p1.get_u64_ne(), TEST_DATA_1);

        //  validate second packet
        let (mut p2, c2) = decoder.decode().unwrap().unwrap();
        assert_eq!(c2, ChannelId(1));
        assert_eq!(p2.get_u64_ne(), TEST_DATA_2);
    }

    // #[test]
    // fn tcp_collect() {
    //     let mut encoder = TcpEncoder::new();
    //     let mut decoder = TcpDecoder::new();

    //     // write test data
    //     encoder.encode(ChannelId(0), &(TEST_DATA_1).to_ne_bytes());
    //     encoder.encode(ChannelId(1), &(TEST_DATA_2).to_ne_bytes());

    //     // flush encoder to vec
    //     let mut flush = Vec::new();
    //     assert_eq!(encoder.flush(&mut flush).unwrap(), 28);

    //     // collect packets to vec
    //     let mut packets = Vec::new();
    //     decoder
    //         .collect(
    //             (&mut flush.as_slice()).reader(),
    //             Session::ZERO,
    //             &mut packets,
    //         )
    //         .unwrap();

    //     // get packets from buffer
    //     let p2 = packets.pop().unwrap();
    //     let p1 = packets.pop().unwrap();

    //     // validate packets
    //     assert_eq!(p1.channel, ChannelId(0));
    //     assert_eq!(p1.cast::<u64>(), Some(&TEST_DATA_1));
    //     assert_eq!(p2.channel, ChannelId(1));
    //     assert_eq!(p2.cast::<u64>(), Some(&TEST_DATA_2));
    // }
}
