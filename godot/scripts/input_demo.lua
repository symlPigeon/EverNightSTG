-- input_demo.lua
-- 输入系统验证 demo：
--   方向键控制玩家移动（is_key_pressed）
--   Space/Enter 发射子弹（is_key_just_pressed）
--   子弹 90 tick 后自动消亡，演示 Lifetime 组件
--
-- 需要在 EvernightBridge Inspector 中配置 input_actions：
--   ["ui_left", "ui_right", "ui_up", "ui_down", "ui_accept"]
local TEXTURE = "res://icon.svg"

local PLAYER_SPEED = 30.0 -- 每帧像素速度
local BULLET_SPEED = 60.0 -- 子弹垂直速度（向上）
local BULLET_LIFE = 90 -- 子弹存活 tick 数（60hz ≈ 1.5s）
local PLAYER_SCALE = 1.0
local BULLET_SCALE = 0.4

local initialized = false
local player = nil
local bullets = {} -- entity_id → true，用于帧末清理已消亡子弹的 sprite

-- ── 初始化 ────────────────────────────────────────────────────────────────────

local function init(ctx)
    ctx:log("[input_demo] 初始化 — 操作：方向键移动，Space/Enter 发射")

    player = ctx:spawn()
    ctx:set_component(player, "Transform", {
        x = 0,
        y = 80,
        rotation = 0
    })
    ctx:set_component(player, "Velocity", {
        vx = 0,
        vy = 0,
        angular = 0
    })
    -- create_sprite 在 on_spawn 回调中执行（SpawnCommit 后组件已提交，位置正确）

    initialized = true
end

-- ── 发射子弹 ─────────────────────────────────────────────────────────────────

local function fire_bullet(ctx)
    local pt = ctx:get_component(player, "Transform")
    if not pt then
        return
    end

    local b = ctx:spawn()
    ctx:set_component(b, "Transform", {
        x = pt.x,
        y = pt.y - 30,
        rotation = 0
    })
    ctx:set_component(b, "Velocity", {
        vx = 0,
        vy = -BULLET_SPEED,
        angular = 0
    })
    ctx:set_component(b, "Lifetime", {
        remaining = BULLET_LIFE
    })
    -- create_sprite 在 on_spawn 回调中执行
    bullets[b] = true

    ctx:log("[input_demo] 发射子弹 id=" .. tostring(b))
end

-- ── 每帧逻辑 ──────────────────────────────────────────────────────────────────

function on_frame(ctx)
    -- 延迟初始化（首帧 spawn 不影响碰撞/移动阶段）
    if not initialized then
        init(ctx)
        return
    end

    -- 0. 先清理已消亡子弹（必须在发射新子弹之前）
    -- 原因：lifetime_system 在 step 6 直接 deallocate ID；若本帧发射新子弹
    -- 且分配器复用了同一 ID，is_alive() 会对新实体返回 true，导致老 canvas
    -- item 永远无法销毁，产生残影。
    local dead = {}
    for b, _ in pairs(bullets) do
        if not ctx:is_alive(b) then
            ctx:destroy_sprite(b)
            dead[#dead + 1] = b
        end
    end
    for _, b in ipairs(dead) do
        bullets[b] = nil
    end

    -- 1. 读取移动输入（held）
    local vx, vy = 0, 0
    if ctx:is_key_pressed("ui_left") then
        vx = vx - PLAYER_SPEED
    end
    if ctx:is_key_pressed("ui_right") then
        vx = vx + PLAYER_SPEED
    end
    if ctx:is_key_pressed("ui_up") then
        vy = vy - PLAYER_SPEED
    end
    if ctx:is_key_pressed("ui_down") then
        vy = vy + PLAYER_SPEED
    end

    -- 移动时轻微旋转
    local angular = 0
    if vx ~= 0 then
        angular = vx * 0.04
    end
    ctx:set_component(player, "Velocity", {
        vx = vx,
        vy = vy,
        angular = angular
    })

    -- 2. 发射（just_pressed）
    if ctx:is_key_just_pressed("ui_accept") then
        fire_bullet(ctx)
    end

    -- 3. 松开 Space 时记录（演示 just_released）
    if ctx:is_key_just_released("ui_accept") then
        ctx:log("[input_demo] ui_accept 释放")
    end

    -- 4. 同步 Transform → 渲染层
    local player_tf = ctx:get_component(player, "Transform")
    if player_tf then
        ctx:update_sprite(player, player_tf.x, player_tf.y, player_tf.rotation, PLAYER_SCALE, PLAYER_SCALE)
    end

    for b, _ in pairs(bullets) do
        local btf = ctx:get_component(b, "Transform")
        if btf then
            ctx:update_sprite(b, btf.x, btf.y, btf.rotation, BULLET_SCALE, BULLET_SCALE)
        end
    end
end

-- ── 生命周期回调 ──────────────────────────────────────────────────────────────

function on_spawn(ctx, entity)
    -- SpawnCommit 已完成、Movement 也已运行，直接读 Transform 即可
    local tf = ctx:get_component(entity, "Transform")
    if not tf then
        return
    end

    if entity == player then
        ctx:create_sprite(entity, TEXTURE, 0)
        ctx:update_sprite(entity, tf.x, tf.y, tf.rotation, PLAYER_SCALE, PLAYER_SCALE)
    elseif bullets[entity] then
        ctx:create_sprite(entity, TEXTURE, 1)
        ctx:update_sprite(entity, tf.x, tf.y, tf.rotation, BULLET_SCALE, BULLET_SCALE)
    end
    ctx:log("[input_demo] spawned entity " .. tostring(entity))
end

function on_lifetime_expired(ctx, entity) ctx:log("[input_demo] 子弹消亡 id=" .. tostring(entity)) end
