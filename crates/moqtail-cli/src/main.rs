use clap::{Args, Parser, Subcommand};
use moqtail_core::{compile, payload, Matcher, Message};
#[cfg(feature = "tls")]
use rumqttc::Transport;
use rumqttc::{Client, Event, Incoming, MqttOptions, Publish, QoS};
use std::borrow::Cow;
use std::collections::HashMap;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// MQTT client ID (auto-generated if omitted)
    #[arg(long)]
    client_id: Option<String>,
    /// Use TLS for the connection
    #[cfg(feature = "tls")]
    #[arg(long)]
    tls: bool,
}

static CLIENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn resolve_client_id(cmd: &SubArgs) -> String {
    if let Some(client_id) = &cmd.client_id {
        return client_id.clone();
    }

    let pid = process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = CLIENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("moqtail-cli-{pid}-{nanos:x}-{counter:x}")
}

#[cfg(test)]
thread_local! {
    pub static TEST_OPTIONS: RefCell<Option<MqttOptions>> = const { RefCell::new(None) };
}

pub(crate) fn run_sub(cmd: SubArgs) -> Result<(), String> {
    let selector = compile(&cmd.query).map_err(|e| format!("Failed to compile selector: {e}"))?;
    println!("{selector}");
    // The broker only understands plain MQTT filters, so subscribe to the
    // narrowest one that covers the selector and evaluate the rest here.
    let filter = selector.subscription_filter();
    eprintln!("subscribing to MQTT filter {filter:?}");

    let mut mqttoptions = MqttOptions::new(resolve_client_id(&cmd), cmd.host, cmd.port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    match (cmd.username.as_deref(), cmd.password.as_deref()) {
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

    let mut matcher = Matcher::new(selector);
    let (client, mut connection) = Client::new(mqttoptions, 10);
    // Delivery QoS is the lower of the publish and subscription QoS, so
    // subscribe at 2 for `qos` predicates to see what the publisher sent.
    if let Err(e) = client.subscribe(filter, QoS::ExactlyOnce) {
        return Err(connection_error(e, cmd.password.as_deref()));
    }
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                if let Some(line) = evaluate_publish(&mut matcher, &p, Instant::now()) {
                    println!("{line}");
                }
            }
            Ok(_) => {}
            Err(e) => return Err(connection_error(e, cmd.password.as_deref())),
        }
    }
    Ok(())
}

/// Applies the selector to one delivered message and returns the line to print
/// for it, if any.
///
/// Selectors see the delivery's `qos`, `retained` and `dup` flags and the
/// payload decoded as JSON or CBOR. A selector with pipeline stages prints the
/// stage result instead of the payload.
fn evaluate_publish(matcher: &mut Matcher, p: &Publish, now: Instant) -> Option<String> {
    let qos = match p.qos {
        QoS::AtMostOnce => "0",
        QoS::AtLeastOnce => "1",
        QoS::ExactlyOnce => "2",
    };
    let flag = |b: bool| Cow::Borrowed(if b { "true" } else { "false" });
    let headers = HashMap::from([
        (Cow::Borrowed("qos"), Cow::Borrowed(qos)),
        (Cow::Borrowed("retained"), flag(p.retain)),
        (Cow::Borrowed("dup"), flag(p.dup)),
    ]);
    let mut msg = Message {
        topic: &p.topic,
        headers,
        payload: None,
    };
    if matcher.selector().needs_payload() && matcher.may_match(&msg) {
        msg.payload = payload::from_bytes(&p.payload).ok();
    }

    if matcher.selector().stages.is_empty() {
        matcher
            .matches(&msg)
            .then(|| format!("{}: {}", p.topic, String::from_utf8_lossy(&p.payload)))
    } else {
        matcher
            .process(&msg, now)
            .map(|value| format!("{}: {value}", p.topic))
    }
}

fn connection_error(error: impl std::fmt::Display, password: Option<&str>) -> String {
    let raw = format!("Connection error: {error}");
    redact_password(&raw, password)
}

