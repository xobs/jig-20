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
pub enum BroadcastMessageContents {
    Hello(String),
    Log(String),

    /// DESCRIBE [type] [field] [item] [value]
    Describe(String, String, String, String),

    /// SCENARIO [string] -- Sets the scenario to the specified id
    Scenario(String),

    /// SCENARIOS [first] [...] -- Lists the scenarios that are available
    Scenarios(Vec<String>),

    /// JIG [jig-id] -- Identifies the Jig with the provided ID
    Jig(String),

    /// SHUTDOWN [reason] -- Shuts down the test infrastructure
    Shutdown(String),

    /// PING [string] -- Sends a challenge.  Must respond with PONG [string]
    Ping(String),

    /// TESTS [list of tests] -- Report the tests associated with a scenario
    Tests(String, Vec<String>),

    /// START [scenario] -- Report when a scenario is started
    Start(String),

    /// FINISH [scenario] [code] [reason] -- Report when a scenario is finished
    Finish(String, u32, String),

    /// RUNNING [test] -- Report when a test has started running
    Running(String),

    /// FAILED [test] [reason] -- Report when a test has failed
    Failed(String, String),
}

#[derive(Clone, Debug)]
pub enum ControlMessageContents {
    Log(String),
    Hello(String),
    Scenario(String),
    Pong(String),
    GetScenarios,
    GetJig,
    GetTests(Option<String>),
    /// TESTS
    StartTests(Option<String>),
    AbortTests,
    Shutdown(Option<String>),
}

#[derive(Clone, Debug)]
pub struct BroadcastMessage {

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
    pub message: BroadcastMessageContents,
}

#[derive(Clone, Debug)]
pub struct ControlMessage {

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
    pub message: ControlMessageContents,
}

#[derive(Debug)]
pub enum ControllerError {
}

pub struct Controller {
    broadcast: Arc<Mutex<bus::Bus<BroadcastMessage>>>,
    control: mpsc::Sender<ControlMessage>,
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

    pub fn controller_thread(rx: mpsc::Receiver<ControlMessage>,
                             bus: Arc<Mutex<bus::Bus<BroadcastMessage>>>,
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

                // Get a reference to the testset, but let 'myself' be
                // unlocked at the end of the whole exercise.
                let ref mut testset = match myself.try_lock() {
                    Err(_) => continue 'retry_mutex,
                    Ok(mut me) => {
                        let me = me.deref_mut();
                        match *(me.testset.lock().unwrap()) {
                            None => {
                                Controller::broadcast_internal(&bus,
                                                    BroadcastMessageContents::Log("TestSet is None".to_string()));
                                continue 'new_msg;
                            },
                            Some(ref mut s) => {
                                s.clone()
                            },
                        }
                    },
                };
                let ref mut testset = testset.lock().unwrap();

                match msg.message {
                    /// Log messages: simply rebroadcast them onto the broadcast bus.
                    ControlMessageContents::Log(l) => {
                        let bc_msg = BroadcastMessage {
                            message_type: msg.message_type,
                            unit_id: msg.unit_id,
                            unit_type: msg.unit_type,
                            unix_time: msg.unix_time,
                            unix_time_nsecs: msg.unix_time_nsecs,
                            message: BroadcastMessageContents::Log(l),
                        };
                        bus.lock().unwrap().deref_mut().broadcast(bc_msg);
                    },

                    // Get the current jig information and broadcast it on the bus.
                    ControlMessageContents::GetJig => {
                        let jig_name = testset.get_jig_name();
                        Controller::broadcast_internal(&bus, BroadcastMessageContents::Jig(jig_name));
                    },

                    // Set the current scenario to the specified one.
                    ControlMessageContents::Scenario(s) => {
                        testset.set_scenario(&s);
                    },

                    ControlMessageContents::Hello(s) => {
                        testset.set_interface_hello(msg.unit_id, s);
                        process::exit(0);
                    },

                    ControlMessageContents::Shutdown(s) => {
                        let me = myself.lock().unwrap();
                        let mut should_exit = (*me).should_exit.lock().unwrap();
                        match s {
                            Some(s) => println!("Shutdown called: {}", s),
                            None => println!("Shutdown called (no reason)"),
                        }
                        *(should_exit.deref_mut()) = true;
                    },

                    // Respond to a PING command.  Unimplemented.
                    ControlMessageContents::Pong(s) => (),

                    // Start running tests.  Unimplemented.
                    ControlMessageContents::StartTests(s) => testset.start_scenario(s),
                    ControlMessageContents::AbortTests => (),

                    ControlMessageContents::GetScenarios => testset.send_scenarios(),
                    ControlMessageContents::GetTests(s) => testset.send_tests(s),

                    //_ => println!("Unhandled message: {:?}", msg),
                };

                continue 'new_msg;
            }
        };
    }

    fn broadcast_internal(bus: &Arc<Mutex<bus::Bus<BroadcastMessage>>>,
                          msg: BroadcastMessageContents) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        bus.lock().unwrap().deref_mut().broadcast(BroadcastMessage {
            message_type: 2,
            unit_id: "internal".to_string(),
            unit_type: "core".to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: msg,
        });
    }

    pub fn add_logger<F>(&mut self, logger_func: F)
        where F: Send + 'static + Fn(BroadcastMessage) {

        self.add_broadcast(move |msg| match msg {
            BroadcastMessage { message: BroadcastMessageContents::Log(_), .. } => logger_func(msg),
            _ => (),
        });
    }

    pub fn add_broadcast<F>(&mut self, broadcast_func: F)
        where F: Send + 'static + Fn(BroadcastMessage) {

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

    pub fn control_message(&self, message: &ControlMessage) {
        self.control.send(message.clone()).unwrap();
    }

    pub fn send_control(&self,
                        unit_name: String,
                        unit_type: String,
                        contents: &ControlMessageContents) {

        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.control_message(&ControlMessage {
            message_type: 2,
            unit_id: unit_name,
            unit_type: unit_type,
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: contents.clone(),
        });
    }

    pub fn send_broadcast(&self,
                          unit_name: String,
                          unit_type: String,
                          contents: BroadcastMessageContents) {

        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.broadcast.lock().unwrap().deref_mut().broadcast(BroadcastMessage {
            message_type: 2,
            unit_id: unit_name,
            unit_type: unit_type,
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: contents,
        });
    }
}