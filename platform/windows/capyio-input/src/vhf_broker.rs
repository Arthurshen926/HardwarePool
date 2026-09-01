use std::fmt;

use capyio_input::{
    InputContractError, TouchpadButtonState, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind,
};

pub const VHF_BROKER_RECORD_SIZE: usize = 50;
pub const VHF_BROKER_MAX_CONTACTS: usize = 5;

const MAGIC: u32 = 0x3150_5443;
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 16;
const HELLO: u16 = 1;
const DATA: u16 = 2;
const ACK: u16 = 3;
const CLOSE: u16 = 4;
const CONFIDENCE: u8 = 0x01;
const TIP: u8 = 0x02;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VhfBrokerContact {
    pub contact_id: u8,
    pub confidence: bool,
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VhfBrokerSnapshot {
    pub scan_time: u16,
    pub buttons: u8,
    contacts: [VhfBrokerContact; VHF_BROKER_MAX_CONTACTS],
    contact_count: u8,
}

impl VhfBrokerSnapshot {
    #[must_use]
    pub fn contacts(&self) -> &[VhfBrokerContact] {
        &self.contacts[..usize::from(self.contact_count)]
    }
}

#[derive(Debug)]
pub enum VhfBrokerProjectionError {
    Contract(InputContractError),
    ContactIdOutOfRange(u32),
    TimestampRegression,
}

impl fmt::Display for VhfBrokerProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "VHF Broker snapshot projection failed: {self:?}")
    }
}

impl std::error::Error for VhfBrokerProjectionError {}

#[derive(Clone, Debug)]
pub struct VhfBrokerSnapshotProjector {
    descriptor: TouchpadDescriptor,
    base_timestamp_nanos: Option<u64>,
    last_timestamp_nanos: Option<u64>,
}

impl VhfBrokerSnapshotProjector {
    pub fn new(descriptor: TouchpadDescriptor) -> Result<Self, VhfBrokerProjectionError> {
        descriptor
            .validate()
            .map_err(VhfBrokerProjectionError::Contract)?;
        Ok(Self {
            descriptor,
            base_timestamp_nanos: None,
            last_timestamp_nanos: None,
        })
    }

    pub fn project(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<VhfBrokerSnapshot, VhfBrokerProjectionError> {
        frame
            .validate(&self.descriptor)
            .map_err(VhfBrokerProjectionError::Contract)?;
        if self
            .last_timestamp_nanos
            .is_some_and(|last| frame.header.source_timestamp_nanos < last)
        {
            return Err(VhfBrokerProjectionError::TimestampRegression);
        }

        let base = self
            .base_timestamp_nanos
            .unwrap_or(frame.header.source_timestamp_nanos);
        let elapsed = frame
            .header
            .source_timestamp_nanos
            .checked_sub(base)
            .ok_or(VhfBrokerProjectionError::TimestampRegression)?;
        let scan_time = (elapsed / 100_000) as u16;
        let mut contacts = [VhfBrokerContact::default(); VHF_BROKER_MAX_CONTACTS];

        if frame.kind == TouchpadFrameKind::Update {
            for (index, contact) in frame.contacts.iter().enumerate() {
                let contact_id = u8::try_from(contact.contact_id)
                    .ok()
                    .filter(|id| *id <= 0x0f)
                    .ok_or(VhfBrokerProjectionError::ContactIdOutOfRange(
                        contact.contact_id,
                    ))?;
                contacts[index] = VhfBrokerContact {
                    contact_id,
                    confidence: contact.confidence,
                    x: scale_axis(
                        contact.position.x_himetric,
                        self.descriptor.physical_size.width_himetric,
                    ),
                    y: scale_axis(
                        contact.position.y_himetric,
                        self.descriptor.physical_size.height_himetric,
                    ),
                };
            }
        }

        let snapshot = VhfBrokerSnapshot {
            scan_time,
            buttons: u8::from(
                frame.kind == TouchpadFrameKind::Update
                    && frame.button == TouchpadButtonState::Pressed,
            ),
            contacts,
            contact_count: if frame.kind == TouchpadFrameKind::Update {
                frame.contacts.len() as u8
            } else {
                0
            },
        };
        self.base_timestamp_nanos = Some(base);
        self.last_timestamp_nanos = Some(frame.header.source_timestamp_nanos);
        Ok(snapshot)
    }

    fn reset_epoch(&mut self) {
        self.base_timestamp_nanos = None;
        self.last_timestamp_nanos = None;
    }
}

fn scale_axis(value: u32, maximum: u32) -> u16 {
    ((u64::from(value) * 4095) / u64::from(maximum)) as u16
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VhfBrokerCodecError {
    ZeroGeneration,
    InvalidState,
    SequenceExhausted,
    TooManyContacts(usize),
    InvalidContactId(u8),
    DuplicateContactId(u8),
    CoordinateOutOfRange { contact_id: u8, x: u16, y: u16 },
    InvalidButtons(u8),
    InvalidAck,
}

impl fmt::Display for VhfBrokerCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid VHF Broker record: {self:?}")
    }
}

