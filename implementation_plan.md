# Implementation Plan: Native DCC Application in Rust using `wgpu`

This plan details the design of a native, high-performance Substance Painter-inspired Digital Content Creation (DCC) application in Rust, located in `/Users/js36fe/Developer/aidemos/dcc`.

The application will use the **`wgpu`** crate for modern GPU-accelerated 3D/2D rendering and the **`winit`** crate (or `sdl3` if preferred) for windowing, event handling, and cross-platform native context creation.

---

## Native Architecture & Design Decisions

To achieve high-performance interactive rendering and texture painting on macOS, we will build a single native desktop application using a clean Rust/WGPU architecture:

1. **Windowing & Events**: Handled by `winit` (the standard windowing library for Rust graphics). This provides smooth window resizing, high-dpi handling, native mouse/keyboard event loops, and input coordinate tracking.
2. **Dual-Pass GPU Renderer**:
   - **Pass 1: 3D Viewport**: Renders the 3D model (e.g. a sphere or cube) in the center of the window using a 3D WebGPU pipeline. Features dynamic PBR-lite lighting, camera orbit/pan/zoom control, and material textures.
   - **Pass 2: 2D UI Overlay**: Renders all panels, menus, and controls directly on top of and around the viewport using a dedicated 2D WebGPU pipeline. UI elements (panels, borders, buttons, sliders, text) will be drawn using textured and colored 2D quads.
3. **Texture Painting & Raycasting**:
   - When the user drags the brush over the 3D viewport, a ray is projected from the camera through the mouse coordinates.
   - The ray intersects the 3D mesh, resolving to a specific `(u, v)` texture coordinate using the Möller-Trumbore ray-triangle intersection algorithm on the CPU.
   - The paint stroke is drawn directly onto a CPU-side texture buffer and uploaded to the GPU texture using `queue.write_texture` in real-time.
4. **Dockable/Configurable Panel Layout**:
   - The layout is defined by a coordinate grid that calculates bounding boxes for the top menu, left toolbar, right properties/layers panels, and bottom asset shelf.
   - Panels can be hidden, shown, or collapsed via UI buttons.

---

## User Review Required

> [!IMPORTANT]
> **Rust Environment & Toolchain**:
> - Running this application requires a working Rust installation (Cargo/rustc).
> - We will use standard pure Rust crates (`wgpu`, `winit`, `glam`, `bytemuck`, `image`, `pollster`) which compile natively and statically on macOS without external dynamic library dependencies (unlike SDL3 which would require `brew install sdl3`).
> - If you strongly prefer to use `sdl3` via Rust bindings instead of `winit`, let us know! (Note: `winit` is the industry standard for Rust-WGPU applications and requires no local C/C++ library pre-installation).

**Project Directory**: The project will be built in the `aidemos/dcc` directory: `/Users/js36fe/Developer/aidemos/dcc`.

---

## Open Questions

> [!NOTE]
> 1. **Windowing Backend Preference**: Do you prefer **`winit`** (pure Rust, easiest to compile, standard for WGPU) or **`sdl3`** (requires local SDL3 library setup)?
> 2. **3D Mesh Object**: We will provide a default procedural 3D Sphere/Cube mesh for painting. Do you have a custom 3D model (e.g. in `.obj` or `.gltf` format) you would like to load instead?
> 3. **Font File**: For rendering text in the native WebGPU interface, we will bundle a basic font atlas texture. If you want a specific font style (e.g. Inter or Roboto), let us know.

---

## Proposed Changes

We will create the files under `/Users/js36fe/Developer/aidemos/dcc`.

### 1. Main Entry & Orchestration

#### [NEW] [Cargo.toml](file:///Users/js36fe/Developer/aidemos/dcc/Cargo.toml)
- Declares dependencies:
  ```toml
  [package]
  name = "dcc-painter"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  wgpu = "0.20.0"
  winit = "0.29.10"
  glam = "0.25.0"         # Linear algebra library (fast vector/matrix math)
  bytemuck = { version = "1.16.0", features = ["derive"] } # Safe casting of structs to bytes
  image = "0.25.1"        # For texture loading/saving
  pollster = "0.3.0"      # For running async functions in main
  log = "0.4.21"
  env_logger = "0.11.3"
  ```

