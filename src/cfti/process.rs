// From http://stackoverflow.com/a/36870954
extern crate wait_timeout;
extern crate shlex;

use std::process::Command;
use std::time::Duration;
use self::wait_timeout::ChildExt;
use super::testset::TestSet;

#[derive(Debug)]
pub enum CommandError {
    NoCommandSpecified,
    MakeCommandError(String),
    SpawnError(String),
    GetResultError(String),
    WaitResultError(String),
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

pub fn try_command_completion<F>(cmd: &str, wd: &Option<String>, max: Duration, completion: F)
        where F: Send + 'static + Fn(Result<bool, CommandError>) {

    let mut cmd = match make_command(cmd) {
        Err(e) => {
            completion(Err(CommandError::MakeCommandError(format!("{:?}", e).to_string())));
            return;
        },
        Ok(val) => val,
    };

    match *wd {
        None => (),
        Some(ref s) => {cmd.current_dir(s); },
    };

    let mut child = match cmd.spawn() {
        Err(err) => {
            completion(Err(CommandError::SpawnError(format!("{}", err).to_string())));
            return;
        },
        Ok(s) => s,
    };

    let status_code = match child.wait_timeout(max).unwrap() {
        Some(status) => status.code().unwrap(),
        None => {
            // child hasn't exited yet
            if let Err(err) = child.kill() {
                completion(Err(CommandError::GetResultError(format!("{}", err).to_string())));
                return;
            }

            // Call wait() on child, which should return immediately
            match child.wait() {
                Err(err) => {
                    completion(Err(CommandError::WaitResultError(format!("{}", err).to_string())));
                    return;
                },
                Ok(res) => res.code().unwrap()
            }
        }
    };

    // If it's a nonzero exit code, that counts as an error.
    if status_code != 0 {
        completion(Err(CommandError::ReturnCodeError(status_code)));
        return
    }
    completion(Ok(true));
 }