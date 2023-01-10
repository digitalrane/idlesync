# idlesync

`idlesync` is a rust-based utility daemon for monitoring IMAP mailboxes using IMAP IDLE.
When new mail is detected, or after a configurable timeout, a series of configurable commands are run.
The main reason to use idlesync is to make the process of watching for new mail and synchronising and indexing it when it arrives much more efficient,
as the available options for syncrhonising mail in the background are mostly either `offlineimap` (which is slow and resource intensive), or `mbsync`, (which does not have IDLE support and can only synchronise on a timer).
The intended use case is for synchronising and indexing email efficiently using an external tool such as `mbsync` and `mu`, but this is configurable for other tools.

## Background

I wrote this mostly as a way to teach myself more rust, especially concurrency and data structures, and
also because I'm currently using offlineimap, but really want to be using mbsync due to the much lower
resource usage.

## Design Goals

 * As async and non-blocking as possible
 * Low resource (and power) usage, I'm a low resource and low power kinda gal
 * Configurable for different mail clients, mail indexers and sync daemons

## Known Limitations

 * Currently only monitors INBOX for IDLE
 * Currently the actual sync command(s) will block
 * *NIX only for now, it works on OSX, but not Windows without a few small changes to handle spawning processes

## Quick Start

### Install
`cargo install --git https://gitlab.com/ec0/idlesync.git`

### Configure
Configuration should be written to `~/.config/idlesync/config.yaml`

Example:
```
---
accounts:
    - name: home1
      host: mail.mac.com
      user: tyrellw1
      pass: hack_hack_hack
      tls: true
      commands:
          - mbsync account1
          - notmuch new
    - name: work1
      host: mail.evilcorp.com
      user: tyrell
      pass: more_Of_a_kd3_pers0n
      tls: true
      commands:
          - mbsync work1
          - notmuch new
```

### Run
`idlesync`
