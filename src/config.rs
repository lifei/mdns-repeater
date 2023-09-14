use clap::Parser;
use lazy_static::lazy_static;


/// mdns-repeater
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Name of the NIC interface list
    #[arg(short, long, required = true)]
    interface: Vec<String>,
}

lazy_static! {
    pub static ref ARGS: Args = Args::parse();
}


