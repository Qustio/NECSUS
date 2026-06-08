# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build              # debug build
cargo build --release    # release build
cargo run -p game        # run the game
cargo check -p engine    # fast type-check engine only
cargo clippy --workspace # lint
```

No test suite exists yet.

## Workspace Structure

Two-crate workspace:
- **`engine/`** — the engine library
- **`game/`** — the game binary that wires modules together

`engine/src/_modules/` contains archived/experimental code (prefixed `_` so it doesn't compile). Do not treat it as active code.

## Architecture

### Engine Lifecycle

```
Engine::new()
  .import::<CoreModule>()       ← time, event registry
  .import::<WindowModule>()     ← winit window, input
  .import::<RendererModule>()   ← Vulkan init, render loop
  .run()                        ← starts winit event loop
```

`run()` builds Shipyard workloads from registered systems, then hands control to winit's `ApplicationHandler`:
- `resumed()` → runs `Startup` workload (creates window, initializes Vulkan)
- `about_to_wait()` → runs `PreUpdate → Update → PostUpdate → Render` each frame, then clears event queues
- `exiting()` → runs `Cleanup` workload

`RendererModule` inserts a `Render` state between `PostUpdate` and `Cleanup`.

### Module Pattern

Modules are the plugin system. Each implements:
```rust
pub trait Module {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn Error>>;
}
```

Inside `build`, modules push `System` entries onto `engine.systems` and can call `engine.register_event::<T>()`. Systems carry a state label (which workload they join), an optional `.label()` for ordering, and `.before()`/`.after()` constraints.

### ECS (Shipyard)

- **Uniques** are global singletons added with `world.add_unique(...)`. Accessed in systems as `UniqueView<T>` / `UniqueViewMut<T>`.
- **Components** live on entities. Current active components: `Transform`, `MeshHandle`, `MaterialHandle`.
- **Events** use `EventQueue<T>` uniques, cleared each frame by `EventRegistry`.

### Renderer Module Structure

Located in `engine/src/modules/renderer/`. All Vulkan objects follow RAII: each wrapper holds an `Arc` of its dependency to enforce drop order, and implements `Drop` to destroy the Vulkan handle.

Drop order that must be preserved (outermost destroyed first):
```
Allocator → Device → Surface / DebugMsg → Instance
```

In `VulkanContext`, `instance` must be the **last** field so it drops last.

**Vulkan wrapper pattern** — `derive_more::Deref` gives transparent access to the inner ash type:
```rust
#[derive(derive_more::Deref)]
pub struct Device {
    #[deref]
    device: ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub graphics_queue_index: u32,
    pub graphics_queue: Mutex<vk::Queue>,
    instance: Arc<Instance>,
}
```

**Renderer systems** are split into `render_record` (label `"Record"`) and `render_submit` (runs `.after("Record")`). Parallel render passes each get their own `Arc<CommandPool>` + `vk::CommandBuffer` (stored in `FrameCommands`) so Shipyard can run them concurrently without sharing a Unique.

**Key Uniques registered by RendererModule:**
- `VulkanContext` — Instance, DebugMsg, Surface, Device, Allocator
- `Swapchain` — swapchain handle, images, image views, format, extent
- `FrameSync` — per-frame semaphores, fences, current frame index, acquired image index
- `CommandContext` — `Vec<FrameCommands>` indexed by `current_frame`
- `MaterialManager` — material registry + pipeline cache
- `MeshAssetManager` — loaded mesh data (vertex/index buffers)

### Vulkan API Choices

- Vulkan 1.3 — dynamic rendering and synchronization2 are used (core, no extensions needed, but must be opted in via `PhysicalDeviceVulkan13Features` in device creation)
- VMA (`vk-mem`) for all image and buffer allocations
- `queue_submit2` / `cmd_pipeline_barrier2` for submit and barriers
- No render passes or framebuffers — dynamic rendering only

### Material System

Located in `engine/src/modules/renderer/material/`.

**Material trait** (`material/mod.rs`):
```rust
pub trait Material: Send + Sync {
    fn create_pipeline(&self) -> Result<Vec<(PassId, Pipeline)>, ...>;
    fn bind(&self, cmd: &FrameCommand, pipeline: &Pipeline, ctx: &BindContext);
}
```

**BindContext** — per-draw data passed from the pass to `bind()`:
```rust
pub struct BindContext<'a> {
    pub transform: &'a Transform,
    pub camera: &'a Camera,
    pub extent: &'a vk::Extent2D,
    pub frame_id: usize,
}
```

**MaterialManager** (Unique) — stores materials and a pipeline cache:
- `register(name, material)` — creates all pipelines eagerly, caches by `(name, PassId)`
- `get_pipeline(name, PassId)` — looks up cached pipeline
- `get_material(name)` — looks up material for bind call

**PassId** — enum identifying which pass a pipeline is for:
```rust
pub enum PassId {
    Geometry,
    // Shadow,
    // Lighting,
}
```

**MaterialHandle** — component on entities, string key into MaterialManager.

**StandardMaterial** (`material/standart.rs`) — vertex pipeline, push constants (model+view+proj as `DrawConstants`).

**Main pass recording loop** (`pass/main.rs`):
```
for (mesh, transform, mat_handle) in entities:
    get pipeline from material_manager
    cmd_bind_pipeline
    material.bind(cmd, pipeline, &BindContext { transform, camera, extent, frame_id })
    cmd_bind_vertex_buffers / cmd_bind_index_buffer
    cmd_draw_indexed
