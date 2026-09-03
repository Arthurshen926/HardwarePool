use serde::{Deserialize, Serialize};

use crate::{VideoContractError, VideoStreamCapabilities};

const MAX_CONTROLS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LensFacing {
    Front,
    Back,
    External,
    Unknown,
}

/// Cross-platform camera controls with closed, type-safe semantics.
///
/// Zoom values use thousandths of 1x, so `1000` means 1.0x. Platform-specific
/// focus, exposure and lens-selection modes remain Adapter inventory metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CameraControlDescriptor {
    ZoomRatio {
        minimum_milli: u32,
        maximum_milli: u32,
        step_milli: u32,
        default_milli: u32,
    },
    Torch,
}

impl CameraControlDescriptor {
    pub fn validate(self) -> Result<(), VideoContractError> {
        match self {
            Self::Torch => Ok(()),
            Self::ZoomRatio {
                minimum_milli,
                maximum_milli,
                step_milli,
                default_milli,
            } => {
                if minimum_milli == 0
                    || minimum_milli > maximum_milli
                    || step_milli == 0
                    || default_milli < minimum_milli
                    || default_milli > maximum_milli
                    || maximum_milli > 1_000_000
                {
                    return Err(VideoContractError::InvalidCameraControl(
                        "zoom requires positive ordered values, an in-range default, and a 1000x upper bound"
                            .to_owned(),
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Camera semantics attached to one Core Camera Capability.
///
/// Core owns the stable Capability ID and display name. Camera2 logical/physical
/// IDs, hardware level and concurrency combinations remain in the owning Adapter's
/// bounded inventory DTO until their cross-platform semantics are proven.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraDescriptor {
    pub lens_facing: LensFacing,
    pub sensor_orientation_degrees: u16,
    pub streams: VideoStreamCapabilities,
    pub controls: Vec<CameraControlDescriptor>,
}

impl CameraDescriptor {
    pub fn validate(&self) -> Result<(), VideoContractError> {
        if !matches!(self.sensor_orientation_degrees, 0 | 90 | 180 | 270) {
            return Err(VideoContractError::InvalidCameraDescriptor(
                "sensor orientation must be 0, 90, 180, or 270 degrees".to_owned(),
            ));
        }
        self.streams.validate()?;
        if self.controls.len() > MAX_CONTROLS {
            return Err(VideoContractError::InvalidCameraDescriptor(format!(
                "camera controls are limited to {MAX_CONTROLS} entries"
            )));
        }
        let mut saw_zoom = false;
        let mut saw_torch = false;
        for control in &self.controls {
            control.validate()?;
            let seen = match control {
                CameraControlDescriptor::ZoomRatio { .. } => &mut saw_zoom,
                CameraControlDescriptor::Torch => &mut saw_torch,
            };
            if *seen {
                return Err(VideoContractError::InvalidCameraDescriptor(
                    "camera control kinds must be unique".to_owned(),
                ));
            }
            *seen = true;
        }
        Ok(())
    }
}
