use clap::{Parser, Subcommand};
use moqtail_core::compile;
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use std::time::Duration;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile and print a subscription selector
    Sub {
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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sub {
            query,
            host,
            port,
            dry_run,
        } => match compile(&query) {
            Ok(selector) => {
                println!("{selector}");
                if dry_run {
                    return;
                }

                let mut mqttoptions = MqttOptions::new("moqtail-cli", host, port);
                mqttoptions.set_keep_alive(Duration::from_secs(5));

                let (client, mut connection) = Client::new(mqttoptions, 10);
                client
                    .subscribe(selector.to_string(), QoS::AtMostOnce)
                    .unwrap();

                for event in connection.iter().flatten() {
                    if let Event::Incoming(Incoming::Publish(p)) = event {
                        println!("{}: {}", p.topic, String::from_utf8_lossy(&p.payload));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to compile selector: {e}");
                std::process::exit(1);
            }
        },
    }
}
