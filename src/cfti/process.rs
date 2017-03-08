// From http://stackoverflow.com/a/36870954
extern crate clonablechild;
extern crate shlex;

use self::clonablechild::{ChildExt, ClonableChild};

use std::io::{self, BufRead};
use std::process::Command;
use std::time::Duration;
use std::thread;
use std::fmt;
use std::process::{Stdio, ChildStdin, ChildStdout, ChildStderr, ExitStatus};

use cfti::controller::{Controller, ControlMessageContents};
use cfti::types::unit::Unit;

#[derive(Debug)]
pub enum CommandError {
    NoCommandSpecified,
    MakeCommandError(String),
    SpawnError(String),
    ChildTimeoutTerminateError(String),
    ChildTerminatedBySignal,
    ReturnCodeError(i32),
}

#[derive(Clone)]
pub struct ChildProcess {
    child: ClonableChild,
}

pub struct Process {
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
    pub child: ChildProcess,
}

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Process {}", self.child.id())
    }
}

pub fn make_command(cmd: &str) -> Result<Command, CommandError> {
    let cmd = cmd.to_string().replace("\\", "\\\\");
    let cmd = cmd.as_str();
    let args = shlex::split(cmd);
    if args.is_none() {
        return Err(CommandError::NoCommandSpecified);
    }
    let mut args = args.unwrap();
    let mut cmd = Command::new(args.remove(0));
    cmd.args(&args);
    Ok(cmd)
}

pub fn try_command(controller: &Controller, cmd: &str, wd: &Option<String>, max: Duration) -> bool {
    let mut cmd = match make_command(cmd) {
        Err(e) => {
            controller.debug("internal", "unknown", format!("Unable to make command: {:?}", e));
            return false;
        },
        Ok(val) => val,
    };

    if let Some(ref s) = *wd {
        cmd.current_dir(s);
    }

    let child = match cmd.spawn() {
        Err(e) => {
            controller.debug("process", "process", format!("Unable to spawn child {:?}: {:?}", cmd, e));
            return false;
        },
        Ok(o) => o.into_clonable(),
    };

    let thr_child = child.clone();
    let thr = thread::spawn(move || {
        thread::park_timeout(max);
        thr_child.kill().ok();
    });

    let status_code = match child.wait() {
        Ok(status) => status.code(),
        Err(e) => {
            thr.thread().unpark();
            controller.debug("process", "process", format!("Unable to wait() for child: {:?}", e));
            return false;
        }
    };

    thr.thread().unpark();
    return status_code.unwrap() == 0
}

pub fn log_output<T: io::Read + Send + 'static, U: Unit>(stream: T, unit: &U, stream_name: &str) {

    let thr_stream_name = stream_name.to_string();

    watch_output(stream, unit, move |msg, unit| {
        Controller::control_class_unit(thr_stream_name.as_str(),
                                       unit,
                                       &ControlMessageContents::Log(msg));
        Ok(())
    });
}

pub fn watch_output<T: io::Read + Send + 'static, F, U: Unit>(stream: T,
                                                     unit: &U,
                                                     mut msg_func: F)
        where F: Send + 'static + FnMut(String, &Unit) -> Result<(), ()> {
    // Monitor the child process' stderr, and pass values to the controller.
    let builder = thread::Builder::new()
        .name(format!("I-E {} -> CFTI", unit.id()).into());
    let thr_unit = unit.to_simple_unit();

    builder.spawn(move || {
        for line in io::BufReader::new(stream).lines() {
            match line {
                Err(e) => {
                    Controller::debug_unit(&thr_unit, format!("Error in interface: {}", e));
                    return;
                },
                Ok(l) => if let Err(e) = msg_func(l, &thr_unit) {
                    Controller::debug_unit(&thr_unit, format!("Message func returned error: {:?}", e));
                    return;
                }
            }
        }
    }).unwrap();
}

/// Formats `cmd_str` as a Command, runs it, and returns the Process.
///
/// Runs the specified command and returns the result.  The command can be
/// waited upon, or timed out.  It is possible to interact with its stdin,
/// stdout, and stderr.
pub fn spawn_cmd<T: Unit>(cmd_str: &str,
                 unit: &T,
                 working_directory: &Option<String>)
        -> Result<Process, CommandError> {

    let cmd = match make_command(cmd_str) {
        Ok(c) => c,
        Err(e) => return  Err(e),
    };

    match spawn(cmd, unit, working_directory) {
        Ok(o) => Ok(o),
        Err(e) => Err(CommandError::SpawnError(format!("{}", e))),
    }
}

