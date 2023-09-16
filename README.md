# mdns-repeater

Refactored in rust, and brings cross-platform features

## Usage
``` bash
$ mdns-repeater -h
mdns-repeater

Usage: mdns-repeater [OPTIONS] [-- <INTERFACE>...]

Arguments:
  [INTERFACE]...  Name of the NIC interface list

Options:
  -l, --list-interfaces  Show the list of NIC interface
  -h, --help             Print help
  -V, --version          Print version
  
$ mdns-repeater -l
[2023-09-16T07:18:47Z INFO  mdns_repeater] The list of NIC interface:
    [
        "vEthernet (k8s cluster)",
        "WLAN",
        "vEthernet (Default Switch)",
        "vEthernet (WSL)",
    ]

$ mdns-repeater -- "WLAN" "vEthernet (WSL)"
[2023-09-16T07:19:11Z INFO  mdns_repeater] The following mdns information for the network card is repeated:
    "WLAN": 192.168.10.19
    "vEthernet (WSL)": 172.31.144.1

```