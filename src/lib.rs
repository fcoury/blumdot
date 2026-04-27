use anyhow::{Context, Result, anyhow, bail};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, imageops::FilterType};
use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;
const CELL_ASPECT_RATIO: f32 = 0.5;
const TRIGONOMETRY_EPSILON: f32 = 1e-6;
const MAX_ANIMATION_FRAMES: u32 = 10_000;
const ANSI_HIDE_CURSOR: &str = "\x1b[?25l";
const ANSI_SHOW_CURSOR: &str = "\x1b[?25h";
const ANSI_CLEAR_SCREEN: &str = "\x1b[2J";
const ANSI_HOME: &str = "\x1b[H";
const ANSI_CLEAR_TO_END: &str = "\x1b[J";
const TRANSPARENT_BLANK: Rgba<u8> = Rgba([255, 255, 255, 0]);
const BACKGROUND_DISTANCE_THRESHOLD: f32 = 36.0;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLayout {
    pub columns: u32,
    pub rows: u32,
    pub sample_width: u32,
    pub sample_height: u32,
}

impl RenderOptions {
    pub fn with_width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationOptions {
    pub render_options: RenderOptions,
    pub degree_step: f32,
    pub frame_delay: Duration,
    pub loop_animation: bool,
}

impl Default for AnimationOptions {
    fn default() -> Self {
        Self {
            render_options: RenderOptions::default(),
            degree_step: 10.0,
            frame_delay: Duration::from_millis(50),
            loop_animation: true,
        }
    }
}

struct LoadedImage {
    bytes: Vec<u8>,
    hint: Option<String>,
    resources_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ExtractedLayer {
    pub name: String,
    pub image: DynamicImage,
}

pub fn render_source(source: InputSource, options: RenderOptions) -> Result<String> {
    let loaded = load_source(source)?;
    let image = decode_image(&loaded)?;
    Ok(render_image(&image, options))
}

pub fn decode_image_bytes(bytes: &[u8], hint: Option<&str>) -> Result<DynamicImage> {
    let loaded = LoadedImage {
        bytes: bytes.to_vec(),
        hint: hint.map(str::to_owned),
        resources_dir: None,
    };

    decode_image(&loaded)
}

pub fn extract_layers(image: &DynamicImage, options: RenderOptions) -> Vec<ExtractedLayer> {
    let source = image.to_rgba8();
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return vec![ExtractedLayer {
            name: "Artwork".to_owned(),
            image: DynamicImage::ImageRgba8(source),
        }];
    }

    let background = border_background_color(&source, options.alpha_cutoff);
    let mut background_like = vec![false; (width * height) as usize];
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found_content = false;

    for y in 0..height {
        for x in 0..width {
            let index = pixel_index(width, x, y);
            let pixel = source.get_pixel(x, y);
            let is_background = pixel_matches_background(pixel, background, options.alpha_cutoff);
            background_like[index] = is_background;

            if !is_background {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found_content = true;
            }
        }
    }

    if !found_content {
        return vec![ExtractedLayer {
            name: "Artwork".to_owned(),
            image: DynamicImage::ImageRgba8(source),
        }];
    }

    let mut artwork = ImageBuffer::from_pixel(width, height, TRANSPARENT_BLANK);
    for y in 0..height {
        for x in 0..width {
            if !background_like[pixel_index(width, x, y)] {
                artwork.put_pixel(x, y, *source.get_pixel(x, y));
            }
        }
    }

    let border_connected = flood_background_from_edges(width, height, &background_like);
    let mut glyphs = ImageBuffer::from_pixel(width, height, TRANSPARENT_BLANK);
    let mut found_glyphs = false;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let index = pixel_index(width, x, y);
            if background_like[index] && !border_connected[index] {
                if let Some(pixel) = nearest_content_pixel(&source, &background_like, x, y) {
                    artwork.put_pixel(x, y, pixel);
                }
                glyphs.put_pixel(
                    x,
                    y,
                    Rgba([background[0], background[1], background[2], 255]),
                );
                found_glyphs = true;
            }
        }
    }

    let mut layers = vec![ExtractedLayer {
        name: "Cloud".to_owned(),
        image: DynamicImage::ImageRgba8(artwork),
    }];

    if found_glyphs {
        layers.push(ExtractedLayer {
            name: "Prompt glyphs > _".to_owned(),
            image: DynamicImage::ImageRgba8(glyphs),
        });
    }

    layers
}

