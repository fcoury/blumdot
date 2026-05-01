# bloomdot

Render images as monochrome Unicode braille art on the command line.

`bloomdot` reads a local image or URL, packs every 2×4 pixel block into a single
braille code point (`U+2800`–`U+28FF`), and prints the result. Raster formats
are decoded via [`image`](https://crates.io/crates/image); SVG inputs are
rasterized through [`resvg`](https://crates.io/crates/resvg) before sampling.

![source](assets/composite.png)

## Install

From source:

```sh
cargo install --path .
```

Or run it directly from the workspace:

```sh
cargo run --release -- <input> [options]
```

## Usage

```
bloomdot <input> [--width N] [--threshold N] [--invert] [--alpha-cutoff N]
                [--rotate DEG]
bloomdot animate <input> <degree-step> [--frame-delay-ms N] [--no-loop]
                [--export-file PATH]
```

`<input>` is either a local image path or an `http(s)://` URL. Remote responses
are capped at 10 MiB.

| Flag                | Default | Description                                                 |
| ------------------- | ------- | ----------------------------------------------------------- |
| `-w`, `--width`     | `40`    | Output width in braille cells (each cell is 2 pixels wide). |
| `-t`, `--threshold` | `180`   | Luminance below this value becomes ink.                     |
| `--invert`          | off     | Treat light pixels as ink instead of dark ones.             |
| `--alpha-cutoff`    | `16`    | Alpha values below this are treated as blank.               |
| `--rotate`          | `0`     | Rotate the source image clockwise before rendering.         |
| `--no-loop`         | off     | Stop animation after one full rotation.                     |
| `--export-file`     | unset   | Write one full rotation of animation frames to a text file. |

Examples:

```sh
bloomdot logo.png --width 60
bloomdot https://example.com/icon.svg --invert
bloomdot photo.jpg --threshold 128 --width 80
bloomdot logo.png --rotate 90
bloomdot animate logo.png 10 --width 60
bloomdot animate logo.png 10 --no-loop
bloomdot animate logo.png 10 --export-file frames.txt
```

## Library

`bloomdot` is also a library. The main entry points are `render_source` (load
and render in one step) and `render_image` (render an in-memory
`image::DynamicImage`):

```rust
use bloomdot::{InputSource, RenderOptions, render_source};

let art = render_source(
    InputSource::parse("logo.png"),
    RenderOptions::default().with_width(60),
)?;
println!("{art}");
```

## Trivia

The name comes from a jingle in a 90s Brazilian appliances ad that went
"bloom bop" — when I was reaching for a word to pair with "dot", that jingle
came straight to mind. You can hear it
[here, at 0:27](https://youtu.be/QhPK0CE_6uw?si=bxDLsp7rRqXUoHJz&t=27).

## Development

```sh
cargo test
cargo run -- <input>
```
