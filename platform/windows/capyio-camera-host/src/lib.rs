#![forbid(unsafe_code)]

//! Headless lifecycle ownership for the Windows decoded-camera producer.

pub const IMPLEMENTATION_STATUS: &str =
    "decoded-frame-producer-owner-no-network-codec-registration";

#[cfg(windows)]
mod windows_host {
    use std::{error::Error, fmt};

    use capyio_core::StreamId;
    use capyio_windows_camera::GeneratedVideoFrame;
    use capyio_windows_camera_share::{CameraSharedIngressError, CameraSharedIngressProducer};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CameraProducerHostState {
        Stopped,
        Active,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CameraProducerStopOutcome {
        AlreadyStopped,
        Stopped { published_frames: u64 },
    }

    pub struct CameraProducerHost {
        stream_id: StreamId,
        stream_epoch: u64,
        producer: Option<CameraSharedIngressProducer>,
        published_frames: u64,
    }

    impl CameraProducerHost {
        pub fn new(
            stream_id: StreamId,
            stream_epoch: u64,
        ) -> Result<Self, CameraProducerHostError> {
            if stream_epoch == 0 {
                return Err(CameraProducerHostError::InvalidStreamEpoch);
            }
            Ok(Self {
                stream_id,
                stream_epoch,
                producer: None,
                published_frames: 0,
            })
        }

        pub fn start(&mut self) -> Result<(), CameraProducerHostError> {
            let stream_id = self.stream_id;
            let stream_epoch = self.stream_epoch;
            self.start_with(|| CameraSharedIngressProducer::create(stream_id, stream_epoch))
        }

        /// Starts the exact-name, current-session mapping used by the explicit
        /// camera integration lab. It is never selected by registered COM
        /// activation, which opens only the production Global mapping.
        #[cfg(feature = "lab-support")]
        pub fn start_local_lab(&mut self) -> Result<(), CameraProducerHostError> {
            let stream_id = self.stream_id;
            let stream_epoch = self.stream_epoch;
            self.start_with(|| {
                CameraSharedIngressProducer::create_local_lab(stream_id, stream_epoch)
            })
        }

        #[cfg(test)]
        fn start_local_test(&mut self, mapping_name: &str) -> Result<(), CameraProducerHostError> {
            let stream_id = self.stream_id;
            let stream_epoch = self.stream_epoch;
            self.start_with(|| {
                CameraSharedIngressProducer::create_local_test(
                    mapping_name,
                    stream_id,
                    stream_epoch,
                )
            })
        }

        fn start_with<F>(&mut self, create: F) -> Result<(), CameraProducerHostError>
        where
            F: FnOnce() -> Result<CameraSharedIngressProducer, CameraSharedIngressError>,
        {
            if self.producer.is_some() {
                return Err(CameraProducerHostError::AlreadyActive);
            }
            let producer = create()?;
            self.producer = Some(producer);
            self.published_frames = 0;
            Ok(())
        }

        pub fn publish(
            &mut self,
            frame: GeneratedVideoFrame,
        ) -> Result<u64, CameraProducerHostError> {
            let producer = self
                .producer
                .as_mut()
                .ok_or(CameraProducerHostError::Inactive)?;
            let publication = producer.publish(frame)?;
            self.published_frames = publication;
            Ok(publication)
        }

        pub fn stop(&mut self) -> CameraProducerStopOutcome {
            if self.producer.take().is_none() {
                CameraProducerStopOutcome::AlreadyStopped
            } else {
                CameraProducerStopOutcome::Stopped {
                    published_frames: self.published_frames,
                }
            }
        }

        #[must_use]
        pub const fn state(&self) -> CameraProducerHostState {
            if self.producer.is_some() {
                CameraProducerHostState::Active
            } else {
                CameraProducerHostState::Stopped
            }
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
        pub const fn published_frames(&self) -> u64 {
            self.published_frames
        }
    }

    impl Drop for CameraProducerHost {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    #[derive(Debug)]
    pub enum CameraProducerHostError {
        InvalidStreamEpoch,
        AlreadyActive,
        Inactive,
        Shared(CameraSharedIngressError),
    }

    impl fmt::Display for CameraProducerHostError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidStreamEpoch => formatter.write_str("stream epoch must be positive"),
                Self::AlreadyActive => {
                    formatter.write_str("camera producer host is already active")
                }
                Self::Inactive => formatter.write_str("camera producer host is not active"),
                Self::Shared(error) => error.fmt(formatter),
            }
        }
    }

