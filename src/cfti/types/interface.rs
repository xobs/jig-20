extern crate json;
extern crate runny;

use self::runny::Runny;
use self::runny::running::Running;

use cfti::types::unit::Unit;
use cfti::controller::{self, Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::process;
use cfti::unitfile;
use cfti::config;
use cfti::testset;

use std::io::Write;
use std::fmt::{Formatter, Display, Error};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
enum InterfaceFormat {
    Text,
    JSON,
}

#[derive(Debug)]
pub enum InterfaceError {
    FileLoadError,
    MissingInterfaceSection,
    MissingExecSection,
    ExecCommandFailed,
    InvalidType(String),
}

impl Display for InterfaceError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            &InterfaceError::FileLoadError => write!(f, "Unable to load file"),
            &InterfaceError::MissingInterfaceSection => {
                write!(f, "Unit file is missing interface section")
            }
            &InterfaceError::MissingExecSection => write!(f, "Unit file is missing exec entry"),
            &InterfaceError::ExecCommandFailed => write!(f, "Unable to exec command"),
            &InterfaceError::InvalidType(ref s) => write!(f, "Invalid interface type: {}", s),
        }
    }
}

#[derive(Debug)]
pub struct Interface {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this logger.
    name: String,

    /// description: Paragraph describing this logger.
    description: Option<String>,

    /// format: The format requested by this interface.
    format: InterfaceFormat,

    /// exec_start: A command to run when starting the interface.
    exec_start: String,

    /// working_directory: The path where the program will be run from.
    working_directory: Option<String>,

    /// The controller where messages come and go.
    controller: Controller,

    /// The value set by the "HELLO" command
    hello: String,

    /// The currently running process
    process: Arc<Mutex<Option<Running>>>,
}

