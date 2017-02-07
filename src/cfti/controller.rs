extern crate bus;
use std::thread;
use std::fmt;
use std::time;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

use super::testset::TestSet;

#[derive(Clone, Debug)]
pub enum MessageContents {
    Hello(String),
    Log(String),

    /// DESCRIBE [type] [field] [item] [value]
    Describe(String, String, String, String),

    /// SCENARIO [string] -- Sets the scenario to the specified id
    Scenario(String),

    GetJig,
    Jig(String),
}

#[derive(Clone, Debug)]
pub struct Message {

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

    /// The actual contents of the message being sent.
    pub message: MessageContents,
}

#[derive(Debug)]
pub enum ControllerError {
}

pub struct Controller {
    broadcast: Arc<Mutex<bus::Bus<Message>>>,
    control: mpsc::Sender<Message>,
    testset: Arc<Mutex<Option<Arc<Mutex<TestSet>>>>>,
}

impl fmt::Debug for Controller {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Controller")
    }
}

impl Controller {

    pub fn new() -> Result<Arc<Mutex<Controller>>, ControllerError> {
        let (tx, rx) = mpsc::channel();
        let bus = Arc::new(Mutex::new(bus::Bus::new(4096)));
        let controller = Arc::new(Mutex::new(Controller {
            broadcast: bus.clone(),
            control: tx,
            testset: Arc::new(Mutex::new(Option::None)),
        }));

        // The controller runs in its own thread.
        let controller_clone = controller.clone();
        thread::spawn(move || Controller::controller_thread(rx, bus, controller_clone));

        Ok(controller)
    }

    pub fn set_testset(&mut self, testset: Arc<Mutex<TestSet>>) {
        let mut t = self.testset.lock().unwrap();
        *t = Some(testset.clone());
    }

    pub fn controller_thread(rx: mpsc::Receiver<Message>,
                             bus: Arc<Mutex<bus::Bus<Message>>>,
                             myself: Arc<Mutex<Controller>>) {
        loop {
            let msg = match rx.recv() {
                Err(e) => {println!("Error receiving: {:?}", e); continue; },
                Ok(o) => o,
            };

            let me = myself.lock().unwrap();
            let testsetref = me.testset.lock().unwrap();
            let ref testset = testsetref.as_ref();

            if testset.is_none() {
                Controller::broadcast_internal(&bus, MessageContents::Log("TestSet is None".to_string()));
                continue;
            }

            let mut testset = testset.unwrap().lock().unwrap();
            let mut testset = testset.deref_mut();

            match msg.message {
                /// Log messages: simply rebroadcast them onto the broadcast bus.
                MessageContents::Log(_) => bus.lock().unwrap().deref_mut().broadcast(msg),

                // Get the current jig information and broadcast it on the bus.
                MessageContents::GetJig => {
                    let jig_name = testset.get_jig_name();
                    Controller::broadcast_internal(&bus, MessageContents::Jig(jig_name));
                },

                MessageContents::Scenario(s) => {
                    testset.set_scenario(&s);
                },

                _ => println!("Unrecognized message"),
            };
        };
    }

    fn broadcast_internal(bus: &Arc<Mutex<bus::Bus<Message>>>,
                          msg: MessageContents) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        bus.lock().unwrap().deref_mut().broadcast(Message {
            message_type: 2,
            unit: "internal".to_string(),
            unit_type: "core".to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: msg,
        });
    }

    pub fn add_logger<F>(&mut self, logger_func: F)
        where F: Send + 'static + Fn(Message) {

        self.add_broadcast(move |msg| match msg {
            Message { message: MessageContents::Log(_), .. } => logger_func(msg),
            _ => (),
        });
    }

    pub fn add_broadcast<F>(&mut self, broadcast_func: F)
        where F: Send + 'static + Fn(Message) {

        let mut console_rx_channel = self.broadcast.lock().unwrap().deref_mut().add_rx();
        thread::spawn(move ||
            loop {
                match console_rx_channel.recv() {
                    Err(e) => { println!("DEBUG!! Channel closed, probably quitting.  Err: {:?}", e); return; },
                    Ok(msg) => broadcast_func(msg),
                };
            }
        );
    }
    
    /*
    pub fn add_listener(&mut self) -> bus::BusReader<Message> {
        self.broadcast.lock().unwrap().deref_mut().add_rx()
    }

    pub fn add_sender(&self) -> mpsc::Sender<Message> {
        self.control.clone()
    }
    */

    pub fn control_message(&self, message: &Message) {
        self.control.send(message.clone()).unwrap();
    }

    pub fn send_control(&self,
                        unit_name: String,
                        unit_type: String,
                        contents: &MessageContents) {

        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.control_message(&Message {
            message_type: 2,
            unit: unit_name,
            unit_type: unit_type,
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: contents.clone(),
        });
    }
}