fn redact_password(message: &str, password: Option<&str>) -> String {
    match password {
        Some(password) if !password.is_empty() => message.replace(password, "[REDACTED]"),
        _ => message.to_string(),
    }
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
            client_id: None,
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        assert_eq!(
            opts.credentials(),
            Some(("user".to_owned(), "pass".to_owned()))
        );
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
            client_id: None,
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        assert_eq!(opts.credentials(), Some(("user".to_owned(), "".to_owned())));

        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: None,
            password: Some("pass".into()),
            client_id: None,
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        assert_eq!(opts.credentials(), Some(("".to_owned(), "pass".to_owned())));
    }

    #[test]
    fn connection_errors_redact_password() {
        let password = "super-secret";
        let err = connection_error(
            format!("auth failed for password={password} at broker"),
            Some(password),
        );
        assert!(err.contains("[REDACTED]"));
        assert!(!err.contains(password));
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
            client_id: None,
            tls: true,
        };
        let transport = opts_from(cmd).transport();
        assert!(matches!(transport, rumqttc::Transport::Tls(_)));
    }

    #[test]
    fn uses_explicit_client_id() {
        let client_id = "test-client-id-123";
        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: None,
            password: None,
            client_id: Some(client_id.into()),
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        assert_eq!(opts.client_id(), client_id);
    }

    fn publish(topic: &str, qos: QoS, retain: bool, payload: &str) -> Publish {
        let mut p = Publish::new(topic, qos, payload.as_bytes().to_vec());
        p.retain = retain;
        p
    }

    fn eval(selector: &str, p: &Publish) -> Option<String> {
        let mut matcher = Matcher::new(compile(selector).unwrap());
        evaluate_publish(&mut matcher, p, Instant::now())
    }

    #[test]
    fn post_filters_on_topic_headers_and_payload() {
        let hot = publish("site/sensor", QoS::AtLeastOnce, false, r#"{"temp":35}"#);
        let cold = publish("site/sensor", QoS::AtLeastOnce, false, r#"{"temp":25}"#);
        assert_eq!(
            eval("//sensor[json$.temp>30]", &hot).as_deref(),
            Some(r#"site/sensor: {"temp":35}"#)
        );
        assert_eq!(eval("//sensor[json$.temp>30]", &cold), None);
        assert_eq!(eval("/site/actuator", &hot), None);

        assert!(eval("/msg[qos=1]//sensor", &hot).is_some());
        assert!(eval("/msg[qos=2]//sensor", &hot).is_none());
        assert!(eval("/msg[retained=false][dup=false]//sensor", &hot).is_some());
        let retained = publish("site/sensor", QoS::AtMostOnce, true, "x");
        assert!(eval("/msg[retained=true]//sensor", &retained).is_some());
    }

    #[test]
    fn stages_print_their_result() {
        let mut matcher =
            Matcher::new(compile("//sensor |> window(10s) |> avg(json$.temp)").unwrap());
        let start = Instant::now();
        let first = publish("a/sensor", QoS::AtMostOnce, false, r#"{"temp":10}"#);
        let second = publish("b/sensor", QoS::AtMostOnce, false, r#"{"temp":20}"#);
        assert_eq!(
            evaluate_publish(&mut matcher, &first, start).as_deref(),
            Some("a/sensor: 10")
        );
        assert_eq!(
            evaluate_publish(&mut matcher, &second, start + Duration::from_secs(1)).as_deref(),
            Some("b/sensor: 15")
        );
    }

    #[test]
    fn generates_default_client_id() {
        let cmd = SubArgs {
            query: "/foo".into(),
            host: "localhost".into(),
            port: 1883,
            dry_run: true,
            username: None,
            password: None,
            client_id: None,
            #[cfg(feature = "tls")]
            tls: false,
        };
        let opts = opts_from(cmd);
        assert!(opts.client_id().starts_with("moqtail-cli-"));
    }
}
