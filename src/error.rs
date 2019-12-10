use std::fmt;

use std::error::Error as StdError;
use tungstenite::Error as TungsteniteError;
use warp::Error as WarpError;

/// Errors that can happen inside warp.
pub struct Error(Box<Kind>);

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Skip showing worthless `Error { .. }` wrapper.
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0.as_ref() {
            Kind::Warp(ref e) => fmt::Display::fmt(e, f),
            Kind::Tungstenite(ref e) => fmt::Display::fmt(e, f),
        }
    }
}

impl StdError for Error {
    #[allow(deprecated)]
    fn cause(&self) -> Option<&dyn StdError> {
        match self.0.as_ref() {
            Kind::Warp(ref e) => e.cause(),
            Kind::Tungstenite(ref e) => e.cause(),
        }
    }
}

pub enum Kind {
    Warp(WarpError),
    Tungstenite(TungsteniteError),
}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Kind::Warp(ref e) => fmt::Debug::fmt(e, f),
            Kind::Tungstenite(ref e) => fmt::Debug::fmt(e, f),
        }
    }
}

impl From<Kind> for Error {
    fn from(kind: Kind) -> Error {
        Error(Box::new(kind))
    }
}

#[test]
fn error_size_of() {
    assert_eq!(
        ::std::mem::size_of::<Error>(),
        ::std::mem::size_of::<usize>()
    );
}