impl Interface {
    pub fn new(id: &str,
               path: &str,
               test_set: &testset::TestSet,
               config: &config::Config)
               -> Option<Result<Interface, InterfaceError>> {

        let jigs = test_set.jigs();

        let unit_file = match unitfile::UnitFile::new(path) {
            Err(_) => return Some(Err(InterfaceError::FileLoadError)),
            Ok(f) => f,
        };

        if !unit_file.has_section("Interface") {
            return Some(Err(InterfaceError::MissingInterfaceSection));
        }

        // Check to see if this interface is compatible with this jig.
        match unit_file.get("Interface", "Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> =
                    s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break;
                    }
                }
                if found_it == false {
                    test_set.warn(format!("The interface '{}' is not compatible with this jig",
                                             id));
                    return None;
                }
            }
        }

        let description = match unit_file.get("Interface", "Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match unit_file.get("Interface", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match unit_file.get("Interface", "ExecStart") {
            None => return Some(Err(InterfaceError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let working_directory = match unit_file.get("Interface", "WorkingDirectory") {
            None => config.default_working_directory().clone(),
            Some(s) => Some(s.to_string()),
        };

        let format = match unit_file.get("Interface", "Format") {
            None => InterfaceFormat::Text,
            Some(s) => {
                match s.to_string().to_lowercase().as_ref() {
                    "text" => InterfaceFormat::Text,
                    "json" => InterfaceFormat::JSON,
                    _ => return Some(Err(InterfaceError::InvalidType(s.to_string()))),
                }
            }
        };

        Some(Ok(Interface {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            working_directory: working_directory,
            format: format,
            controller: test_set.controller().clone(),
            hello: "".to_string(),
            process: Arc::new(Mutex::new(None)),
        }))
    }

    pub fn set_hello(&mut self, hello: String) {
        self.hello = hello;
    }

    fn text_write<T>(stdin: &mut T, msg: controller::BroadcastMessage) -> Result<(), String>
        where T: Write
    {
        let result = match msg.message {
            BroadcastMessageContents::Log(l) => {
                writeln!(stdin,
                         "LOG {}\t{}\t{}\t{}\t{}\t{}",
                         msg.message_class,
                         msg.unit_id,
                         msg.unit_type,
                         msg.unix_time,
                         msg.unix_time_nsecs,
                         l.to_string()
                             .replace("\\", "\\\\")
                             .replace("\t", "\\t")
                             .replace("\n", "\\n")
                             .replace("\r", "\\r"))
            }
            BroadcastMessageContents::Jig(j) => writeln!(stdin, "JIG {}", j.to_string()),
            BroadcastMessageContents::Describe(class, field, name, value) => {
                writeln!(stdin, "DESCRIBE {} {} {} {}", class, field, name, value)
            }
            BroadcastMessageContents::Scenario(name) => writeln!(stdin, "SCENARIO {}", name),
            BroadcastMessageContents::Scenarios(list) => {
                writeln!(stdin, "SCENARIOS {}", list.join(" "))
            }
            //            BroadcastMessageContents::Hello(name) => writeln!(stdin,
            //                                                "HELLO {}", name),
            //            BroadcastMessageContents::Ping(val) => writeln!(stdin,
            //                                                "PING {}", val),
            BroadcastMessageContents::Shutdown(reason) => writeln!(stdin, "EXIT {}", reason),
            BroadcastMessageContents::Tests(scenario, tests) => {
                writeln!(stdin, "TESTS {} {}", scenario, tests.join(" "))
            }
            BroadcastMessageContents::Running(test) => writeln!(stdin, "RUNNING {}", test),
            BroadcastMessageContents::Skip(test, reason) => {
                writeln!(stdin, "SKIP {} {}", test, reason)
            }
            BroadcastMessageContents::Fail(test, reason) => {
                writeln!(stdin, "FAIL {} {}", test, reason)
            }
            BroadcastMessageContents::Pass(test, reason) => {
                writeln!(stdin, "PASS {} {}", test, reason)
            }
            BroadcastMessageContents::Start(scenario) => writeln!(stdin, "START {}", scenario),
            BroadcastMessageContents::Finish(scenario, result, reason) => {
                writeln!(stdin, "FINISH {} {} {}", scenario, result, reason)
            }
        };
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    fn json_write<T>(stdin: &mut T, msg: controller::BroadcastMessage) -> Result<(), String>
        where T: Write
    {
        let mut object = json::JsonValue::new_object();
        object["message_class"] = msg.message_class.into();
        object["unit_id"] = msg.unit_id.into();
        object["unit_type"] = msg.unit_type.into();
        object["unix_time"] = msg.unix_time.into();
        object["unix_time_nsecs"] = msg.unix_time_nsecs.into();
        match msg.message {
            BroadcastMessageContents::Log(l) => {
                object["type"] = "log".into();
                object["message"] = l.into();
            }
            BroadcastMessageContents::Jig(j) => {
                object["type"] = "jig".into();
                object["id"] = j.into();
            }
            BroadcastMessageContents::Describe(class, field, name, value) => {
                object["type"] = "describe".into();
                object["class"] = class.into();
                object["field"] = field.into();
                object["name"] = name.into();
                object["value"] = value.into();
            }
            BroadcastMessageContents::Scenario(name) => {
                object["type"] = "scenario".into();
                object["id"] = name.into();
            }
            BroadcastMessageContents::Scenarios(list) => {
                object["type"] = "scenarios".into();
                let mut scenarios: Vec<json::JsonValue> = vec![];
                for scenario in list {
                    scenarios.push(scenario.clone().into());
                }
                object["scenarios"] = scenarios.into();
            }
            //            BroadcastMessageContents::Hello(name) => {
            //                object["type"] = "hello".into();
            //                object["id"] = name.into();
            //            },
            //            BroadcastMessageContents::Ping(val) => {
            //                object["type"] = "ping".into();
            //                object["val"] = val.into();
            //            },
            BroadcastMessageContents::Shutdown(reason) => {
                object["type"] = "shutdown".into();
                object["reason"] = reason.into();
            }
            BroadcastMessageContents::Tests(scenario, list) => {
                object["type"] = "tests".into();
                object["scenario"] = scenario.into();
                let mut tests: Vec<json::JsonValue> = vec![];
                for test in list {
                    tests.push(test.clone().into());
                }
                object["tests"] = tests.into();
            }
            BroadcastMessageContents::Running(test) => {
                object["type"] = "running".into();
                object["test"] = test.into();
            }
            BroadcastMessageContents::Skip(test, reason) => {
                object["type"] = "skip".into();
                object["test"] = test.into();
                object["reason"] = reason.into();
            }
            BroadcastMessageContents::Fail(test, reason) => {
                object["type"] = "fail".into();
                object["test"] = test.into();
                object["reason"] = reason.into();
            }
            BroadcastMessageContents::Pass(test, reason) => {
                object["type"] = "pass".into();
                object["test"] = test.into();
                object["reason"] = reason.into();
            }
            BroadcastMessageContents::Start(scenario) => {
                object["type"] = "start".into();
                object["scenario"] = scenario.into();
            }
            BroadcastMessageContents::Finish(scenario, result, reason) => {
                object["type"] = "finish".into();
                object["scenario"] = scenario.into();
                object["result"] = result.into();
                object["reason"] = reason.into();
            }
        };
        match writeln!(stdin, "{}", json::stringify(object)) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    fn cfti_unescape(msg: String) -> String {
        msg.replace("\\t", "\t").replace("\\n", "\n").replace("\\r", "\r").replace("\\\\", "\\")
    }

    fn text_read<T: Unit + ?Sized>(line: String, unit: &T) -> Result<(), ()> {
        unit.debug(format!("CFTI interface input: {}", line));
        let mut words: Vec<String> =
            line.split_whitespace().map(|x| Self::cfti_unescape(x.to_string())).collect();

        // Don't crash if we get a blank line.
        if words.len() == 0 {
            return Ok(());
        }

        let verb = words[0].to_lowercase();
        words.remove(0);

        let response = match verb.as_str() {
            "scenario" => ControlMessageContents::Scenario(words[0].to_lowercase()),
            "scenarios" => ControlMessageContents::GetScenarios,
            "tests" => {
                if words.is_empty() {
                    ControlMessageContents::GetTests(None)
                } else {
                    ControlMessageContents::GetTests(Some(words[0].to_lowercase()))
                }
            }
            "start" => {
                if words.is_empty() {
                    ControlMessageContents::StartScenario(None)
                } else {
                    ControlMessageContents::StartScenario(Some(words[0].to_lowercase()))
                }
            }
            "abort" => ControlMessageContents::AbortTests,
            "pong" => ControlMessageContents::Pong(words[0].to_lowercase()),
            "jig" => ControlMessageContents::GetJig,
            "hello" => ControlMessageContents::Hello(words.join(" ")),
            "shutdown" => {
                if words.is_empty() {
                    ControlMessageContents::Shutdown(None)
                } else {
                    ControlMessageContents::Shutdown(Some(words.join(" ")))
                }
            }
            "log" => ControlMessageContents::Log(words.join(" ")),
            _ => ControlMessageContents::Log(format!("Unimplemented verb: {}", verb)),
        };

        Controller::control_unit(unit, &response);
        Ok(())
    }

    pub fn start(&self, working_directory: &Option<String>) -> Result<(), InterfaceError> {

        let working_directory = match self.working_directory {
            Some(ref s) => Some(s.clone()),
            None => {
                match *working_directory {
                    Some(ref s) => Some(s.clone()),
                    None => None,
                }
            }
        };

        let mut running =
            match Runny::new(self.exec_start.as_str()).directory(&working_directory).start() {
                Ok(p) => p,
                Err(e) => {
                    self.debug(format!("Unable to run interface command {}: {:?}",
                                       self.exec_start,
                                       e));
                    return Err(InterfaceError::ExecCommandFailed);
                }
            };

        self.debug(format!("Launched interface"));
        let mut stdin = running.take_input();
        let stdout = running.take_output();
        let stderr = running.take_error();

        match self.format {
            InterfaceFormat::Text => {

                // Send some initial information to the client.
                writeln!(stdin, "HELLO Jig/20 1.0").unwrap();

                // Send all broadcasts to the stdin of the child process.
                self.controller.listen(move |msg| Interface::text_write(&mut stdin, msg));
                process::log_output(stderr, self, "stderr").unwrap();
                process::watch_output(stdout, self, move |line, u| Interface::text_read(line, u))
                    .unwrap();
            }
            InterfaceFormat::JSON => {
                self.controller.listen(move |msg| Interface::json_write(&mut stdin, msg));
            }
        };

        *(self.process.lock().unwrap()) = Some(running);
        Ok(())
    }
}

impl Unit for Interface {
    fn id(&self) -> &str {
        &self.id.as_ref()
    }

    fn kind(&self) -> &str {
        "interface"
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn description(&self) -> &str {
        match self.description {
            Some(ref s) => s.as_str(),
            None => "",
        }
    }

    fn controller(&self) -> &Controller {
        &self.controller
    }
}