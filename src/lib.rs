use anyhow::{Context, Result, anyhow, bail};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, imageops::FilterType};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;
const CELL_ASPECT_RATIO: f32 = 0.5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputSource {
    Path(PathBuf),
    Url(String),
}

impl InputSource {
    pub fn parse(input: impl AsRef<str>) -> Self {
        let input = input.as_ref();
        if input.starts_with("http://") || input.starts_with("https://") {
            Self::Url(input.to_owned())
        } else {
            Self::Path(PathBuf::from(input))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderOptions {
    pub width: u32,
    pub threshold: u8,
    pub invert: bool,
    pub alpha_cutoff: u8,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 40,
            threshold: 180,
            invert: false,
            alpha_cutoff: 16,
        }
    }
}

impl RenderOptions {
    pub fn with_width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }
}

struct LoadedImage {
    bytes: Vec<u8>,
    hint: Option<String>,
    resources_dir: Option<PathBuf>,
}

pub fn render_source(source: InputSource, options: RenderOptions) -> Result<String> {
    let loaded = load_source(source)?;
    let image = decode_image(&loaded)?;
    Ok(render_image(&image, options))
}

pub fn render_image(image: &DynamicImage, options: RenderOptions) -> String {
    let columns = options.width.max(1);
    let (source_width, source_height) = image.dimensions();
    let aspect = source_height as f32 / source_width.max(1) as f32;
    let rows = ((columns as f32 * aspect * CELL_ASPECT_RATIO).round() as u32).max(1);
    let sample_width = columns * 2;
    let sample_height = rows * 4;
    let resized = image
        .resize_exact(sample_width, sample_height, FilterType::Triangle)
        .to_rgba8();

    let mut output = String::new();
    for cell_y in 0..rows {
        if cell_y > 0 {
            output.push('\n');
        }

        for cell_x in 0..columns {
            output.push(render_cell(&resized, cell_x, cell_y, options));
        }
    }

    output
}

fn render_cell(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> char {
    let mut mask = 0u8;
    for y in 0..4 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 4 + y);
            if pixel_is_ink(pixel, options) {
                mask |= braille_bit(x, y);
            }
        }
    }

    if mask == 0 {
        ' '
    } else {
        char::from_u32(0x2800 + u32::from(mask)).expect("valid braille mask")
    }
}

fn pixel_is_ink(pixel: &Rgba<u8>, options: RenderOptions) -> bool {
    let [red, green, blue, alpha] = pixel.0;
    if alpha < options.alpha_cutoff {
        return false;
    }

    let luminance = (0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue))
        .round() as u8;
    let dark = luminance < options.threshold;
    if options.invert { !dark } else { dark }
}

fn braille_bit(x: u32, y: u32) -> u8 {
    match (x, y) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        (0, 3) => 0x40,
        (1, 3) => 0x80,
        _ => 0,
    }
}

fn load_source(source: InputSource) -> Result<LoadedImage> {
    match source {
        InputSource::Path(path) => load_path(&path),
        InputSource::Url(url) => load_url(&url),
    }
}

fn load_path(path: &Path) -> Result<LoadedImage> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let resources_dir = path.parent().map(Path::to_path_buf);

    Ok(LoadedImage {
        bytes,
        hint: path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_owned),
        resources_dir,
    })
}

fn load_url(url: &str) -> Result<LoadedImage> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to create HTTP client")?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to fetch {url}"))?;

    let hint = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| url.rsplit('.').next().map(str::to_owned));

    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read response from {url}"))?;

    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        bail!("response from {url} exceeded 10 MiB limit");
    }

    Ok(LoadedImage {
        bytes,
        hint,
        resources_dir: None,
    })
}

fn decode_image(loaded: &LoadedImage) -> Result<DynamicImage> {
    if looks_like_svg(&loaded.bytes, loaded.hint.as_deref()) {
        render_svg(&loaded.bytes, loaded.resources_dir.clone())
    } else {
        image::load_from_memory(&loaded.bytes).context("failed to decode image")
    }
}

fn looks_like_svg(bytes: &[u8], hint: Option<&str>) -> bool {
    if hint
        .map(|hint| hint.to_ascii_lowercase().contains("svg"))
        .unwrap_or(false)
    {
        return true;
    }

    let prefix_len = bytes.len().min(1024);
    std::str::from_utf8(&bytes[..prefix_len])
        .map(|prefix| prefix.trim_start().starts_with("<svg") || prefix.contains("<svg"))
        .unwrap_or(false)
}

fn render_svg(bytes: &[u8], resources_dir: Option<PathBuf>) -> Result<DynamicImage> {
    let mut options = resvg::usvg::Options {
        resources_dir,
        ..resvg::usvg::Options::default()
    };
    options.fontdb_mut().load_system_fonts();

    let tree = resvg::usvg::Tree::from_data(bytes, &options).context("failed to parse SVG")?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow!("failed to allocate SVG pixmap"))?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    let rgba = pixmap.take_demultiplied();
    let image = ImageBuffer::from_raw(size.width(), size.height(), rgba)
        .ok_or_else(|| anyhow!("failed to convert SVG pixmap"))?;

    Ok(DynamicImage::ImageRgba8(image))
}
