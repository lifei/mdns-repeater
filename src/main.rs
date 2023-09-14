mod config;
mod server;

use std::net::{Ipv4Addr, SocketAddr, IpAddr};
use std::io::{Result};
use std::io::{Error, ErrorKind::Other};
use std::sync::{mpsc};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::{thread};
use std::thread::JoinHandle;
use std::time::Duration;
use if_addrs2::get_if_addrs;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use lazy_static::lazy_static;

static SERVER_DONE: AtomicBool = AtomicBool::new(false);

lazy_static! {
    static ref MDNS_ADDR: SocketAddr = "224.0.0.251:5353".parse::<SocketAddr>().unwrap();
    static ref BIND_ADDR: SocketAddr = "0.0.0.0:5353".parse::<SocketAddr>().unwrap();
    static ref DEFAULT_ADDR: SocketAddr = "224.0.0.0:5353".parse::<SocketAddr>().unwrap();
    static ref ADDR_LIST: Vec<(String, IpAddr)> = get_address_list().unwrap();
}

#[cfg(windows)]
fn bind_multicast(socket: &Socket, addr: SocketAddr) -> Result<()> {
    let addr = match addr {
        SocketAddr::V4(_addr) => *BIND_ADDR,
        SocketAddr::V6(_addr) => Err(Error::new(Other, "IPv6 is not supported"))?
    };
    socket.bind(&socket2::SockAddr::from(addr))
}

/// On Windows, unlike all Unix variants, it is improper to bind to the multicast address
///
/// see https://msdn.microsoft.com/en-us/library/windows/desktop/ms737550(v=vs.85).aspx
/// On unixes we bind to the multicast address, which causes multicast packets to be filtered
#[cfg(unix)]
fn bind_multicast(socket: &Socket, addr: SocketAddr) -> Result<()> {
    socket.bind(&socket2::SockAddr::from(addr))
}

// this will be common for all our sockets
fn new_socket(addr: SocketAddr) -> Result<Socket> {
    let socket = Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?;

    // we're going to use read timeouts so that we don't hang waiting for packets
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;

    Ok(socket)
}

fn get_address_list() -> Result<Vec<(String, IpAddr)>> {
    Ok(get_if_addrs()?
        .iter()
        .filter(|iface| !iface.is_loopback())
        .map(|iface| (iface.name.clone(), iface.ip()))
        .collect())
}

fn join_multicast(socket: &Socket, multiaddr: &Ipv4Addr) -> Result<()> {
    let addresses = get_address_list()?;
    if addresses.is_empty() {
        socket.join_multicast_v4(multiaddr, &Ipv4Addr::UNSPECIFIED)
    } else {
        for (_, address) in addresses {
            if let IpAddr::V4(ip) = address {
                socket.join_multicast_v4(multiaddr, &ip)?;
            }
        }
        Ok(())
    }
}

// receive mdns message
fn receiver(tx: Sender<(Box<[u8]>, SockAddr)>) -> Result<JoinHandle<()>> {
    let socket = new_socket(*MDNS_ADDR)?;
    match *MDNS_ADDR {
        SocketAddr::V4(addr) => join_multicast(&socket, &addr.ip())?,
        _ => Err(Error::new(Other, "IPv6 is not supported"))?,
    };
    socket.set_reuse_address(true)?;

    bind_multicast(&socket, *BIND_ADDR)?;
    let mut buf = [0u8; 1024];
    let handler: JoinHandle<_> = thread::spawn(move || {
        while !SERVER_DONE.load(std::sync::atomic::Ordering::Relaxed) {
            // we're assuming failures were timeouts, the client_done loop will stop us
            match socket.recv_from(&mut buf) {
                Ok((len, remote_addr)) => {
                    let data = buf[..len].to_vec().into_boxed_slice();
                    tx.send((data, remote_addr)).unwrap();
                }
                Err(err) => {
                    println!("server: got an error: {}", err);
                    SERVER_DONE.store(true, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }
        }
        SERVER_DONE.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    Ok(handler)
}

// send mdns message to other interface
fn announcer(rx: Receiver<(Box<[u8]>, SockAddr)>) -> Result<JoinHandle<()>> {
    let socket = new_socket(*DEFAULT_ADDR)?;
    match *MDNS_ADDR {
        SocketAddr::V4(addr) => join_multicast(&socket, &addr.ip())?,
        _ => Err(Error::new(Other, "IPv6 is not supported"))?,
    };
    socket.set_reuse_address(true)?;

    bind_multicast(&socket, *BIND_ADDR)?;

    let handler: JoinHandle<_> = thread::spawn(move || {
        while !SERVER_DONE.load(std::sync::atomic::Ordering::Relaxed) {
            // recv the receiver's data
            let (data, remote_addr) = match rx.recv() {
                Ok(data) => data,
                Err(_err) => {
                    println!("server error {_err}");
                    break;
                }
            };
            println!(
                "server: got data: {} from: {}",
                String::from_utf8_lossy(&*data),
                remote_addr.as_std().unwrap()
            );
            for (_, if_addrs) in &*ADDR_LIST {
                match socket.send_to(&*data, &SockAddr::from(SocketAddr::new(*if_addrs, 5353))) {
                    Err(_err) => {
                        println!("server error {_err}");
                        break;
                    }
                    _ => ()
                };
            }
        }
        SERVER_DONE.store(true, std::sync::atomic::Ordering::Relaxed)
    });
    Ok(handler)
}


fn main() -> Result<()> {
    println!("{:#?}", *config::ARGS);


    let mut threads = Vec::new();
    let (tx, rx) = mpsc::channel();
    match receiver(tx) {
        Ok(handler) => threads.push(handler),
        Err(err) => Err(Error::new(Other, err))?
    }
    match announcer(rx) {
        Ok(handler) => threads.push(handler),
        Err(_err) => {
            SERVER_DONE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // wait for all threads to exit
    for thread in threads {
        thread.join().unwrap();
    }
    Ok(())
}