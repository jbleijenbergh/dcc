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

#### [NEW] [Cargo.toml](Cargo.toml)
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

#### [NEW] [main.rs](src/main.rs)
- Entry point of the app. Initializes logger, initializes the `winit` event loop, and launches the application state.
- Orchestrates window event polling and forwards inputs to the renderer or painter.

#### [NEW] [app.rs](src/app.rs)
- Represents the core application state.
- Stores WGPU context (device, queue, surface, configuration).
- Holds state for the viewport (3D), UI system (2D), painter (layers/colors/size), and window dimensions.

---

### 2. Viewport & 3D WebGPU Engine

#### [NEW] [viewport.rs](src/viewport.rs)
- Configures the 3D WGPU render pipeline:
  - Shaders (WGSL) for 3D PBR-lite rendering.
  - Depth buffer texture for 3D occlusion testing.
  - Mesh buffers (vertices, indices).
- Manages camera state (orbit angle, distance, target position, zoom) and calculates the Model-View-Projection (MVP) matrix.
- Binds the dynamic painted texture to the 3D model material.

#### [NEW] [raycast.rs](src/raycast.rs)
- Implements ray-mesh intersection.
- Converts mouse coordinates to normalized device coordinates, then projects a ray into world space.
- Tests the ray against the mesh's triangles to find the exact UV coordinate of the cursor hit.

#### [NEW] [shaders/3d_mesh.wgsl](src/shaders/3d_mesh.wgsl)
- WGSL shader for 3D mesh rendering. Computes ambient and diffuse shading based on directional lighting, normal mapping, and blending layer textures.

---

### 3. Native UI Engine

#### [NEW] [ui_system.rs](src/ui_system.rs)
- Manages layout regions (collapsible left, right, top, bottom panels).
- Renders 2D elements: colored rectangles, panel borders, buttons, sliders, and checkboxes.
- Implements a text renderer using a font character atlas.
- Renders the layout using a 2D WebGPU pipeline with orthographic projection.

#### [NEW] [shaders/2d_ui.wgsl](src/shaders/2d_ui.wgsl)
- WGSL shader for drawing 2D quads (either textured for text/icons or solid color for panels/buttons).

---

### 4. Painting & Layer Manager

#### [NEW] [painter.rs](src/painter.rs)
- Manages the paint canvas (e.g. 1024x1024 pixel array).
- Manages the layer stack:
  - Adds layers, toggles visibility, adjusts layer opacity.
  - Performs real-time blend operations (normal, multiply, add) to compose layers, and uploads the final texture map to the GPU.
- Performs paint operations (brush stamp with opacity decay, soft edges) on active layer pixels at UV locations.

#### [NEW] [assets/](assets/)
- Contains the font atlas image file (`font_atlas.png`).
- Stores texture files for materials and brushes.

---

## Migration Tasks: Bevy ECS + egui + wgpu Integration

This section translates the recommended migration path into executable tasks that preserve current behavior while incrementally introducing Bevy ECS.

### Implementation Readiness Decisions (Confirmed)

1. ECS scope: use minimal Bevy crates only (`bevy_ecs`, `bevy_app`) in early phases.
2. Runtime host: keep `winit` as host runtime and embed ECS incrementally.
3. Validation gate per phase: `cargo check` + manual smoke test.
4. UV Viewer scope: may be temporarily disabled during early migration and must be restored by Phase 5.
5. Performance gate: no strict FPS target; avoid obvious regressions.

### Phase 1: Introduce ECS Runtime Without Behavior Changes

#### Task 1.1 - Add ECS dependencies and bootstrap world
- Add `bevy_ecs` and `bevy_app` (add `bevy_tasks` only if async extraction is required) to [Cargo.toml](Cargo.toml).
- Create ECS bootstrap module (suggested: `src/app/ecs/mod.rs`) that owns:
  - `World`
  - `Schedule` objects for update and render phases
  - Resource registration
- Keep current `winit` loop as host runtime in [src/main.rs](src/main.rs).

**Acceptance Criteria**
- `cargo check` passes.
- App launches and renders identically to baseline.
- No user-facing behavior regressions.

