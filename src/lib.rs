/// An adapter library to run Tokio runtime on the main thread of
/// Godot runtime.
///
/// Using this should remove the pain point of needing to avoid
/// all Godot API in rust-async contexts. Also allows awaiting
/// rust futures, using `Signal`s as adapters between rust and
/// godot worlds.
///
/// There is only one limitation of this library - you must never
/// create a future while there is no `SceneTree` available, as it
/// is used to drive spawned futures, during `process_frame` signal.
mod st_tokio;

pub use st_tokio::TokioRuntime;
