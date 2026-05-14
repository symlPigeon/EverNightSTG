-- benchmark.lua
-- 全链路性能压测：N 个小球在屏幕内随机运动、边界弹射、相互弹性碰撞。
-- 可调参数见 CONFIG。
-- ── 配置 ─────────────────────────────────────────────────────────────────────
local NUM_BALLS = 2000 -- 小球数量（试试 100 / 500 / 1000）
local BALL_R = 5 -- 球半径（像素，碰撞体半径）
local SPRITE_SIZE = 32 -- icon.svg 原始尺寸（像素），用于计算缩放
local BALL_SCALE = (BALL_R * 2) / SPRITE_SIZE -- 让精灵直径 = 2×BALL_R
local SPEED = 50 -- 初始速度绝对值
local HW = 590 -- 世界半宽（1280/2 - 50 边距）
local HH = 300 -- 世界半高（720/2  - 60 边距）
local LOG_EVERY = 300 -- 每隔多少帧打印一次日志

-- ── 状态 ─────────────────────────────────────────────────────────────────────
local initialized = false
local balls = {} -- entity_id → true（用 pairs 遍历）
local frame_count = 0

-- 碰撞去重：同一帧内同一对球只处理一次弹性碰撞（避免能量重复叠加）
local collided_this_frame = {}

-- ── 工具函数 ─────────────────────────────────────────────────────────────────
-- math.random/randomseed 被沙箱移除；使用内联 LCG 伪随机数生成器（种子固定可复现）
local _rng_state = 114514
local function rand()
    _rng_state = (_rng_state * 1664525 + 1013904223) % 4294967296
    return _rng_state / 4294967296 -- [0, 1)
end
local function rand_range(lo, hi) return lo + rand() * (hi - lo) end

-- 保证速度绝对值不低于下限（防止卡顿）
local function clamp_speed(v, min_abs)
    if math.abs(v) < min_abs then
        return v >= 0 and min_abs or -min_abs
    end
    return v
end

-- ── 初始化 ───────────────────────────────────────────────────────────────────
local function init(ctx)
    -- 圆形碰撞体（本地坐标原点中心）
    local circle = {
        type = "circle",
        cx = 0,
        cy = 0,
        r = BALL_R
    }
    -- Hitbox：layer=0，检测 layer-0 的 Hurtbox（group bit 0 = 1），不限次碰撞
    local hitbox = {
        shape = circle,
        layer = 0,
        group = 1,
        hit_once = false
    }
    -- Hurtbox：占据 layer=0
    local hurtbox = {
        shape = circle,
        layer = 0
    }

    for i = 1, NUM_BALLS do
        local e = ctx:spawn()

        -- 随机不重叠起始位置（简单随机，不精确排布）
        local x = rand_range(-(HW - BALL_R), HW - BALL_R)
        local y = rand_range(-(HH - BALL_R), HH - BALL_R)

        -- 随机方向单位速度，保证各分量不为零
        local angle = rand_range(0, 2 * math.pi)
        local vx = clamp_speed(SPEED * math.cos(angle), 10)
        local vy = clamp_speed(SPEED * math.sin(angle), 10)

        ctx:set_component(e, "Transform", {
            x = x,
            y = y,
            rotation = 0
        })
        ctx:set_component(e, "Velocity", {
            vx = vx,
            vy = vy,
            angular = 0
        })
        ctx:set_component(e, "Hitbox", hitbox)
        ctx:set_component(e, "Hurtbox", hurtbox)

        ctx:create_sprite(e, "res://icon.svg", 0)
        balls[e] = true
    end

    ctx:log(string.format("[benchmark] spawned %d balls  R=%d  HW=%d  HH=%d", NUM_BALLS, BALL_R, HW, HH))
    initialized = true
end

-- ── 每帧逻辑 ─────────────────────────────────────────────────────────────────
function on_frame(ctx)
    if not initialized then
        init(ctx)
    end

    frame_count = frame_count + 1
    collided_this_frame = {} -- 每帧重置去重表

    for e, _ in pairs(balls) do
        local tf = ctx:get_component(e, "Transform")
        local vel = ctx:get_component(e, "Velocity")
        if tf and vel then
            local x, y = tf.x, tf.y
            local vx, vy = vel.vx, vel.vy
            local changed = false

            -- ── 边界弹射（位置钳制 + 速度翻转）────────────────────────────
            local bx = HW - BALL_R
            local by = HH - BALL_R
            if x < -bx then
                x = -bx;
                vx = math.abs(vx);
                changed = true
            end
            if x > bx then
                x = bx;
                vx = -math.abs(vx);
                changed = true
            end
            if y < -by then
                y = -by;
                vy = math.abs(vy);
                changed = true
            end
            if y > by then
                y = by;
                vy = -math.abs(vy);
                changed = true
            end

            if changed then
                ctx:set_component(e, "Transform", {
                    x = x,
                    y = y,
                    rotation = 0
                })
                ctx:set_component(e, "Velocity", {
                    vx = vx,
                    vy = vy,
                    angular = 0
                })
            end

            -- ── 同步渲染位置 ─────────────────────────────────────────────
            ctx:update_sprite(e, tf.x, tf.y, 0, BALL_SCALE, BALL_SCALE)
        end
    end

    -- ── 帧数日志 ─────────────────────────────────────────────────────────
    if frame_count % LOG_EVERY == 0 then
        ctx:log(string.format("[benchmark] frame=%d  balls=%d", frame_count, NUM_BALLS))
    end
end

-- ── 球间弹性碰撞响应 ──────────────────────────────────────────────────────────
-- 参数：attacker / defender = entity_id（u32）
--       cx, cy = 接触点（世界坐标）
--       nx, ny = 单位法线，方向为 attacker → defender
function on_collision(ctx, attacker, defender, cx, cy, nx, ny)
    -- 去重：同一对（A,B）和（B,A）只处理一次
    local key = attacker < defender and attacker * 100000 + defender or defender * 100000 + attacker
    if collided_this_frame[key] then
        return
    end
    collided_this_frame[key] = true

    local va = ctx:get_component(attacker, "Velocity")
    local vb = ctx:get_component(defender, "Velocity")
    if not va or not vb then
        return
    end

    -- 沿法线方向的相对速度（等质量弹性碰撞 = 交换法线分量）
    -- impulse = (va - vb)·n；仅当球在靠近时（impulse > 0）才处理
    local impulse = (va.vx - vb.vx) * nx + (va.vy - vb.vy) * ny
    if impulse <= 0 then
        return
    end

    -- 速度更新
    ctx:set_component(attacker, "Velocity", {
        vx = va.vx - impulse * nx,
        vy = va.vy - impulse * ny,
        angular = 0
    })
    ctx:set_component(defender, "Velocity", {
        vx = vb.vx + impulse * nx,
        vy = vb.vy + impulse * ny,
        angular = 0
    })
end
