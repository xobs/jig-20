// From http://stackoverflow.com/a/36870954
extern crate wait_timeout;
extern crate shlex;

use std::io::Read;
use std::process::Child;
use std::process::Command;
use std::thread;
use std::time::Duration;
use self::wait_timeout::ChildExt;
use std::result;

pub fn try_command(cmd: &str, max: Duration) -> bool {
    let mut args = shlex::split(cmd);
    if args.is_none() {
        println!("No command specified");
        return false;
    }
    let mut args = args.unwrap();

    let mut cmd = Command::new(args.remove(0));
    cmd.args(&args);
    let mut child = cmd.spawn();
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
