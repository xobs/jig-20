extern crate bus;
use std::thread;
use std::fmt;
use std::time;
use std::sync::mpsc;

use super::log;
use super::interface;

#[derive(Debug)]
pub enum MessagingError {
    UnableToCreateLog,
    UnableToCreateInterface,
}

#[derive(Debug, Clone)]
pub enum Message {
    Log(log::LogItem),
    Interface(interface::InterfaceItem),
}

pub struct Messaging {
    broadcast: bus::Bus<Message>,
    control_rx: mpsc::Receiver<Message>,
    control_tx: mpsc::Sender<Message>,
}

impl fmt::Debug for Messaging {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Messaging")
    }
}

impl Messaging {
    pub fn new() -> Result<Messaging, MessagingError> {
        let (tx, rx) = mpsc::channel();

        let mut messaging = Messaging {
            broadcast: bus::Bus::new(4096),
            control_tx: tx,
            control_rx: rx,
        };

        messaging.attach_logger(|msg| println!("DEBUG>>: {:?}", msg));

        Ok(messaging)
    }



    pub fn get_control_channel(&mut self) -> mpsc::Sender<Message> {
        // Send a clone of the normal control channel.
        self.control_tx.clone()
    }

    /// Sends a control message to the core.
    pub fn send_control_message(&mut self)
    pub fn debug(&mut self, unit_type: &str, unit: &str, msg: &str) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.broadcast.broadcast(Message::Log(log::LogItem {
            message_type: 2,
            unit: unit.to_string(),
            unit_type: unit_type.to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: msg.to_string(),
        }));
    }
}