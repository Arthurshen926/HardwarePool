use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{InputContractError, InputFrameHeader, NormalizedMagnitude, NormalizedPosition};

const MAX_TOUCH_CONTACTS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchContact {
    pub contact_id: u16,
    pub position: NormalizedPosition,
    pub pressure: Option<NormalizedMagnitude>,
}

/// Complete active-contact snapshot. An empty list explicitly releases every contact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchFrame {
    pub header: InputFrameHeader,
    pub contacts: Vec<TouchContact>,
}

impl TouchFrame {
    pub fn validate(&self) -> Result<(), InputContractError> {
        self.header.validate()?;
        if self.contacts.len() > MAX_TOUCH_CONTACTS {
            return Err(InputContractError::InvalidTouchFrame(format!(
                "touch frame supports at most {MAX_TOUCH_CONTACTS} active contacts"
            )));
        }
        let mut ids = BTreeSet::new();
        for contact in &self.contacts {
            if !ids.insert(contact.contact_id) {
                return Err(InputContractError::InvalidTouchFrame(
                    "contact IDs must be unique inside a snapshot".to_owned(),
                ));
            }
        }
        Ok(())
    }
}