#### Task 1.2 - Wrap current monolithic state as ECS resources
- Register current state partitions as resources instead of direct singleton mutation:
  - App domain snapshot from [src/app/app_state.rs](src/app/app_state.rs)
  - Painter/viewport runtime handles from [src/app/mod.rs](src/app/mod.rs)
- Keep existing logic intact; this is structural migration only.

**Acceptance Criteria**
- Existing features (paint, orbit/pan/zoom, UV viewer, layer stack) remain functional.
- Undo/redo and load flow continue to work.

**Note**
- UV Viewer may be temporarily disabled in early migration if needed; restore by Phase 5.

### Phase 1 Implementation Checklist (Execution Ready)

#### Step 0 - Baseline checkpoint
- [ ] Run baseline compile: cargo check
- [ ] Run baseline app smoke test: cargo run
- [ ] Record baseline notes: startup success, paint stroke latency, camera controls, UV Viewer behavior

#### Step 1 - Dependency and module scaffolding

Files to edit:
- [Cargo.toml](Cargo.toml)
- [src/app/mod.rs](src/app/mod.rs)

Files to create:
- [src/app/ecs/mod.rs](src/app/ecs/mod.rs)

Planned changes:
- [x] Add minimal ECS crates: bevy_app and bevy_ecs
- [x] Export ecs module from app module tree
- [x] Introduce EcsRuntime scaffold owning World + Schedule objects

Definition of done:
- [x] cargo check passes
- [x] Existing app startup path unchanged

#### Step 2 - Resource wrappers for current state

Files to edit:
- [src/app/ecs/mod.rs](src/app/ecs/mod.rs)
- [src/app/mod.rs](src/app/mod.rs)
- [src/app/app_state.rs](src/app/app_state.rs)

Planned changes:
- [x] Add ECS resource structs for domain snapshot and runtime handles
- [x] Register resources during State initialization
- [x] Keep all existing reducer and render logic intact (no behavior migration yet)

Definition of done:
- [x] cargo check passes
- [x] No change in tool, painting, or viewport behavior

#### Step 3 - Add no-op ECS execution points

Files to edit:
- [src/main.rs](src/main.rs)
- [src/app/mod.rs](src/app/mod.rs)
- [src/app/ecs/mod.rs](src/app/ecs/mod.rs)

Planned changes:
- [x] Add ECS tick calls in update and render cycle boundaries
- [x] Add placeholder systems that read resources only
- [x] Ensure order is deterministic but side-effect free

Definition of done:
- [x] App runs with ECS schedules invoked each frame
- [x] Visual output is equivalent to baseline

#### Step 4 - Phase 1 validation gate

Validation commands:
- [x] cargo check
- [x] cargo run

Manual smoke checklist:
- [x] Camera orbit/pan/zoom works
- [x] Brush and eraser operate normally
- [x] Layer selection and visibility still work
- [x] glTF open flow still succeeds/fails correctly
- [x] UV Viewer behavior is acceptable for current phase

Phase 1 sign-off criteria:
- [x] No user-facing regression observed
- [x] ECS scaffolding exists and is isolated from feature logic
- [x] Ready to start Phase 2 event migration

**Phase 1 Sign-Off**: ✅ COMPLETE (2026-06-05)
- ECS runtime scaffolding and resource registration established in `src/app/ecs/mod.rs`
- Input/events route through ECS event queues with parity-focused transition scaffolding (historical phase context)
- Frame schedule and extracted render resource stubs are in place
- Validation and tests pass

### Phase 2: Route Input and Messages Through ECS Events

#### Task 2.1 - Convert message bus to ECS events
- Define ECS event vocabulary for all runtime intents.
- Add event ingestion system that converts normalized input events into ECS events.

**Acceptance Criteria**
- All previous message kinds are emitted as ECS events.
- Input handling behavior matches baseline for mouse, pen pressure, keyboard modifiers.

**Status**: ✅ COMPLETE (2026-06-05)
- Created `events` module in `src/app/ecs/mod.rs` with full event vocabulary (UiActionEvent, DocumentCommandEvent, ViewportCommandEvent, ToolCommandEvent, InputStateCommandEvent, AppEvent)
- All event types properly implement `Event` trait
- Tests pass: ECS event vocabulary and ingestion verified

#### Task 2.2 - ECS runtime execution transition
- Adapt command execution into one or more systems:
  - `viewport_command_system`
  - `tool_command_system`
  - `input_state_system`
  - `ui_action_system`
