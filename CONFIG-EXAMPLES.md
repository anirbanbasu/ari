# RINA Configuration Examples

This document provides comprehensive examples of how to configure and run ARI IPCPs in different modes.

## Quick Start

### Demo Mode (No Configuration Needed)
```bash
cargo run
```

Runs the original demonstration with hardcoded values. No networking, just showcases all components.

---

## Bootstrap IPCP

The bootstrap IPCP is the first IPCP in a DIF. It has a static address and manages address allocation for joining members.

### Using Configuration File

**File: config/bootstrap.toml**
```toml
[ipcp]
name = "ipcp-bootstrap"
type = "normal"
mode = "bootstrap"

[dif]
name = "production-dif"
address = 1001
address_pool_start = 1002
address_pool_end = 1999

[shim]
bind_address = "0.0.0.0"
bind_port = 7000

[enrolment]
bootstrap_peers = []
```

**Run:**
```bash
cargo run -- --config config/bootstrap.toml
```

### Using Command-Line Arguments

```bash
cargo run -- \
  --mode bootstrap \
  --name ipcp-bootstrap \
  --dif-name production-dif \
  --address 1001 \
  --bind 0.0.0.0:7000
```

**With custom address pool:**
```bash
cargo run -- \
  --mode bootstrap \
  --name ipcp-bootstrap \
  --dif-name production-dif \
  --address 1001 \
  --bind 0.0.0.0:7000 \
  --address-pool-start 2000 \
  --address-pool-end 2999
```

---

## Member IPCP

Member IPCPs enroll with a bootstrap IPCP to join a DIF. They receive their address dynamically during enrolment.

### Using Configuration File

**File: config/member.toml**
```toml
[ipcp]
name = "ipcp-member-1"
type = "normal"
mode = "member"

[dif]
name = "production-dif"
# Address omitted - will be assigned during enrolment

[shim]
bind_address = "0.0.0.0"
bind_port = 7001

[enrolment]
bootstrap_peers = [
    { address = "127.0.0.1:7000", rina_addr = 1001 }
]
```

**Run:**
```bash
cargo run -- --config config/member.toml
```

### Using Command-Line Arguments

```bash
cargo run -- \
  --mode member \
  --name ipcp-member-1 \
  --dif-name production-dif \
  --bind 0.0.0.0:7001 \
  --bootstrap-peers 127.0.0.1:7000
```

**Multiple bootstrap peers:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-1 \
  --dif-name production-dif \
  --bind 0.0.0.0:7001 \
  --bootstrap-peers 127.0.0.1:7000,192.168.1.10:7000
```

---

## Multi-IPCP Scenarios

### Scenario 1: Local Development (Single Machine)

Run these in separate terminals:

**Terminal 1 - Bootstrap:**
```bash
cargo run -- --config config/bootstrap.toml
```

**Terminal 2 - Member 1:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-1 \
  --dif-name production-dif \
  --bind 0.0.0.0:7001 \
  --bootstrap-peers 127.0.0.1:7000
```

**Terminal 3 - Member 2:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-2 \
  --dif-name production-dif \
  --bind 0.0.0.0:7002 \
  --bootstrap-peers 127.0.0.1:7000
```

### Scenario 2: Distributed Deployment (Multiple Machines)

**Machine 1 (192.168.1.10) - Bootstrap:**
```bash
cargo run -- \
  --mode bootstrap \
  --name ipcp-bootstrap \
  --dif-name production-dif \
  --address 1001 \
  --bind 0.0.0.0:7000
```

**Machine 2 (192.168.1.20) - Member:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-1 \
  --dif-name production-dif \
  --bind 0.0.0.0:7000 \
  --bootstrap-peers 192.168.1.10:7000
```

**Machine 3 (192.168.1.30) - Member:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-2 \
  --dif-name production-dif \
  --bind 0.0.0.0:7000 \
  --bootstrap-peers 192.168.1.10:7000
```

### Scenario 3: Docker Compose

**docker-compose.yml:**
```yaml
version: '3.8'

