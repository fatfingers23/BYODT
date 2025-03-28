use anyhow::{Ok, Result};
use clap::Parser;
use embedded_graphics::{image::Image, pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};
use env_logger::Env;
use log::info;
use tinybmp::Bmp;
use tokio::sync::mpsc::{self, Sender};

mod models;

/// Setup arguments
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct ApiArguments {
    /// Your API key found in TRMNL's developer settings
    #[arg(short, long)]
    api_key: String,

    /// Base url for the API
    #[arg(short, long, default_value = "https://usetrmnl.com")]
    base_url: String,
}

enum Message<'a> {
    NewImage(Bmp<'a, BinaryColor>),
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);

    let (tx, mut rx) = mpsc::channel::<Message>(1);
    tokio::spawn(async move {
        web_calls(tx).await.unwrap();
    });

    let output_settings = OutputSettingsBuilder::new()
        .scale(1)
        .theme(BinaryColorTheme::Default)
        .build();
    let mut window = Window::new("TRMNL", &output_settings);
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(800, 480));

    loop {
        //Waits for a new message
        let message = rx.recv().await;

        match message {
            Some(message) => match message {
                Message::NewImage(bmp) => {
                    info!("New display update received");
                    Image::new(&bmp, Point::zero()).draw(&mut display)?;
                    window.show_static(&display);
                }
            },
            None => {
                info!("Channel has been closed, can end the process");
                break;
            }
        }
    }

    Ok(())
}

async fn web_calls(sender: Sender<Message<'_>>) -> Result<()> {
    let bmp_data = include_bytes!("../test/byod_error.bmp");
    let bmp = Bmp::<BinaryColor>::from_slice(bmp_data).unwrap();
    let _ = sender.send(Message::NewImage(bmp)).await;
    Ok(())
}