impl std::error::Error for VhfBrokerCodecError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VhfBrokerTransportError {
    UnsupportedPlatform,
    DriverInterfaceAbsent,
    AmbiguousDriverInterfaces,
    DevicePathTooLong(u32),
    UnexpectedOutputSize(u32),
    Win32(u32),
}

impl fmt::Display for VhfBrokerTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "VHF Broker transport failed: {self:?}")
    }
}

impl std::error::Error for VhfBrokerTransportError {}

pub trait VhfBrokerRecordTransport {
    fn transact(
        &mut self,
        record: &[u8; VHF_BROKER_RECORD_SIZE],
    ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VhfBrokerClientError {
    Codec(VhfBrokerCodecError),
    Transport(VhfBrokerTransportError),
    Poisoned,
}

impl fmt::Display for VhfBrokerClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "VHF Broker client failed: {self:?}")
    }
}

impl std::error::Error for VhfBrokerClientError {}

pub struct VhfBrokerClient<T: VhfBrokerRecordTransport> {
    transport: T,
    encoder: VhfBrokerRecordEncoder,
    poisoned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VhfTouchpadSessionState {
    Active,
    Failed,
    Closed,
}

#[derive(Debug)]
pub enum VhfTouchpadSessionError {
    Projection(VhfBrokerProjectionError),
    Client(VhfBrokerClientError),
    Inactive(VhfTouchpadSessionState),
}

impl fmt::Display for VhfTouchpadSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "VHF touchpad session failed: {self:?}")
    }
}

impl std::error::Error for VhfTouchpadSessionError {}

/// Composes direction-neutral touchpad frames with one exclusive Broker file
/// session. Route admission remains the caller's responsibility.
pub struct VhfTouchpadSession<T: VhfBrokerRecordTransport> {
    projector: VhfBrokerSnapshotProjector,
    client: VhfBrokerClient<T>,
    state: VhfTouchpadSessionState,
}

impl<T: VhfBrokerRecordTransport> VhfTouchpadSession<T> {
    pub fn open(
        transport: T,
        descriptor: TouchpadDescriptor,
        generation: u64,
    ) -> Result<Self, VhfTouchpadSessionError> {
        let projector = VhfBrokerSnapshotProjector::new(descriptor)
            .map_err(VhfTouchpadSessionError::Projection)?;
        let client = VhfBrokerClient::connect(transport, generation)
            .map_err(VhfTouchpadSessionError::Client)?;
        Ok(Self {
            projector,
            client,
            state: VhfTouchpadSessionState::Active,
        })
    }

    #[must_use]
    pub fn state(&self) -> VhfTouchpadSessionState {
        self.state
    }

