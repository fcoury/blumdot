set dotenv-load

app := "src-tauri/target/debug/bundle/macos/Blumdot.app"

alias c := dev
alias i := install

install:
    npm install

default:
    just --list

build:
    npm run build

check:
    npm run build
    cargo check --manifest-path src-tauri/Cargo.toml

test:
    cargo test

fmt:
    cargo fmt

desktop:
    npm run desktop

bundle:
    npm run desktop:bundle

open:
    open {{app}}

raw:
    npm run desktop:raw

dev:
    npm run tauri dev

run sample="assets/codex-color.png" degrees="10":
    cargo run -- animate {{sample}} {{degrees}}
