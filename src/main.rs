use anyhow::Result;
use embedded_graphics::{
    image::Image,
    mono_font::{MonoTextStyle, ascii::FONT_6X9},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};
use tinybmp::Bmp;
// use std::sync::{Arc, Mutex};
use tokio::sync::Mutex;
type TRMNLWindow = Mutex<Window>;

#[tokio::main]
async fn main() -> Result<()> {
    println!("got value from the server; result");
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(800, 480));
    let text_style = MonoTextStyle::new(&FONT_6X9, BinaryColor::On);

    Text::new("Hello World!", Point::new(5, 5), text_style).draw(&mut display)?;

    let output_settings = OutputSettingsBuilder::new()
        .scale(1)
        .theme(BinaryColorTheme::Default)
        .build();
    // let mut window = ;
    let window = Mutex::new(Window::new("TRMNL", &output_settings));
    run_window(window).await.unwrap();
    // tokio::spawn(async move {
    //     run_window(window).await.unwrap();
    // })
    // .await
    // .unwrap();
    // window.show_static(&display);

    // for event in window.events() {
    //     println!("{:?}", event);
    // }

    Ok(())
}

async fn run_window(window: TRMNLWindow) -> Result<()> {
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(800, 480));
    let text_style = MonoTextStyle::new(&FONT_6X9, BinaryColor::On);

    // Text::new("Hello World!", Point::new(5, 5), text_style).draw(&mut display)?;

    let bmp_data = include_bytes!("../test/byod_error.bmp");
    let bmp: Bmp<'_, BinaryColor> = Bmp::from_slice(bmp_data).unwrap();
    Image::new(&bmp, Point::new(0, 0)).draw(&mut display)?;

    let mut window = window.lock().await;
    window.show_static(&display);

    for event in window.events() {
        println!("{:?}", event);
    }

    Ok(())
}
