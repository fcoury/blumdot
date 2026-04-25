use anyhow::Result;
use blumdot::{InputSource, RenderOptions, render_source};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "blumdot",
    about = "Render images as monochrome Unicode braille art"
)]
struct Args {
    /// Local image path or HTTP/HTTPS image URL.
    input: String,

    /// Output width in braille cells.
    #[arg(short, long, default_value_t = 40)]
    width: u32,

    /// Luminance threshold below which a pixel becomes ink.
    #[arg(short, long, default_value_t = 180)]
    threshold: u8,

    /// Reverse luminance selection so light pixels become ink.
    #[arg(long)]
    invert: bool,

    /// Alpha value below which pixels are treated as blank.
    #[arg(long, default_value_t = 16)]
    alpha_cutoff: u8,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let output = render_source(
        InputSource::parse(args.input),
        RenderOptions {
            width: args.width,
            threshold: args.threshold,
            invert: args.invert,
            alpha_cutoff: args.alpha_cutoff,
        },
    )?;

    println!("{output}");
    Ok(())
}