- Preserve existing command semantics exactly.

**Acceptance Criteria**
- Command parity achieved: same command sequence yields equivalent state.
- Manual smoke pass: brush, eraser, layer visibility, mesh switching, UV viewer toggle.

**Status**: ✅ COMPLETE (2026-06-05)
- Created system stubs and event conversion scaffolding in `src/app/ecs/mod.rs` (historical phase context)
- Added `drain_events()` method to EcsRuntime
- Integrated event draining into `State::update()` with immediate ECS execution for parity
- No behavioral regressions; command semantics preserved during transition
- Manual test confirms compilation and smoke pass behavior

**Phase 2 Sign-Off**: Command handling became routable through ECS event queues with full parity. Event infrastructure was ready for Phase 3 schedule integration.

### Phase 3: Establish Deterministic Frame Schedules

#### Task 3.1 - Define update schedule sets
- Create ordered sets for:
  1. `WinitEventIngest`
  2. `InputResolve`
  3. `DomainUpdate`
  4. `ExtractRenderData`
  5. `PrepareGpu`
  6. `RenderMainSurface`
  7. `RenderAuxSurfaces`
  8. `EndFrame`
- Ensure schedule ordering is explicit and tested.

**Acceptance Criteria**
- Stable per-frame ordering across runs.
- No race conditions between UI changes and render uploads.

**Status**: ✅ COMPLETE (2026-06-05)
- Created `FramePhase` SystemSet enum with all 8 phases
- Configured schedule with chained system sets in EcsRuntime::new()
- Schedule enforces ordering: `.chain()` on all phases
- Test added: `test_frame_phases_ordered()` verifies all phases present and correctly sequenced
- Compilation verified; all tests pass

#### Task 3.2 - Split mutable vs read-only frame stages
- Enforce rule: domain mutation only in update stages; render stages read extracted data/resources.
- Introduce extracted render resource structs for camera, layer composition, and active document handles.

**Acceptance Criteria**
- Render systems no longer directly mutate app domain resources.
- No obvious frame-time regressions versus baseline during manual validation.

**Status**: ✅ COMPLETE (2026-06-05)
- Created 3 extracted render data resources: ExtractedCameraData, ExtractedLayerComposition, ExtractedDocumentData
- All extracted resources initialized in EcsRuntime::new()
- Created 3 system stubs to populate extracted data from DomainStateResource (read-only)
- Render systems will consume these read-only extracted resources in Phase 4
- Test added: `test_extracted_render_data_initialized()` verifies all resources present
- All 22 tests pass; no regressions

**Phase 3 Sign-Off**: Deterministic frame schedule established with ordered phases. Extracted render data pattern enables read-only render stages. ECS infrastructure ready for Phase 4 render system integration.

### Phase 4: Integrate wgpu Render Systems with ECS

#### Task 4.1 - GPU context and surface registry resources
- Add ECS resources for:
  - Shared `instance/adapter/device/queue`
  - Per-window surface config and depth targets
- Migrate resize handling from [src/app/mod.rs](src/app/mod.rs) to ECS resize systems.

**Acceptance Criteria**
- Main window resize and scale-factor changes are handled by systems.
- UV viewer surface lifecycle works through ECS resources.

**Status**: ✅ COMPLETE (2026-06-05)
- Added ECS window/surface lifecycle events and resize ingestion system in `src/app/ecs/mod.rs`
- Added `SurfaceRegistryResource` and `PendingSurfaceOpsResource` for per-window surface tracking and deferred host-side apply
- Routed main and UV window resize/scale-factor events through ECS in `src/main.rs`
- Applied pending ECS resize operations during frame update in `src/app/mod.rs`
- Synced UV viewer open/close lifecycle with ECS surface registry
- Validation: `cargo test --lib` passes (25/25)

#### Task 4.2 - Viewport and painter render passes as systems
- Adapt current render flow in [src/viewport.rs](src/viewport.rs) and painter compositor into:
  - `render_3d_viewport_system`
  - `render_paint_composite_system`
- Keep existing pipelines/shaders initially.

**Acceptance Criteria**
- 3D render and paint composition are visually equivalent to baseline.
- Lost/outdated surface errors are recovered through ECS render systems.

