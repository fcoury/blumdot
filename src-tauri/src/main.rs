#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::{Engine as _, engine::general_purpose};
use blumdot::{
    GlyphMode, RenderOptions, decode_image_bytes, extract_layers, render_image,
    render_image_ansi,
    render_layout_for_dimensions, rotate_image_in_canvas,
};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;

const TRANSPARENT_BLANK: Rgba<u8> = Rgba([255, 255, 255, 0]);
const MAX_FRAME_COUNT: u32 = 240;

#[derive(Debug, Deserialize)]
struct ImagePayload {
    data_url: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GuiRenderOptions {
    width: u32,
    threshold: u8,
    invert: bool,
    alpha_cutoff: u8,
    glyph_mode: String,
    color_mode: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LayerEffect {
    kind: String,
    degrees_per_frame: f32,
}

#[derive(Debug, Serialize)]
struct LayerDto {
    id: String,
    name: String,
    data_url: String,
    visible: bool,
    opacity: f32,
    effect: LayerEffect,
}

#[derive(Debug, Serialize)]
struct EffectSuggestionDto {
    name: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct ImportedProjectDto {
    width: u32,
    height: u32,
    layers: Vec<LayerDto>,
    suggested_effects: Vec<EffectSuggestionDto>,
}

#[derive(Debug, Deserialize)]
struct RenderLayerPayload {
    id: String,
    data_url: String,
    visible: bool,
    opacity: f32,
    effect: LayerEffect,
    frame_data_urls: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PreviewRequest {
    width: u32,
    height: u32,
    layers: Vec<RenderLayerPayload>,
    options: GuiRenderOptions,
    frame_count: u32,
}

#[derive(Debug, Deserialize)]
struct LayoutRequest {
    width: u32,
    height: u32,
    options: GuiRenderOptions,
}

#[derive(Debug, Serialize)]
struct RenderLayoutDto {
    columns: u32,
    rows: u32,
    sample_width: u32,
    sample_height: u32,
}

struct DecodedLayer {
    base_image: DynamicImage,
    frame_images: HashMap<u32, DynamicImage>,
    visible: bool,
    opacity: f32,
    effect: LayerEffect,
}

#[tauri::command]
async fn import_image(
    payload: ImagePayload,
    options: GuiRenderOptions,
) -> Result<ImportedProjectDto, String> {
    tauri::async_runtime::spawn_blocking(move || import_image_blocking(payload, options))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn render_layout(request: LayoutRequest) -> RenderLayoutDto {
    let layout = render_layout_for_dimensions(
        request.width,
        request.height,
        request.options.into_render_options(),
    );

    RenderLayoutDto {
        columns: layout.columns,
        rows: layout.rows,
        sample_width: layout.sample_width,
        sample_height: layout.sample_height,
    }
}

fn import_image_blocking(
    payload: ImagePayload,
    options: GuiRenderOptions,
) -> Result<ImportedProjectDto, String> {
    let bytes = decode_data_url(&payload.data_url)?;
    let hint = payload
        .name
        .as_deref()
        .and_then(|name| name.rsplit('.').next());
    let image = decode_image_bytes(&bytes, hint).map_err(|error| error.to_string())?;
    let render_options = options.into_render_options();
    let layers = extract_layers(&image, render_options)
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            let data_url = encode_png_data_url(&layer.image)?;
            Ok(LayerDto {
                id: format!("layer-{index}"),
                name: layer.name,
                data_url,
                visible: true,
                opacity: 1.0,
                effect: LayerEffect {
                    kind: if index == 0 {
                        "rotate".to_owned()
                    } else {
                        "none".to_owned()
                    },
                    degrees_per_frame: if index == 0 { 10.0 } else { 0.0 },
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ImportedProjectDto {
        width: image.width(),
        height: image.height(),
        layers,
        suggested_effects: suggested_effects(),
    })
}

#[tauri::command]
async fn render_preview_frames(request: PreviewRequest) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || render_preview_frames_blocking(request))
        .await
        .map_err(|error| error.to_string())?
}

fn render_preview_frames_blocking(request: PreviewRequest) -> Result<Vec<String>, String> {
    let monochrome = request.options.color_mode == "monochrome";
    let options = request.options.into_render_options();
    let frame_count = request.frame_count.clamp(1, MAX_FRAME_COUNT);
    let width = request.width.max(1);
    let height = request.height.max(1);
    let decoded_layers = request
        .layers
        .into_iter()
        .filter(|layer| layer.visible)
        .map(decode_layer)
        .collect::<Result<Vec<_>, String>>()?;

    let mut frames = Vec::with_capacity(frame_count as usize);
    for frame_index in 0..frame_count {
        let mut canvas = ImageBuffer::from_pixel(width, height, TRANSPARENT_BLANK);
        for layer in &decoded_layers {
            if !layer.visible {
                continue;
            }

            let source = layer
                .frame_images
                .get(&frame_index)
                .unwrap_or(&layer.base_image);
            let transformed = match layer.effect.kind.as_str() {
                "rotate" => rotate_image_in_canvas(
                    source,
                    layer.effect.degrees_per_frame * frame_index as f32,
                ),
                _ => source.clone(),
            };
            composite_layer(&mut canvas, &transformed, layer.opacity);
        }

        let frame = if monochrome {
            render_image(&DynamicImage::ImageRgba8(canvas), options)
        } else {
            render_image_ansi(&DynamicImage::ImageRgba8(canvas), options)
        };
        frames.push(frame);
    }

    Ok(frames)
}

fn decode_layer(layer: RenderLayerPayload) -> Result<DecodedLayer, String> {
    let base_image = decode_png_data_url(&layer.data_url)?;
    let mut frame_images = HashMap::new();
    for (frame, data_url) in layer.frame_data_urls {
        let frame_index = frame
            .parse::<u32>()
            .map_err(|error| format!("invalid frame override for layer {}: {error}", layer.id))?;
        frame_images.insert(frame_index, decode_png_data_url(&data_url)?);
    }

    Ok(DecodedLayer {
        base_image,
        frame_images,
        visible: layer.visible,
        opacity: layer.opacity.clamp(0.0, 1.0),
        effect: layer.effect,
    })
}

fn decode_png_data_url(data_url: &str) -> Result<DynamicImage, String> {
    let bytes = decode_data_url(data_url)?;
    image::load_from_memory(&bytes).map_err(|error| error.to_string())
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, encoded)| encoded)
        .unwrap_or(data_url);

    general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("failed to decode image data: {error}"))
}

fn encode_png_data_url(image: &DynamicImage) -> Result<String, String> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(bytes)
    ))
}