    impl Error for CameraProducerHostError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::Shared(error) => Some(error),
                _ => None,
            }
        }
    }

    impl From<CameraSharedIngressError> for CameraProducerHostError {
        fn from(value: CameraSharedIngressError) -> Self {
            Self::Shared(value)
        }
    }

    #[cfg(test)]
    mod tests {
        use std::str::FromStr;

        use capyio_windows_camera::DeterministicNv12Source;
        use capyio_windows_camera_share::CameraSharedIngressConsumer;

        use super::*;

        const TEST_STREAM: &str = "00000000-0000-4000-8000-00000000c018";

        fn stream_id() -> StreamId {
            StreamId::from_str(TEST_STREAM).expect("fixed stream id")
        }

        fn mapping_name(label: &str) -> String {
            format!(
                "Local\\CapyIO.CameraIngress.v1.test.{}.host.{label}",
                std::process::id()
            )
        }

        #[test]
        fn lifecycle_owns_publishes_and_releases_one_mapping() {
            assert!(matches!(
                CameraProducerHost::new(stream_id(), 0),
                Err(CameraProducerHostError::InvalidStreamEpoch)
            ));
            let name = mapping_name("lifecycle");
            let mut host = CameraProducerHost::new(stream_id(), 41).expect("host");
            assert_eq!(host.state(), CameraProducerHostState::Stopped);
            assert!(matches!(
                host.publish(
                    DeterministicNv12Source::new(stream_id(), 41, 13_000_000_000)
                        .unwrap()
                        .next_frame()
                        .unwrap()
                ),
                Err(CameraProducerHostError::Inactive)
            ));

            host.start_local_test(&name).expect("start host");
            assert_eq!(host.state(), CameraProducerHostState::Active);
            assert!(matches!(
                host.start_local_test(&name),
                Err(CameraProducerHostError::AlreadyActive)
            ));
            let mut source = DeterministicNv12Source::new(stream_id(), 41, 13_000_000_000).unwrap();
            let expected = source.next_frame().unwrap();
            assert_eq!(host.publish(expected.clone()).unwrap(), 1);
            assert_eq!(host.published_frames(), 1);

            let mut consumer = CameraSharedIngressConsumer::open_local_test_current(&name).unwrap();
            assert_eq!(consumer.stream_id(), stream_id());
            assert_eq!(consumer.stream_epoch(), 41);
            assert_eq!(consumer.try_read_latest().unwrap(), Some(expected));
            drop(consumer);
            assert_eq!(
                host.stop(),
                CameraProducerStopOutcome::Stopped {
                    published_frames: 1
                }
            );
            assert_eq!(host.state(), CameraProducerHostState::Stopped);
            assert_eq!(host.stop(), CameraProducerStopOutcome::AlreadyStopped);
            assert!(CameraSharedIngressConsumer::open_local_test(&name, stream_id(), 41).is_err());
        }

        #[test]
        fn failed_duplicate_owner_start_leaves_host_stopped_and_retryable() {
            let name = mapping_name("duplicate");
            let mut first = CameraProducerHost::new(stream_id(), 43).unwrap();
            let mut second = CameraProducerHost::new(stream_id(), 43).unwrap();
            first.start_local_test(&name).unwrap();
            assert!(matches!(
                second.start_local_test(&name),
                Err(CameraProducerHostError::Shared(
                    CameraSharedIngressError::AlreadyOwned
                ))
            ));
            assert_eq!(second.state(), CameraProducerHostState::Stopped);
            first.stop();
            second.start_local_test(&name).expect("retry after release");
            assert_eq!(second.state(), CameraProducerHostState::Active);
        }
    }
}

#[cfg(windows)]
pub use windows_host::{
    CameraProducerHost, CameraProducerHostError, CameraProducerHostState, CameraProducerStopOutcome,
};