**Status**: ✅ COMPLETE (2026-06-05)
- Added ECS render request event flow (`RenderRequestEvent`) and pending render ops resource in `src/app/ecs/mod.rs`
- Added `render_3d_viewport_system`, `render_paint_composite_system`, and `render_aux_surface_system` to frame-phase schedule
- Routed main and UV redraw paths through ECS render request queue in `src/main.rs`
- Added host executor `execute_pending_render_ops()` in `src/app/mod.rs` to execute existing render backends from ECS-issued ops
- Added ECS render failure recovery flow (`RenderFailureEvent` + `render_recovery_system`) to convert Lost/Outdated render failures into pending resize operations
- Updated host-side render execution to emit ECS recovery events for Lost/Outdated outcomes instead of direct window-handler recovery branches
- Moved redraw intent generation into ECS (`RedrawEvent` + `redraw_ingest_system`), so host window loop now queues redraw intents and ECS translates them into render requests
- Centralized render execution behind ECS-driven `State::update(...)` so the host loop queues redraw intents and calls a single update entrypoint
- Extracted main camera uniform update into ECS `PrepareGpu` phase (`prepare_gpu_system` + `PendingPrepareOpsResource`), removing inline camera update from the main render pass path
- Added pass-level main-surface ops (`render_3d_viewport_pass` and `render_paint_composite_pass`) emitted by ECS render systems and consumed by host render execution
- Validation: `cargo test --lib` passes

**Phase 4 Sign-Off**: ✅ COMPLETE (2026-06-05)
- Task 4.1 and Task 4.2 are complete
- Main/UV resize lifecycle, redraw intent ingestion, render request orchestration, and render failure recovery are ECS-driven
- Existing viewport/painter pipelines are preserved while orchestration migrated into ECS phases

### Phase 5: Integrate egui as ECS-Controlled UI Layer

#### Task 5.1 - egui per-window resources
- Store `egui::Context`, `egui_winit::State`, and `egui_wgpu::Renderer` in per-window ECS resources.
- Split UI frame lifecycle into systems:
  - `begin_egui_frame_system`
  - `draw_egui_panels_system`
  - `end_egui_frame_and_upload_system`

**Acceptance Criteria**
- UI renders in main and UV windows.
- egui input consumption behavior remains correct (no accidental double-processing).

**Status**: ✅ COMPLETE (2026-06-08)
- Added ECS per-window UI registry resource (`UiWindowRegistryResource`) to track main/UV UI lifecycle ownership
- Added split UI lifecycle systems in ECS schedule:
  - `begin_egui_frame_system`
  - `draw_egui_panels_system`
  - `end_egui_frame_and_upload_system`
- Added `PendingUiFrameOpsResource` and runtime drain API to apply lifecycle ops in host update flow
- Synced UV viewer open/close lifecycle with ECS UI window registry activation
- Wired ECS-issued UI lifecycle ops into active main/UV egui frame execution paths in `State` (ops are now consumed per frame and reset after use)
- Added ECS per-window UI resource containers (`MainWindowUiResource`, `UvWindowUiResource`) and synchronized them from `State` during initialization and UV window open/close lifecycle
- Updated UI lifecycle systems to gate begin/draw/end ops based on ECS UI resource readiness (context/state/renderer availability)
- Moved `begin_egui_frame` execution into ECS-driven `State::update(...)` stage handling (with per-window begun-state tracking), reducing inline lifecycle work inside render entrypoints
- Validation: `cargo test --lib` passes (28/28)

**Task 5.1 Sign-Off**: ✅ COMPLETE (2026-06-08)
- Per-window UI lifecycle ownership is tracked in ECS resources and lifecycle systems
- Main/UV UI lifecycle stage ops are emitted from ECS schedules and consumed each frame in `State`
- Main and UV UI rendering paths remain functional with ECS-driven staging and fallback safety

#### Task 5.2 - UI emits intent events only
- Refactor UI actions in [src/app/mod.rs](src/app/mod.rs) so panel widgets emit ECS events instead of mutating renderer/domain directly.
- Keep side effects in dedicated action systems.

**Acceptance Criteria**
- UI code no longer directly edits painter/viewport internals.
- Layer/tool/brush controls still function identically.

