use capyio_core::{RouteId, SessionId, StreamId};

use crate::{
    EncodedDisplayError, EncodedDisplayStreamSpec, PrivateDisplayRecord,
    decode_private_display_record,
};

/// Receive-side identity selected by an authenticated outer transport.
///
/// This value is not an authentication token and performs no cryptography. The
/// outer transport owns peer authentication, AEAD and Route authorization, then
/// creates one binding for the selected reliable or datagram receive path. The
/// private record decoder uses it only to prevent stream/epoch/format confusion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayTransportBinding {
    session_id: SessionId,
    route_id: RouteId,
    stream_id: StreamId,
    stream_epoch: u64,
    selected: EncodedDisplayStreamSpec,
}

impl DisplayTransportBinding {
    pub fn new(
        session_id: SessionId,
        route_id: RouteId,
        stream_id: StreamId,
        stream_epoch: u64,
        selected: EncodedDisplayStreamSpec,
    ) -> Result<Self, EncodedDisplayError> {
        selected.validate()?;
        if stream_epoch == 0 {
            return Err(EncodedDisplayError::InvalidRecord(
                "bound stream epoch must be positive".to_owned(),
            ));
        }
        Ok(Self {
            session_id,
            route_id,
            stream_id,
            stream_epoch,
            selected,
        })
    }

    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route_id
    }

    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }

    #[must_use]
    pub const fn selected(&self) -> &EncodedDisplayStreamSpec {
        &self.selected
    }
}

/// Stateful record gate for one outer-transport Session/Route binding.
///
/// It requires configuration before access units, rejects records for another
/// stream or epoch, rejects duplicate/regressing sequence numbers and becomes
/// terminal after end-of-stream. Sequence gaps remain visible to the decoder
/// gate so a datagram transport can trigger its key-frame resynchronization
/// policy. This is defense in depth, not a replacement for an AEAD replay
/// window at the packet layer.
#[derive(Debug)]
pub struct BoundPrivateDisplayReceiver {
    binding: DisplayTransportBinding,
    configured: bool,
    greatest_sequence: Option<u64>,
    ended: bool,
}

impl BoundPrivateDisplayReceiver {
    #[must_use]
    pub const fn new(binding: DisplayTransportBinding) -> Self {
        Self {
            binding,
            configured: false,
            greatest_sequence: None,
            ended: false,
        }
    }

    #[must_use]
    pub const fn binding(&self) -> &DisplayTransportBinding {
        &self.binding
    }

    pub fn accept(&mut self, bytes: &[u8]) -> Result<PrivateDisplayRecord, EncodedDisplayError> {
        if self.ended {
            return Err(invalid_binding("record arrived after end-of-stream"));
        }

        let record = decode_private_display_record(bytes, &self.binding.selected)?;
        match &record {
            PrivateDisplayRecord::DecoderConfig(config) => {
                self.verify_identity(config.stream_id, config.stream_epoch)?;
                self.configured = true;
            }
            PrivateDisplayRecord::AccessUnit(unit) => {
                if !self.configured {
                    return Err(invalid_binding(
                        "access unit arrived before decoder configuration",
                    ));
                }
                self.verify_identity(unit.descriptor.stream_id, unit.descriptor.stream_epoch)?;
                if self
                    .greatest_sequence
                    .is_some_and(|greatest| unit.descriptor.sequence <= greatest)
                {
                    return Err(invalid_binding(
                        "access-unit sequence is duplicate or regressing",
                    ));
                }
                self.greatest_sequence = Some(unit.descriptor.sequence);
                self.ended = unit.descriptor.flags.end_of_stream;
            }
        }
        Ok(record)
    }

    fn verify_identity(
        &self,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<(), EncodedDisplayError> {
        if stream_id != self.binding.stream_id || stream_epoch != self.binding.stream_epoch {
            return Err(invalid_binding(
                "record stream identity does not match the transport binding",
            ));
        }
        Ok(())
    }
}

fn invalid_binding(message: &str) -> EncodedDisplayError {
    EncodedDisplayError::InvalidRecord(message.to_owned())
}