    pub fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), VhfTouchpadSessionError> {
        self.require_active()?;
        let snapshot = self
            .projector
            .project(frame)
            .map_err(VhfTouchpadSessionError::Projection)?;
        if let Err(error) = self.client.submit(&snapshot) {
            self.state = VhfTouchpadSessionState::Failed;
            return Err(VhfTouchpadSessionError::Client(error));
        }
        Ok(())
    }

    pub fn advance_epoch(&mut self, new_epoch: u64) -> Result<(), VhfTouchpadSessionError> {
        self.require_active()?;
        if let Err(error) = self.client.advance_generation(new_epoch) {
            self.state = VhfTouchpadSessionState::Failed;
            return Err(VhfTouchpadSessionError::Client(error));
        }
        self.projector.reset_epoch();
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), VhfTouchpadSessionError> {
        self.require_active()?;
        if let Err(error) = self.client.close() {
            self.state = VhfTouchpadSessionState::Failed;
            return Err(VhfTouchpadSessionError::Client(error));
        }
        self.state = VhfTouchpadSessionState::Closed;
        Ok(())
    }

    fn require_active(&self) -> Result<(), VhfTouchpadSessionError> {
        if self.state == VhfTouchpadSessionState::Active {
            Ok(())
        } else {
            Err(VhfTouchpadSessionError::Inactive(self.state))
        }
    }
}

impl<T: VhfBrokerRecordTransport> Drop for VhfTouchpadSession<T> {
    fn drop(&mut self) {
        if self.state == VhfTouchpadSessionState::Active {
            let _ = self.client.close();
            self.state = VhfTouchpadSessionState::Closed;
        }
    }
}

impl<T: VhfBrokerRecordTransport> VhfBrokerClient<T> {
    pub fn connect(mut transport: T, generation: u64) -> Result<Self, VhfBrokerClientError> {
        let mut encoder =
            VhfBrokerRecordEncoder::new(generation).map_err(VhfBrokerClientError::Codec)?;
        let hello = encoder.hello().map_err(VhfBrokerClientError::Codec)?;
        transact_and_validate(&mut transport, &hello)?;
        Ok(Self {
            transport,
            encoder,
            poisoned: false,
        })
    }

