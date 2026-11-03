# Security

Kingdom Hearts: Connected (KHC) is an unofficial fan project run by one person. There is no bug bounty.

## Reporting a problem

Please do not open a public issue for a security problem. Use GitHub's private reporting instead:
**Security → Report a vulnerability** on this repository. Say what an attacker can do, what they need
(a code, being on the same network, running the relay) and how to reproduce it. You should hear back
within two weeks.

In scope: the network code (`source/net`), the relay (`source/relay`), room codes, reply codes, LAN
discovery, router port mapping (UPnP / NAT-PMP), and anything that lets another player or a stranger
crash, read files on, or run code on a player's PC.

Out of scope: cheating by the host in your own match (see below), and anything that needs the attacker
to already control your PC or your game install.

## What to know before you play online

- **A room code is your address.** A twelve-character direct code (`ABCD-EFGH-JKMN`) holds your public IP
  address and port; the mixing only catches typos and is not secret. Give it only to the person you
  want to play, and do not show it on a stream. The same goes for a joiner's reply code.
- **Hosting opens a port.** With a direct code the game asks your router (UPnP or NAT-PMP) to open its
  UDP port (47620 unless you choose another) for 30 minutes at a time, and removes it when the session
  ends. If the game is closed by force the mapping stays until its lease runs out.
- **The host runs the match.** The joiner only sends buttons; the host simulates both players and sends
  the result. A modified host can do anything to the match it hosts. Play people you trust.
- **Nothing is encrypted.** Game traffic, names and player ids travel in the clear, directly between the
  two players or through a relay if one is used. Anyone on the path can read them.
- **Player ids are not verified.** The id a player sends is not checked against the secret behind it,
  so do not treat an id as proof of who someone is.
- **Matching builds is not anti-cheat.** The build hash both sides compare stops honest version
  mismatches; a modified client can send any hash.
- **Your game files stay yours.** KHC reads your own installs and never sends game data over the network;
  effects and sounds are named by file and each side plays them from its own copy.

## Running a relay

`khc_relay` is optional; direct codes and the LAN work without one. If you run a public relay, keep it
on a machine with nothing else on it, keep the per-source limits in `source/relay/src/lib.rs`, and know
that it can see (but does not log) the addresses of the players who use it.
