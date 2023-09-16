use std::net::{Ipv4Addr, SocketAddr, IpAddr};
use std::io::{Result};
use std::io::{Error, ErrorKind::Other};
use std::sync::mpsc::{Receiver, Sender};
use std::{thread};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use if_addrs2::get_if_addrs;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use lazy_static::lazy_static;
use log::{error, info};
use crate::config;

lazy_static! {
    static ref MDNS_ADDR: SocketAddr = "224.0.0.251:5353".parse::<SocketAddr>().unwrap();
    static ref BIND_ADDR: SocketAddr = "0.0.0.0:5353".parse::<SocketAddr>().unwrap();
    static ref DEFAULT_ADDR: SocketAddr = "224.0.0.0:5353".parse::<SocketAddr>().unwrap();
    pub static ref ADDR_LIST: Vec<(String, IpAddr)> = get_address_list().unwrap();
}

#[cfg(windows)]
fn bind_multicast(socket: &Socket, addr: SocketAddr) -> Result<()> {
    let addr = match addr {
        SocketAddr::V4(_addr) => addr,
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
fn new_socket() -> Result<Socket> {
    let socket = Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?;
    Ok(socket)
}

pub(crate) fn get_all_interface_list() -> Result<Vec<String>> {
    Ok(get_if_addrs()?
        .iter()
        .filter(|iface| !iface.is_loopback())
        .map(|iface| iface.name.clone())
        .collect())
}

fn get_address_list() -> Result<Vec<(String, IpAddr)>> {
    Ok(get_if_addrs()?
        .iter()
        .filter(|iface| !iface.is_loopback())
        .filter(|iface| -> bool { config::ARGS.lock().unwrap().interface.contains(&iface.name) })
        .map(|iface| (iface.name.clone(), iface.ip()))
        .collect())
}

fn join_multicast(socket: &Socket, multiaddr: &Ipv4Addr) -> Result<()> {
    let addresses = ADDR_LIST.clone();
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
pub(crate) fn receiver(tx: Sender<(Box<[u8]>, SockAddr)>, server_done: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let socket = new_socket()?;
    match *MDNS_ADDR {
        SocketAddr::V4(addr) => join_multicast(&socket, &addr.ip())?,
        _ => Err(Error::new(Other, "IPv6 is not supported"))?,
    };
    socket.set_reuse_address(true)?;
    socket.set_multicast_loop_v4(true)?;

    bind_multicast(&socket, *BIND_ADDR)?;
    let mut buf = [0u8; 1024];
    let handler: JoinHandle<_> = thread::spawn(move || {
        while !server_done.load(std::sync::atomic::Ordering::Relaxed) {
            match socket.recv_from(&mut buf) {
                Ok((len, remote_addr)) => {
                    let data = buf[..len].to_vec().into_boxed_slice();
                    tx.send((data, remote_addr)).unwrap_or_else(|err| {
                        error!("send msg to chan failed: {err}")
                    });
                }
                Err(err) => {
                    error!("recv msg error: {err}");
                    server_done.store(true, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }
        }
        server_done.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    Ok(handler)
}

// send mdns message to other interface
pub(crate) fn announcer(rx: Receiver<(Box<[u8]>, SockAddr)>, server_done: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let socket = new_socket()?;
    match *MDNS_ADDR {
        SocketAddr::V4(addr) => join_multicast(&socket, &addr.ip())?,
        _ => Err(Error::new(Other, "IPv6 is not supported"))?,
    };
    socket.set_reuse_address(true)?;
    socket.set_multicast_loop_v4(true)?;

    bind_multicast(&socket, *BIND_ADDR)?;

    let handler: JoinHandle<_> = thread::spawn(move || {
        'l: while !server_done.load(std::sync::atomic::Ordering::Relaxed) {
            // recv the receiver's data
            let (data, remote_addr) = match rx.recv() {
                Ok(data) => data,
                Err(_err) => {
                    error!("recv msg from chan failed: {_err}");
                    break;
                }
            };
            info!(
                "[server]: got data: {} from: {}",
                String::from_utf8_lossy(&*data),
                remote_addr.as_std().unwrap()
            );
            for (_, addr) in &*ADDR_LIST {
                if remote_addr.as_inet().unwrap().ip().to_string() == addr.to_string() {
                    continue 'l;
                }
            }
            for (_if_name, if_addrs) in &*ADDR_LIST {
                match remote_addr.as_inet() {
                    Some(addr) => {
                        if addr.ip().eq(if_addrs) {
                            continue;
                        }
                    }
                    None => {
                        error!("remote_addr as_inet failed: {:#?}", remote_addr);
                        break;
                    }
                }
                match if_addrs {
                    IpAddr::V4(_addr) => {
                        socket.set_multicast_if_v4(_addr).unwrap();
                    }
                    IpAddr::V6(_) => {
                        error!("server error");
                        break;
                    }
                }
                match socket.send_to(&*data, &SockAddr::from(*MDNS_ADDR)) {
                    Err(_err) => {
                        error!("server error {_err}");
                        break;
                    }
                    _ => ()
                };
            }
        }
        server_done.store(true, std::sync::atomic::Ordering::Relaxed)
    });
    Ok(handler)
}