    pub fn submit(&mut self, snapshot: &VhfBrokerSnapshot) -> Result<(), VhfBrokerClientError> {
        if self.poisoned {
            return Err(VhfBrokerClientError::Poisoned);
        }
        let record = self
            .encoder
            .data(snapshot.scan_time, snapshot.buttons, snapshot.contacts())
            .map_err(VhfBrokerClientError::Codec)?;
        if let Err(error) = transact_and_validate(&mut self.transport, &record) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    pub fn advance_generation(&mut self, generation: u64) -> Result<(), VhfBrokerClientError> {
        if self.poisoned {
            return Err(VhfBrokerClientError::Poisoned);
        }
        let mut next =
            VhfBrokerRecordEncoder::new(generation).map_err(VhfBrokerClientError::Codec)?;
        let hello = next.hello().map_err(VhfBrokerClientError::Codec)?;
        let close = self.encoder.close().map_err(VhfBrokerClientError::Codec)?;
        if let Err(error) = transact_and_validate(&mut self.transport, &close) {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = transact_and_validate(&mut self.transport, &hello) {
            self.poisoned = true;
            return Err(error);
        }
        self.encoder = next;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), VhfBrokerClientError> {
        if self.poisoned {
            return Err(VhfBrokerClientError::Poisoned);
        }
        let record = self.encoder.close().map_err(VhfBrokerClientError::Codec)?;
        if let Err(error) = transact_and_validate(&mut self.transport, &record) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }
}

fn transact_and_validate<T: VhfBrokerRecordTransport>(
    transport: &mut T,
    record: &[u8; VHF_BROKER_RECORD_SIZE],
) -> Result<(), VhfBrokerClientError> {
    let sequence = read_u32(record, 12);
    let ack = transport
        .transact(record)
        .map_err(VhfBrokerClientError::Transport)?;
    VhfBrokerRecordEncoder::validate_ack(&ack, sequence).map_err(VhfBrokerClientError::Codec)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    New,
    Open,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VhfBrokerRecordEncoder {
    generation: u64,
    next_sequence: u32,
    state: State,
}

impl VhfBrokerRecordEncoder {
    pub fn new(generation: u64) -> Result<Self, VhfBrokerCodecError> {
        if generation == 0 {
            return Err(VhfBrokerCodecError::ZeroGeneration);
        }
        Ok(Self {
            generation,
            next_sequence: 0,
            state: State::New,
        })
    }

    pub fn hello(&mut self) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerCodecError> {
        if self.state != State::New {
            return Err(VhfBrokerCodecError::InvalidState);
        }
        let mut record = record_header(HELLO, 8, 0);
        record[HEADER_SIZE..HEADER_SIZE + 8].copy_from_slice(&self.generation.to_le_bytes());
        self.next_sequence = 1;
        self.state = State::Open;
        Ok(record)
    }

    pub fn data(
        &mut self,
        scan_time: u16,
        buttons: u8,
        contacts: &[VhfBrokerContact],
    ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerCodecError> {
        if self.state != State::Open {
            return Err(VhfBrokerCodecError::InvalidState);
        }
        if self.next_sequence == u32::MAX {
            return Err(VhfBrokerCodecError::SequenceExhausted);
        }
        validate_data(buttons, contacts)?;

        let sequence = self.next_sequence;
        let mut record = record_header(DATA, 34, sequence);
        record[16..18].copy_from_slice(&scan_time.to_le_bytes());
        record[18] = contacts.len() as u8;
        record[19] = buttons;
        for (index, contact) in contacts.iter().enumerate() {
            let offset = 20 + index * 6;
            record[offset] = contact.contact_id;
            record[offset + 1] = TIP | (u8::from(contact.confidence) * CONFIDENCE);
            record[offset + 2..offset + 4].copy_from_slice(&contact.x.to_le_bytes());
            record[offset + 4..offset + 6].copy_from_slice(&contact.y.to_le_bytes());
        }
        self.next_sequence += 1;
        Ok(record)
    }

    pub fn close(&mut self) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerCodecError> {
        if self.state != State::Open {
            return Err(VhfBrokerCodecError::InvalidState);
        }
        let record = record_header(CLOSE, 0, self.next_sequence);
        self.state = State::Closed;
        Ok(record)
    }

    pub fn validate_ack(
        record: &[u8; VHF_BROKER_RECORD_SIZE],
        expected_sequence: u32,
    ) -> Result<(), VhfBrokerCodecError> {
        let valid = read_u32(record, 0) == MAGIC
            && read_u16(record, 4) == VERSION
            && read_u16(record, 6) == ACK
            && read_u32(record, 8) == 8
            && read_u32(record, 12) == expected_sequence
            && read_u32(record, 16) == expected_sequence
            && read_u32(record, 20) == 0
            && record[24..].iter().all(|byte| *byte == 0);
        valid.then_some(()).ok_or(VhfBrokerCodecError::InvalidAck)
    }
}

fn validate_data(buttons: u8, contacts: &[VhfBrokerContact]) -> Result<(), VhfBrokerCodecError> {
    if buttons & !0x07 != 0 {
        return Err(VhfBrokerCodecError::InvalidButtons(buttons));
    }
    if contacts.len() > VHF_BROKER_MAX_CONTACTS {
        return Err(VhfBrokerCodecError::TooManyContacts(contacts.len()));
    }
    for (index, contact) in contacts.iter().enumerate() {
        if contact.contact_id > 0x0f {
            return Err(VhfBrokerCodecError::InvalidContactId(contact.contact_id));
        }
        if contact.x > 4095 || contact.y > 4095 {
            return Err(VhfBrokerCodecError::CoordinateOutOfRange {
                contact_id: contact.contact_id,
                x: contact.x,
                y: contact.y,
            });
        }
        if contacts[..index]
            .iter()
            .any(|prior| prior.contact_id == contact.contact_id)
        {
            return Err(VhfBrokerCodecError::DuplicateContactId(contact.contact_id));
        }
    }
    Ok(())
}

fn record_header(kind: u16, payload_length: u32, sequence: u32) -> [u8; VHF_BROKER_RECORD_SIZE] {
    let mut record = [0_u8; VHF_BROKER_RECORD_SIZE];
    record[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    record[4..6].copy_from_slice(&VERSION.to_le_bytes());
    record[6..8].copy_from_slice(&kind.to_le_bytes());
    record[8..12].copy_from_slice(&payload_length.to_le_bytes());
    record[12..16].copy_from_slice(&sequence.to_le_bytes());
    record
}

fn read_u16(record: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([record[offset], record[offset + 1]])
}

fn read_u32(record: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        record[offset],
        record[offset + 1],
        record[offset + 2],
        record[offset + 3],
    ])
}

#[cfg(test)]
fn read_u64(record: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(record[offset..offset + 8].try_into().expect("eight bytes"))
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use capyio_core::StreamId;
    use capyio_input::{
        InputFrameHeader, TouchpadButtonType, TouchpadContact, TouchpadPhysicalSize,
        TouchpadPosition,
    };

    #[derive(Default)]
    struct FakeTransport {
        requests: Vec<[u8; VHF_BROKER_RECORD_SIZE]>,
        fail_on_call: Option<usize>,
        corrupt_ack_on_call: Option<usize>,
    }

    impl VhfBrokerRecordTransport for FakeTransport {
        fn transact(
            &mut self,
            record: &[u8; VHF_BROKER_RECORD_SIZE],
        ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError> {
            let call = self.requests.len();
            self.requests.push(*record);
            if self.fail_on_call == Some(call) {
                return Err(VhfBrokerTransportError::Win32(31));
            }
            let sequence = read_u32(record, 12);
            let mut ack = record_header(ACK, 8, sequence);
            ack[16..20].copy_from_slice(&sequence.to_le_bytes());
            if self.corrupt_ack_on_call == Some(call) {
                ack[49] = 1;
            }
            Ok(ack)
        }
    }

    #[derive(Clone, Default)]
    struct SharedFakeTransport(Rc<RefCell<FakeTransport>>);

    impl VhfBrokerRecordTransport for SharedFakeTransport {
        fn transact(
            &mut self,
            record: &[u8; VHF_BROKER_RECORD_SIZE],
        ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError> {
            self.0.borrow_mut().transact(record)
        }
    }

    fn touchpad_descriptor() -> TouchpadDescriptor {
        TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: 10_000,
                height_himetric: 6_000,
            },
            max_contacts: 5,
            button_type: TouchpadButtonType::ClickPad,
            reports_contact_size: false,
            reports_pressure: false,
        }
    }

    fn touchpad_frame(timestamp: u64) -> TouchpadFrame {
        TouchpadFrame {
            header: InputFrameHeader {
                stream_id: StreamId::new(),
                stream_epoch: 1,
                sequence: 0,
                source_timestamp_nanos: timestamp,
            },
            kind: TouchpadFrameKind::Update,
            button: TouchpadButtonState::Released,
            contacts: vec![TouchpadContact {
                contact_id: 3,
                position: TouchpadPosition {
                    x_himetric: 5_000,
                    y_himetric: 3_000,
                },
                confidence: true,
                size: None,
                pressure: None,
            }],
        }
    }

    fn contact(contact_id: u8) -> VhfBrokerContact {
        VhfBrokerContact {
            contact_id,
            confidence: true,
            x: 1000 + u16::from(contact_id),
            y: 2000 + u16::from(contact_id),
        }
    }

    #[test]
    fn hello_data_close_are_canonical_fixed_records() {
        let mut encoder = VhfBrokerRecordEncoder::new(0x0807_0605_0403_0201).unwrap();
        let hello = encoder.hello().unwrap();
        assert_eq!(
            &hello[0..16],
            &[0x43, 0x54, 0x50, 0x31, 1, 0, 1, 0, 8, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(&hello[16..24], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(hello[24..].iter().all(|byte| *byte == 0));

        let data = encoder.data(0x1234, 5, &[contact(2), contact(7)]).unwrap();
        assert_eq!(read_u16(&data, 6), DATA);
        assert_eq!(read_u32(&data, 8), 34);
        assert_eq!(read_u32(&data, 12), 1);
        assert_eq!(&data[16..20], &[0x34, 0x12, 2, 5]);
        assert_eq!(&data[20..22], &[2, CONFIDENCE | TIP]);
        assert!(data[32..].iter().all(|byte| *byte == 0));

        let close = encoder.close().unwrap();
        assert_eq!(read_u16(&close, 6), CLOSE);
        assert_eq!(read_u32(&close, 8), 0);
        assert_eq!(read_u32(&close, 12), 2);
        assert!(close[16..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn validation_is_transactional_and_bounded() {
        let mut encoder = VhfBrokerRecordEncoder::new(7).unwrap();
        encoder.hello().unwrap();
        assert_eq!(
            encoder.data(1, 0, &[contact(1), contact(1)]),
            Err(VhfBrokerCodecError::DuplicateContactId(1))
        );
        assert_eq!(read_u32(&encoder.data(2, 0, &[contact(1)]).unwrap(), 12), 1);

        let oversized = [
            contact(0),
            contact(1),
            contact(2),
            contact(3),
            contact(4),
            contact(5),
        ];
        assert_eq!(
            encoder.data(3, 0, &oversized),
            Err(VhfBrokerCodecError::TooManyContacts(6))
        );
    }

    #[test]
    fn ack_requires_exact_sequence_and_zero_padding() {
        let mut ack = record_header(ACK, 8, 9);
        ack[16..20].copy_from_slice(&9_u32.to_le_bytes());
        assert_eq!(VhfBrokerRecordEncoder::validate_ack(&ack, 9), Ok(()));
        ack[49] = 1;
        assert_eq!(
            VhfBrokerRecordEncoder::validate_ack(&ack, 9),
            Err(VhfBrokerCodecError::InvalidAck)
        );
    }

    #[test]
    fn client_sends_hello_data_close_and_poison_is_terminal() {
        let snapshot = VhfBrokerSnapshot {
            scan_time: 5,
            buttons: 0,
            contacts: [
                contact(1),
                VhfBrokerContact::default(),
                VhfBrokerContact::default(),
                VhfBrokerContact::default(),
                VhfBrokerContact::default(),
            ],
            contact_count: 1,
        };
        let mut client = VhfBrokerClient::connect(FakeTransport::default(), 9).unwrap();
        client.submit(&snapshot).unwrap();
        client.close().unwrap();
        assert_eq!(client.transport().requests.len(), 3);
        assert_eq!(
            client
                .transport()
                .requests
                .iter()
                .map(|record| read_u16(record, 6))
                .collect::<Vec<_>>(),
            vec![HELLO, DATA, CLOSE]
        );

        let transport = FakeTransport {
            corrupt_ack_on_call: Some(1),
            ..FakeTransport::default()
        };
        let mut client = VhfBrokerClient::connect(transport, 10).unwrap();
        assert_eq!(
            client.submit(&snapshot),
            Err(VhfBrokerClientError::Codec(VhfBrokerCodecError::InvalidAck))
        );
        assert_eq!(
            client.submit(&snapshot),
            Err(VhfBrokerClientError::Poisoned)
        );
        assert_eq!(client.transport().requests.len(), 2);

        let transport = FakeTransport {
            fail_on_call: Some(1),
            ..FakeTransport::default()
        };
        let mut client = VhfBrokerClient::connect(transport, 11).unwrap();
        assert_eq!(
            client.submit(&snapshot),
            Err(VhfBrokerClientError::Transport(
                VhfBrokerTransportError::Win32(31)
            ))
        );
        assert_eq!(client.close(), Err(VhfBrokerClientError::Poisoned));
        assert_eq!(client.transport().requests.len(), 2);
    }

    #[test]
    fn composed_session_projects_submits_closes_and_drop_releases() {
        let transport = SharedFakeTransport::default();
        let observer = transport.clone();
        let mut session = VhfTouchpadSession::open(transport, touchpad_descriptor(), 22).unwrap();
        session.submit_frame(&touchpad_frame(1_000_000)).unwrap();
        session.close().unwrap();
        assert_eq!(session.state(), VhfTouchpadSessionState::Closed);
        assert_eq!(
            observer
                .0
                .borrow()
                .requests
                .iter()
                .map(|record| read_u16(record, 6))
                .collect::<Vec<_>>(),
            vec![HELLO, DATA, CLOSE]
        );
        assert!(matches!(
            session.submit_frame(&touchpad_frame(1_100_000)),
            Err(VhfTouchpadSessionError::Inactive(
                VhfTouchpadSessionState::Closed
            ))
        ));

        let transport = SharedFakeTransport::default();
        let observer = transport.clone();
        {
            let _session = VhfTouchpadSession::open(transport, touchpad_descriptor(), 23).unwrap();
        }
        assert_eq!(
            observer
                .0
                .borrow()
                .requests
                .iter()
                .map(|record| read_u16(record, 6))
                .collect::<Vec<_>>(),
            vec![HELLO, CLOSE]
        );
    }

    #[test]
    fn composed_session_releases_and_rebinds_on_epoch_advance() {
        let transport = SharedFakeTransport::default();
        let observer = transport.clone();
        let mut session = VhfTouchpadSession::open(transport, touchpad_descriptor(), 1).unwrap();
        session.submit_frame(&touchpad_frame(5_000_000)).unwrap();
        session.advance_epoch(2).unwrap();
        session.submit_frame(&touchpad_frame(1_000_000)).unwrap();
        session.close().unwrap();

        let transport = observer.0.borrow();
        assert_eq!(
            transport
                .requests
                .iter()
                .map(|record| read_u16(record, 6))
                .collect::<Vec<_>>(),
            vec![HELLO, DATA, CLOSE, HELLO, DATA, CLOSE]
        );
        assert_eq!(read_u64(&transport.requests[0], HEADER_SIZE), 1);
        assert_eq!(read_u64(&transport.requests[3], HEADER_SIZE), 2);
        assert_eq!(read_u16(&transport.requests[4], HEADER_SIZE), 0);
    }

    #[test]
    fn zero_generation_rejection_does_not_close_the_current_client() {
        let mut client = VhfBrokerClient::connect(FakeTransport::default(), 8).unwrap();
        assert_eq!(
            client.advance_generation(0),
            Err(VhfBrokerClientError::Codec(
                VhfBrokerCodecError::ZeroGeneration
            ))
        );
        client.close().unwrap();
        assert_eq!(
            client
                .transport()
                .requests
                .iter()
                .map(|record| read_u16(record, 6))
                .collect::<Vec<_>>(),
            vec![HELLO, CLOSE]
        );
    }

    #[test]
    fn composed_session_is_failed_after_unknown_delivery() {
        let transport = SharedFakeTransport::default();
        transport.0.borrow_mut().fail_on_call = Some(1);
        let mut session = VhfTouchpadSession::open(transport, touchpad_descriptor(), 24).unwrap();
        assert!(matches!(
            session.submit_frame(&touchpad_frame(1_000_000)),
            Err(VhfTouchpadSessionError::Client(
                VhfBrokerClientError::Transport(VhfBrokerTransportError::Win32(31))
            ))
        ));
        assert_eq!(session.state(), VhfTouchpadSessionState::Failed);
        assert!(matches!(
            session.close(),
            Err(VhfTouchpadSessionError::Inactive(
                VhfTouchpadSessionState::Failed
            ))
        ));
    }

    #[test]
    fn projector_scales_himetric_snapshots_and_is_transactional() {
        let descriptor = touchpad_descriptor();
        let stream_id = StreamId::new();
        let frame = TouchpadFrame {
            header: InputFrameHeader {
                stream_id,
                stream_epoch: 1,
                sequence: 0,
                source_timestamp_nanos: 1_000_000,
            },
            kind: TouchpadFrameKind::Update,
            button: TouchpadButtonState::Pressed,
            contacts: vec![TouchpadContact {
                contact_id: 3,
                position: TouchpadPosition {
                    x_himetric: 5_000,
                    y_himetric: 6_000,
                },
                confidence: true,
                size: None,
                pressure: None,
            }],
        };
        let mut projector = VhfBrokerSnapshotProjector::new(descriptor).unwrap();
        let first = projector.project(&frame).unwrap();
        assert_eq!(first.scan_time, 0);
        assert_eq!(first.buttons, 1);
        assert_eq!(first.contacts()[0].x, 2047);
        assert_eq!(first.contacts()[0].y, 4095);

        let mut later = frame.clone();
        later.header.sequence = 1;
        later.header.source_timestamp_nanos += 250_000;
        later.kind = TouchpadFrameKind::CancelAll;
        later.button = TouchpadButtonState::Released;
        later.contacts.clear();
        assert_eq!(projector.project(&later).unwrap().scan_time, 2);

        let mut regressed = later.clone();
        regressed.header.source_timestamp_nanos -= 1;
        assert!(matches!(
            projector.project(&regressed),
            Err(VhfBrokerProjectionError::TimestampRegression)
        ));
        later.header.source_timestamp_nanos += 100_000;
        assert_eq!(projector.project(&later).unwrap().scan_time, 3);
    }
}
