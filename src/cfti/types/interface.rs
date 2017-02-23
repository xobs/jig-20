extern crate bus;

use cfti::types::Jig;
use cfti::testset::TestSet;
use cfti::controller::{self, Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::process;
use cfti::unitfile;
use cfti::config;

use std::collections::HashMap;
use std::process::{Stdio, ChildStdin};
use std::sync::{Arc, Mutex};
use std::thread;
use std::io::{BufRead, BufReader, Write};
use std::fmt::{Formatter, Display, Error};

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
    MakeCommandFailed,
    ExecCommandFailed,
    InvalidType(String),
}

impl Display for InterfaceError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            &InterfaceError::FileLoadError => write!(f, "Unable to load file"),
            &InterfaceError::MissingInterfaceSection => write!(f, "Unit file is missing interface section"),
            &InterfaceError::MissingExecSection => write!(f, "Unit file is missing exec entry"),
            &InterfaceError::MakeCommandFailed => write!(f, "Unable to make command"),
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
}

impl Interface {
    pub fn new(id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               config: &config::Config,
               controller: &Controller) -> Option<Result<Interface, InterfaceError>> {

        let unit_file = match unitfile::UnitFile::new(path) {
            Err(e) => return Some(Err(InterfaceError::FileLoadError)),
            Ok(f) => f,
        };

        if ! unit_file.has_section("Interface") {
            return Some(Err(InterfaceError::MissingInterfaceSection));
        }

        // Check to see if this interface is compatible with this jig.
        match unit_file.get("Interface", "Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> = s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break
                    }
                }
                if found_it == false {
                    controller.debug("interface", id, format!("The interface '{}' is not compatible with this jig", id));
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
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let format = match unit_file.get("Interface", "Format") {
            None => InterfaceFormat::Text,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "text" => InterfaceFormat::Text,
                "json" => InterfaceFormat::JSON,
                _ => return Some(Err(InterfaceError::InvalidType(s.clone()))),
            },
        };

