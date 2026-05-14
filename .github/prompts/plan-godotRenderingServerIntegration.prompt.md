# Plan: Godot 4 RenderingServer 图形系统集成

## TL;DR
从零创建 Godot 4 项目 + `evernight_godot` GDExtension cdylib。ECS 跑在独立线程，通过 mpsc channel 生产 `RenderCommand`，Godot 主线程的 `_process()` 消费命令并调用 `RenderingServer`，彻底避免跨线程访问 RenderingServer。Lua 脚本通过新增的 `ctx:create_sprite/update_sprite/destroy_sprite` API 动态控制渲染对象。

---

## 阶段 0 — 工作区 & Godot 项目骨架

**目标**：能编译的 cdylib，Godot 编辑器能加载扩展。

1. `Cargo.toml` workspace.members 加入 `"crates/evernight_godot"`
2. 新建 `crates/evernight_godot/Cargo.toml`：`crate-type = ["cdylib"]`，deps: `godot = "0.2"` + `evernight_lua` + `evernight_runtime` + `evernight_core`
3. `src/lib.rs` — 最小 ExtensionLibrary 入口（`#[gdextension] unsafe impl ExtensionLibrary for EvernightExt {}`）
4. 创建 Godot 项目：
   - `godot/project.godot`（最小文本配置）
   - `godot/addons/evernight/evernight.gdextension`（指向 `target/debug/evernight_godot.dll`）
   - `godot/scenes/main.tscn`（含 EvernightBridge 节点）

---

## 阶段 1 — ECS 线程桥 + 渲染命令管道

**目标**：ECS 线程运行，render channel 建立，`_process` 能收到命令（先打日志验证）。

5. **`render_cmd.rs`**（放在 `evernight_lua` crate 内，无 Godot 依赖）定义 `RenderCommand` enum：

```rust
pub enum RenderCommand {
    CreateSprite    { handle: u64, texture_path: String, z_index: i32 },
    UpdateTransform { handle: u64, x: f32, y: f32, rotation: f32, scale_x: f32, scale_y: f32 },
    SetVisible      { handle: u64, visible: bool },
    SetModulate     { handle: u64, r: f32, g: f32, b: f32, a: f32 },
    DestroySprite   { handle: u64 },
}
```

6. **`bridge.rs`**：`EvernightBridge` GodotClass (`Base<Node>`)
   - `ready()`: 创建 `std::sync::mpsc` channel，启动 ECS 线程（拥有 `App` + `LuaEngine`），sender 通过 `engine.set_render_sender(tx)` 注入
   - `process(delta)`: `while let Ok(cmd) = rx.try_recv() { ... }` 处理命令（阶段 1 先 `godot_print!`）
   - `notification(PREDELETE)`: 设置 `Arc<AtomicBool>` shutdown flag，join 线程

ECS 线程结构：
```rust
loop {
    app.step().unwrap();
    if shutdown.load(Ordering::Relaxed) { break; }
}
```

---

## 阶段 2 — Lua 渲染 API（生产侧）

**目标**：Lua 脚本能调用 `ctx` 方法推送 `RenderCommand`。

7. `LuaEngine` 新增字段 `render_tx: Option<Sender<RenderCommand>>` + 方法 `set_render_sender(tx: Sender<RenderCommand>)`
8. `CtxUserdata` 在 `lua.scope()` 内持有 sender 引用，新增 5 个 Lua 方法：
   - `ctx:create_sprite(handle, texture_path, z_index)`
   - `ctx:update_sprite(handle, x, y, rot, sx, sy)`
   - `ctx:set_sprite_visible(handle, visible)`
   - `ctx:set_sprite_modulate(handle, r, g, b, a)`
   - `ctx:destroy_sprite(handle)`

Lua 脚本惯用模式：
```lua
function on_spawn(ctx, eid)
    ctx:create_sprite(eid, "res://bullet.png", 0)
end

function on_frame(ctx)
    for _, eid in ipairs(ctx:iter_entities("Transform")) do
        local t = ctx:get_component(eid, "Transform")
        ctx:update_sprite(eid, t.x, t.y, t.rot, 1, 1)
    end
end

function on_despawn(ctx, eid)
    ctx:destroy_sprite(eid)
end
```

