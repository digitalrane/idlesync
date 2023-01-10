#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
extern crate xdg;
extern crate serde_yaml;

mod conf;
mod errors;

use async_imap::extensions::idle::IdleResponse::*;
use async_native_tls::TlsConnector;
use clap::{Command as ClapCommand, Arg, crate_version};
use fern::colors::{Color, ColoredLevelConfig};
use futures::stream::FuturesUnordered;
use futures_util::stream::StreamExt;
use std::process::Command;
use tokio::{task, time::{sleep, Duration}};

use crate::conf::Conf;

pub fn setup_logger(level: &str) -> Result<(), fern::InitError> {

    let _fern = fern::Dispatch::new()
	.format(|out, message, record| {
	    let colors = ColoredLevelConfig::new()
		.info(Color::Green)
		.warn(Color::Yellow)
		.error(Color::Red)
		.debug(Color::White);

            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
		colors.color(record.level()),
                record.target(),
                message
            ))
	})
	.level(match level {
	    "debug" => log::LevelFilter::Debug,
	    "info" => log::LevelFilter::Info,
	    "warn" => log::LevelFilter::Warn,
	    "error" => log::LevelFilter::Error,
	    &_ => log::LevelFilter::Info,
	})
	.chain(std::io::stdout())
	.apply()?;

    Ok(())
}

#[tokio::main]
async fn main() {

    //Set up our app
    let args = ClapCommand::new("idlesync")
	.version(crate_version!())
	.about("Syncronise your mail using IMAP, with IDLE support")
	.arg(Arg::new("config")
		.help("Path to config file")
		.value_name("CONFIG")
		.short('c')
	     .long("config"))
	.arg(Arg::new("debug")
		.help("Enable debug logging")
		.short('D')
                .value_parser(clap::value_parser!(bool))
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("true"))
	.get_matches();

    let path: String = match args.get_one::<String>("config") {
	Some(path) => path.to_string(),
	None => {
            let xdg_dirs = match xdg::BaseDirectories::with_prefix("idlesync") {
		Ok(xdg_dirs) => xdg_dirs,
		Err(e) => {
		    panic!("failed to find config file: {:?}", e);
		}
	    };
            let config_path = match xdg_dirs.find_config_file("config.yaml") {
		Some(config_path) => {
		    config_path
		},
		None => {
		    panic!("configuration file missing!");
		}
	    };
	    config_path.to_string_lossy().to_string()
	}
    };
    info!("using configuration file: {:?}", path);
    let mut config = Conf::new(Some(&path)).unwrap();

    if args.contains_id("debug") {
	config.log.level = "debug".to_string()
    }

    setup_logger(&config.log.level).unwrap();

    info!("starting idlesync");

    // generate async futures for each account and schedule them
    let workers = FuturesUnordered::new();
    for account in config.accounts {
	let cloned_account = account.clone();
	let cloned_idle_timeout = config.idle_timeout.clone();
	let cloned_retry = config.retry.clone();
	workers.push(task::spawn( async move { monitor_account(cloned_account, cloned_idle_timeout, cloned_retry).await}));
    }

    let _results: Vec<_> = workers.collect().await;
}

async fn run_command(command: String) -> Result<(), Box<dyn std::error::Error>> {
    // just *nix for now, would be easy to extend
    let command_result = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .expect("failed to execute process");
    if !command_result.status.success() {
        error!("Command {} failed, stderr: {}", command, String::from_utf8_lossy(&command_result.stderr));
    }
    Ok(())
}

async fn run_handlers(name: String, commands: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    for command in commands {
        info!("Running command {}", command);
        let run_result = run_command(command.clone()).await;
        match run_result {
            Ok(()) => info!("Command {} ran successfully", command),
            Err(e) => {
		error!("{}: error running {}: {}", name, command, e);
		return Err(e)
	    }
        }
    }
    Ok(())
}

