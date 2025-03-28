use anyhow::{Ok, Result, anyhow};
use clap::Parser;
use dotenv::dotenv;
use embedded_graphics::{image::Image, pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};
use env_logger::Env;
use log::{error, info};
use models::DisplayResponse;
use reqwest::Client;
use reqwest::header;
use std::time::Duration;
use tinybmp::Bmp;
use tokio::signal;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::oneshot;
use tokio::time::interval;
mod models;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

/// Simulate your TRMNL display on your desktop, or use with a BYOD license
#[derive(Parser, Debug)]
#[command(version, about)]
struct ApiArguments {
    /// Your API key found in TRMNL's developer settings
    #[arg(short, long)]
    api_key: String,

    /// Base url for the API
    #[arg(short, long, default_value = "https://usetrmnl.com")]
    base_url: String,
}

enum Message {
    NewImage(Vec<u8>),
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);

    let env_api_key = std::env::var("API_KEY");
    let env_base_url = std::env::var("API_URL_BASE");

    let args = if env_api_key.is_ok() && env_base_url.is_ok() {
        info!("Using API_KEY and API_URL_BASE from environment variables");
        ApiArguments {
            api_key: env_api_key.unwrap(),
            base_url: env_base_url.unwrap(),
        }
    } else {
        info!("Using command-line arguments for API_KEY and API_URL_BASE");
        ApiArguments::parse()
    };

    // I think 1 will be fine for now, but I might need to increase this later
    let (tx, rx) = mpsc::channel::<Message>(1);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    //TODO still not working need to close window first to close
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Ctrl-C received, shutting down");
            shutdown_tx.send(()).unwrap();
            return Ok(());
        },
        _ = web_calls(tx, args) => {},
        _ = run_display(rx, shutdown_rx) => {},
    }

    Ok(())
}

async fn run_display(
    mut rx: mpsc::Receiver<Message>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<()> {
    let output_settings = OutputSettingsBuilder::new()
        .scale(1)
        .theme(BinaryColorTheme::Default)
        .build();
    let mut window = Window::new("TRMNL", &output_settings);
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(800, 480));

    tokio::select! {
        _ = shutdown_rx => {
            info!("Shutdown signal received, exiting display loop");
            return Ok(());
        },
        _ = async {
            while let Some(message) = rx.recv().await {
                match message {
                    Message::NewImage(bmp_bytes) => {
                        info!("New display update received");
                        let bmp = Bmp::<BinaryColor>::from_slice(&bmp_bytes).unwrap();
                        let _ = Image::new(&bmp, Point::zero()).draw(&mut display);
                        window.show_static(&display);
                    }
                }
            }
            info!("Channel has been closed, can end the process");
        } => {},
    }

    Ok(())
}

async fn web_calls(sender: Sender<Message>, config: ApiArguments) -> Result<()> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "access-token",
        header::HeaderValue::from_str(&config.api_key)?,
    );
    let client = Client::builder()
        .user_agent(APP_USER_AGENT)
        .default_headers(headers)
        .build()?;

    let mut tick_interval = interval(Duration::from_secs(600));

    loop {
        let sleep_time = tick_interval.period().as_secs();
        info!("Sleeping for {} seconds", sleep_time);
        tick_interval.tick().await;

        let result = client
            .get(format!("{}/api/display", config.base_url))
            .send()
            .await;

        if result.is_err() {
            error!("Failed to get response from api");
            continue;
        }
        let result = result.unwrap();
        let status = result.status().clone();
        let body_as_string = result.text().await?;
        let parse_result = serde_json::from_str::<DisplayResponse>(&body_as_string.clone());
        if parse_result.is_err() {
            error!("Failed to parse response from api\nStatus: {}", status);
            error!("{:#?}", body_as_string);
            continue;
        }

        info!("{parse_result:#?}");

        let resp = parse_result?;
        //Not sure on a successful one yet
        if resp.status == 500 {
            match resp.error {
                Some(err_msg) => {
                    error!("Error from api: {}", err_msg);
                }
                None => {
                    error!("Web request failed but no error from api.")
                }
            };

            continue;
        }

        match resp.image_url {
            Some(image_url) => {
                let new_bytes = client.get(image_url).send().await?.bytes().await?.to_vec();
                let sender = sender.send(Message::NewImage(new_bytes)).await;
                if sender.is_err() {
                    error!("Failed to send new image to display");
                    return Err(anyhow!("Failed to send new image to display"));
                }
            }
            None => {
                return Err(anyhow!(
                    "An image_url was not returned from the api response"
                ));
            }
        }

        match resp.refresh_rate {
            Some(refresh_rate) => {
                // info!(
                //     "Sleeping for {} seconds since that's what the api asked for.",
                //     refresh_rate
                // );

                tick_interval = interval(Duration::from_secs(refresh_rate));
                tick_interval.tick().await;
            }
            None => {
                info!("No refresh rate from api, sleeping for 10mins")
            }
        }
    }
}