fn nearest_content_pixel(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    background_like: &[bool],
    x: u32,
    y: u32,
) -> Option<Rgba<u8>> {
    let (width, height) = image.dimensions();
    let max_radius = width.max(height);

    for radius in 1..=max_radius {
        let min_x = x.saturating_sub(radius);
        let min_y = y.saturating_sub(radius);
        let max_x = (x + radius).min(width.saturating_sub(1));
        let max_y = (y + radius).min(height.saturating_sub(1));
        let mut closest: Option<(u64, Rgba<u8>)> = None;

        for candidate_y in min_y..=max_y {
            for candidate_x in min_x..=max_x {
                if candidate_x != min_x
                    && candidate_x != max_x
                    && candidate_y != min_y
                    && candidate_y != max_y
                {
                    continue;
                }

                if background_like[pixel_index(width, candidate_x, candidate_y)] {
                    continue;
                }

                let distance_x = i64::from(candidate_x) - i64::from(x);
                let distance_y = i64::from(candidate_y) - i64::from(y);
                let distance = (distance_x * distance_x + distance_y * distance_y) as u64;
                let pixel = *image.get_pixel(candidate_x, candidate_y);
                if closest
                    .map(|(closest_distance, _)| distance < closest_distance)
                    .unwrap_or(true)
                {
                    closest = Some((distance, pixel));
                }
            }
        }

        if let Some((_, pixel)) = closest {
            return Some(pixel);
        }
    }

    None
}

pub fn render_animation_frame(
    image: &DynamicImage,
    options: RenderOptions,
    rotation_degrees: f32,
) -> String {
    let image = prepare_animation_image(image, options);
    let (layout_width, layout_height) = image.dimensions();
    let rotated = rotate_about_center_fixed(&image, options.rotation_degrees + rotation_degrees);

    render_image_with_layout(
        &rotated,
        RenderOptions {
            rotation_degrees: 0.0,
            ..options
        },
        layout_width,
        layout_height,
    )
}

pub fn animate_source<W: Write>(
    source: InputSource,
    options: AnimationOptions,
    writer: &mut W,
) -> Result<()> {
    let frame_count = animation_frame_count(options.degree_step)?;
    let loaded = load_source(source)?;
    let image = decode_image(&loaded)?;

    let mut terminal = AnimationTerminal::start(writer)?;
    loop {
        for frame_index in 0..frame_count {
            let output = render_animation_frame(
                &image,
                options.render_options,
                options.degree_step * frame_index as f32,
            );

            terminal.draw_frame(&output)?;

            if !options.frame_delay.is_zero() {
                std::thread::sleep(options.frame_delay);
            }
        }

        if !options.loop_animation {
            break;
        }
    }

    Ok(())
}

pub fn render_image(image: &DynamicImage, options: RenderOptions) -> String {
    let rotated;
    let image = if should_rotate(options.rotation_degrees) {
        rotated = rotate_about_center(image, options.rotation_degrees);
        &rotated
    } else {
        image
    };
    let (layout_width, layout_height) = image.dimensions();

    render_image_with_layout(image, options, layout_width, layout_height)
}

pub fn render_image_ansi(image: &DynamicImage, options: RenderOptions) -> String {
    let rotated;
    let image = if should_rotate(options.rotation_degrees) {
        rotated = rotate_about_center(image, options.rotation_degrees);
        &rotated
    } else {
        image
    };
    let (layout_width, layout_height) = image.dimensions();

    render_image_ansi_with_layout(image, options, layout_width, layout_height)
}

pub fn render_image_visible(image: &DynamicImage, options: RenderOptions) -> String {
    let rotated;
    let image = if should_rotate(options.rotation_degrees) {
        rotated = rotate_about_center(image, options.rotation_degrees);
        &rotated
    } else {
        image
    };
    let (layout_width, layout_height) = image.dimensions();

    render_image_visible_with_layout(image, options, layout_width, layout_height)
}

pub fn render_layout(image: &DynamicImage, options: RenderOptions) -> RenderLayout {
    let (layout_width, layout_height) = image.dimensions();
    render_layout_for_dimensions(layout_width, layout_height, options)
}

pub fn render_layout_for_dimensions(
    layout_width: u32,
    layout_height: u32,
    options: RenderOptions,
) -> RenderLayout {
    let columns = options.width.max(1);
    let aspect = layout_height as f32 / layout_width.max(1) as f32;
    let base_rows = ((columns as f32 * aspect * CELL_ASPECT_RATIO).round() as u32).max(1);

    match options.glyph_mode {
        GlyphMode::Braille => RenderLayout {
            columns,
            rows: base_rows,
            sample_width: columns * 2,
            sample_height: base_rows * 4,
        },
        GlyphMode::Solid => RenderLayout {
            columns,
            rows: base_rows * 2,
            sample_width: columns * 2,
            sample_height: base_rows * 4,
        },
    }
}

