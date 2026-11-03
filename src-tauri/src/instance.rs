//! A new process and the running DataBee: handing a launch over, taking the instance lock, and asking
//! the running app on behalf of a subcommand.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use crate::fail;
use crate::ipc::{self, Forward, IpcMessage};

const LOCK_NAME: &str = "databee.lock";
const LOCK_POLL: Duration = Duration::from_millis(200);
const LOCK_POLLS: u32 = 25;

/// Exits when a running instance takes the message; otherwise returns the instance lock, held for the
/// app's lifetime so two launches that both find no listener can't both start. `None` when the lock
/// cannot be taken at all.
pub fn forward_or_lock(
    config_dir: &Path,
    socket: &Path,
    message: &IpcMessage,
    exit_code: i32,
) -> Option<File> {
    forward(socket, message, exit_code);
    match lock_instance(config_dir) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => forward_to_starting_instance(socket, message, exit_code),
        Err(e) => {
            eprintln!("Cannot lock the DataBee instance: {e}");
            None
        }
    }
}

/// `None` when another process holds the lock.
fn lock_instance(dir: &Path) -> std::io::Result<Option<File>> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_NAME))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Exits when a running instance answers or refuses the message; returns when none is listening.
pub fn forward(socket: &Path, message: &IpcMessage, exit_code: i32) {
    match ipc::try_forward(socket, message) {
        Ok(Forward::Delivered) => std::process::exit(exit_code),
        Ok(Forward::NoInstance) => {}
        Ok(Forward::Rejected) => fail("The running DataBee instance did not accept the request"),
        Err(e) => fail(&format!(
            "Could not reach the running DataBee instance: {e}"
        )),
    }
}

/// The lock holder may still be starting up and not listen yet.
fn forward_to_starting_instance(socket: &Path, message: &IpcMessage, exit_code: i32) -> ! {
    for _ in 0..LOCK_POLLS {
        std::thread::sleep(LOCK_POLL);
        forward(socket, message, exit_code);
    }
    fail("Another DataBee instance is starting but does not answer")
}

/// Exits with the running instance's answer; returns when none is listening.
pub fn ask(socket: &Path, message: &IpcMessage) {
    match ipc::request(socket, message.clone()) {
        Ok(None) => {}
        Ok(Some(reply)) => match reply.into_result() {
            Ok(result) => {
                if !result.is_null() {
                    println!("{result}");
                }
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        Err(e) => fail(&format!(
            "Could not reach the running DataBee instance: {e}"
        )),
    }
}

/// `databee status`: prints the running app's answer, or that none runs; always exits.
pub fn status(socket: &Path, json: bool) -> ! {
    let reply = match ipc::request(socket, IpcMessage::Status) {
        Ok(None) => serde_json::json!({ "running": false }),
        Ok(Some(reply)) => reply
            .into_result()
            .unwrap_or_else(|error| fail(&error.to_string())),
        Err(e) => fail(&format!(
            "Could not reach the running DataBee instance: {e}"
        )),
    };
    if json {
        println!("{reply}");
    } else if reply["running"] != true {
        println!("DataBee is not running");
    } else if reply["windowVisible"] == true {
        println!("DataBee is running; its window shows");
    } else {
        println!("DataBee is running; its window is hidden");
    }
    std::process::exit(0);
}