fn spawn<T: Unit>(mut cmd: Command,
         unit: &T,
         working_directory: &Option<String>)
        -> Result<Process, io::Error> {
    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let &Some(ref d) = working_directory {
        cmd.current_dir(d);
    }

    let mut child = match cmd.spawn() {
        Err(e) => return Err(e),
        Ok(child) => child.into_clonable(),
    };
    
    let stdin = child.stdin().unwrap();
    let stdout = child.stdout().unwrap();
    let stderr = child.stderr().unwrap();
    let child_thr = child.clone();
    let unit_thr = unit.to_simple_unit();

    thread::spawn(move || {
        match child_thr.wait() {
            Ok(status) => Controller::debug_unit(&unit_thr, format!("Child exited successfully with result: {:?}", status)),
            Err(e) => Controller::debug_unit(&unit_thr, format!("Child errored with exit: {:?}", e)),
        };
    });

    Ok(Process {
        stdin: stdin,
        stdout: stdout,
        stderr: stderr,
        child: ChildProcess { child: child },
    })
}

/// Tries to run `cmd`.
///
/// If `wd` is specified, then runs the command in that working directory.
/// Will only allow the command to run for `max` duration.
/// When the command finishes or times out, `completion` will be called.
///
/// # Errors
/// `CommandError::MakeCommandError(String)` - Unable to make a command for some reason.
/// `CommandError::SpawnError(String)` - Unable to spawn the command for some reason.
/// `CommandError::ChildTimeoutTerminateError(String)` - Couldn't terminate the child after it timed out.
/// `CommandError::ChildTimeoutWaitError(String)` - Couldn't wait for the child after it timed out.
/// `CommandError::ChildTimeout` - Child timed out and was successfully terminated.

pub fn try_command_completion<F>(cmd_str: &str, wd: &Option<String>, max: Duration, completion: F)
        -> Result<Process, CommandError>
        where F: Send + 'static + FnOnce(Result<(), CommandError>)
{

    let mut cmd = match make_command(cmd_str) {
        Err(e) => {
            completion(Err(CommandError::MakeCommandError(format!("{:?}", e))));
            return Err(CommandError::MakeCommandError(format!("{:?}", e)));
        },
        Ok(val) => val,
    };

    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let Some(ref s) = *wd {
        cmd.current_dir(s);
    }

    let mut child = match cmd.spawn() {
        Err(err) => {
            completion(Err(CommandError::SpawnError(format!("{}", err))));
            return Err(CommandError::SpawnError(format!("{}", err)));
        },
        Ok(s) => s.into_clonable(),
    };

    // Take the child's stdio handles and replace them with None.  This allows
    // us to have the thread take ownership of `child` and return the handles.
    let stdout = child.stdout().unwrap();
    let stdin = child.stdin().unwrap();
    let stderr = child.stderr().unwrap();

    let thr_child = child.clone();
    let thr = thread::spawn(move || {
        thread::park_timeout(max);
        thr_child.kill().ok();
    });

    let thr_child = child.clone();
    thread::spawn(move || {
        let status_code = match thr_child.wait() {
            Ok(status) => match status.code() {
                None => {
                    thr.thread().unpark();
                    completion(Err(CommandError::ChildTerminatedBySignal));
                    return;
                },
                Some(s) => s,
            },
            Err(e) => {
                thr.thread().unpark();
                completion(Err(CommandError::ChildTimeoutTerminateError(format!("{}", e))));
                return;
            },
        };

        // If it's a nonzero exit code, that counts as an error.
        if status_code != 0 {
            thr.thread().unpark();
            completion(Err(CommandError::ReturnCodeError(status_code)));
            return;
        }
        thr.thread().unpark();
        completion(Ok(()));
    });

    // Return the file handles so that the calling process can monitor them.
    Ok(Process {
        stdin: stdin,
        stdout: stdout,
        stderr: stderr,
        child: ChildProcess { child: child },
    })
}

impl fmt::Debug for ChildProcess {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Process {}", self.child.id())
    }
}

impl ChildProcess {
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn wait(&self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
    pub fn kill(&self) -> io::Result<()> {
        self.child.kill()
    }
}