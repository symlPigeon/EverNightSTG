extends Node

## Lua 脚本路径，可在 Inspector 中修改切换（demo.lua / benchmark.lua）
@export var lua_file: String = "res://scripts/benchmark.lua"

@onready var bridge: EvernightBridge = $EvernightBridge

func _ready() -> void:
	bridge.load_script_file(lua_file)
