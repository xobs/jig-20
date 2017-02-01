extern crate bus;
use std::thread;
use std::fmt;
use std::time;

#[derive(Debug)]
pub enum LogError {

}

#[derive(Debug, Clone)]
pub struct LogItem {
    /// A numerical indication of the type of message. 0 is internal messages such as test-start, 1 is test log output from various units, 2 is internal debug log.
    pub message_type: u32,

    /// The name of the unit that generated the message.
    pub unit: String,

    /// The type of unit, such as "test", "logger", "trigger", etc.
    pub unit_type: String,

    /// Number of seconds since the epoch
    pub unix_time: u64,

    /// Number of nanoseconds since the epoch
    pub unix_time_nsecs: u32,

    /// Textual representation of the message, minus linefeeds.
    pub message: String,
}

pub struct Log {
    bus: bus::Bus<LogItem>,
}

impl fmt::Debug for Log {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Logger")
    }
}

impl Log {
    pub fn new() -> Result<Log, LogError> {

        // Create a new channel pair.  Log messages will be broadcast to "rx"
        let mut log: Log = Log {
            bus: bus::Bus::new(4096),
        };

        log.start_logger(|msg| println!("DEBUG>>: {:?}", msg));
        Ok(log)
    }

    pub fn start_logger<F>(&mut self, logger_func: F)
        where F: Send + 'static + Fn(LogItem) {

        let mut console_rx_channel = self.bus.add_rx();
        thread::spawn(move ||
            loop {
                let msg = match console_rx_channel.recv() {
                    Err(e) => { println!("DEBUG!! Channel closed, probably quitting.  Err: {:?}", e); return; },
                    Ok(s) => s,
                };
                logger_func(msg);
            }
        );
    }

    pub fn debug(&mut self, unit_type: &str, unit: &str, msg: &str) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.bus.broadcast(LogItem {
            message_type: 2,
            unit: unit.to_string(),
            unit_type: unit_type.to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: msg.to_string(),
        });
    }
}