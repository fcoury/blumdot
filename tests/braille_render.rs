use blumdot::{
    GlyphMode, RenderOptions, extract_layers, render_animation_frame, render_image,
    render_image_ansi, rotate_image_in_canvas,
};
use image::{DynamicImage, ImageBuffer, Rgba};

#[test]
fn empty_cells_render_as_spaces() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([255, 255, 255, 255])));

    let output = render_image(&image, RenderOptions::default().with_width(1));

    assert_eq!(output, " ");
}

#[test]
fn full_cells_render_as_all_dots() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([0, 0, 0, 255])));

    let output = render_image(&image, RenderOptions::default().with_width(1));

    assert_eq!(output, "\u{28ff}");
}

#[test]
fn solid_full_cells_render_as_stacked_full_blocks() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([0, 0, 0, 255])));

    let output = render_image(
        &image,
        RenderOptions {
            glyph_mode: GlyphMode::Solid,
            ..RenderOptions::default().with_width(1)
        },
    );

    assert_eq!(output, "█\n█");
}

#[test]
fn rotation_happens_before_terminal_aspect_sampling() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([0, 0, 0, 255])));

    let output = render_image(
        &image,
        RenderOptions {
            rotation_degrees: 90.0,
            ..RenderOptions::default().with_width(2)
        },
    );

    assert_eq!(output, "\u{28ff}\u{28ff}");
}

#[test]
fn animation_frame_keeps_terminal_layout_from_source_bounds() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(8, 16, Rgba([0, 0, 0, 255])));

    let output = render_animation_frame(&image, RenderOptions::default().with_width(8), 45.0);
    let lines: Vec<_> = output.lines().collect();

    assert_eq!(lines.len(), 8);
    assert!(lines.iter().all(|line| line.chars().count() == 8));
}

#[test]
fn animation_frame_ignores_blank_source_edges() {
    let mut pixels = ImageBuffer::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
    for x in 0..8 {
        pixels.put_pixel(x, 0, Rgba([255, 255, 255, 255]));
        pixels.put_pixel(x, 7, Rgba([255, 255, 255, 255]));
    }
    for y in 0..8 {
        pixels.put_pixel(0, y, Rgba([255, 255, 255, 255]));
        pixels.put_pixel(7, y, Rgba([255, 255, 255, 255]));
    }
    for y in 3..5 {
        for x in 3..5 {
            pixels.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    let image = DynamicImage::ImageRgba8(pixels);
    let content_only =
        DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([0, 0, 0, 255])));

    let output = render_animation_frame(&image, RenderOptions::default().with_width(8), 45.0);
    let content_output =
        render_animation_frame(&content_only, RenderOptions::default().with_width(8), 45.0);

    assert_eq!(output, content_output);
}

#[test]
fn solid_cells_use_quadrant_block_shapes() {
    let cases = [
        ((0, 0), "▘\n "),
        ((1, 0), "▝\n "),
        ((0, 1), "▖\n "),
        ((1, 1), "▗\n "),
        ((0, 2), " \n▘"),
        ((1, 2), " \n▝"),
        ((0, 3), " \n▖"),
        ((1, 3), " \n▗"),
    ];

    for ((x, y), expected) in cases {
        let mut pixels = ImageBuffer::from_pixel(2, 4, Rgba([255, 255, 255, 255]));
        pixels.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        let image = DynamicImage::ImageRgba8(pixels);

        let output = render_image(
            &image,
            RenderOptions {
                glyph_mode: GlyphMode::Solid,
                ..RenderOptions::default().with_width(1)
            },
        );

        assert_eq!(output, expected, "solid block at ({x}, {y})");
    }
}

