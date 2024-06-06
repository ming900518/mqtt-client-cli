use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use clap::Parser;
use colored::Colorize;
use futures_util::stream::StreamExt;
use mimalloc::MiMalloc;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json, to_value, Value};
use std::{
    collections::HashMap, net::SocketAddr, path::PathBuf, process::exit, sync::Arc, time::Duration,
};
use tokio::{fs, net::TcpListener, sync::RwLock};

use paho_mqtt::{
    properties, AsyncClient, AsyncReceiver, ConnectOptionsBuilder, CreateOptionsBuilder, Message,
    PropertyCode, SslOptions,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host. Required
    #[arg(short = 'H', long, value_name = "HOST URL")]
    host: String,

    /// Username. Optional
    #[arg(short, long)]
    username: Option<String>,

    /// Password. Optional
    #[arg(short, long)]
    password: Option<String>,

    /// Topic. Optional (Default = "#")
    #[arg(short, long)]
    topic: Option<String>,

    /// Output Path. All data from the MQTT stream will be stored into the specified file.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Mqtt Version. (Default = 3)
    #[arg(short = 'v', long)]
    mqtt_version: Option<u8>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(untagged)]
enum InnerValue {
    String(String),
    Json(HashMap<String, Value>),
    JsonArray(Vec<HashMap<String, Value>>),
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let data_map_lock: Arc<RwLock<HashMap<String, InnerValue>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let mut connect_options = {
        match (args.host.starts_with("ws"), args.mqtt_version) {
            (true, Some(5)) => ConnectOptionsBuilder::new_ws_v5(),
            (false, Some(5)) => ConnectOptionsBuilder::new_v5(),
            (true, _) => ConnectOptionsBuilder::new_ws(),
            (false, _) => ConnectOptionsBuilder::new(),
        }
    };

    if args.host.starts_with("wss") {
        connect_options.ssl_options(SslOptions::default());
    }

    let create_options = CreateOptionsBuilder::new()
        .server_uri(args.host.clone())
        .mqtt_version(if args.mqtt_version == Some(5) { 5 } else { 3 })
        .client_id("")
        .finalize();

    let mut client = match AsyncClient::new(create_options) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("Error when creating async client: {error}.");
            exit(1)
        }
    };

    let stream = client.get_stream(300);

    connect_options.keep_alive_interval(Duration::from_secs(30));
    connect_options.properties(properties![PropertyCode::SessionExpiryInterval => 3600]);
    connect_options.clean_session(true);

    if let Some(username) = args.username {
        connect_options.user_name(username);
    }
    if let Some(password) = args.password {
        connect_options.password(password);
    }

    let built_connect_options = connect_options.finalize();

    if let Err(error) = client.connect(built_connect_options).await {
        eprintln!("{} MQTT connection failed: {}.", "[ERROR] ".red(), error);
        exit(1)
    }

    match client
        .subscribe(args.topic.clone().unwrap_or_else(|| "#".to_owned()), 1)
        .await
    {
        Err(error) => {
            eprintln!("{} Failed to subscribe topic: {}.", "[ERROR]".red(), error);
            exit(1)
        }
        _ => eprintln!(
            "{} Connected to {} with topic \"{}\".",
            "[INFO]".blue(),
            args.host,
            args.topic.unwrap_or_else(|| "#".to_owned())
        ),
    };

    start_http_server(args.output, stream, data_map_lock).await;
}

async fn start_http_server(
    output: Option<PathBuf>,
    mut stream: AsyncReceiver<Option<Message>>,
    data_map_lock: Arc<RwLock<HashMap<String, InnerValue>>>,
) {
    let data_map_lock_cloned = data_map_lock.clone();
    tokio::spawn(async move {
        while let Some(message_option) = stream.next().await {
            if let Some(message) = message_option {
                let topic = message.topic().to_owned();
                let payload = (*String::from_utf8_lossy(message.payload())).to_owned();
                match &output {
                    Some(path) => {
                        fs::write(path, message.topic())
                            .await
                            .expect("Unable to write data.");
                        fs::write(path, [b' ', b'-', b' '])
                            .await
                            .expect("Unable to write data.");
                        fs::write(path, message.payload())
                            .await
                            .expect("Unable to write data.");
                        fs::write(path, [b'\n'])
                            .await
                            .expect("Unable to write data.");
                    }
                    None => println!("{}\n{}", format!("[{}]", &topic).green(), &payload),
                }
                if payload.starts_with('[') {
                    if let Ok(deserialized_data) = from_str::<Vec<HashMap<String, Value>>>(&payload)
                    {
                        data_map_lock_cloned
                            .write()
                            .await
                            .insert(topic, InnerValue::JsonArray(deserialized_data));
                    } else {
                        data_map_lock_cloned
                            .write()
                            .await
                            .insert(topic, InnerValue::String(payload));
                    };
                } else if let Ok(deserialized_data) = from_str::<HashMap<String, Value>>(&payload) {
                    data_map_lock_cloned
                        .write()
                        .await
                        .insert(topic, InnerValue::Json(deserialized_data));
                } else {
                    data_map_lock_cloned
                        .write()
                        .await
                        .insert(topic, InnerValue::String(payload));
                }
            } else {
                println!("{} No message from the stream.", "[WARN]".yellow());
            }
        }
    });

    let router = Router::new()
        .route("/", get(get_mqtt_info))
        .with_state(data_map_lock.clone())
        .into_make_service();

    let addr = SocketAddr::from(([0, 0, 0, 0], 12345));

    eprintln!("{} Starting HTTP Server... You can fetch all available data from http://{}/ if no error reported.", "[INFO]".blue(), addr);

    match TcpListener::bind(addr)
        .await
        .map(|tcp_server| axum::serve(tcp_server, router))
    {
        Ok(serve) => {
            if let Err(error) = serve.await {
                eprintln!(
                    "{} HTTP Server failed to initalize, reason: {}.",
                    "[WARN]".yellow(),
                    error
                );
            };
        }
        Err(error) => {
            eprintln!(
                "{} TCP Listener failed to initalize, reason: {}.",
                "[WARN]".yellow(),
                error
            );
        }
    }
}

async fn get_mqtt_info(
    State(data_map_lock): State<Arc<RwLock<HashMap<String, InnerValue>>>>,
) -> impl IntoResponse {
    let data_map = data_map_lock.read().await.clone();
    match to_value(data_map) {
        Ok(body) => (StatusCode::OK, Json(body)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error_message": err.to_string()})),
        ),
    }
}
