use std::time::{SystemTime, SystemTimeError};

// Add structure for timer
pub struct Timer {
    start_time: SystemTime
}

// Logic for timer
impl Timer {
    pub fn new() -> Timer {
        return Timer {
            // Start timer at the current system time
            start_time: SystemTime::now()
        }
    }

    pub fn get_time(&self) -> Result<u64, SystemTimeError> {
        // Determine difference from now to when the timer was created
        return match SystemTime::now().duration_since(self.start_time) {
            Ok(v) => Ok(v.as_secs()),
            Err(e) => Err(e),
        }
    }
}