#### [NEW] [main.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/main.rs)
- Entry point of the app. Initializes logger, initializes the `winit` event loop, and launches the application state.
- Orchestrates window event polling and forwards inputs to the renderer or painter.

#### [NEW] [app.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/app.rs)
- Represents the core application state.
- Stores WGPU context (device, queue, surface, configuration).
- Holds state for the viewport (3D), UI system (2D), painter (layers/colors/size), and window dimensions.

---

### 2. Viewport & 3D WebGPU Engine

#### [NEW] [viewport.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/viewport.rs)
- Configures the 3D WGPU render pipeline:
  - Shaders (WGSL) for 3D PBR-lite rendering.
  - Depth buffer texture for 3D occlusion testing.
  - Mesh buffers (vertices, indices).
- Manages camera state (orbit angle, distance, target position, zoom) and calculates the Model-View-Projection (MVP) matrix.
- Binds the dynamic painted texture to the 3D model material.

#### [NEW] [raycast.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/raycast.rs)
- Implements ray-mesh intersection.
- Converts mouse coordinates to normalized device coordinates, then projects a ray into world space.
- Tests the ray against the mesh's triangles to find the exact UV coordinate of the cursor hit.

#### [NEW] [shaders/3d_mesh.wgsl](file:///Users/js36fe/Developer/aidemos/dcc/src/shaders/3d_mesh.wgsl)
- WGSL shader for 3D mesh rendering. Computes ambient and diffuse shading based on directional lighting, normal mapping, and blending layer textures.

---

### 3. Native UI Engine

#### [NEW] [ui_system.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/ui_system.rs)
- Manages layout regions (collapsible left, right, top, bottom panels).
- Renders 2D elements: colored rectangles, panel borders, buttons, sliders, and checkboxes.
- Implements a text renderer using a font character atlas.
- Renders the layout using a 2D WebGPU pipeline with orthographic projection.

#### [NEW] [shaders/2d_ui.wgsl](file:///Users/js36fe/Developer/aidemos/dcc/src/shaders/2d_ui.wgsl)
- WGSL shader for drawing 2D quads (either textured for text/icons or solid color for panels/buttons).

---

### 4. Painting & Layer Manager

#### [NEW] [painter.rs](file:///Users/js36fe/Developer/aidemos/dcc/src/painter.rs)
- Manages the paint canvas (e.g. 1024x1024 pixel array).
- Manages the layer stack:
  - Adds layers, toggles visibility, adjusts layer opacity.
  - Performs real-time blend operations (normal, multiply, add) to compose layers, and uploads the final texture map to the GPU.
- Performs paint operations (brush stamp with opacity decay, soft edges) on active layer pixels at UV locations.

#### [NEW] [assets/](file:///Users/js36fe/Developer/aidemos/dcc/assets/)
- Contains the font atlas image file (`font_atlas.png`).
- Stores texture files for materials and brushes.

---

## Verification Plan

### Automated / Syntax Check
- Verify Cargo builds successfully:
  ```bash
  cargo check
  ```

### Manual Verification
1. **Launch**: Run `cargo run`. Check that the native window opens.
2. **DCC Interface**: Verify the layout: top menu bar, left tools toolbar, center 3D viewport, right panels (layers, properties, display), and bottom asset shelf.
3. **Orbit & Pan**: Use Left-Click + Drag inside the center viewport to rotate the 3D model. Use Right-Click + Drag to pan, and Scroll Wheel to zoom.
4. **3D Paint**: Click the Brush tool on the left toolbar. Drag the cursor over the 3D model. Verify colored strokes appear on the model's surface in real-time.
5. **UI Interaction**:
   - Click UI buttons to toggle panel visibility (Window menu).
   - Adjust the Brush Size slider in the properties panel and verify the paint stroke size changes.
   - Adjust lighting rotation and intensity sliders in the Display settings to check real-time shading updates.
   - Add/remove layers in the Layer Stack, and toggle layer visibility to confirm composition.
6. **Export**: Trigger `File -> Export Textures` to verify composed PNGs write to disk.
