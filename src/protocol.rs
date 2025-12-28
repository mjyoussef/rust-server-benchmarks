use std::{
    io::{Error, ErrorKind, Read, Result, Write},
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

pub trait Serialize<T> {
    fn serialize(self, bytes: &mut T) -> Result<()>;
}

pub trait Deserialize<T> {
    fn deserialize(bytes: &mut T) -> Result<Self>
    where
        Self: Sized;
}

/// Represents a client request.
pub struct Request {
    /// The time (in nanoseconds) the request was sent.
    pub send_time: u64,

    /// The work to do.
    pub work: Work,
}

impl<T: Write> Serialize<T> for Request {
    fn serialize(self, bytes: &mut T) -> Result<()> {
        bytes.write_all(&self.send_time.to_be_bytes())?;
        self.work.serialize(bytes)?;
        Ok(())
    }
}

impl<T: Read> Deserialize<T> for Request {
    fn deserialize(bytes: &mut T) -> Result<Self> {
        let mut send_time_bytes = [0u8; 8];
        bytes.read_exact(&mut send_time_bytes)?;

        let send_time = u64::from_be_bytes(send_time_bytes);
        let work = Work::deserialize(bytes)?;
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

impl<T: Write> Serialize<T> for Response {
    fn serialize(self, bytes: &mut T) -> Result<()> {
        bytes.write_all(&self.client_send_time.to_be_bytes())?;
        Ok(())
    }
}

impl<T: Read> Deserialize<T> for Response {
    fn deserialize(bytes: &mut T) -> Result<Self> {
        let mut send_time_bytes = [0u8; 8];
        bytes.read_exact(&mut send_time_bytes)?;

        let client_send_time = u64::from_be_bytes(send_time_bytes);
        Ok(Self { client_send_time })
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

impl<T: Write> Serialize<T> for Work {
    fn serialize(self, bytes: &mut T) -> Result<()> {
        match self {
            Work::Constant => {
                bytes.write_all(&[0])?;
                bytes.write_all(&[0u8; 8])?;
            }
            Work::Busy { amt } => {
                bytes.write_all(&[1])?;
                bytes.write_all(&amt.to_be_bytes())?;
            }
            Work::Sleep { micros } => {
                bytes.write_all(&[2])?;
                bytes.write_all(&micros.to_be_bytes())?;
            }
        }

        Ok(())
    }
}

impl<T: Read> Deserialize<T> for Work {
    fn deserialize(bytes: &mut T) -> Result<Self> {
        let mut id = [0u8; 1];
        bytes.read_exact(&mut id)?;

        match id[0] {
            0 => {
                bytes.read_exact(&mut [0u8; 8])?;
                Ok(Work::Constant)
            }
            1 => {
                let mut amt_bytes = [0u8; 8];
                bytes.read_exact(&mut amt_bytes)?;
                Ok(Work::Busy {
                    amt: u64::from_be_bytes(amt_bytes),
                })
            }
            2 => {
                let mut micros_bytes = [0u8; 8];
                bytes.read_exact(&mut micros_bytes)?;
                Ok(Work::Sleep {
                    micros: u64::from_be_bytes(micros_bytes),
                })
            }
            n => Err(Error::new(
                ErrorKind::InvalidData,
                format!("failed to deserialize work message: {n} is an invalid work id"),
            )),
        }
    }
}
