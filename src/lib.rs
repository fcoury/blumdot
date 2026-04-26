use anyhow::{Context, Result, anyhow, bail};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, imageops::FilterType};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;
const CELL_ASPECT_RATIO: f32 = 0.5;
const TRIGONOMETRY_EPSILON: f32 = 1e-6;

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
pub enum GlyphMode {
    Braille,
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    pub width: u32,
    pub threshold: u8,
    pub invert: bool,
    pub alpha_cutoff: u8,
    pub glyph_mode: GlyphMode,
    pub rotation_degrees: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 40,
            threshold: 180,
            invert: false,
            alpha_cutoff: 16,
            glyph_mode: GlyphMode::Braille,
            rotation_degrees: 0.0,
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
    let rotated;
    let image = if should_rotate(options.rotation_degrees) {
        rotated = rotate_about_center(image, options.rotation_degrees);
        &rotated
    } else {
        image
    };
    let (source_width, source_height) = image.dimensions();
    let aspect = source_height as f32 / source_width.max(1) as f32;
    let rows = ((columns as f32 * aspect * CELL_ASPECT_RATIO).round() as u32).max(1);
    let sample_width = columns * 2;
    let sample_height = rows * 4;
    let resized = image
        .resize_exact(sample_width, sample_height, FilterType::Triangle)
        .to_rgba8();

    match options.glyph_mode {
        GlyphMode::Braille => render_braille_grid(&resized, columns, rows, options),
        GlyphMode::Solid => render_solid_grid(&resized, columns, rows * 2, options),
    }
}

fn should_rotate(degrees: f32) -> bool {
    degrees.is_finite() && degrees.rem_euclid(360.0).abs() > f32::EPSILON
}

fn rotate_about_center(image: &DynamicImage, degrees: f32) -> DynamicImage {
    let source = image.to_rgba8();
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return DynamicImage::ImageRgba8(source);
    }

    let radians = degrees.to_radians();
    let sin = snap_unit(radians.sin());
    let cos = snap_unit(radians.cos());
    let source_center_x = (width as f32 - 1.0) / 2.0;
    let source_center_y = (height as f32 - 1.0) / 2.0;

    let corners = [
        (0.0, 0.0),
        (width as f32 - 1.0, 0.0),
        (0.0, height as f32 - 1.0),
        (width as f32 - 1.0, height as f32 - 1.0),
    ];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for (x, y) in corners {
        let centered_x = x - source_center_x;
        let centered_y = y - source_center_y;
        let rotated_x = centered_x * cos - centered_y * sin;
        let rotated_y = centered_x * sin + centered_y * cos;
        min_x = min_x.min(rotated_x);
        min_y = min_y.min(rotated_y);
        max_x = max_x.max(rotated_x);
        max_y = max_y.max(rotated_y);
    }

    let output_width = (max_x - min_x).ceil() as u32 + 1;
    let output_height = (max_y - min_y).ceil() as u32 + 1;
    let mut output = ImageBuffer::from_pixel(output_width, output_height, Rgba([0, 0, 0, 0]));

    for output_y in 0..output_height {
        for output_x in 0..output_width {
            let rotated_x = output_x as f32 + min_x;
            let rotated_y = output_y as f32 + min_y;
            let source_x = rotated_x * cos + rotated_y * sin + source_center_x;
            let source_y = -rotated_x * sin + rotated_y * cos + source_center_y;

            let source_x = source_x.round() as i32;
            let source_y = source_y.round() as i32;

            if source_x >= 0 && source_y >= 0 && source_x < width as i32 && source_y < height as i32
            {
                let pixel = source.get_pixel(source_x as u32, source_y as u32);
                output.put_pixel(output_x, output_y, *pixel);
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

fn snap_unit(value: f32) -> f32 {
    if value.abs() < TRIGONOMETRY_EPSILON {
        0.0
    } else if (value - 1.0).abs() < TRIGONOMETRY_EPSILON {
        1.0
    } else if (value + 1.0).abs() < TRIGONOMETRY_EPSILON {
        -1.0
    } else {
        value
    }
}

fn render_braille_grid(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    columns: u32,
    rows: u32,
    options: RenderOptions,
) -> String {
    let mut output = String::new();
    for cell_y in 0..rows {
        if cell_y > 0 {
            output.push('\n');
        }

        for cell_x in 0..columns {
            output.push(render_braille_cell(image, cell_x, cell_y, options));
        }
    }

    output
}

fn render_solid_grid(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    columns: u32,
    rows: u32,
    options: RenderOptions,
) -> String {
    let mut output = String::new();
    for cell_y in 0..rows {
        if cell_y > 0 {
            output.push('\n');
        }

        for cell_x in 0..columns {
            output.push(render_solid_cell(image, cell_x, cell_y, options));
        }
    }

    output
}

fn render_braille_cell(
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

fn render_solid_cell(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> char {
    let mut mask = 0u8;
    for y in 0..2 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 2 + y);
            if pixel_is_ink(pixel, options) {
                mask |= quadrant_bit(x, y);
            }
        }
    }

    quadrant_block(mask)
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

fn quadrant_bit(x: u32, y: u32) -> u8 {
    match (x, y) {
        (0, 0) => 0x1,
        (1, 0) => 0x2,
        (0, 1) => 0x4,
        (1, 1) => 0x8,
        _ => 0,
    }
}

fn quadrant_block(mask: u8) -> char {
    match mask {
        0x0 => ' ',
        0x1 => '▘',
        0x2 => '▝',
        0x3 => '▀',
        0x4 => '▖',
        0x5 => '▌',
        0x6 => '▞',
        0x7 => '▛',
        0x8 => '▗',
        0x9 => '▚',
        0xa => '▐',
        0xb => '▜',
        0xc => '▄',
        0xd => '▙',
        0xe => '▟',
        _ => '█',
    }
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
