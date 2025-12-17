use std::{thread, time::Duration};

/// Represents a client request.
pub struct Request {
    /// The time (in nanoseconds) the request was sent.
    send_time: u64,

    /// The work to do.
    work: Work,
}

impl Request {
    pub fn serialize(self, bytes: &mut [u8]) {
        bytes[0..8].copy_from_slice(&self.send_time.to_be_bytes());
        self.work.serialize(&mut bytes[8..]);
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        let send_time = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let work = Work::deserialize(&bytes[8..]);
        Self { send_time, work }
    }
}

/// Represents a server response.
pub struct Response {
    /// The time (in nanoseconds) the response was sent.
    send_time: u64,
}

impl Response {
    pub fn serialize(self, bytes: &mut [u8]) {
        bytes[0..8].copy_from_slice(&self.send_time.to_be_bytes());
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        let send_time = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        Self { send_time }
    }
}

/// Work for a client request.
pub enum Work {
    /// Do nothing.
    Constant,

    /// Loop for a specified number of times.
    Busy(u64),

    /// Sleep for a specified number of microseconds.
    Sleep(u64),
}

impl Work {
    pub fn serialize(self, bytes: &mut [u8]) {
        match self {
            Work::Constant => {
                bytes[0] = 0;
            }
            Work::Busy(amt) => {
                bytes[0] = 1;
                bytes[1..].copy_from_slice(&amt.to_be_bytes());
            }
            Work::Sleep(micros) => {
                bytes[0] = 2;
                bytes[1..].copy_from_slice(&micros.to_be_bytes());
            }
        }
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        let id = bytes[0];
        match id {
            0 => Work::Constant,
            1 => {
                let amt = u64::from_be_bytes(bytes[1..].try_into().unwrap());
                Work::Busy(amt)
            }
            2 => {
                let micros = u64::from_be_bytes(bytes[1..].try_into().unwrap());
                Work::Sleep(micros)
            }
            _ => {
                panic!("invalid Work id")
            }
        }
    }

    pub fn do_Work(self) {
        match self {
            Work::Constant => {}
            Work::Busy(amt) => for _ in 0..amt {},
            Work::Sleep(micros) => {
                thread::sleep(Duration::from_micros(micros));
            }
        }
    }
}
