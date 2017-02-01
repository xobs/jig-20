use std::sync::Mutex;
use super::log;

#[derive(Debug)]
pub enum MessagingError {
    UnableToCreateLog,
}

#[derive(Debug)]
pub struct Messaging {
    pub log: log::Log,
}

impl Messaging {
    pub fn new() -> Result<Messaging, MessagingError> {
        let log = match log::Log::new() {
            Err(_) => return Err(MessagingError::UnableToCreateLog),
            Ok(s) => s,
        };

        Ok(Messaging {
            log: log,
        })
    }

    pub fn debug(&mut self, unit_type: &str, unit: &str, msg: &str) {
        self.log.debug(unit_type, unit, msg);
    }
}