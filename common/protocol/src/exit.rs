use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Used to inform a remote of disconnection and the reason for that disconnection.
#[derive(Clone, Debug, Default)]
pub struct ExitCode {
    /// Which error occurred
    pub status: ExitStatus,
    /// A brief, human-readable explanation of the error, if any.
    pub short: Option<String>,
    /// A longer explanation, possibly including the source of
    /// the error in the code or the error itself via fmt::Debug.
    pub long: Option<String>,
}

impl ExitCode {
    /// The client disconnected from the server with no further info.
    pub const DISCONNECTED: Self = ExitCode {
        status: ExitStatus::Disconnected,
        short: None,
        long: None,
    };

    #[inline]
    pub fn encoded_len(&self) -> usize {
        let mut sum = 5;
        if let Some(short) = &self.short {
            sum += short.len().min(1024);
        }
        if let Some(long) = &self.long {
            sum += long.len().min(1024);
        }
        sum
    }

    #[inline]
    pub fn to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.encoded_len());
        self.into_buf(&mut buf);
        buf.freeze()
    }

    /// YOU WILL NEED TO RESERVE SPACE IN THE BUF (use ExitCode::encoded_len)
    pub fn into_buf(&self, mut buf: impl BufMut) {
        buf.put_u8(self.status as u8);
        let short_len = self.short.as_ref().map(|s| s.len().min(1024)).unwrap_or(0);
        let long_len = self.long.as_ref().map(|s| s.len().min(1024)).unwrap_or(0);
        buf.put_u16_le(short_len as u16);
        buf.put_u16_le(long_len as u16);
        if let Some(short) = &self.short {
            buf.put_slice(&short.as_bytes()[..short_len]);
        }
        if let Some(long) = &self.long {
            buf.put_slice(&long.as_bytes()[..long_len]);
        }
    }

    pub fn from_bytes(mut buf: impl Buf) -> Self {
        if buf.remaining() < 5 {
            return Self::default();
        }

        let status = buf.get_u8();
        let short_len = buf.get_u16_le() as usize;
        let long_len = buf.get_u16_le() as usize;

        if short_len > 1024 || long_len > 1024 {
            return Self {
                status: status.into(),
                ..Default::default()
            };
        }

        let short = if short_len != 0 && buf.remaining() >= short_len {
            let mut bytes = vec![0u8; short_len];
            buf.try_copy_to_slice(&mut bytes).unwrap();
            Some(String::from_utf8_lossy_owned(bytes))
        } else {
            None
        };

        let long = if long_len != 0 && buf.remaining() >= long_len {
            let mut bytes = vec![0u8; long_len];
            buf.try_copy_to_slice(&mut bytes).unwrap();
            Some(String::from_utf8_lossy_owned(bytes))
        } else {
            None
        };

        Self {
            status: status.into(),
            short,
            long,
        }
    }
}

impl<S: ToString> Into<ExitCode> for (ExitStatus, &str, S) {
    fn into(self) -> ExitCode {
        ExitCode {
            status: self.0,
            short: Some(self.1.into()),
            long: Some(self.2.to_string()),
        }
    }
}

impl<S: ToString> Into<ExitCode> for (ExitStatus, S) {
    fn into(self) -> ExitCode {
        ExitCode {
            status: self.0,
            short: Some(self.1.to_string()),
            long: None,
        }
    }
}

impl Into<ExitCode> for ExitStatus {
    fn into(self) -> ExitCode {
        ExitCode {
            status: self,
            short: None,
            long: None,
        }
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.status)?;
        if let Some(short) = &self.short {
            write!(f, ": {}", short)?;
        }
        if let Some(long) = &self.long {
            if self.short.is_some() {
                write!(f, "\n{}", long)?;
            } else {
                write!(f, ": {}", long)?;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ExitStatus {
    /// Disconnection was requested.
    #[default]
    Disconnected = 0,

    /// An unresolable IO error occurred that forced disconnection.
    NetworkError = 1,

    /// A required action failed to receive a response.
    TimedOut = 2,

    /// A packet violated a format expectation.
    ProtocolViolation = 3,

    /// Couldn't resolve an IP string to a socket addr (during connection)
    AddressUnresolvable = 4,

    /// Expected an exit code, but it failed to deserialize.
    InvalidExitCode = 5,
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ExitStatus::*;
        let text = match self {
            Disconnected => "Disconnected",
            NetworkError => "Network Error",
            TimedOut => "Timed Out",
            ProtocolViolation => "Protocol Violation",
            AddressUnresolvable => "Address Unresolvable",
            InvalidExitCode => "Invalid Exit Code",
        };
        f.write_str(text)
    }
}

impl From<u8> for ExitStatus {
    fn from(value: u8) -> Self {
        use ExitStatus::*;
        match value {
            1 => NetworkError,
            2 => TimedOut,
            3 => ProtocolViolation,
            4 => AddressUnresolvable,
            5 => InvalidExitCode,
            _ => Disconnected,
        }
    }
}
