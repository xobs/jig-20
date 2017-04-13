extern crate runny;

use std::io::{self, BufRead};
use std::time::Duration;
use std::thread;
use std::env;

use self::runny::{Runny, RunnyError};
use self::runny::running::Running;

use cfti::controller::{Controller, ControlMessageContents};
use cfti::types::unit::Unit;

#[derive(Debug)]
pub enum CommandError {
    SpawnError(String, String),
    ReturnCodeError(i32),
    ChildTerminationError(String),
    RunnyError(String, RunnyError),
}

pub fn log_output<T: io::Read + Send + 'static, U: Unit>
    (stream: T,
     unit: &U,
     stream_name: &str)
     -> Result<thread::JoinHandle<()>, io::Error> {

    let thr_stream_name = stream_name.to_string();

    watch_output(stream, unit, move |msg, unit| {
        Controller::control_class_unit(thr_stream_name.as_str(),
                                       unit,
                                       &ControlMessageContents::Log(msg));
        Ok(())
    })
}

pub fn watch_output<T: io::Read + Send + 'static, F, U: Unit>
    (stream: T,
     unit: &U,
     mut msg_func: F)
     -> Result<thread::JoinHandle<()>, io::Error>
    where F: Send + 'static + FnMut(String, &Unit) -> Result<(), ()>
{
    // Monitor the child process' stderr, and pass values to the controller.
    let builder = thread::Builder::new().name(format!("I-E {} -> CFTI", unit.id()).into());
    let thr_unit = unit.to_simple_unit();

    builder.spawn(move || {
        for line in io::BufReader::new(stream).lines() {
            match line {
                Err(e) => {
                    thr_unit.debug(format!("Error in interface: {}", e));
                    return;
                }
                Ok(l) => {
                    if let Err(e) = msg_func(l, &thr_unit) {
                        thr_unit.debug(format!("Message func returned error: {:?}", e));
                        return;
                    }
                }
            }
        }
    })
}

pub fn try_command<T: Unit>(unit: &T, cmd: &str, wd: &Option<String>, max: Duration) -> bool {
    let paths = match env::var_os("PATH") {
        Some(path) => {
            env::split_paths(&path).map(|x| x.to_str().unwrap().to_string()).collect::<Vec<_>>()
        }
        None => vec![],
    };

    let mut running = match Runny::new(cmd).directory(wd).timeout(max).path(paths).start() {
        Ok(r) => r,
        Err(e) => {
            unit.debug(format!("Unable to start command {}: {:?}", cmd, e));
            return false;
        }
    };
    running.result() == 0
}

/// Formats `cmd_str` as a Command, runs it, and returns the Process.
///
/// Runs the specified command and returns the result.  The command can be
/// waited upon, or timed out.  It is possible to interact with its stdin,
/// stdout, and stderr.
pub fn spawn_cmd<T: Unit>(cmd_str: &str,
                          unit: &T,
                          working_directory: &Option<String>)
                          -> Result<Running, CommandError> {
    let paths = match env::var_os("PATH") {
        Some(path) => {
            env::split_paths(&path).map(|x| x.to_str().unwrap().to_string()).collect::<Vec<_>>()
        }
        None => vec![],
    };

    let process = match Runny::new(cmd_str).directory(working_directory).path(paths).start() {
        Ok(p) => p,
        Err(e) => {
            unit.debug(format!("Unable to spawn command {}: {:?}", cmd_str, e));
            return Err(CommandError::RunnyError(cmd_str.to_string(), e));
        }
    };

    Ok(process)
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

pub fn try_command_completion<F>(cmd_str: &str,
                                 wd: &Option<String>,
                                 max: Duration,
                                 completion: F)
                                 -> Result<Running, CommandError>
    where F: Send + 'static + FnOnce(Result<(), CommandError>)
{
    let mut cmd = Runny::new(cmd_str);

    let paths = match env::var_os("PATH") {
        Some(path) => {
            env::split_paths(&path).map(|x| x.to_str().unwrap().to_string()).collect::<Vec<_>>()
        }
        None => vec![],
    };

    cmd.directory(wd).timeout(max).path(paths);

    // Fork off and exec the child process.
    let child = match cmd.start() {
        Err(err) => {
            completion(Err(CommandError::RunnyError(cmd_str.to_string(), err)));
            return Err(CommandError::SpawnError(cmd_str.to_string(),
                                                format!("Dunno what went wrong")));
        }
        Ok(s) => s,
    };

    let waiter = child.waiter();

    thread::spawn(move || {
        // Wait for the thread to exit.
        match waiter.result() {
            x if x == 0 => completion(Ok(())),
            x if x > 0 => completion(Err(CommandError::ReturnCodeError(x))),
            x => {
                completion(Err(CommandError::ChildTerminationError(format!("Termination \
                                                                            returned error: {}",
                                                                           x))))
            }
        };
    });

    Ok(child)
}