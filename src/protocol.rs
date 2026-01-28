use std::{
    io::{Error, ErrorKind, Result},
    thread,
    time::Duration,
};

use clap::Subcommand;

use crate::get_time;

pub const REQUEST_SIZE: usize = 17;
pub const RESPONSE_SIZE: usize = 8;

pub struct LatencyRecord {
    pub send_time: u64,
    pub recv_time: u64,
}

/// Represents a client request.
pub struct Request {
    /// The time (in nanoseconds) the request was sent.
    pub send_time: u64,

    /// The work to do.
    pub work: Work,
}

impl Request {
    pub fn serialize(self, bytes: &mut [u8]) {
        bytes[0..8].copy_from_slice(&self.send_time.to_be_bytes());
        self.work.serialize(&mut bytes[8..]);
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        let send_time = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let work = Work::deserialize(&bytes[8..])?;
        Ok(Self { send_time, work })
    }
}

impl Request {
    pub fn do_work(self) -> Response {
        self.work.do_work();
        Response {
            client_send_time: self.send_time,
        }
    }
}

/// Represents a server response.
pub struct Response {
    /// The time (in nanoseconds) the request was sent by the client.
    pub client_send_time: u64,
}

impl Response {
    pub fn serialize(self, bytes: &mut [u8]) {
        bytes[0..8].copy_from_slice(&self.client_send_time.to_be_bytes());
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        let client_send_time = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        Ok(Self { client_send_time })
    }

    pub fn to_latency_record(&self) -> LatencyRecord {
        let send_time = self.client_send_time;
        let recv_time = get_time();

        if recv_time < send_time {
            panic!("error: send/recv times are inconsistent")
        }

        LatencyRecord {
            send_time: self.client_send_time,
            recv_time: get_time(),
        }
    }
}

/// Work for a client request.
#[derive(Clone, Copy, Debug, Subcommand)]
pub enum Work {
    /// Do nothing.
    Constant,

    /// Loop for a specified number of times.
    Busy { amt: u64 },

    /// Sleep for a specified number of microseconds.
    Sleep { micros: u64 },
}

impl Work {
    pub fn do_work(self) {
        match self {
            Work::Constant => {}
            Work::Busy { amt } => for _ in 0..amt {},
            Work::Sleep { micros } => {
                thread::sleep(Duration::from_micros(micros));
            }
        }
    }
}

impl Work {
    fn serialize(&self, bytes: &mut [u8]) {
        match self {
            Work::Constant => {
                bytes[0] = 0;
            }
            Work::Busy { amt } => {
                bytes[0] = 1;
                bytes[1..9].copy_from_slice(&amt.to_be_bytes());
            }
            Work::Sleep { micros } => {
                bytes[0] = 2;
                bytes[1..9].copy_from_slice(&micros.to_be_bytes());
            }
        }
    }

    fn deserialize(bytes: &[u8]) -> Result<Self> {
        match bytes[0] {
            0 => Ok(Work::Constant),
            1 => Ok(Work::Busy {
                amt: u64::from_be_bytes(bytes[1..9].try_into().unwrap()),
            }),
            2 => Ok(Work::Sleep {
                micros: u64::from_be_bytes(bytes[1..9].try_into().unwrap()),
            }),
            n => Err(Error::new(
                ErrorKind::InvalidData,
                format!("failed to deserialize work message: {n} is an invalid work id"),
            )),
        }
    }
}
