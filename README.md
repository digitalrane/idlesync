idlesync
========

idlesync is a rust async utility daemon for monitoring IMAP mailboxes using IMAP IDLE.
When new mail is detected, or after a configurable IDLE timeout, a series of configurable commands are run.
The intended use case is for synchronising email using an external tool such as mbsync/isync, which lacks
IDLE or polling support itself.

Background
==========

I wrote this mostly as a way to teach myself more rust, especially concurrency and data structures, and
also because I'm currently using offlineimap, but really want to be using mbsync due to the much lower
resource usage.

Design Goals
============

 * As async as possible
 * Low resource (and power) usage
 * Configurabile

Known Limitations
=================

 * Currently only monitors INBOX for IDLE
 * Currently the sync command(s) will block, until async-std has a `std::process::Command` implementation
 * *NIX only for now, in theory should work on OSX, but not Windows without a few small changes

Quick Start
===========

== Install ==
`cargo install --git https://github.com/devec0/idlesync.git`

== Configure ==
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

== Run ==
`idlesync`