```

### What Still Needs To Be Done

#### Immediate / Near-term

1. **Uniform Buffer Objects (UBOs)** — move `view` and `proj` matrices out of push constants into a per-frame UBO. Requires:
   - `Buffer::uniform()` constructor in `buffer.rs`
   - Descriptor pool + descriptor set layout + descriptor sets (one per frame-in-flight)
   - Update pipeline layout to include set 0
   - Update `main.slang` shader to read from UBO

2. **Texture / image assets** — `texture_assets` (or `image_assets`) manager for GPU image uploads, needed by materials that use albedo/normal maps.

3. **Pipeline cache** — currently pipelines are created eagerly on `register()`. No caching across identical pipeline configurations yet.

#### Architecture — Render World (Deferred)

The current `BindContext` approach is a pragmatic workaround. The proper solution (as Bevy implements) is a **render world**:

- Two separate Shipyard worlds: `main_world` (game logic) and `render_world` (GPU data only)
- Each frame: **extract** phase copies `Transform`, `Camera`, `MeshHandle`, `MaterialHandle` from main world → render world
- Rendering code holds `&render_world` freely and passes it to `material.bind()` — no `BindContext` needed, materials query whatever they want
- Enables render world to run on a separate thread (pipelining: render frame N while simulating frame N+1)

**Why deferred**: significant architectural investment. Current `BindContext` works fine until material data requirements diverge enough to make it painful.

#### Future / Exploratory

- **Mesh shaders** (`VK_EXT_mesh_shader`) — for metaball/procedural geometry. Current dev GPU (Intel UHD 630) does not support it. Lavapipe (software) can be used for API development via registry entry:
  ```
  HKLM\SOFTWARE\Khronos\Vulkan\Drivers → path\to\lvp_icd.x86_64.json = 0
  ```
  Select lavapipe device by name check (`"llvmpipe"` / `"lavapipe"`) in physical device selection.
- **Tessellation shaders** — supported on Intel UHD 630, right tool for smooth subdivision surfaces (Catmull-Clark style).
- **Compute shader metaballs** — marching cubes on GPU → `vkCmdDrawIndirect` from storage buffer. Does not require mesh shaders.
- **Deferred rendering / G-buffer** — geometry pass writes albedo/normal/depth, lighting pass resolves. PassId enum already designed for this.
