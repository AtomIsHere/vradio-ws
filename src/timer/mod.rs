use std::time::{SystemTime, SystemTimeError};

pub struct Timer {
    start_time: SystemTime
}

impl Timer {
    pub fn new() -> Timer {
        return Timer {
            start_time: SystemTime::now()
        }
    }

    pub fn get_time(&self) -> Result<u64, SystemTimeError> {
        return match SystemTime::now().duration_since(self.start_time) {
            Ok(v) => Ok(v.as_secs()),
            Err(e) => Err(e),
        }
    }
}