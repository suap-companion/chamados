# Ambiente de build do núcleo Rust para Android (cargo-ndk + alvos). O NDK é montado em /ndk.
FROM rust:latest
RUN rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android \
    && cargo install cargo-ndk --locked
ENV ANDROID_NDK_HOME=/ndk