services:
  bootstrap:
    build: .
    command: >
      /app/ari
      --mode bootstrap
      --name ipcp-bootstrap
      --dif-name docker-dif
      --address 1001
      --bind 0.0.0.0:7000
    ports:
      - "7000:7000/udp"
    networks:
      rina_net:
        ipv4_address: 172.20.0.10

  member1:
    build: .
    command: >
      /app/ari
      --mode member
      --name ipcp-member-1
      --dif-name docker-dif
      --bind 0.0.0.0:7000
      --bootstrap-peers 172.20.0.10:7000
    ports:
      - "7001:7000/udp"
    networks:
      rina_net:
        ipv4_address: 172.20.0.11
    depends_on:
      - bootstrap

  member2:
    build: .
    command: >
      /app/ari
      --mode member
      --name ipcp-member-2
      --dif-name docker-dif
      --bind 0.0.0.0:7000
      --bootstrap-peers 172.20.0.10:7000
    ports:
      - "7002:7000/udp"
    networks:
      rina_net:
        ipv4_address: 172.20.0.12
    depends_on:
      - bootstrap

networks:
  rina_net:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/16
```

---

## Configuration Parameter Reference

### Required Parameters by Mode

#### Bootstrap Mode
- `--name` or `ipcp.name`: IPCP name
- `--mode bootstrap` or `ipcp.mode = "bootstrap"`
- `--dif-name` or `dif.name`: DIF name
- `--address` or `dif.address`: RINA address for this IPCP
- `--bind` or `shim.bind_address`+`shim.bind_port`: UDP socket address

#### Member Mode
- `--name` or `ipcp.name`: IPCP name
- `--mode member` or `ipcp.mode = "member"`
- `--dif-name` or `dif.name`: DIF name
- `--bind` or `shim.bind_address`+`shim.bind_port`: UDP socket address
- `--bootstrap-peers` or `enrolment.bootstrap_peers`: Bootstrap IPCP addresses

#### Demo Mode
- No parameters required
- Use `--mode demo` or just run without arguments

### Optional Parameters

- `--address-pool-start` (default: 1002): Start of address pool for bootstrap
- `--address-pool-end` (default: 1999): End of address pool for bootstrap

### Configuration File vs Command Line

Command-line arguments take precedence over config file values. If `--config` is specified, all other arguments are ignored unless explicitly overridden.

---

## Troubleshooting

### "Configuration error: --name is required"
Make sure you provide all required parameters for the mode you're using.

### "Bind error: Address already in use"
Another process is using that port. Either:
- Kill the other process
- Use a different port number
- Check if another IPCP is already running

### "Configuration validation error: Bootstrap mode requires an address"
Bootstrap IPCPs need a static RINA address. Add `--address 1001` (or another value).

### "Member mode requires at least one bootstrap peer"
Member IPCPs need to know where to enroll. Add `--bootstrap-peers 127.0.0.1:7000`.

---

## Environment Variables (Future Enhancement)

The current implementation uses CLI and TOML configs. Environment variable support could be added:

```bash
export IPCP_NAME=ipcp-1
export IPCP_MODE=bootstrap
export DIF_NAME=prod-dif
export DIF_ADDRESS=1001
export SHIM_BIND=0.0.0.0:7000

cargo run
```

---

## Best Practices

1. **Use config files for production** - Easier to version control and maintain
2. **Use CLI args for testing** - Faster iteration during development
3. **Document your DIF topology** - Keep track of which IPCPs have which addresses
4. **Reserve address ranges** - Use different ranges for different purposes
5. **Use meaningful names** - Name IPCPs after their function or location
6. **Separate shim ports** - Each IPCP on the same machine needs a unique port

---

## Example Topologies

### Simple: 1 Bootstrap + 2 Members
```
┌─────────────────┐
│ Bootstrap       │
│ addr: 1001      │
│ port: 7000      │
└────────┬────────┘
         │
    ┌────┴─────┐
    ↓          ↓
┌────────┐ ┌────────┐
│Member-1│ │Member-2│
│addr:   │ │addr:   │
│1002    │ │1003    │
│port:   │ │port:   │
│7001    │ │7002    │
└────────┘ └────────┘
```

### Hierarchical: 1 Bootstrap + 2 Regions
```
        ┌─────────────────┐
        │ Central         │
        │ Bootstrap       │
        │ addr: 1001      │
        └────────┬────────┘
                 │
        ┌────────┴────────┐
        ↓                 ↓
┌────────────┐    ┌────────────┐
│ Region-A   │    │ Region-B   │
│ addr: 1002 │    │ addr: 1003 │
└──────┬─────┘    └──────┬─────┘
       │                 │
   ┌───┴───┐         ┌───┴───┐
   ↓       ↓         ↓       ↓
┌─────┐ ┌─────┐   ┌─────┐ ┌─────┐
│1004 │ │1005 │   │1006 │ │1007 │
└─────┘ └─────┘   └─────┘ └─────┘
```

---

For more information, see [RUNNING.md](RUNNING.md) for operational details.
