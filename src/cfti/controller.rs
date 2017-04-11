extern crate bus;
use std::thread;
use std::fmt;
use std::time;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;
use std::sync::mpsc::Sender;

use cfti::testset::TestSetCommand;
use cfti::types::unit::Unit;

#[derive(Clone, Debug)]
pub enum BroadcastMessageContents {
    // Hello(String),
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
    // Ping(String),
    /// TESTS [list of tests] -- Report the tests associated with a scenario
    Tests(String, Vec<String>),

    /// START [scenario] -- Report when a scenario is started
    Start(String),

    /// FINISH [scenario] [code] [reason] -- Report when a scenario is finished
    Finish(String, u32, String),

    /// SKIP [scenario] [reason] -- Don't run a test, for some reason
    Skip(String, String),

    /// RUNNING [test] -- Report when a test has started running
    Running(String),

    /// PASS [test] [message] -- Report when a test has passed
    Pass(String, String),

    /// FAIL [test] [reason] -- Report when a test has failed
    Fail(String, String),
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
    StartScenario(Option<String>),
    AbortTests,
    Shutdown(Option<String>),

    /// Causes the currently-executing Scenario to move to the next step.
    AdvanceScenario,

    /// Sets the communications channel to control the TestSet
    SetTestsetChannel(Sender<TestSetCommand>),
}

#[derive(Clone, Debug)]
pub struct BroadcastMessage {
    /// A string identifying whta type of message it is.  Common predefined values
    /// are "internal-debug", "internal-status", and "normal".
    pub message_class: String,

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
    /// A string indicating what sort of message it is.
    pub message_class: String,

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

#[derive(Clone)]
pub struct Controller {
    broadcast: Arc<Mutex<bus::Bus<BroadcastMessage>>>,
    control: mpsc::Sender<ControlMessage>,
}

impl fmt::Debug for Controller {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Controller")
    }
}

impl Controller {
    pub fn new() -> Result<Controller, ControllerError> {
        let (tx, rx) = mpsc::channel();
        let bus = Arc::new(Mutex::new(bus::Bus::new(4096)));
        let controller = Controller {
            broadcast: bus.clone(),
            control: tx,
        };

        // The controller runs in its own thread.
        let builder = thread::Builder::new().name("C-Rx".into());
        builder.spawn(move || Controller::controller_thread(rx, bus))
            .unwrap();

        Ok(controller)
    }

    pub fn controller_thread(rx: mpsc::Receiver<ControlMessage>,
                             bus: Arc<Mutex<bus::Bus<BroadcastMessage>>>) {
        let mut testset_opt: Option<Sender<TestSetCommand>> = None;
        loop {
            let msg = match rx.recv() {
                // Receiving an error means the sender has closed.
                Err(_) => return,
                Ok(o) => o,
            };

            // Set the testset output control channel, if that's the message we just got.
            if let ControlMessageContents::SetTestsetChannel(chan) = msg.message {
                testset_opt = Some(chan);
                continue;
            }

            let testset = match testset_opt {
                Some(ref s) => s,
                None => {
                    println!("Warning: TestSet is still None");
                    continue;
                }
            };

            match msg.message {
                /// Log messages: simply rebroadcast them onto the broadcast bus.
                ControlMessageContents::Log(l) => {
                    let bc_msg = BroadcastMessage {
                        message_class: msg.message_class,
                        unit_id: msg.unit_id,
                        unit_type: msg.unit_type,
                        unix_time: msg.unix_time,
                        unix_time_nsecs: msg.unix_time_nsecs,
                        message: BroadcastMessageContents::Log(l),
                    };
                    bus.lock().unwrap().deref_mut().broadcast(bc_msg);
                }

                // Get the current jig information and broadcast it on the bus.
                ControlMessageContents::GetJig => {
                    testset.send(TestSetCommand::DescribeJig).unwrap();
                }

                // Set the current scenario to the specified one.
                ControlMessageContents::Scenario(s) => {
                    // If there is a scenario running already, stop it.
                    testset.send(TestSetCommand::AbortScenario).unwrap();
                    testset.send(TestSetCommand::SetScenario(s)).unwrap();
                }

                ControlMessageContents::Hello(s) => {
                    testset.send(TestSetCommand::SetInterfaceHello(msg.unit_id, s)).unwrap();
                }

                ControlMessageContents::Shutdown(s) => {
                    let reason = match s {
                        Some(s) => s,//println!("Shutdown called: {}", s),
                        None => "(no reason)".to_string(),//println!("Shutdown called (no reason)"),
                    };
                    let bc_msg = BroadcastMessage {
                        message_class: msg.message_class,
                        unit_id: msg.unit_id,
                        unit_type: msg.unit_type,
                        unix_time: msg.unix_time,
                        unix_time_nsecs: msg.unix_time_nsecs,
                        message: BroadcastMessageContents::Shutdown(reason),
                    };
                    bus.lock().unwrap().deref_mut().broadcast(bc_msg);
                    testset.send(TestSetCommand::Shutdown).unwrap();
                }

                // Respond to a PING command.  Unimplemented.
                ControlMessageContents::Pong(_) => (),

                // Start running tests.
                ControlMessageContents::StartScenario(s) => {
                    testset.send(TestSetCommand::StartScenario(s)).unwrap();
                }
                ControlMessageContents::AbortTests => {
                    testset.send(TestSetCommand::AbortTests).unwrap()
                }
                ControlMessageContents::AdvanceScenario => {
                    testset.send(TestSetCommand::AdvanceScenario).unwrap();
                }

                ControlMessageContents::GetScenarios => {
                    testset.send(TestSetCommand::SendScenarios).unwrap()
                }
                ControlMessageContents::GetTests(s) => {
                    testset.send(TestSetCommand::SendTests(s)).unwrap()
                }

                ControlMessageContents::SetTestsetChannel(_) => {
                    // This condition was handled previously.
                }
            }
        }
    }

