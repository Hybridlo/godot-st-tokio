pub mod async_runtime;

use godot::prelude::*;
use godot::classes::{Engine, ISprite2D, Sprite2D};

use crate::async_runtime::TokioRuntime;

#[derive(GodotClass)]
#[class(base=Sprite2D)]
struct Player {
    //speed: f64,
    angular_speed: f64,

    #[var]
    time_elapsed: f64,

    base: Base<Sprite2D>
}

#[godot_api]
impl ISprite2D for Player {
    fn init(base: Base<Sprite2D>) -> Self {
        godot_print!("Hello, world!"); // Prints to the Godot console
        
        Self {
            //speed: 400.0,
            time_elapsed: 0.0,
            angular_speed: std::f64::consts::PI,
            base,
        }
    }

    fn physics_process(&mut self, delta: f64) {
        // In GDScript, this would be: 
        // rotation += angular_speed * delta
        
        let radians = (self.angular_speed * delta) as f32;
        self.base_mut().rotate(radians);
        // The 'rotate' method requires a f32, 
        // therefore we convert 'self.angular_speed * delta' which is a f64 to a f32
    }
}

#[godot_api]
impl Player {
    #[signal]
    fn my_signal(thing: i32);

    #[func]
    fn do_the_thing(&mut self) -> Signal {
        // Needs to become a pointer to be movable. `to_gd` warning apply
        let self_gd = self.to_gd();

        // `spawn_signal` returns a `Signal`, that can be awaited in Godot
        let signal = TokioRuntime::spawn_signal(async move {
            // Tokio can handle awaiting for signals
            // Also, we can access Godot values, like `Gd` pointers
            // as `Send` bound is not required for these futures
            let _thing = self_gd.signals().my_signal().to_future().await;

            reqwest::get("https://example.com")
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        });

        let signal2 = TokioRuntime::spawn_signal(async move {
            // A Godot-native signal can be `await`ed as well, but the type information
            // will not be available, so you have to provide the types manually
            let (res,): (GString,) = signal.to_future().await;
            // The future runs on the same thread as the `spawn` call,
            // so we can access binds like `godot_print`
            godot_print!("{res}");

            "signal2 finished"
        });

        signal2
    }
}

struct MyExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MyExtension {
    fn on_level_deinit(level: InitLevel) {
        match level {
            InitLevel::Scene => {
                let mut engine = Engine::singleton();

                // Here is where we free our async runtime singleton from memory.
                if let Some(async_singleton) = engine.get_singleton(TokioRuntime::SINGLETON) {
                    engine.unregister_singleton(TokioRuntime::SINGLETON);
                    async_singleton.free();
                }
            }
            _ => (),
        }
    }
}