#[test]
fn individual_braille_dots_use_standard_dot_order() {
    let cases = [
        ((0, 0), '\u{2801}'),
        ((0, 1), '\u{2802}'),
        ((0, 2), '\u{2804}'),
        ((1, 0), '\u{2808}'),
        ((1, 1), '\u{2810}'),
        ((1, 2), '\u{2820}'),
        ((0, 3), '\u{2840}'),
        ((1, 3), '\u{2880}'),
    ];

    for ((x, y), expected) in cases {
        let mut pixels = ImageBuffer::from_pixel(2, 4, Rgba([255, 255, 255, 255]));
        pixels.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        let image = DynamicImage::ImageRgba8(pixels);

        let output = render_image(&image, RenderOptions::default().with_width(1));

        assert_eq!(output, expected.to_string(), "dot at ({x}, {y})");
    }
}

#[test]
fn transparent_pixels_are_blank() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([0, 0, 0, 0])));

    let output = render_image(&image, RenderOptions::default().with_width(1));

    assert_eq!(output, " ");
}

#[test]
fn ansi_preview_renders_visible_light_pixels_in_color() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([255, 255, 255, 255])));

    let output = render_image_ansi(&image, RenderOptions::default().with_width(1));

    assert_eq!(output, "\x1b[38;2;255;255;255m\u{28ff}\x1b[0m");
}

#[test]
fn invert_reverses_luminance_selection() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 4, Rgba([255, 255, 255, 255])));

    let output = render_image(
        &image,
        RenderOptions {
            invert: true,
            ..RenderOptions::default().with_width(1)
        },
    );

    assert_eq!(output, "\u{28ff}");
}

#[test]
fn square_images_use_terminal_cell_aspect_ratio() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(8, 8, Rgba([0, 0, 0, 255])));

    let output = render_image(&image, RenderOptions::default().with_width(4));
    let lines: Vec<_> = output.lines().collect();

    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.chars().count() == 4));
}

#[test]
fn rendered_lines_do_not_exceed_selected_width() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(18, 12, Rgba([0, 0, 0, 255])));

    for width in [10, 16, 24] {
        let output = render_image(&image, RenderOptions::default().with_width(width));
        let lines: Vec<_> = output.lines().collect();

        assert!(!lines.is_empty(), "width {width} should render at least one line");
        assert!(
            lines.iter().any(|line| !line.trim().is_empty()),
            "width {width} should render visible output",
        );
        assert!(
            lines.iter().all(|line| line.chars().count() <= width as usize),
            "width {width} should cap every rendered line",
        );
    }
}

#[test]
fn layer_extraction_splits_colored_artwork_from_enclosed_light_glyphs() {
    let mut pixels = ImageBuffer::from_pixel(6, 6, Rgba([255, 255, 255, 255]));
    for y in 1..5 {
        for x in 1..5 {
            pixels.put_pixel(x, y, Rgba([56, 96, 240, 255]));
        }
    }
    pixels.put_pixel(2, 2, Rgba([255, 255, 255, 255]));
    pixels.put_pixel(3, 2, Rgba([255, 255, 255, 255]));

    let layers = extract_layers(&DynamicImage::ImageRgba8(pixels), RenderOptions::default());

    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].name, "Cloud");
    assert_eq!(layers[1].name, "Prompt glyphs > _");

    let cloud = layers[0].image.to_rgba8();
    let glyphs = layers[1].image.to_rgba8();
    assert_eq!(cloud.get_pixel(1, 1).0, [56, 96, 240, 255]);
    assert_eq!(cloud.get_pixel(2, 2).0, [56, 96, 240, 255]);
    assert_eq!(glyphs.get_pixel(2, 2).0, [255, 255, 255, 255]);
    assert_eq!(glyphs.get_pixel(1, 1).0[3], 0);
}

#[test]
fn rotate_image_in_canvas_keeps_original_dimensions() {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(8, 12, Rgba([0, 0, 0, 255])));

    let rotated = rotate_image_in_canvas(&image, 45.0);

    assert_eq!(rotated.width(), 8);
    assert_eq!(rotated.height(), 12);
}
