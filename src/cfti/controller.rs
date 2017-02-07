extern crate bus;
use std::thread;
use std::fmt;
use std::time;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;
use std::process;

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

    /// SHUTDOWN [reason] -- Shuts down the test infrastructure
    Shutdown(String),
}

#[derive(Clone, Debug)]
pub struct Message {

    /// A numerical indication of the type of message. 0 is internal messages such as test-start, 1 is test log output from various units, 2 is internal debug log.
    pub message_type: u32,

    /// The name of the unit that generated the message.
    pub unit_id: String,

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
    should_exit: Arc<Mutex<bool>>,
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
            should_exit: Arc::new(Mutex::new(false)),
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

    pub fn should_exit(&self) -> bool {
        *self.should_exit.lock().unwrap()
    }

    pub fn controller_thread(rx: mpsc::Receiver<Message>,
                             bus: Arc<Mutex<bus::Bus<Message>>>,
                             myself: Arc<Mutex<Controller>>) {
        'new_msg: loop {
            let msg = match rx.recv() {
                Err(e) => {println!("Error receiving: {:?}", e); continue; },
                Ok(o) => o,
            };

            // If the mutex is locked, we come back up here and try it agian.
            // Doubly-locked mutexes can happen if, for example, someone has
            // locked the testset and is sending a message (waiting on locking
            // the controller), and we are running our own code while locking
            // ourselves.
            'retry_mutex: loop {

                let ref mut me = myself.lock().unwrap();
                let mut testset_option_ref = me.testset.lock().unwrap();
                let mut testset_option = testset_option_ref.deref_mut();
                let ref mut testset_ref = match testset_option {
                    &mut None => {
                        Controller::broadcast_internal(&bus,
                                                    MessageContents::Log("TestSet is None".to_string()));
                        continue 'new_msg;
                    },
                    &mut Some(ref mut s) => s,
                };

                let ref mut testset_ref = match testset_ref.try_lock() {
                    Err(_) => continue 'retry_mutex,
                    Ok(r) => r,
                };
                let mut testset = testset_ref.deref_mut();

                match msg.message {
                    /// Log messages: simply rebroadcast them onto the broadcast bus.
                    MessageContents::Log(_) => bus.lock().unwrap().deref_mut().broadcast(msg),

                    // Get the current jig information and broadcast it on the bus.
                    MessageContents::GetJig => {
                        let jig_name = testset.get_jig_name();
                        Controller::broadcast_internal(&bus, MessageContents::Jig(jig_name));
                    },

                    // Set the current scenario to the specified one.
                    MessageContents::Scenario(s) => {
                        testset.set_scenario(&s);
                    },

                    MessageContents::Hello(s) => {
                        testset.set_interface_hello(msg.unit_id, s);
                        process::exit(0);
                    },

                    MessageContents::Shutdown(s) => {
                        let mut should_exit = me.should_exit.lock().unwrap();
                        println!("Shutting down: {}", s);
                        *(should_exit.deref_mut()) = true;
                    },

                    _ => println!("Unhandled message: {:?}", msg),
                };

                continue 'new_msg;
            }
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
            unit_id: "internal".to_string(),
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
            unit_id: unit_name,
            unit_type: unit_type,
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: contents.clone(),
        });
    }
}