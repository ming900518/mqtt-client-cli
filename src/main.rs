use axum::{
    extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router, Server,
};
use clap::Parser;
use colored::Colorize;
use futures_util::stream::StreamExt;
use mimalloc::MiMalloc;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json, to_value, Value};
use std::{
    collections::HashMap, net::SocketAddr, path::PathBuf, process::exit, sync::Arc,
    time::Duration,
};
use tokio::{sync::RwLock, fs};

use paho_mqtt::{
    properties, AsyncClient, ConnectOptionsBuilder, CreateOptionsBuilder, PropertyCode,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
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

    let create_options = CreateOptionsBuilder::new()
        .server_uri(args.host.clone())
        .client_id("")
        .finalize();

    let mut client = match AsyncClient::new(create_options) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("Error when creating async client: {}.", error);
            exit(1)
        }
    };

    let mut stream = client.get_stream(300);

    let mut connect_options = ConnectOptionsBuilder::new();
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
        .subscribe(args.topic.clone().unwrap_or("#".to_owned()), 1)
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
            args.topic.unwrap_or("#".to_owned())
        ),
    };
    let data_map_lock_cloned = data_map_lock.clone();

    tokio::spawn(async move {
        while let Some(message_option) = stream.next().await {
            if let Some(message) = message_option {
                let topic = message.topic().to_owned();
                let payload = (*String::from_utf8_lossy(message.payload())).to_owned();
                match &args.output {
                    Some(path) => {
                        fs::write(path, message.topic()).await.expect("Unable to write data.");
                        fs::write(path, [b' ', b'-', b' ']).await.expect("Unable to write data.");
                        fs::write(path, message.payload()).await.expect("Unable to write data.");
                        fs::write(path, [b'\n']).await.expect("Unable to write data.");
                    },
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

    if let Err(err) = Server::bind(&addr).serve(router).await {
        eprintln!(
            "{} HTTP Server not started, reason: {}.",
            "[WARN]".yellow(),
            err
        )
    }
}

async fn get_mqtt_info(
    State(data_map_lock): State<Arc<RwLock<HashMap<String, InnerValue>>>>,
) -> impl IntoResponse {
    loop {
        let Ok(data_map) = data_map_lock.try_read() else {
            continue;
        };
        match to_value(data_map.clone()) {
            Ok(body) => {
                return (StatusCode::OK, Json(body));
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error_message": err.to_string()})),
                );
            }
        }
    }
}
