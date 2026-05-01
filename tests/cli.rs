use assert_cmd::Command;
use mockito::Server;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn tiny_png() -> Vec<u8> {
    let image = image::ImageBuffer::from_pixel(2, 4, image::Rgba([0u8, 0, 0, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

#[test]
fn renders_local_png_to_stdout() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg(&path)
        .arg("--width")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{28ff}\n"));
}

#[test]
fn renders_local_png_as_solid_blocks() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg(&path)
        .arg("--width")
        .arg("1")
        .arg("--solid")
        .assert()
        .success()
        .stdout(predicate::str::contains("█\n█\n"));
}

#[test]
fn rotates_local_png_before_rendering() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg(&path)
        .arg("--width")
        .arg("2")
        .arg("--rotate")
        .arg("90")
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{28ff}\u{28ff}\n"));
}

#[test]
fn animates_local_png_with_ansi_redraws() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg("animate")
        .arg(&path)
        .arg("180")
        .arg("--width")
        .arg("1")
        .arg("--frame-delay-ms")
        .arg("0")
        .arg("--no-loop")
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("\x1b[?25l\x1b[2J\x1b[H")
                .and(predicate::str::contains("\x1b[H\x1b[J"))
                .and(predicate::str::contains("\x1b[?25h")),
        );
}

#[test]
fn exports_animation_frames_to_text_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    let export_path = dir.path().join("frames.txt");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg("animate")
        .arg(&path)
        .arg("180")
        .arg("--width")
        .arg("1")
        .arg("--frame-delay-ms")
        .arg("0")
        .arg("--export-file")
        .arg(&export_path)
        .assert()
        .success()
        .stdout("");

    let output = fs::read_to_string(export_path).unwrap();
    assert!(output.contains("--- frame 1/2 rotation 0deg ---"));
    assert!(output.contains("--- frame 2/2 rotation 180deg ---"));
    assert!(!output.contains("\x1b[?25l"));
    assert!(!output.contains("\x1b[2J"));
}

#[test]
fn animate_rejects_zero_degree_step() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg("animate")
        .arg(&path)
        .arg("0")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "animation degree step must be non-zero",
        ));
}

#[test]
fn export_rejects_zero_degree_step() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.png");
    let export_path = dir.path().join("frames.txt");
    fs::write(&path, tiny_png()).unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg("animate")
        .arg(&path)
        .arg("0")
        .arg("--export-file")
        .arg(export_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "animation degree step must be non-zero",
        ));
}

#[test]
fn renders_local_svg_to_stdout() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("logo.svg");
    fs::write(
        &path,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8">
  <rect width="8" height="8" fill="#000000"/>
</svg>"##,
    )
    .unwrap();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg(&path)
        .arg("--width")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{28ff}\n"));
}

#[test]
fn renders_http_image_url_to_stdout() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/logo.png")
        .with_status(200)
        .with_header("content-type", "image/png")
        .with_body(tiny_png())
        .create();

    Command::cargo_bin("blumdot")
        .unwrap()
        .arg(format!("{}/logo.png", server.url()))
        .arg("--width")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{28ff}\n"));

    mock.assert();
}

#[test]
fn missing_file_returns_non_zero_status() {
    Command::cargo_bin("blumdot")
        .unwrap()
        .arg("/definitely/not/a/real/image.png")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read"));
}
