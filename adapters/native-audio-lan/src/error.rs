use std::io;

use capyio_audio::AudioDataError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeLanError {
    #[error("invalid native LAN configuration: {0}")]
    InvalidConfiguration(&'static str),

    #[error("invalid native LAN datagram: {0}")]
    InvalidDatagram(&'static str),

    #[error("native LAN packet does not match its selected audio binding: {0}")]
    Audio(#[from] AudioDataError),

    #[error("native LAN socket operation failed: {0}")]
    Io(#[from] io::Error),

    #[error("native LAN receive deadline elapsed")]
    ReceiveTimeout,
}
