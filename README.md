# Godot to single-threaded Tokio runtime adapter library

Uses a combination of ideas from [`godot_tokio`](https://github.com/2-3-5-41/godot_tokio), [async runtime integration PR](https://github.com/godot-rust/gdext/pull/1228) and [gdnative tokio recipe](https://godot-rust.github.io/gdnative-book/recipes/async-tokio.html)

Provides a library that allows spawning futures on a Tokio runtime, that is driven by Godot process_frame.