fn composite_layer(
    canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    layer: &DynamicImage,
    opacity: f32,
) {
    let layer = layer.to_rgba8();
    let width = canvas.width().min(layer.width());
    let height = canvas.height().min(layer.height());
    let opacity = opacity.clamp(0.0, 1.0);

    for y in 0..height {
        for x in 0..width {
            let source = layer.get_pixel(x, y).0;
            let source_alpha = (f32::from(source[3]) / 255.0) * opacity;
            if source_alpha <= f32::EPSILON {
                continue;
            }

            let destination = canvas.get_pixel(x, y).0;
            let destination_alpha = f32::from(destination[3]) / 255.0;
            let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
            if output_alpha <= f32::EPSILON {
                continue;
            }

            let mut output = [0u8; 4];
            for channel in 0..3 {
                let source_channel = f32::from(source[channel]);
                let destination_channel = f32::from(destination[channel]);
                output[channel] = ((source_channel * source_alpha
                    + destination_channel * destination_alpha * (1.0 - source_alpha))
                    / output_alpha)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            output[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            canvas.put_pixel(x, y, Rgba(output));
        }
    }
}

fn suggested_effects() -> Vec<EffectSuggestionDto> {
    [
        ("Pulse", "Scale a layer subtly on beats or selected frames."),
        ("Drift", "Move a layer across a short vector path."),
        ("Reveal", "Wipe or type-on dots from left to right."),
        (
            "Shimmer",
            "Modulate threshold noise for glowing terminal texture.",
        ),
    ]
    .into_iter()
    .map(|(name, description)| EffectSuggestionDto {
        name: name.to_owned(),
        description: description.to_owned(),
    })
    .collect()
}

impl GuiRenderOptions {
    fn into_render_options(self) -> RenderOptions {
        RenderOptions {
            width: self.width.max(1),
            threshold: self.threshold,
            invert: self.invert,
            alpha_cutoff: self.alpha_cutoff,
            glyph_mode: if self.glyph_mode == "solid" {
                GlyphMode::Solid
            } else {
                GlyphMode::Braille
            },
            rotation_degrees: 0.0,
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            import_image,
            render_layout,
            render_preview_frames
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
