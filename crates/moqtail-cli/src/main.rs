use clap::{Args, Parser, Subcommand};
use moqtail_core::compile;
#[cfg(feature = "tls")]
use rumqttc::Transport;
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use std::time::Duration;

#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::thread_local;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile and print a subscription selector
    Sub(SubArgs),
}

#[derive(Args, Clone)]
struct SubArgs {
    /// Query selector string
    query: String,
    /// Broker hostname
    #[arg(long, default_value = "localhost")]
    host: String,
    /// Broker port
    #[arg(long, default_value_t = 1883)]
    port: u16,
    /// Only compile selector without connecting
    #[arg(long)]
    dry_run: bool,
    /// Username for authentication
    #[arg(long)]
    username: Option<String>,
    /// Password for authentication
    #[arg(long)]
    password: Option<String>,
    /// Use TLS for the connection
    #[cfg(feature = "tls")]
    #[arg(long)]
    tls: bool,
}

#[cfg(test)]
thread_local! {
    pub static TEST_OPTIONS: RefCell<Option<MqttOptions>> = RefCell::new(None);
}

pub(crate) fn run_sub(cmd: SubArgs) -> Result<(), String> {
    let selector = compile(&cmd.query).map_err(|e| format!("Failed to compile selector: {e}"))?;
    println!("{selector}");

    let mut mqttoptions = MqttOptions::new("moqtail-cli", cmd.host, cmd.port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    match (cmd.username, cmd.password) {
        (Some(u), Some(p)) => {
            mqttoptions.set_credentials(u, p);
        }
        (Some(u), None) => {
            mqttoptions.set_credentials(u, "");
        }
        (None, Some(p)) => {
            mqttoptions.set_credentials("", p);
        }
        (None, None) => {}
    }
    #[cfg(feature = "tls")]
    if cmd.tls {
        mqttoptions.set_transport(Transport::tls_with_default_config());
    }
    #[cfg(test)]
    TEST_OPTIONS.with(|cell| {
        *cell.borrow_mut() = Some(mqttoptions.clone());
    });
    if cmd.dry_run {
        return Ok(());
    }

    let (client, mut connection) = Client::new(mqttoptions, 10);
    if let Err(e) = client.subscribe(selector.to_string(), QoS::AtMostOnce) {
        return Err(format!("Connection error: {e}"));
    }
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                println!("{}: {}", p.topic, String::from_utf8_lossy(&p.payload));
            }
            Ok(_) => {}
            Err(e) => return Err(format!("Connection error: {e}")),
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sub(cmd) => {
            if let Err(e) = run_sub(cmd) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts_from(cmd: SubArgs) -> MqttOptions {
        run_sub(cmd).unwrap();
        TEST_OPTIONS.with(|cell| cell.borrow().clone().unwrap())
    }

    #[test]
    fn sets_credentials() {
        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: Some("user".into()),
            password: Some("pass".into()),
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        let dbg = format!("{:?}", opts);
        assert!(dbg.contains("user"));
        assert!(dbg.contains("pass"));
    }

    #[test]
    fn single_credential_flags() {
        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: Some("user".into()),
            password: None,
            #[cfg(feature = "tls")]
            tls: false,
        };
        let dbg = format!("{:?}", opts_from(cmd));
        assert!(dbg.contains("user"));

        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: None,
            password: Some("pass".into()),
            #[cfg(feature = "tls")]
            tls: false,
        };
        let dbg = format!("{:?}", opts_from(cmd));
        assert!(dbg.contains("pass"));
    }

    #[cfg(feature = "tls")]
    #[test]
    fn enables_tls_flag() {
        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: None,
            password: None,
            tls: true,
        };
        let transport = opts_from(cmd).transport();
        assert!(matches!(transport, rumqttc::Transport::Tls(_)));
    }
}