       Some(Ok(Interface {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            working_directory: working_directory,
            format: format,
            controller: controller.clone(),
            hello: "".to_string(),
       }))
    }

    pub fn id(&self) -> &str {
        return &self.id.as_ref();
    }

    pub fn kind(&self) -> &str {
        return "interface"
    }

    pub fn set_hello(&mut self, hello: String) {
        self.hello = hello;
    }

    pub fn log(&self, msg: String) {
        self.broadcast(BroadcastMessageContents::Log(msg));
    }

    pub fn broadcast(&self, msg: BroadcastMessageContents) {
        self.controller.broadcast(self.id(), self.kind(), &msg);
    }

    fn text_write(stdin: &mut ChildStdin, msg: controller::BroadcastMessage) {
        //println!("Sending data to interface: {:?}", msg);
        match msg.message {
            BroadcastMessageContents::Log(l) => writeln!(stdin,
                                                "LOG {}\t{}\t{}\t{}\t{}\t{}",
                                                msg.message_class,
                                                msg.unit_id,
                                                msg.unit_type,
                                                msg.unix_time,
                                                msg.unix_time_nsecs,
                                                l.to_string().replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r")).unwrap(),
            BroadcastMessageContents::Jig(j) => writeln!(stdin,
                                                "JIG {}", j.to_string()).unwrap(),
            BroadcastMessageContents::Describe(class, field, name, value) =>
                                        writeln!(stdin,
                                        "DESCRIBE {} {} {} {}",
                                        class, field, name, value).unwrap(),
            BroadcastMessageContents::Scenario(name) => writeln!(stdin,
                                                "SCENARIO {}", name).unwrap(),
            BroadcastMessageContents::Scenarios(list) => writeln!(stdin,
                                                "SCENARIOS {}", list.join(" ")).unwrap(),
            BroadcastMessageContents::Hello(name) => writeln!(stdin,
                                                "HELLO {}", name).unwrap(),
            BroadcastMessageContents::Ping(val) => writeln!(stdin,
                                                "PING {}", val).unwrap(),
            BroadcastMessageContents::Shutdown(reason) => writeln!(stdin,
                                                "SHUTDOWN {}", reason).unwrap(),
            BroadcastMessageContents::Tests(scenario, tests) => writeln!(stdin,
                                                "TESTS {} {}", scenario, tests.join(" ")).unwrap(),
            BroadcastMessageContents::Running(test) => writeln!(stdin,
                                                "RUNNING {}", test).unwrap(),
            BroadcastMessageContents::Skip(test, reason) => writeln!(stdin,
                                                "SKIP {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Fail(test, reason) => writeln!(stdin,
                                                "FAIL {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Pass(test, reason) => writeln!(stdin,
                                                "PASS {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Start(scenario) => writeln!(stdin,
                                                "START {}", scenario).unwrap(),
            BroadcastMessageContents::Finish(scenario, result, reason) => writeln!(stdin,
                                                "FINISH {} {} {}", scenario, result, reason).unwrap(),
        }
    }

    fn json_write(stdin: &mut ChildStdin, msg: controller::BroadcastMessage) {
    }

    fn text_read(line: String, id: &String, controller: &Controller) {
        controller.debug(id, "interface", format!("CFTI interface input: {}", line));
        let mut words: Vec<String> = line.split_whitespace().map(|x| x.to_string()).collect();
        let verb = words[0].to_lowercase();
        words.remove(0);

        let response = match verb.as_str() {
            "scenario" => ControlMessageContents::Scenario(words[0].to_lowercase()),
            "scenarios" => ControlMessageContents::GetScenarios,
            "tests" =>
                if words.is_empty() {
                    ControlMessageContents::GetTests(None)
                } else {
                    ControlMessageContents::GetTests(Some(words[0].to_lowercase()))
                },
            "start" =>
                if words.is_empty() {
                    ControlMessageContents::StartScenario(None)
                } else {
                    ControlMessageContents::StartScenario(Some(words[0].to_lowercase()))
                },
            "abort" => ControlMessageContents::AbortTests,
            "pong" => ControlMessageContents::Pong(words[0].to_lowercase()),
            "jig" => ControlMessageContents::GetJig,
            "hello" => ControlMessageContents::Hello(words.join(" ")),
            "shutdown" =>
                if words.is_empty() {
                    ControlMessageContents::Shutdown(None)
                } else {
                    ControlMessageContents::Shutdown(Some(words.join(" ")))
                },
            "log" => ControlMessageContents::Log(words.join(" ")),
            _ => ControlMessageContents::Log(format!("Unimplemented verb: {}", verb)),
        };

        controller.control(id, "interface", &response);
    }

    pub fn start(&self, ts: &TestSet) -> Result<(), InterfaceError> {
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => {
                self.log(format!("Unable to run logger: {:?}", e));
                return Err(InterfaceError::MakeCommandFailed)
            },
        };

        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        if let Some(ref s) = self.working_directory {
            cmd.current_dir(s);
        }

        self.log(format!("About to run command: {:?}", cmd));

        let child = match cmd.spawn() {
            Err(e) => {
                self.log(format!("Unable to spawn {:?}: {}", cmd, e));
                return Err(InterfaceError::ExecCommandFailed);
            },
            Ok(s) => s,
        };

        self.log(format!("Launched an interface: {}", self.id()));
        let mut stdin = child.stdin.unwrap();
        let stdout = child.stdout.unwrap();

        // Send some initial information to the client.
        writeln!(stdin, "HELLO Jig/20 1.0").unwrap();
        writeln!(stdin, "JIG {}", ts.get_jig_id()).unwrap();
        writeln!(stdin, "DESCRIBE JIG NAME {}", ts.get_jig_name()).unwrap();
        writeln!(stdin, "DESCRIBE JIG DESCRIPTION {}", ts.get_jig_description()).unwrap();

        match self.format {
            InterfaceFormat::Text => {
                // Send all broadcasts to the stdin of the child process.
                self.controller.listen(move |msg| Interface::text_write(&mut stdin, msg));

                // Monitor the child process' stdout, and pass values to the controller.
                let controller = self.controller.clone();
                let id = self.id.clone();
                let builder = thread::Builder::new()
                    .name(format!("I {} -> CFTI", id).into());

                builder.spawn(move || {
                    for line in BufReader::new(stdout).lines() {
                        match line {
                            Err(e) => {println!("Error in interface: {}", e); return; },
                            Ok(l) => Interface::text_read(l, &id, &controller),
                        }
                    }
                }).unwrap();
            },
            InterfaceFormat::JSON => {
                self.controller.listen(move |msg| {Interface::json_write(&mut stdin, msg);});
            },
        };
        Ok(())
    }
}