fn render_image_with_layout(
    image: &DynamicImage,
    options: RenderOptions,
    layout_width: u32,
    layout_height: u32,
) -> String {
    let layout = render_layout_for_dimensions(layout_width, layout_height, options);
    let resized = image
        .resize_exact(layout.sample_width, layout.sample_height, FilterType::Triangle)
        .to_rgba8();

    match options.glyph_mode {
        GlyphMode::Braille => render_braille_grid(&resized, layout.columns, layout.rows, options),
        GlyphMode::Solid => render_solid_grid(&resized, layout.columns, layout.rows, options),
    }
}

fn render_image_ansi_with_layout(
    image: &DynamicImage,
    options: RenderOptions,
    layout_width: u32,
    layout_height: u32,
) -> String {
    let layout = render_layout_for_dimensions(layout_width, layout_height, options);
    let resized = image
        .resize_exact(layout.sample_width, layout.sample_height, FilterType::Triangle)
        .to_rgba8();

    match options.glyph_mode {
        GlyphMode::Braille => render_braille_grid_ansi(&resized, layout.columns, layout.rows, options),
        GlyphMode::Solid => render_solid_grid_ansi(&resized, layout.columns, layout.rows, options),
    }
}

fn render_image_visible_with_layout(
    image: &DynamicImage,
    options: RenderOptions,
    layout_width: u32,
    layout_height: u32,
) -> String {
    let layout = render_layout_for_dimensions(layout_width, layout_height, options);
    let resized = image
        .resize_exact(layout.sample_width, layout.sample_height, FilterType::Triangle)
        .to_rgba8();

    match options.glyph_mode {
        GlyphMode::Braille => render_braille_grid_visible(&resized, layout.columns, layout.rows, options),
        GlyphMode::Solid => render_solid_grid_visible(&resized, layout.columns, layout.rows, options),
    }
}

fn animation_frame_count(degree_step: f32) -> Result<u32> {
    if !degree_step.is_finite() || degree_step == 0.0 {
        bail!("animation degree step must be non-zero and finite");
    }

    let frame_count = (360.0 / degree_step.abs()).ceil() as u32;
    if frame_count > MAX_ANIMATION_FRAMES {
        bail!("animation degree step would produce more than {MAX_ANIMATION_FRAMES} frames");
    }

    Ok(frame_count.max(1))
}

struct AnimationTerminal<'writer, W: Write> {
    writer: &'writer mut W,
}

impl<'writer, W: Write> AnimationTerminal<'writer, W> {
    fn start(writer: &'writer mut W) -> Result<Self> {
        write!(writer, "{ANSI_HIDE_CURSOR}{ANSI_CLEAR_SCREEN}{ANSI_HOME}")?;
        writer.flush()?;
        Ok(Self { writer })
    }

    fn draw_frame(&mut self, output: &str) -> Result<()> {
        write!(self.writer, "{ANSI_HOME}{ANSI_CLEAR_TO_END}{output}")?;
        self.writer.flush()?;
        Ok(())
    }
}

impl<W: Write> Drop for AnimationTerminal<'_, W> {
    fn drop(&mut self) {
        let _ = write!(self.writer, "{ANSI_SHOW_CURSOR}");
        let _ = self.writer.flush();
    }
}

fn should_rotate(degrees: f32) -> bool {
    degrees.is_finite() && degrees.rem_euclid(360.0).abs() > f32::EPSILON
}

