use clap::Parser;
use lazy_static::lazy_static;
use std::sync::Mutex;

/// mdns-repeater
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, action = append)]
#[command(name = "mdns-repeater")]
#[command(bin_name = "mdns-repeater")]
pub struct Args {
    /// Name of the NIC interface list
    #[arg(last = true)]
    pub interface: Vec<String>,

    /// Show the list of NIC interface
    #[arg(short, long = "list-interfaces", default_value_t = false)]
    pub list: bool,
}

lazy_static! {
    pub static ref ARGS: Mutex<Args> = Mutex::new(Args::parse());
}


