# Stage 1: Build WASM with Trunk
FROM rust:1.92-bookworm AS builder

# Install Trunk plus the nightly toolchain needed for `build-std` in
# `.cargo/config.toml`, so reference-types can be disabled consistently.
RUN rustup target add wasm32-unknown-unknown
RUN rustup toolchain install nightly --component rust-src
RUN rustup target add wasm32-unknown-unknown --toolchain nightly
RUN cargo install trunk --locked

# Install wasm-opt for size optimization
RUN apt-get update && apt-get install -y binaryen && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies — copy manifests first
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ .cargo/
COPY game_state/Cargo.toml game_state/Cargo.toml
RUN mkdir -p src game_state/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > game_state/src/lib.rs
RUN cargo fetch --target wasm32-unknown-unknown

# Copy source
COPY src/ src/
COPY game_state/ game_state/
COPY index.html .trunk.toml ./

# Copy only the asset subtrees needed by the web build.
COPY assets/fonts/ assets/fonts/
COPY assets/shaders/ assets/shaders/
COPY assets/icons/ assets/icons/
COPY assets/KayKit_Forest_Nature/Assets/gltf/ assets/KayKit_Forest_Nature/Assets/gltf/
COPY assets/UltimateFantasyRTS/glTF/ assets/UltimateFantasyRTS/glTF/
COPY assets/ToonyTinyPeople/models/buildings/ assets/ToonyTinyPeople/models/buildings/
COPY assets/ToonyTinyPeople/models/units/ assets/ToonyTinyPeople/models/units/
COPY assets/ToonyTinyPeople/textures/buildings/ assets/ToonyTinyPeople/textures/buildings/
COPY assets/ToonyTinyPeople/textures/units/ assets/ToonyTinyPeople/textures/units/
COPY assets/KayKit_Skeletons/characters/gltf/ assets/KayKit_Skeletons/characters/gltf/
COPY assets/KayKit_Character_Animations/Animations/gltf/Rig_Medium/ assets/KayKit_Character_Animations/Animations/gltf/Rig_Medium/

# Disable reference-types/externref to avoid browser failures in the
# generated wasm-bindgen bootstrap on stricter WebAssembly runtimes.
ENV WASM_BINDGEN_EXTERNREF=0
ENV RUSTUP_TOOLCHAIN=nightly
ENV RUSTFLAGS="-Ctarget-feature=-reference-types"
RUN trunk build --release --config .trunk.toml

# Stage 2: Serve the built WASM client with nginx.
FROM nginx:alpine

COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 8080

# Nginx config: listen on 8080, serve static files, SPA fallback
RUN printf 'server {\n\
    listen 8080;\n\
    root /usr/share/nginx/html;\n\
    location / {\n\
        try_files $uri $uri/ /index.html;\n\
    }\n\
    location ~* \\.(wasm)$ {\n\
        types { application/wasm wasm; }\n\
    }\n\
}\n' > /etc/nginx/conf.d/default.conf

CMD ["nginx", "-g", "daemon off;"]