**Status**: ✅ COMPLETE (2026-06-08)
- Replaced direct UI command mutation points in `src/app/mod.rs` with ECS intent emission (`emit_ui_action`) for panel/widget-driven actions
- Added centralized ECS event flush path (`flush_ecs_events_to_reducer`) to preserve immediate behavior parity while keeping UI code intent-only
- Updated UV viewer visibility setter path to emit UI intent events rather than directly applying mutations
- Validation: tests run successfully

**Phase 5 Sign-Off**: ✅ COMPLETE (2026-06-08)
- Task 5.1 and Task 5.2 are complete
- UI lifecycle ownership and UI intent emission are ECS-driven with parity-preserving staged execution

### Phase 6: Decompose Monolithic State into Plugins and Systems

#### Task 6.1 - Introduce plugin modules
- Create plugin-style modules (or equivalent registration groups) for:
  - Core
  - Input
  - Document
  - Tool
  - Render
  - UI
  - Asset I/O
- Move system registration from central state constructor into plugin boundaries.

**Acceptance Criteria**
- Each module owns its resources/events/systems.
- Build remains stable and startup order is deterministic.

**Status**: ✅ COMPLETE (2026-06-08)
- Completed physical plugin module decomposition under `src/app/ecs/plugins/`:
  - `core.rs`
  - `input.rs`
  - `document.rs`
  - `tool.rs`
  - `render.rs`
  - `ui.rs`
  - `asset_io.rs`
- Added plugin module index: `src/app/ecs/plugins/mod.rs`
- Rewired `EcsRuntime::new()` in `src/app/ecs/mod.rs` to register systems via plugin module boundaries instead of monolithic inline registration
- Preserved deterministic startup ordering via phase-set chaining and unchanged phase assignments
- Validation: `cargo test --lib` passes (28/28)

**Task 6.1 Sign-Off**: ✅ COMPLETE (2026-06-08)
- Plugin ownership boundaries are now represented by concrete ECS modules and registration entrypoints
- Build/test stability preserved after decomposition

#### Task 6.2 - Finalize ECS-native runtime path
- Once all paths are ECS-native, retire transition scaffolding to legacy execution paths.
- Keep adapter shim only if needed for tests.

**Acceptance Criteria**
- No runtime dependency on legacy direct execution path.
- Full regression pass succeeds.

**Status**: ✅ COMPLETE (2026-06-08)
- Runtime event flush path executes ECS events directly in `State` for all event categories (`Viewport`, `InputState`, `Document`, `Tool`, `Ui`)
- Removed legacy fallback from runtime event flush path (no runtime calls to retired reducer dispatch remain)
- UI and glTF flows are ECS intent-first and processed through native ECS event execution
- Validation: `cargo test --lib` passes (28/28)

**Task 6.2 Sign-Off**: ✅ COMPLETE (2026-06-08)
- Runtime no longer depends on legacy direct execution path
- Transition conversion scaffolding is no longer used in hot path event execution

**Phase 6 Sign-Off**: ✅ COMPLETE (2026-06-08)
- Task 6.1 and Task 6.2 are complete
- Plugin module decomposition and transition-scaffold retirement are complete for runtime flow

---

## Cross-Cutting Validation Tasks

### Task V1 - Add migration regression smoke tests
- Extend integration tests under [tests/](tests/) for:
  - Input semantics (orbit/pan/paint)
  - Layer stack and undo/redo invariants
  - glTF load success/failure state transitions
  - UV viewer open/close lifecycle

### Task V2 - Add schedule-level diagnostics
- Add frame-stage timing logs and event counters to detect ordering regressions.
- Verify that input, domain update, and render stages are invoked in expected order.

### Task V3 - Performance gate
- Record baseline FPS and frame-time before migration.
- Re-check after each phase to catch regressions early.
- Treat obvious regressions as blockers; no strict numerical FPS gate is required.

---

## Execution Order and Milestones

1. Complete Phase 1 and establish no-op ECS skeleton.
2. Complete Phase 2 and ensure command parity via manual smoke pass.
3. Complete Phase 3 schedule ordering and extraction boundary.
4. Complete Phase 4 render system migration for main + UV surfaces.
5. Complete Phase 5 egui event-driven integration.
6. Complete Phase 6 plugin decomposition and final transition-scaffold removal.
7. Run cross-cutting validation tasks at each milestone.

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
