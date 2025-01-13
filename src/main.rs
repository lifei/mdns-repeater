mod config;
mod server;

use log::{error, info};
use std::env;
use std::io::Result;
use std::io::{Error, ErrorKind::Other};
use std::process::exit;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

fn main() -> Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    if config::ARGS.lock().unwrap().list {
        if let Ok(ifaces) = server::get_all_interface_list() {
            info!("The list of NIC interface:\n{:#?}", ifaces);
        }
        return Ok(());
    }
    if config::ARGS.lock().unwrap().interface.is_empty() {
        error!("No interface specified");
        exit(1);
    }

    let list = server::ADDR_LIST
        .iter()
        .map(|(n, a)| format!("\"{}\": {}", n, a))
        .collect::<Vec<_>>()
        .join("\n");
    info!(
        "The following mdns information for the network card is repeated:\n{}",
        list
    );
    let server_done = Arc::new(AtomicBool::new(false));

    let mut threads = Vec::new();
    let (tx, rx) = mpsc::channel();
    match server::receiver(tx, Arc::clone(&server_done)) {
        Ok(handler) => threads.push(handler),
        Err(err) => Err(Error::new(Other, err))?,
    }
    match server::announcer(rx, Arc::clone(&server_done)) {
        Ok(handler) => threads.push(handler),
        Err(_err) => {
            server_done.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // wait for all threads to exit
    for thread in threads {
        thread.join().unwrap();
    }
    Ok(())
}

