// From http://stackoverflow.com/a/36870954
extern crate wait_timeout;
extern crate shlex;

use std::process::Command;
use std::time::Duration;
use std::thread;
use std::process::{Stdio, ChildStdin, ChildStdout};
use self::wait_timeout::ChildExt;
use super::testset::TestSet;

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

pub fn try_command(ts: &TestSet, cmd: &str, wd: &Option<String>, max: Duration) -> bool {
    let mut cmd = match make_command(cmd) {
        Err(_) => {
            ts.debug("internal", "unknown", "Unable to make command");
            return false;
        },
        Ok(val) => val,
    };

    match *wd {
        None => (),
        Some(ref s) => {cmd.current_dir(s); },
    };

    let child = cmd.spawn();
    if child.is_err() {
        let err = child.err().unwrap();
        println!("Unable to spawn child: {}", err);
        return false;
    }
    let mut child = child.unwrap();

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

pub fn try_command_completion<F>(cmd: &str, wd: &Option<String>, max: Duration, completion: F)
        -> Result<(ChildStdout, ChildStdin), CommandError>
        where F: Send + 'static + FnOnce(Result<(), CommandError>)
{

    let mut cmd = match make_command(cmd) {
        Err(e) => {
            completion(Err(CommandError::MakeCommandError(format!("{:?}", e).to_string())));
            return Err(CommandError::MakeCommandError(format!("{:?}", e).to_string()));
        },
        Ok(val) => val,
    };

    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    match *wd {
        None => (),
        Some(ref s) => {cmd.current_dir(s); },
    };

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
                    Ok(res) => {
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
    Ok((stdout, stdin))
 }