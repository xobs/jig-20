// From http://stackoverflow.com/a/36870954
extern crate wait_timeout;
extern crate shlex;

use std::io::{self, BufRead};
use std::process::Command;
use std::time::Duration;
use std::thread;
use std::process::{Stdio, ChildStdin, ChildStdout, ChildStderr};
use self::wait_timeout::ChildExt;
use cfti::controller::{Controller, ControlMessageContents};

#[derive(Debug)]
pub enum CommandError {
    NoCommandSpecified,
    MakeCommandError(String),
    SpawnError(String),
    ChildTimeoutTerminateError(String),
    ChildTimeoutWaitError(String),
    ChildTimeout,
    ReturnCodeError(i32),
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

    match *wd {
        None => (),
        Some(ref s) => {cmd.current_dir(s); },
    };

    let mut child = match cmd.spawn() {
        Err(e) => {
            controller.debug("process", "process", format!("Unable to spawn child {:?}: {:?}", cmd, e));
            return false;
        },
        Ok(o) => o,
    };

    let status_code = match child.wait_timeout(max).unwrap() {
        Some(status) => status.code(),
        None => {
            // child hasn't exited yet
            let res = child.kill();
            if res.is_err() {
                let err = res.err().unwrap();
                println!("Unable to get result: {}", err);
                return false;
            }

            let res = child.wait();
            if res.is_err() {
                let err = res.err().unwrap();
                println!("Unable to wait for result: {}", err);
                return false;
            }
            res.unwrap().code()
        }
    };
    return status_code.unwrap() == 0
}

pub fn log_output<T: io::Read + Send + 'static>(stream: T, controller: &Controller, id: &str, kind: &str, stream_name: &str) {

    let thr_controller = controller.clone();
    let thr_id = id.to_string();
    let thr_kind = kind.to_string();
    let thr_stream_name = stream_name.to_string();

    watch_output(stream, controller, id, kind, stream_name, move |msg| {
        thr_controller.control_class(thr_stream_name.as_str(),
                                     thr_id.as_str(),
                                     thr_kind.as_str(),
                                     &ControlMessageContents::Log(msg));
        Ok(())
    });
}

pub fn watch_output<T: io::Read + Send + 'static, F>(stream: T, controller: &Controller,
                                                     id: &str, kind: &str, stream_name: &str,
                                                     mut msg_func: F)
        where F: Send + 'static + FnMut(String) -> Result<(), ()> {
    // Monitor the child process' stderr, and pass values to the controller.
    let controller = controller.clone();
    let id = id.to_string();
    let kind = kind.to_string();
    let stream_name = stream_name.to_string();
    let builder = thread::Builder::new()
        .name(format!("I-E {} -> CFTI", id).into());

    builder.spawn(move || {
        for line in io::BufReader::new(stream).lines() {
            match line {
                Err(e) => {
                    controller.debug(id.as_str(), kind.as_str(),format!("Error in interface: {}", e));
                    return;
                },
                Ok(l) => if let Err(e) = msg_func(l) {
                    controller.debug(id.as_str(), kind.as_str(), format!("Message func returned error: {:?}", e));
                    return;
                }
            }
        }
    }).unwrap();
}

pub fn spawn(mut cmd: Command, id: &str, kind: &str, controller: &Controller)
        -> Result<(Option<ChildStdin>, Option<ChildStdout>, Option<ChildStderr>), io::Error> {
    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let controller = controller.clone();
    let mut child = cmd.spawn();
    let id = id.to_string();
    let kind = kind.to_string();

    match child {
        Err(e) => return Err(e),
        Ok(mut child) => {
            let stdin = child.stdin;
            child.stdin = None;
            let stdout = child.stdout;
            child.stdout = None;
            let stderr = child.stderr;
            child.stderr = None;

            thread::spawn(move || {
                match child.wait() {
                    Ok(status) => controller.debug(id.as_str(), kind.as_str(), format!("Child exited successfully with result: {:?}", status)),
                    Err(e) => controller.debug(id.as_str(), kind.as_str(), format!("Child errored with exit: {:?}", e)),
                };
            });
            Ok((stdin, stdout, stderr))
        },
    }
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
        -> Result<(ChildStdin, ChildStdout, ChildStderr), CommandError>
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
            completion(Err(CommandError::SpawnError(format!("{}", err).to_string())));
            return Err(CommandError::SpawnError(format!("{}", err).to_string()));
        },
        Ok(s) => s,
    };

    // Take the child's stdio handles and replace them with None.  This allows
    // us to have the thread take ownership of `child` and return the handles.
    let stdout = child.stdout.unwrap();
    child.stdout = None;
    let stdin = child.stdin.unwrap();
    child.stdin = None;
    let stderr = child.stderr.unwrap();
    child.stderr = None;

    thread::spawn(move || {
        let status_code = match child.wait_timeout(max).unwrap() {
            Some(status) => status.code().unwrap(),
            None => {
                // child hasn't exited yet, so terminate it.
                if let Err(err) = child.kill() {
                    completion(Err(CommandError::ChildTimeoutTerminateError(format!("{}", err).to_string())));
                    return;
                }

                // Call wait() on child, which should return immediately
                match child.wait() {
                    Err(err) => {
                        completion(Err(CommandError::ChildTimeoutWaitError(format!("{}", err).to_string())));
                        return;
                    },
                    Ok(_) => {
                        completion(Err(CommandError::ChildTimeout));
                        return;
                    }
                }
            }
        };

        // If it's a nonzero exit code, that counts as an error.
        if status_code != 0 {
            completion(Err(CommandError::ReturnCodeError(status_code)));
            return;
        }
        completion(Ok(()));
    });

    // Return the stdout so that the 
    Ok((stdin, stdout, stderr))
 }