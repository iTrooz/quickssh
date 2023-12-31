use std::{env, fs::File};

use log::warn;
use russh_keys::key::{KeyPair, PublicKey};

use crate::{
    cli::Command,
    ssh::{self, init::Password},
};

fn init_server_key() -> anyhow::Result<KeyPair> {
    let xdg = xdg::BaseDirectories::with_prefix("quickssh")?;
    let existing_prv_key_path = xdg.find_config_file("private.key");
    if let Some(existing_prv_key_path) = existing_prv_key_path {
        let keypair = russh_keys::load_secret_key(&existing_prv_key_path, None)?;
        log::debug!(
            "Loaded private key from {}",
            existing_prv_key_path.display()
        );
        Ok(keypair)
    } else {
        let keypair = KeyPair::generate_ed25519();

        let path = xdg.place_config_file("private.key")?;
        let f = File::create(&path)?;
        russh_keys::encode_pkcs8_pem(&keypair, f)?;

        log::info!("Created new ed25519 private key at {}", path.display());

        Ok(keypair)
    }
}

fn parse_key(full_key: &str) -> anyhow::Result<PublicKey> {
    let mut split = full_key.split_whitespace();
    match (split.next(), split.next()) {
        (Some(_), Some(key)) => Ok(russh_keys::parse_public_key_base64(key)?),
        (Some(key), None) => Ok(russh_keys::parse_public_key_base64(key)?),
        _ => anyhow::bail!("Failed to parse key {full_key}"),
    }
}

fn read_authorized_keys() -> anyhow::Result<Vec<PublicKey>> {
    let xdg = xdg::BaseDirectories::with_prefix("quickssh")?;
    let path = xdg.find_config_file("authorized_keys");
    if let Some(existing_path) = path {
        let mut keys: Vec<PublicKey> = vec![];
        for (i, line) in std::fs::read_to_string(existing_path)?.lines().enumerate() {
            match parse_key(line) {
                Ok(key) => keys.push(key),
                Err(err) => warn!("Failed to parse key from authorized_keys:{} : {}", i, err),
            };
        }
        Ok(keys)
    } else {
        Ok(vec![])
    }
}

fn init_logger(verbose: u8) {
    let mut tmp = env_logger::builder();
    let mut log_builder = &mut tmp;

    match verbose {
        0 => log_builder = log_builder.filter_level(log::LevelFilter::Info),
        1 => {
            log_builder = log_builder.filter_level(log::LevelFilter::Info);
            log_builder = log_builder.filter_module("quickssh", log::LevelFilter::Debug);
            // Also log packets
            log_builder = log_builder.filter_module("russh_sftp::server", log::LevelFilter::Debug);
        }
        2.. => log_builder = log_builder.filter_level(log::LevelFilter::Debug),
    };
    log_builder.init();

    match verbose {
        0 => {}
        1 => log::info!("Debug logging enabled for quickssh. Use -vv to also debug libraries"),
        2.. => log::debug!("Debug logging enabled for all code"),
    };
}

pub async fn run(cmd: Command) -> anyhow::Result<()> {
    init_logger(cmd.verbose);

    let keypair = init_server_key()?;

    let mut pubkeys = read_authorized_keys()?;
    for (i, key) in cmd.pubkey.iter().enumerate() {
        match parse_key(key) {
            Ok(key) => pubkeys.push(key),
            Err(err) => warn!("Failed to parse key from authorized_keys:{} : {}", i, err),
        };
    }

    let options = ssh::ServerOptions {
        user: cmd.user.unwrap_or(crate::utils::get_username()?),
        password: Some(cmd.password.map(Password::Raw).unwrap_or(Password::Su)),
        pubkeys,
        shell: cmd
            .shell
            .unwrap_or_else(|| env::var("SHELL").expect("No SHELL variable defined")),
        no_shell: cmd.no_shell,
        no_sftp: cmd.no_sftp,
        port: cmd.port,
    };

    log::info!("Listening on 0.0.0.0:{}", options.port);
    log::info!("User is {}", options.user);
    log::info!(
        "Password is {}",
        match options.password {
            Some(Password::Raw(ref password)) => password,
            Some(Password::Su) => "checked using su",
            None => "unset",
        } // if let Some(ref password) = options.password {
          //     password
          // } else {
          //     "checked using `su`"
          // }
    );
    log::info!("{} public key(s) loaded", options.pubkeys.len());

    println!();

    ssh::start_ssh_server(options, keypair).await?;
    Ok(())
}
