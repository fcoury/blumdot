use anyhow::{Context, Result};
use blumdot::{
    AnimationExportOptions, AnimationOptions, GlyphMode, InputSource, RenderOptions,
    animate_source, export_animation_source, render_source,
};
use clap::{Args as ClapArgs, Parser, Subcommand};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "blumdot",
    about = "Render images as monochrome Unicode braille art",
    arg_required_else_help = true
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Local image path or HTTP/HTTPS image URL.
    input: Option<String>,

    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Rotate the source through a full circle and redraw it in place.
    Animate(AnimateArgs),
}

#[derive(Clone, Copy, Debug, ClapArgs)]
struct RenderArgs {
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

    /// Rotate the source image clockwise by this many degrees before rendering.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    rotate: f32,

    /// Use Unicode quadrant block characters instead of braille dots.
    #[arg(long)]
    solid: bool,
}

#[derive(Debug, ClapArgs)]
struct AnimateArgs {
    /// Local image path or HTTP/HTTPS image URL.
    input: String,

    /// Degrees to rotate between animation frames.
    #[arg(allow_hyphen_values = true)]
    degrees: f32,

    /// Milliseconds to wait between animation frames.
    #[arg(long, default_value_t = 50)]
    frame_delay_ms: u64,

    /// Stop after one full rotation instead of looping forever.
    #[arg(long)]
    no_loop: bool,

    /// Write one full rotation of animation frames to a text file.
    #[arg(long)]
    export_file: Option<PathBuf>,

    #[command(flatten)]
    render: RenderArgs,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Command::Animate(animate)) => {
            let render_options = render_options(animate.render);
            if let Some(export_file) = animate.export_file {
                let mut file = File::create(&export_file).with_context(|| {
                    format!("failed to create export file {}", export_file.display())
                })?;
                export_animation_source(
                    InputSource::parse(animate.input),
                    AnimationExportOptions {
                        render_options,
                        degree_step: animate.degrees,
                    },
                    &mut file,
                )?;
            } else {
                let mut stdout = io::stdout().lock();
                animate_source(
                    InputSource::parse(animate.input),
                    AnimationOptions {
                        render_options,
                        degree_step: animate.degrees,
                        frame_delay: Duration::from_millis(animate.frame_delay_ms),
                        loop_animation: !animate.no_loop,
                    },
                    &mut stdout,
                )?;
            }
        }
        None => {
            let input = args
                .input
                .expect("clap requires an input unless a subcommand is used");
            let output = render_source(InputSource::parse(input), render_options(args.render))?;

            println!("{output}");
        }
    }

    Ok(())
}

fn render_options(args: RenderArgs) -> RenderOptions {
    RenderOptions {
        width: args.width,
        threshold: args.threshold,
        invert: args.invert,
        alpha_cutoff: args.alpha_cutoff,
        glyph_mode: if args.solid {
            GlyphMode::Solid
        } else {
            GlyphMode::Braille
        },
        rotation_degrees: args.rotate,
    }
}
