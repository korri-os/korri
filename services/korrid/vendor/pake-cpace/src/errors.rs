use core::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    InvalidMsgSize,
    InvalidMsgEncoding,
    InvalidPublicKey,
    InvalidSharedKey,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

#[derive(Debug)]
pub struct RandomError(pub(crate) getrandom::Error);

impl fmt::Display for RandomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub struct InvalidAuthTag(pub(crate) ());

impl fmt::Debug for InvalidAuthTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvalidAuthTag").finish()
    }
}

impl fmt::Display for InvalidAuthTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invalid message authentication tag")
    }
}
