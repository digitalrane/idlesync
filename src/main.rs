extern crate xdg;
extern crate pretty_env_logger;
#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
extern crate serde_yaml;

use async_imap::error::{Result};
use async_imap::extensions::idle::IdleResponse::*;
use async_std::task;
use futures::stream;
use futures_util::stream::StreamExt;
use std::time::Duration;
use std::fs::File;
use std::io::BufReader;
use std::process::Command;

#[derive(Serialize, Deserialize, Debug)]
struct Account {
    host: String,
    user: String,
    pass: String,
    tls: bool,
    commands: Vec<String>,
    port: Option<u16>,
    name: Option<String>,
    folders: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Settings {
    accounts: Vec<Account>,
    idle_timeout: Option<u64>,
    retry: Option<u64>,
}

fn status_out(message: String) {
        info!("{}", message);
        println!("{}", message);
}

async fn async_main() -> Result<()> {

        // set up logging
        pretty_env_logger::init();
        // hello
        status_out("Starting idlesync".to_string());

        // find configuration file
        let xdg_dirs = xdg::BaseDirectories::with_prefix("idlesync").unwrap();
        let config_path = xdg_dirs.find_config_file("config.yaml").expect("Could not find configuration file.");
        trace!("Configuration base directory: {}", config_path.display());

        // read and deserialise configuration
        trace!("Reading settings...");
        let config_file = File::open(config_path)?;
        let config_file_reader = BufReader::new(config_file);
        let mut settings: Settings = serde_yaml::from_reader(config_file_reader).unwrap();
        
        // check and clean settings
        if settings.retry.is_none() {
            settings.retry = Some(60);
        }
        if settings.idle_timeout.is_none() {
            settings.idle_timeout = Some(600);
        }

        for account in settings.accounts.iter_mut() {
            if account.name.is_none() {
                account.name = Some(String::from(&*account.host));
            }
            if account.port.is_none() {
              if account.tls {
                account.port = Some(993)
              } else {
                account.port = Some(143)
              }
            }
        }
        trace!("Settings: {:?}", settings);
        let retry = settings.retry.unwrap();
        let idle_timeout = settings.idle_timeout.unwrap();

        // generate async futures for each account and schedule them
        let account_stream = stream::iter(settings.accounts);
        account_stream
            .for_each_concurrent(None, |account| async move {
                while let result = monitor_account(&account, &idle_timeout).await {
                    match result {
                        Err(e) => {
                            error!("{:?}: IMAP connection error: {}. Will retry in {}.", account.name, e, retry);
                            task::sleep(Duration::from_secs(retry)).await;
                        },
                        Ok(v) => v,
                    }
                }
            }).await;
        Ok(())
}

async fn run_command(command: &String) -> Result<()> {
    // just *nix for now, would be easy to extend
    let command_result = Command::new("sh")
                                  .arg("-c")
                                  .arg(command)
                                  .output()
                                  .expect("failed to execute process");
    if command_result.status.success() == false {
        error!("Command {} failed, stderr: {}", command, String::from_utf8_lossy(&command_result.stderr));
    }
    Ok(())
}

async fn run_handlers(account: &Account) -> Result<()> {
    for command in &account.commands {
        status_out(format!("Running command {}", command));
        run_command(command).await;
    }
    Ok(())
}

async fn monitor_account(account: &Account, idle_timeout: &u64) -> Result<()> {
    let tls = async_native_tls::TlsConnector::new();
    // we dereference (*) the host to get a str for the ToSocketAddrs impl
    let imap_addr = (&*account.host, account.port.unwrap());
    let name = account.name.as_ref().unwrap();
    status_out(format!("{}: Monitoring account", name));

    // we pass in the imap_server twice to check that the server's TLS
    // certificate is valid for the imap_server we're connecting to.
    let client = async_imap::connect(imap_addr, &account.host, tls).await?;
    status_out(format!("{}: Connected to {}:{}", name, imap_addr.0, imap_addr.1));

    // the client we have here is unauthenticated.
    // to do anything useful with the e-mails, we need to log in
    let mut session = client.login(&account.user, &account.pass).await.map_err(|e| e.0)?;
    status_out(format!("{}: Logged in as {}", name, &account.user));

    // we want to fetch some messages from the INBOX
    session.select("INBOX").await?;

    // fetch flags from all messages
    let msg_stream = session.fetch("1:*", "(FLAGS )").await?;
    let msgs = msg_stream.collect::<Vec<_>>().await;
    trace!("{}: Number of fetched msgs: {:?}", name, msgs.len());

    // init idle session
    status_out(format!("{}: Watching folders via IDLE", name));
    let mut idle = session.idle();
    idle.init().await?;

    let (idle_wait, _interrupt) = idle.wait_with_timeout(Duration::from_secs(*idle_timeout));
    let idle_result = idle_wait.await?;
    status_out(format!("{}: IMAP IDLE timed out or woke up", name));
    match idle_result {
        ManualInterrupt => {
            trace!("{}: IDLE manually interrupted, will re-establish", name);
        }
        Timeout => {
            trace!("{}: IDLE timed out, will re-establish", name);
        }
        NewData(data) => {
            let s = String::from_utf8(data.head().to_vec()).unwrap();
            trace!("{}: IDLE data:\n {}", name, s);
        }
    }
    status_out(format!("{}: IMAP IDLE woke up, running handler", name));
    run_handlers(account).await;

    // return the session after we are done with it
    trace!("{}: sending DONE prior to logout", name);
    let mut session = idle.done().await?;

    // be nice to the server and log out
    trace!("{}: logging out of session before creating new IDLE request", name);
    session.logout().await?;
    Ok(())
}

fn main() -> Result<()> {
    task::block_on(async_main())
}