async fn monitor_account(account: conf::Account, idle_timeout: u64, retry: u64)
{

    loop {
	// we dereference (*) the host to get a str for the ToSocketAddrs impl
	let imap_host = account.host.clone();
	let imap_port: u64 = match account.port {
	    Some(port) => port.into(),
	    None => match account.tls {
		true => 993,
		false => 143,
	    }
	};
	let name = match account.name.clone() {
	    Some(name) => name,
	    None => imap_host.clone()
	};
	let user = account.user.clone();
	let password = account.pass.clone();
	info!("{}: Monitoring account", name);

	let tls = TlsConnector::new();
	// we pass in the imap_server twice to check that the server's TLS
	// certificate is valid for the imap_server we're connecting to.
	let client = match async_imap::connect(format!("{}:{}", imap_host, imap_port), imap_host.clone(), tls).await {
	    Ok(client) => {
		info!("{}: connected to {}:{}", name, imap_host, imap_port);
		client
	    },
	    Err(e) => {
		error!("{}: failed to connect to {} {}: {:?}", name, imap_host, imap_port, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    }
	};

	// the client we have here is unauthenticated.
	// to do anything useful with the e-mails, we need to log in
	let mut session = match client.login(user.clone(), password.clone()).await.map_err(|e| e.0) {
	    Ok(session) => {
		info!("{}: logged in to {} {}", name, imap_host, imap_port);
		session
	    },
	    Err(e) => {
		error!("failed to log in to {} at {} {}: {:?}", name, imap_host, imap_port, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    }
	};

	// we want to fetch some messages from the INBOX
	match session.select("INBOX").await {
	    Ok(_) => debug!("{}: selected INBOX", name),
	    Err(e) => {
		error!("{}: failed to select INBOX: {:?}", name, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    }
	}

	// fetch flags from all messages
	let msg_stream = match session.fetch("1:*", "(FLAGS )").await {
	    Ok(msg_stream) => {
		debug!("{}: fetched message flags", name);
		msg_stream
	    },
	    Err(e) => {
		error!("{}: failed to fetch message flags: {:?}", name, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    }
	};
	let msgs = msg_stream.collect::<Vec<_>>().await;
	debug!("{}: Number of fetched msgs: {:?}", name, msgs.len());

	// init idle session
	debug!("{}: Watching folders via IDLE", name);
	let mut idle = session.idle();
	match idle.init().await {
	    Ok(_) => debug!("{}: initialised IDLE", name),
	    Err(e) => error!("{}: failed to initialise IDLE: {:?}", name, e)
	}

	info!("{}: waiting for new mail or timeout of {}s", name, idle_timeout);
	let (idle_wait, _interrupt) = idle.wait_with_timeout(Duration::from_secs(idle_timeout));
	let _idle_result = match idle_wait.await {
            Ok(ManualInterrupt) => {
		info!("{}: IDLE manually interrupted, will re-establish", name);
            },
            Ok(Timeout) => {
		info!("{}: IDLE timed out, will re-establish", name);
            },
            Ok(NewData(data)) => {
		let s = String::from_utf8(data.borrow_raw().to_vec()).unwrap();
		info!("{}: IDLE woke up with new data", name);
		debug!("{}: IDLE new data received:\n {}", name, s);
            },
	    Err(e) => {
		error!("{}: failed to wait for IDLE result, will retry connection: {:?}", name, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    }
	};
	info!("{}: IMAP IDLE woke up, running handler", name);
	let _handler_result = match run_handlers(name.clone(), account.commands.clone()).await {
            Ok(handler_result) => {
		info!("Handlers ran successfully");
		handler_result
	    },
            Err(e) => {
		error!("Hander reported an error: {:?}", e);
	    },
	};

	// return the session after we are done with it
	debug!("{}: sending DONE prior to logout", name);
	let mut session = match idle.done().await {
	    Ok(session) => session,
	    Err(e) => {
		warn!("{}: error sending DONE prior to logout: {:?}", name, e);
		sleep(Duration::from_secs(retry)).await;
		continue
	    },
	};

	// be nice to the server and log out
	debug!("{}: logging out of session before creating new IDLE request", name);
	match session.logout().await {
	    Ok(_) => debug!("{}: logged out", name),
	    Err(e) => error!("{}: failed to log out: {:?}", name, e)
	}

        debug!("{}: IMAP connection ended. will retry in {}s.", name, retry);
        sleep(Duration::from_secs(retry)).await;
    }
}