---

## 阶段 3 — RenderingServer 消费侧

**目标**：屏幕上看到精灵。

9. `EvernightBridge` 新增字段：
   - `canvas: Rid`
   - `rid_map: HashMap<u64, Rid>`（handle → canvas item RID）
   - `texture_cache: HashMap<String, Rid>`（path → texture RID）
10. `ready()` 里：`canvas = rs.canvas_create()`，`rs.viewport_attach_canvas(get_viewport_rid(), canvas)`
11. `process()` 逐条处理命令：
    - `CreateSprite`: `canvas_item_create()` → `canvas_item_set_parent(item, canvas)` → 纹理从 cache / `load::<Texture2D>().get_rid()` → `canvas_item_add_texture_rect(item, rect, tex_rid)` → `canvas_item_set_z_index(item, z)` → 存 rid_map
    - `UpdateTransform`: `canvas_item_set_transform(item, Transform2D::...)`
    - `SetVisible`: `canvas_item_set_visible(item, visible)`
    - `SetModulate`: `canvas_item_set_modulate(item, color)`
    - `DestroySprite`: `canvas_item_clear(item)` + `free_rid(item)` + 移出 rid_map
12. `PREDELETE` 清理：free_rid 所有 canvas item + canvas

---

## 阶段 4 — 2D 光照 / 粒子（后续迭代）

- `RenderCommand::CreateLight2D { handle, color, energy, radius }` → `canvas_light_create`
- 粒子：先用 Godot `GPUParticles2D` 节点封装，或 ECS 内做软粒子后批量提交

---

## 相关文件

| 文件 | 内容 |
|---|---|
| `Cargo.toml` | 加 `evernight_godot` 成员 |
| `crates/evernight_godot/Cargo.toml` | cdylib + godot + evernight_lua deps |
| `crates/evernight_godot/src/lib.rs` | `#[gdextension]` 入口 |
| `crates/evernight_godot/src/bridge.rs` | `EvernightBridge` Node |
| `crates/evernight_godot/src/render_cmd.rs` | re-export `RenderCommand` |
| `crates/evernight_lua/src/render_cmd.rs` | `RenderCommand` 定义（无 Godot dep） |
| `crates/evernight_lua/src/engine.rs` | 加 `render_tx` + Lua render API |
| `godot/project.godot` | Godot 4 项目 |
| `godot/addons/evernight/evernight.gdextension` | 扩展描述 |
| `godot/scenes/main.tscn` | 含 EvernightBridge 的主场景 |

---

## 验证步骤

1. `cargo build -p evernight_godot` — 零错误
2. Godot 4 编辑器打开 `godot/` 项目，扩展加载无报错，`EvernightBridge` 出现在节点类型列表
3. 写最小 Lua 脚本（on_spawn 创建 sprite，on_frame 更新位置），屏幕上看到精灵随 tick 移动

---

## 决策记录

- `RenderingServer` **只在 Godot 主线程调用**，不使用 `experimental-threads` feature
- `RenderCommand` 定义在 `evernight_lua` crate（无 Godot 依赖），`evernight_godot` re-export 复用
- 纹理用 `load::<Texture2D>()` + `get_rid()` 方式，Godot 资源系统管理生命周期，**无需手动 free_rid**
- **单 canvas + `canvas_item_set_z_index()`** 分层，不用多 canvas
- Lua 用 `entity_id as u64` 作 sprite handle，1:1 映射无需额外表
- 2D 光照和粒子留到阶段 4，不阻塞 MVP

## 待明确

- [ ] ECS 线程步进频率：固定 60 Hz sleep 还是不限速？（建议先不限速，由 FixedStep 控制）
- [ ] Canvas 附加到哪个 Viewport？（建议 `get_tree().get_root().get_viewport_rid()`）
- [ ] 精灵中心点对齐：`canvas_item_add_texture_rect` 的 `rect` 是否需要偏移？
- [ ] 纹理 z-index 范围约定（建议：0 = 背景，100 = 子弹，200 = 玩家，300 = UI）
