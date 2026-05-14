-- demo.lua
-- 基础 ECS + 渲染演示：3 个圆点沿圆形轨道公转，并绕自身自转。
-- 依赖内置组件：Transform { x, y, rotation }、Velocity { vx, vy, angular }
local TEXTURE = "res://icon.svg" -- Godot 项目内置 SVG 图标
local RADIUS = 150 -- 公转半径（像素）
local ORBIT_SPD = 0.02 -- 公转角速度（弧度/帧）
local SPIN_SPD = 0.05 -- 自转角速度

local initialized = false
local handles = {} -- entity_id → 用作 sprite handle（u64 兼容）

-- ── 初始化 ─────────────────────────────────────────────────────────────────
local function init(ctx)
    ctx:log("Evernight demo starting — spawning 3 entities")

    for i = 0, 2 do
        local angle = i * (2 * math.pi / 3)
        local e = ctx:spawn()

        ctx:set_component(e, "Transform", {
            x = math.cos(angle) * RADIUS,
            y = math.sin(angle) * RADIUS,
            rotation = angle
        })
        ctx:set_component(e, "Velocity", {
            vx = -math.sin(angle) * ORBIT_SPD * RADIUS,
            vy = math.cos(angle) * ORBIT_SPD * RADIUS,
            angular = SPIN_SPD
        })

        -- 用 entity_id 当作 sprite handle（u32 → u64 隐式转换）
        ctx:create_sprite(e, TEXTURE, i)
        handles[e] = true
    end

    ctx:log("Init done — " .. #handles .. " sprites created")
    initialized = true
end

-- ── 每帧逻辑 ───────────────────────────────────────────────────────────────
function on_frame(ctx)
    if not initialized then
        init(ctx)
    end

    for e, _ in pairs(handles) do
        local tf = ctx:get_component(e, "Transform")
        if tf then
            -- 把 ECS 中的 Transform 同步到渲染层
            ctx:update_sprite(e, tf.x, tf.y, tf.rotation, 1.0, 1.0)
        end
    end
end

-- ── 生命周期回调 ───────────────────────────────────────────────────────────
function on_spawn(ctx, entity)
    -- 新实体生成时自动打印，方便调试
    ctx:log("spawned entity " .. tostring(entity))
end
