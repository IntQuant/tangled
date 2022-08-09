use std::{error::Error, fmt::Display};

use crossbeam::channel::SendError;

use crate::MAX_MESSAGE_LEN;

#[derive(Debug)]
pub enum NetError {
    UnknownPeer,
    Disconnected,
    MessageTooLong,
    Dropped,
}

impl Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::UnknownPeer => write!(f, "No peer with this id"),
            NetError::Disconnected => write!(f, "Not connected"),
            NetError::MessageTooLong => {
                write!(f, "Message len exceeds the limit of {}", MAX_MESSAGE_LEN)
            }
            NetError::Dropped => write!(f, "Message dropped"),
        }
    }
}

impl Error for NetError {}

impl<T> From<SendError<T>> for NetError {
    fn from(_: SendError<T>) -> Self {
        Self::Disconnected
    }
}