pub fn rotate_image_in_canvas(image: &DynamicImage, degrees: f32) -> DynamicImage {
    if !should_rotate(degrees) {
        return image.clone();
    }

    let source = image.to_rgba8();
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return DynamicImage::ImageRgba8(source);
    }

    let radians = degrees.to_radians();
    let sin = snap_unit(radians.sin());
    let cos = snap_unit(radians.cos());
    let center_x = (width as f32 - 1.0) / 2.0;
    let center_y = (height as f32 - 1.0) / 2.0;
    let mut output = ImageBuffer::from_pixel(width, height, TRANSPARENT_BLANK);

    for output_y in 0..height {
        for output_x in 0..width {
            let centered_x = output_x as f32 - center_x;
            let centered_y = output_y as f32 - center_y;
            let source_x = centered_x * cos + centered_y * sin + center_x;
            let source_y = -centered_x * sin + centered_y * cos + center_y;
            let source_x = source_x.round() as i32;
            let source_y = source_y.round() as i32;

            if source_x >= 0 && source_y >= 0 && source_x < width as i32 && source_y < height as i32
            {
                output.put_pixel(
                    output_x,
                    output_y,
                    *source.get_pixel(source_x as u32, source_y as u32),
                );
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

fn rotate_about_center(image: &DynamicImage, degrees: f32) -> DynamicImage {
    rotate_about_center_with_canvas(image, degrees, rotation_canvas_for_angle)
}

fn rotate_about_center_fixed(image: &DynamicImage, degrees: f32) -> DynamicImage {
    rotate_about_center_with_canvas(image, degrees, fixed_rotation_canvas)
}

fn rotate_about_center_with_canvas(
    image: &DynamicImage,
    degrees: f32,
    canvas_for: fn(u32, u32, f32, f32) -> RotationCanvas,
) -> DynamicImage {
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
    let canvas = canvas_for(width, height, sin, cos);
    let mut output = ImageBuffer::from_pixel(canvas.width, canvas.height, TRANSPARENT_BLANK);

    for output_y in 0..canvas.height {
        for output_x in 0..canvas.width {
            let rotated_x = output_x as f32 + canvas.min_x;
            let rotated_y = output_y as f32 + canvas.min_y;
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

fn prepare_animation_image(image: &DynamicImage, options: RenderOptions) -> DynamicImage {
    let source = image.to_rgba8();
    let Some(bounds) = ink_bounds(&source, options) else {
        return DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1, 1, TRANSPARENT_BLANK));
    };

    let mut output = ImageBuffer::from_pixel(bounds.width, bounds.height, TRANSPARENT_BLANK);
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            let pixel = source.get_pixel(bounds.x + x, bounds.y + y);
            if pixel_is_ink(pixel, options) {
                output.put_pixel(x, y, *pixel);
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn ink_bounds(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    options: RenderOptions,
) -> Option<PixelBounds> {
    let (width, height) = image.dimensions();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found_ink = false;

    for y in 0..height {
        for x in 0..width {
            if pixel_is_ink(image.get_pixel(x, y), options) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found_ink = true;
            }
        }
    }

    found_ink.then_some(PixelBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RotationCanvas {
    min_x: f32,
    min_y: f32,
    width: u32,
    height: u32,
}

fn rotation_canvas_for_angle(width: u32, height: u32, sin: f32, cos: f32) -> RotationCanvas {
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

    RotationCanvas {
        min_x,
        min_y,
        width: output_width,
        height: output_height,
    }
}

fn fixed_rotation_canvas(width: u32, height: u32, _sin: f32, _cos: f32) -> RotationCanvas {
    let source_center_x = (width as f32 - 1.0) / 2.0;
    let source_center_y = (height as f32 - 1.0) / 2.0;
    let radius = (source_center_x.powi(2) + source_center_y.powi(2)).sqrt();
    let side = (radius * 2.0).ceil() as u32 + 1;
    let min = -radius;

    RotationCanvas {
        min_x: min,
        min_y: min,
        width: side,
        height: side,
    }
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

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    (y * width + x) as usize
}

fn border_background_color(image: &ImageBuffer<Rgba<u8>, Vec<u8>>, alpha_cutoff: u8) -> [u8; 3] {
    let (width, height) = image.dimensions();
    let mut red = 0u64;
    let mut green = 0u64;
    let mut blue = 0u64;
    let mut count = 0u64;

    for y in 0..height {
        for x in [0, width.saturating_sub(1)] {
            let [r, g, b, alpha] = image.get_pixel(x, y).0;
            if alpha >= alpha_cutoff {
                red += u64::from(r);
                green += u64::from(g);
                blue += u64::from(b);
                count += 1;
            }
        }
    }

    for x in 0..width {
        for y in [0, height.saturating_sub(1)] {
            let [r, g, b, alpha] = image.get_pixel(x, y).0;
            if alpha >= alpha_cutoff {
                red += u64::from(r);
                green += u64::from(g);
                blue += u64::from(b);
                count += 1;
            }
        }
    }

    if count == 0 {
        return [255, 255, 255];
    }

    [
        (red / count) as u8,
        (green / count) as u8,
        (blue / count) as u8,
    ]
}

fn pixel_matches_background(pixel: &Rgba<u8>, background: [u8; 3], alpha_cutoff: u8) -> bool {
    let [red, green, blue, alpha] = pixel.0;
    if alpha < alpha_cutoff {
        return true;
    }

    let red_delta = f32::from(red) - f32::from(background[0]);
    let green_delta = f32::from(green) - f32::from(background[1]);
    let blue_delta = f32::from(blue) - f32::from(background[2]);
    (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt()
        <= BACKGROUND_DISTANCE_THRESHOLD
}

fn flood_background_from_edges(width: u32, height: u32, background_like: &[bool]) -> Vec<bool> {
    let mut connected = vec![false; (width * height) as usize];
    let mut queue = VecDeque::new();

    for x in 0..width {
        queue_background_pixel(width, x, 0, background_like, &mut connected, &mut queue);
        queue_background_pixel(
            width,
            x,
            height.saturating_sub(1),
            background_like,
            &mut connected,
            &mut queue,
        );
    }

    for y in 0..height {
        queue_background_pixel(width, 0, y, background_like, &mut connected, &mut queue);
        queue_background_pixel(
            width,
            width.saturating_sub(1),
            y,
            background_like,
            &mut connected,
            &mut queue,
        );
    }

    while let Some((x, y)) = queue.pop_front() {
        if x > 0 {
            queue_background_pixel(width, x - 1, y, background_like, &mut connected, &mut queue);
        }
        if x + 1 < width {
            queue_background_pixel(width, x + 1, y, background_like, &mut connected, &mut queue);
        }
        if y > 0 {
            queue_background_pixel(width, x, y - 1, background_like, &mut connected, &mut queue);
        }
        if y + 1 < height {
            queue_background_pixel(width, x, y + 1, background_like, &mut connected, &mut queue);
        }
    }

    connected
}

fn queue_background_pixel(
    width: u32,
    x: u32,
    y: u32,
    background_like: &[bool],
    connected: &mut [bool],
    queue: &mut VecDeque<(u32, u32)>,
) {
    let index = pixel_index(width, x, y);
    if background_like[index] && !connected[index] {
        connected[index] = true;
        queue.push_back((x, y));
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

fn render_braille_grid_ansi(
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
            output.push_str(&render_braille_cell_ansi(image, cell_x, cell_y, options));
        }
    }

    output
}

fn render_braille_grid_visible(
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
            output.push(render_braille_cell_visible(image, cell_x, cell_y, options));
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

fn render_solid_grid_ansi(
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
            output.push_str(&render_solid_cell_ansi(image, cell_x, cell_y, options));
        }
    }

    output
}

fn render_solid_grid_visible(
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
            output.push(render_solid_cell_visible(image, cell_x, cell_y, options));
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

fn render_braille_cell_ansi(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> String {
    let mut mask = 0u8;
    let mut color = ColorAccumulator::default();

    for y in 0..4 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 4 + y);
            if pixel_is_visible(pixel, options) {
                mask |= braille_bit(x, y);
                color.add(pixel);
            }
        }
    }

    if mask == 0 {
        " ".to_owned()
    } else {
        color.paint(char::from_u32(0x2800 + u32::from(mask)).expect("valid braille mask"))
    }
}

fn render_braille_cell_visible(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> char {
    let mut mask = 0u8;

    for y in 0..4 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 4 + y);
            if pixel_is_visible(pixel, options) {
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

fn render_solid_cell_ansi(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> String {
    let mut mask = 0u8;
    let mut color = ColorAccumulator::default();

    for y in 0..2 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 2 + y);
            if pixel_is_visible(pixel, options) {
                mask |= quadrant_bit(x, y);
                color.add(pixel);
            }
        }
    }

    let glyph = quadrant_block(mask);
    if glyph == ' ' {
        " ".to_owned()
    } else {
        color.paint(glyph)
    }
}

fn render_solid_cell_visible(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cell_x: u32,
    cell_y: u32,
    options: RenderOptions,
) -> char {
    let mut mask = 0u8;

    for y in 0..2 {
        for x in 0..2 {
            let pixel = image.get_pixel(cell_x * 2 + x, cell_y * 2 + y);
            if pixel_is_visible(pixel, options) {
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

fn pixel_is_visible(pixel: &Rgba<u8>, options: RenderOptions) -> bool {
    pixel.0[3] >= options.alpha_cutoff
}

#[derive(Default)]
struct ColorAccumulator {
    red: u32,
    green: u32,
    blue: u32,
    weight: u32,
}

impl ColorAccumulator {
    fn add(&mut self, pixel: &Rgba<u8>) {
        let [red, green, blue, alpha] = pixel.0;
        let weight = u32::from(alpha).max(1);
        self.red += u32::from(red) * weight;
        self.green += u32::from(green) * weight;
        self.blue += u32::from(blue) * weight;
        self.weight += weight;
    }

    fn paint(&self, glyph: char) -> String {
        if self.weight == 0 {
            return glyph.to_string();
        }

        let red = self.red / self.weight;
        let green = self.green / self.weight;
        let blue = self.blue / self.weight;
        format!("\x1b[38;2;{red};{green};{blue}m{glyph}\x1b[0m")
    }
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
