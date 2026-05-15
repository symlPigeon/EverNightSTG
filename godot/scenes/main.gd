extends Node

## Lua 脚本路径，可在 Inspector 中修改切换（demo.lua / input_demo.lua / benchmark.lua）
@export var lua_file: String = "res://scripts/input_demo.lua"

@onready var bridge: EvernightBridge = $EvernightBridge

func _ready() -> void:
	bridge.load_script_file(lua_file)
