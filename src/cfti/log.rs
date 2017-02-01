extern crate chan;
use std::thread;

#[derive(Debug)]
pub enum LogError {

}

#[derive(Debug)]
pub struct Log {
    log_tx_channel: chan::Sender<String>,
    log_rx_channel: chan::Receiver<String>,
}

impl Log {
    pub fn new() -> Result<Log, LogError> {

        // Create a new channel pair.  Log messages will be broadcast to "out"
        let (tx_channel, rx_channel) = chan::async();

        let console_rx_channel = rx_channel.clone();
        thread::spawn(move || {
            loop {
                let msg = match console_rx_channel.recv() {
                    None => { println!("DEBUG!! Channel closed, probably quitting."); return; },
                    Some(s) => s,
                };
                println!("DEBUG>>: {}", msg);
            }
        });

        Ok(Log {
            log_tx_channel: tx_channel,
            log_rx_channel: rx_channel,
        })
    }

    pub fn debug(&self, msg: &str) {
        self.log_tx_channel.send(msg.to_string());
    }
}