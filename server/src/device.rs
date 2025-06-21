use std::fmt;

use common::types::DeviceID;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    id: DeviceID,
    name: String,
}

impl Device {
    // a.d. TODO best way to take strings like this? AsRef<str>/Cow/Borrowed?
    pub fn new(id: DeviceID, name: String) -> Self {
        Self { id, name }
    }

    pub fn id(&self) -> DeviceID {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.id)
    }
}
