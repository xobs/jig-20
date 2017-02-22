extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use cfti::types::Jig;
use cfti::testset::TestSet;
use cfti::controller::{self, BroadcastMessageContents, ControlMessageContents};
use cfti::process;
use std::process::{Stdio, ChildStdin};
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;
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

    /// The controller where messages go.
    controller: Arc<Mutex<controller::Controller>>,

    /// The value set by the "HELLO" command
    hello: String,
}

impl Interface {
    pub fn new(ts: &TestSet,
               id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               controller: Arc<Mutex<controller::Controller>>) -> Option<Result<Interface, InterfaceError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(InterfaceError::FileLoadError)),
            Ok(s) => s,
        };

        let interface_section = match ini_file.section(Some("Interface")) {
            None => return Some(Err(InterfaceError::MissingInterfaceSection)),
            Some(s) => s,
        };

        // Check to see if this interface is compatible with this jig.
        match interface_section.get("Jigs") {
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
                    ts.debug("interface", id, format!("The interface '{}' is not compatible with this jig", id).as_str());
                    return None;
                }
            }
        }

        let description = match interface_section.get("Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match interface_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match interface_section.get("ExecStart") {
            None => return Some(Err(InterfaceError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let working_directory = match interface_section.get("WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let format = match interface_section.get("Format") {
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
            controller: controller,
            hello: "".to_string(),
       }))
    }

    pub fn id(&self) -> &str {
        return &self.id.as_ref();
    }

    pub fn set_hello(&mut self, hello: String) {
        self.hello = hello;
    }

    fn text_write(stdin: Arc<Mutex<ChildStdin>>, msg: controller::BroadcastMessage) {
        println!("Sending data to interface: {:?}", msg);
        match msg.message {
            BroadcastMessageContents::Log(l) => writeln!(&mut stdin.lock().unwrap(),
                                                "LOG {}\t{}\t{}\t{}\t{}\t{}",
                                                msg.message_class,
                                                msg.unit_id,
                                                msg.unit_type,
                                                msg.unix_time,
                                                msg.unix_time_nsecs,
                                                l.to_string().replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r")).unwrap(),
            BroadcastMessageContents::Jig(j) => writeln!(&mut stdin.lock().unwrap(),
                                                "JIG {}", j.to_string()).unwrap(),
            BroadcastMessageContents::Describe(class, field, name, value) =>
                                        writeln!(&mut stdin.lock().unwrap(),
                                        "DESCRIBE {} {} {} {}",
                                        class, field, name, value).unwrap(),
            BroadcastMessageContents::Scenario(name) => writeln!(&mut stdin.lock().unwrap(),
                                                "SCENARIO {}", name).unwrap(),
            BroadcastMessageContents::Scenarios(list) => writeln!(&mut stdin.lock().unwrap(),
                                                "SCENARIOS {}", list.join(" ")).unwrap(),
            BroadcastMessageContents::Hello(name) => writeln!(&mut stdin.lock().unwrap(),
                                                "HELLO {}", name).unwrap(),
            BroadcastMessageContents::Ping(val) => writeln!(&mut stdin.lock().unwrap(),
                                                "PING {}", val).unwrap(),
            BroadcastMessageContents::Shutdown(reason) => writeln!(&mut stdin.lock().unwrap(),
                                                "SHUTDOWN {}", reason).unwrap(),
            BroadcastMessageContents::Tests(scenario, tests) => writeln!(&mut stdin.lock().unwrap(),
                                                "TESTS {} {}", scenario, tests.join(" ")).unwrap(),
            BroadcastMessageContents::Running(test) => writeln!(&mut stdin.lock().unwrap(),
                                                "RUNNING {}", test).unwrap(),
            BroadcastMessageContents::Skip(test, reason) => writeln!(&mut stdin.lock().unwrap(),
                                                "SKIP {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Fail(test, reason) => writeln!(&mut stdin.lock().unwrap(),
                                                "FAIL {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Pass(test, reason) => writeln!(&mut stdin.lock().unwrap(),
                                                "PASS {} {}", test, reason).unwrap(),
            BroadcastMessageContents::Start(scenario) => writeln!(&mut stdin.lock().unwrap(),
                                                "START {}", scenario).unwrap(),
            BroadcastMessageContents::Finish(scenario, result, reason) => writeln!(&mut stdin.lock().unwrap(),
                                                "FINISH {} {} {}", scenario, result, reason).unwrap(),
        }
    }
    fn json_write(stdin: Arc<Mutex<ChildStdin>>, msg: controller::BroadcastMessage) {
    }

    fn text_read(line: String, id: &String, controller: &mut controller::Controller) {
        println!("Got line: {}", line);
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

        controller.send_control(id, "interface", &response);
    }

    pub fn start(&self, ts: &TestSet) -> Result<(), InterfaceError> {
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => { println!(">>> UNABLE TO RUN INTERFACE: {:?}", e); ts.debug("interface", self.id.as_str(), format!("Unable to run logger: {:?}", e).as_str()); return Err(InterfaceError::MakeCommandFailed) },
        };
        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        match self.working_directory {
            None => (),
            Some(ref s) => {cmd.current_dir(s); },
        }

        let child = match cmd.spawn() {
            Err(e) => { println!("Unable to spawn {:?}: {}", cmd, e); return Err(InterfaceError::ExecCommandFailed) },
            Ok(s) => s,
        };
        println!("Launched an interface: {}", self.id());
        let stdin = Arc::new(Mutex::new(child.stdin.unwrap()));
        let stdout = Arc::new(Mutex::new(child.stdout.unwrap()));

        // Send some initial information to the client.
        writeln!(stdin.lock().unwrap(), "HELLO Jig/20 1.0").unwrap();
        writeln!(stdin.lock().unwrap(), "JIG {}", ts.get_jig_id()).unwrap();
        writeln!(stdin.lock().unwrap(), "DESCRIBE JIG NAME {}", ts.get_jig_name()).unwrap();
        writeln!(stdin.lock().unwrap(), "DESCRIBE JIG DESCRIPTION {}", ts.get_jig_description()).unwrap();

        match self.format {
            InterfaceFormat::Text => {
                // Send all broadcasts to the stdin of the child process.
                ts.monitor_broadcasts(move |msg| Interface::text_write(stdin.clone(), msg));

                // Monitor the child process' stdout, and pass values to the controller.
                let controller_clone = ts.get_controller();
                let id = self.id.clone();
                let builder = thread::Builder::new()
                    .name(format!("Interface {} -> CFTI", id).into());

                builder.spawn(move || {
                    let mut var = stdout.lock().unwrap();
                    let ref mut stdout2 = var.deref_mut();
                    for line in BufReader::new(stdout2).lines() {
                        match line {
                            Err(e) => {println!("Error in interface: {}", e); return; },
                            Ok(l) => Interface::text_read(l, &id, &mut controller_clone.lock().unwrap()),
                        }
                    }
                });
            },
            InterfaceFormat::JSON => {
                ts.monitor_broadcasts(move |msg| {Interface::json_write(stdin.clone(), msg);});
            },
        };
        Ok(())
    }
}