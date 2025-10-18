# Rust futures with Godot `await` example

Uses a combination of ideas from [`godot_tokio`](https://github.com/2-3-5-41/godot_tokio), [async runtime integration PR](https://github.com/godot-rust/gdext/pull/1228) and [gdnative tokio recipe](https://godot-rust.github.io/gdnative-book/recipes/async-tokio.html)

An example godot script that works with the gdextension in this repo

```godot
extends Player

# Called when the node enters the scene tree for the first time.
func _ready() -> void:
	print("start async");
	var res = await self.do_the_thing();
	print("async result:");
	print(res);


# Called every frame. 'delta' is the elapsed time since the previous frame.
func _process(delta: float) -> void:
	self.time_elapsed += delta;
	
	if self.time_elapsed >= 10:
		self.my_signal.emit(10);
		self.time_elapsed = 0;

```

Starts `await`ing on the signal, returned by `self.do_the_thing()`, which, in turn, is waiting for `self.my_signal` and then fires an HTTP request using `tokio` and `reqwest`.