    pub fn listen_logs<F>(&self, mut logger_func: F)
        where F: Send + 'static + FnMut(BroadcastMessage) -> Result<(), String>
    {

        self.listen(move |msg| match msg {
            BroadcastMessage { message: BroadcastMessageContents::Log(_), .. } => logger_func(msg),
            _ => Ok(()),
        });
    }

    pub fn listen<F>(&self, mut broadcast_func: F)
        where F: Send + 'static + FnMut(BroadcastMessage) -> Result<(), String>
    {

        let mut console_rx_channel = self.broadcast.lock().unwrap().deref_mut().add_rx();
        let broadcaster = self.broadcast.clone();
        let builder = thread::Builder::new().name("B-Hook".into());
        builder.spawn(move ||
            loop {
                match console_rx_channel.recv() {
                    Err(e) => { println!("DEBUG!! Channel closed, probably quitting.  Err: {:?}", e); return; },
                    Ok(msg) => if let Err(e) = broadcast_func(msg) {
                        Self::do_broadcast_class(&broadcaster,
                                    "debug",
                                    "controller",
                                    "controller",
                                    &BroadcastMessageContents::Log(format!("Broadcast watcher returned an error: {:?}", e)));

                        return;
                    },
                };
            }
        ).unwrap();
    }

    pub fn control_class(&self,
                         message_class: &str,
                         unit_name: &str,
                         unit_type: &str,
                         contents: &ControlMessageContents) {
        Self::do_control_class(&self.control, message_class, unit_name, unit_type, contents)
    }

    pub fn control(&self, unit_name: &str, unit_type: &str, contents: &ControlMessageContents) {
        Self::do_control_class(&self.control, "standard", unit_name, unit_type, contents)
    }

    pub fn control_class_unit<T: Unit + ?Sized>(message_class: &str,
                                                unit: &T,
                                                contents: &ControlMessageContents) {
        unit.controller().control_class(message_class, unit.id(), unit.kind(), contents);
    }

    pub fn control_unit<T: Unit + ?Sized>(unit: &T, contents: &ControlMessageContents) {
        unit.controller().control(unit.id(), unit.kind(), contents);
    }

    pub fn shutdown(&self, msg: &str) {
        Self::do_control_class(&self.control,
                               "system",
                               "none",
                               "none",
                               &ControlMessageContents::Shutdown(Some(msg.to_string())));
    }

    pub fn broadcast_class(&self,
                           message_class: &str,
                           unit_name: &str,
                           unit_type: &str,
                           contents: &BroadcastMessageContents) {
        Self::do_broadcast_class(&self.broadcast,
                                 message_class,
                                 unit_name,
                                 unit_type,
                                 contents)
    }

    pub fn broadcast_class_unit<T: Unit + ?Sized>(message_class: &str,
                                                  unit: &T,
                                                  contents: &BroadcastMessageContents) {
        unit.controller().broadcast_class(message_class, unit.id(), unit.kind(), contents);
    }

    pub fn broadcast_unit<T: Unit + ?Sized>(unit: &T, contents: &BroadcastMessageContents) {
        unit.controller().broadcast(unit.id(), unit.kind(), contents);
    }

    pub fn broadcast(&self,
                     unit_name: &str,
                     unit_type: &str,
                     contents: &BroadcastMessageContents) {
        Self::do_broadcast_class(&self.broadcast, "standard", unit_name, unit_type, contents)
    }

    // pub fn debug(&self, unit_name: &str, unit_type: &str, msg: String) {
    // Self::do_broadcast_class(&self.broadcast,
    // "debug",
    // unit_name,
    // unit_type,
    // &BroadcastMessageContents::Log(msg))
    // }
    //
    pub fn debug_unit<T: Unit + ?Sized>(unit: &T, msg: String) {
        Self::broadcast_class_unit("debug-internal", unit, &BroadcastMessageContents::Log(msg))
    }

    pub fn warn_unit<T: Unit + ?Sized>(unit: &T, msg: String) {
        Self::broadcast_class_unit("warning", unit, &BroadcastMessageContents::Log(msg))
    }

    fn do_broadcast_class(bus: &Arc<Mutex<bus::Bus<BroadcastMessage>>>,
                          message_class: &str,
                          unit_name: &str,
                          unit_type: &str,
                          contents: &BroadcastMessageContents) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        bus.lock().unwrap().deref_mut().broadcast(BroadcastMessage {
            message_class: message_class.to_string(),
            unit_id: unit_name.to_string(),
            unit_type: unit_type.to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: contents.clone(),
        });
    }

    fn do_control_class(control: &mpsc::Sender<ControlMessage>,
                        message_class: &str,
                        unit_name: &str,
                        unit_type: &str,
                        contents: &ControlMessageContents) {

        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        control.send(ControlMessage {
                message_class: message_class.to_string(),
                unit_id: unit_name.to_string(),
                unit_type: unit_type.to_string(),
                unix_time: elapsed.as_secs(),
                unix_time_nsecs: elapsed.subsec_nanos(),
                message: contents.clone(),
            })
            .unwrap();
    }
}
