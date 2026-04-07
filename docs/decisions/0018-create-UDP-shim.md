---
parent: Decisions
nav_order: 18
# These are optional metadata elements. Feel free to remove any of them.
status: "Proposed"
date: 2026-04-07
decision-makers:
    - abasu
consulted:
    - nacarino
informed:
    - nacarino
---
# Create UDP shim

## Context and Problem Statement

With the decision of utilizing a Shim layer for abstracting the RINA layer and existing network layers, it is necessary to pick a widely used networking technology to abstract via a shim. There a lot of existing network protocols which could be utilized to connect the RINA layer to existing network infrastructure. A decision is required as to which network infrastructure to initially develop and support.

It has been decided that we will limit considerations to protocols that can be easily accessed within operating system userland and that can be easily found on Microsoft Windows, mac OS and GNU/Linux operating systems.

## Considered Options

* UDP.
* TCP.
* QUIC.

## Decision Outcome

Chosen option: UDP, because it is easily accessed within all operating system's userland and does not introduce additional transmission, retransmission or flow control.

## Pros and Cons of the Options

### UDP

* Good, because the protocol is widely supported at kernel and userland levels.
* Good, because the protocol is message oriented.
* Bad, because UDP datagrams with unrecognized payloads are frequently filtered by existing network infrastructure.

### TCP

* Good, because the protocol is widely supported at kernel and userland levels.
* Good, because the protocol is widely supported on existing network infrastructure.
* Bad, because the protocol is byte-stream oriented.
* Bad, because the protocol introduces its own transmission, retransmission and flow control that would need to be mapped to RINA.

### QUIC

* Good, because the protocol is widely supported at userland level.
* Good, because the protocol can be used in a message or stream oriented configuration.
* Bad, because the protocol works on top of UDP.
* Bad, because the protocol integrates TLS 1.3.
* Bad, because it introduces its own transmission, retransmission and flow control that would need to be mapped to RINA.

## More Information

### Conclusion

We choose UDP as the TCP/IP network stack is ubiquitous and its simple implementation permits us to test RINA's transmission, retransmission and flow control without having to worry too much about what the underlying network infrastructure is doing. This approach simplifies the Shim and permits us to focus on RINA's functionalities.
