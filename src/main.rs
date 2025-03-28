use anyhow::{Result, anyhow};
use clap::Parser;
use dotenv::dotenv;
use embedded_graphics::{image::Image, pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::Keycode,
};
use env_logger::Env;
use log::{debug, error, info};
use models::DisplayResponse;
use reqwest::{Client, header};
use std::time::Duration;
use tinybmp::Bmp;
use tokio::{
    signal,
    sync::mpsc::{self, Sender},
    time::sleep,
};

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
    let (tx, rx) = mpsc::channel::<Message>(5);
    // Used to early bail a tokio::sleep in web_calls
    let (early_timeout_bail_sender, early_timeout_bail_receiver) = mpsc::channel::<()>(1);

    tokio::spawn(async move {
        let _ = web_calls(tx, early_timeout_bail_receiver, args).await;
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Ctrl-C received, shutting down");
            return Ok(());
        },
        _ = run_display(rx, early_timeout_bail_sender) => {},
    }

    Ok(())
}

async fn run_display(
    mut rx: mpsc::Receiver<Message>,
    early_timeout_bail: mpsc::Sender<()>,
) -> Result<()> {
    let output_settings = OutputSettingsBuilder::new()
        .scale(1)
        .pixel_spacing(1)
        .theme(BinaryColorTheme::Default)
        .build();
    let mut window = Window::new("TRMNL", &output_settings);
    //800x480
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(800, 480));

    loop {
        _ = match rx.try_recv() {
            Ok(message) => match message {
                Message::NewImage(bmp_bytes) => {
                    info!("New display update received");
                    let bmp = Bmp::<BinaryColor>::from_slice(&bmp_bytes).unwrap();
                    let _ = Image::new(&bmp, Point::zero()).draw(&mut display);
                }
            },
            Err(_) => {}
        };

        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => {
                    return Ok(());
                }
                SimulatorEvent::KeyDown {
                    keycode,
                    keymod: _,
                    repeat: _,
                } => match keycode {
                    Keycode::Return => {
                        debug!("Return key pressed");
                        let _ = early_timeout_bail.send(()).await;
                    }
                    _ => {
                        debug!("Unhandled keycode: {:?}", keycode);
                    }
                },
                _ => {}
            }
        }
        // Have to always update the display or it crashes. Faster fps (lower sleep) helps keep the process down
        // Get to high enough and it will crash
        // And if it's too high keypresses are missed
        sleep(Duration::from_millis(250)).await;
    }
}

async fn web_calls(
    sender: Sender<Message>,
    mut early_timeout_bail: mpsc::Receiver<()>,
    config: ApiArguments,
) -> Result<()> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "access-token",
        header::HeaderValue::from_str(&config.api_key)?,
    );

    let client = Client::builder()
        .user_agent(APP_USER_AGENT)
        .default_headers(headers)
        .build()?;

    let mut sleep_time = 600;
    let mut first_run = true;
    loop {
        if !first_run {
            info!("Sleeping for {} seconds", sleep_time);
            tokio::select! {
                _ = sleep(Duration::from_secs(sleep_time)) => {},
                _ = early_timeout_bail.recv() => {
                    info!("Refreshing now");
                }
            }
        } else {
            first_run = false;
        }

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
        //Not sure on a successful one yet. I think its 0
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
                sleep_time = refresh_rate;
            }
            None => {
                sleep_time = 600;
                info!("No refresh rate from api, sleeping for 10mins")
            }
        }
    }
}
