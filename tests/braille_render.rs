use blumdot::{GlyphMode, RenderOptions, render_image};
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
