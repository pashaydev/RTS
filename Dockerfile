# Stage 1: Build WASM with Trunk
FROM rust:1.92-bookworm AS builder

# Install wasm target and trunk
RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --locked

# Install wasm-opt for size optimization
RUN apt-get update && apt-get install -y binaryen && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies — copy manifests first
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch --target wasm32-unknown-unknown

# Copy source
COPY src/ src/
COPY index.html .trunk.toml ./

# Copy only used asset directories (264MB → ~50MB)
COPY assets/fonts/ assets/fonts/
COPY assets/shaders/ assets/shaders/
COPY assets/icons/ assets/icons/
COPY assets/KayKit_Forest_Nature/Assets/gltf/ assets/KayKit_Forest_Nature/Assets/gltf/
COPY assets/UltimateFantasyRTS/glTF/ assets/UltimateFantasyRTS/glTF/
COPY assets/KayKit_Adventurers/Characters/gltf/ assets/KayKit_Adventurers/Characters/gltf/
COPY assets/KayKit_Skeletons/characters/gltf/ assets/KayKit_Skeletons/characters/gltf/
COPY assets/KayKit_Character_Animations/Animations/gltf/Rig_Medium/ assets/KayKit_Character_Animations/Animations/gltf/Rig_Medium/

# Remove unused UltimateFantasyRTS models (keep FirstAge, Houses_SecondAge, Mine, Mountain)
RUN cd assets/UltimateFantasyRTS/glTF/ && \
    find . -name '*SecondAge*' ! -name 'Houses_SecondAge*' -delete && \
    rm -f Barrel.gltf Crate*.gltf Logs.gltf Dock_*.gltf Farm_Dirt_*.gltf \
          TowerHouse_*.gltf Wall_*.gltf WallTowers_*.gltf WonderWalls_*.gltf

# Disable externref to avoid WebAssembly.Table.grow() failures in browsers
ENV WASM_BINDGEN_EXTERNREF=0
RUN trunk build --release --config .trunk.toml

# Stage 2: Serve with nginx
FROM nginx:alpine

# Remove default nginx config
RUN rm /etc/nginx/conf.d/default.conf

# Copy custom nginx config
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Copy built WASM app
COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
