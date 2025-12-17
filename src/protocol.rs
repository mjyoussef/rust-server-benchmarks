use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

pub trait Serialize<T: Write> {
    fn serialize(self, bytes: &mut T);
}

pub trait Deserialize<T: Read> {
    fn deserialize(bytes: &mut T) -> Self;
}

/// Represents a client request.
pub struct Request {
    /// The time (in nanoseconds) the request was sent.
    pub send_time: u64,

    /// The work to do.
    pub work: Work,
}

impl<T: Write> Serialize<T> for Request {
    fn serialize(self, bytes: &mut T) {
        bytes.write_all(&self.send_time.to_be_bytes()).unwrap();
        self.work.serialize(bytes);
    }
}

impl<T: Read> Deserialize<T> for Request {
    fn deserialize(bytes: &mut T) -> Self {
        let mut send_time_bytes = [0u8; 8];
        bytes.read_exact(&mut send_time_bytes).unwrap();

        let send_time = u64::from_be_bytes(send_time_bytes);
        let work = Work::deserialize(bytes);
        Self { send_time, work }
    }
}

/// Represents a server response.
pub struct Response {
    /// The time (in nanoseconds) the response was sent.
    pub send_time: u64,
}

impl<T: Write> Serialize<T> for Response {
    fn serialize(self, bytes: &mut T) {
        bytes.write_all(&self.send_time.to_be_bytes()).unwrap();
    }
}

impl<T: Read> Deserialize<T> for Response {
    fn deserialize(bytes: &mut T) -> Self {
        let mut send_time_bytes = [0u8; 8];
        bytes.read_exact(&mut send_time_bytes).unwrap();

        let send_time = u64::from_be_bytes(send_time_bytes);
        Self { send_time }
    }
}

/// Work for a client request.
#[derive(Clone, Copy)]
pub enum Work {
    /// Do nothing.
    Constant,

    /// Loop for a specified number of times.
    Busy(u64),

    /// Sleep for a specified number of microseconds.
    Sleep(u64),
}

impl Work {
    pub fn do_work(self) {
        match self {
            Work::Constant => {}
            Work::Busy(amt) => for _ in 0..amt {},
            Work::Sleep(micros) => {
                thread::sleep(Duration::from_micros(micros));
            }
        }
    }
}

impl<T: Write> Serialize<T> for Work {
    fn serialize(self, bytes: &mut T) {
        match self {
            Work::Constant => {
                bytes.write_all(&[0]).unwrap();
            }
            Work::Busy(amt) => {
                bytes.write_all(&[1]).unwrap();
                bytes.write_all(&amt.to_be_bytes()).unwrap();
            }
            Work::Sleep(micros) => {
                bytes.write_all(&[2]).unwrap();
                bytes.write_all(&micros.to_be_bytes()).unwrap();
            }
        }
    }
}

impl<T: Read> Deserialize<T> for Work {
    fn deserialize(bytes: &mut T) -> Self {
        let mut id = [0u8; 1];
        bytes.read_exact(&mut id).unwrap();

        match id[0] {
            0 => Work::Constant,
            1 => {
                let mut amt_bytes = [0u8; 8];
                bytes.read_exact(&mut amt_bytes).unwrap();
                Work::Busy(u64::from_be_bytes(amt_bytes))
            }
            2 => {
                let mut micros_bytes = [0u8; 8];
                bytes.read_exact(&mut micros_bytes).unwrap();
                Work::Sleep(u64::from_be_bytes(micros_bytes))
            }
            n => {
                panic!("failed to deserialize work message: {n} is an invalid work id")
            }
        }
    }
}
