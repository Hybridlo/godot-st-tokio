
use std::rc::Rc;

use godot::{classes::Engine, prelude::*};
use tokio::{
    runtime::{self, Runtime},
    task::{JoinHandle, LocalSet},
};

/// Holds the actual tokio [`Runtime`] that drives the spawned futures,
/// and a [`LocalSet`] which holds said futures to run on the current/main thread
#[derive(GodotClass)]
#[class(base=Object)]
pub struct TokioRuntime {
    base: Base<Object>,
    runtime: Rc<Runtime>,
    local_set: LocalSet,
}

#[godot_api]
impl IObject for TokioRuntime {
    fn init(base: Base<Object>) -> Self {
        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        Self {
            base,
            runtime: Rc::new(runtime),
            local_set: Default::default(),
        }
    }
}

#[godot_api]
impl TokioRuntime {
    pub const SINGLETON: &'static str = "Tokio";

    /// The only right way to create a [`TokioRuntime`]
    /// 
    /// Creates the runtime and starts the [`tick`][Self::tick]ing function,
    /// that drives spawned futures
    /// 
    /// Authors note: there might be a better way to wire into the idle time
    /// of the engine, I didn't find it personally. Also I have no idea what
    /// happens if idle time is starved, but if it is I imagine you have bigger
    /// problems anyway
    fn init_and_start() -> Gd<Self> {
        let mut async_runtime = Self::new_alloc();
        Engine::singleton()
            .get_main_loop()
            .unwrap()
            .cast::<SceneTree>()
            .signals()
            .process_frame()
            .connect_other(
                &mut async_runtime,
                TokioRuntime::tick
            );

        async_runtime
    }

    /// Get an active singleton, create one if it doesn't exist.
    /// 
    /// #### WARNING! Cannot be used during level_init
    /// 
    /// Expects for a main loop to be initialized, which means
    /// it's usually initialized on the first call to [`spawn`][Self::spawn]
    /// or [`spawn_signal`][Self::spawn_signal]
    /// 
    /// Authors note: emits an error on gdextension load. Again, there might
    /// be a better way to do this over a singleton, but I'm not sure what
    /// way is
    pub fn singleton() -> Gd<TokioRuntime> {
        match Engine::singleton().get_singleton(Self::SINGLETON) {
            Some(singleton) => singleton.cast::<Self>(),
            None => {
                let singleton = TokioRuntime::init_and_start();
                Engine::singleton()
                    .register_singleton(TokioRuntime::SINGLETON, &singleton);

                singleton
            },
        }
    }

    /// #### WARNING! [`Spawn`][Self::spawn] warnings apply
    /// 
    /// Spawns a provided future on the runtime, and returns a [`Signal`]
    /// that can be `await`ed on the Godot side (and on Rust side, with
    /// [`.to_future`][Signal::to_future]).
    /// 
    /// The resulting [`Signal`] is type-erased, because [`TypedSignal`][godot::register::TypedSignal]
    /// doesn't have public API constructor, nor does it work with godot FFI
    /// 
    /// Since the runtime is running on the current thread - you can
    /// move `!Send` values in, like [`Gd`] pointers
    /// 
    /// The signal does not need to be `await`ed to finish
    pub fn spawn_signal<F, R>(future: F) -> Signal
    where
        F: Future<Output = R> + 'static,
        R: ToGodot + 'static,
    {
        let mut signal_holder = RefCounted::new_gd();
        signal_holder.add_user_signal("finished");
        let signal = Signal::from_object_signal(&signal_holder, "finished");

        Self::spawn(async move {
            let result = future.await;

            let callable = Callable::from_fn("TokioRuntime::spawn_signal::callable", move |_args| {
                signal_holder.emit_signal("finished", &[result.to_variant()]);
                Variant::nil()
            });

            callable.call_deferred(&[]);
        });

        signal
    }

    /// #### WARNING! Be extra careful with blocking operations!
    /// Blocking operations **WILL** hang the thread, and thus the game.
    /// Use [`spawn_blocking`][Self::spawn_blocking] for blocking operations.  
    /// <br><br>
    ///   
    /// A wrapper function for the [`tokio::spawn`] function.
    /// 
    /// Can be called both in async and non-async contexts.
    /// 
    /// Allows `!Send` futures
    pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + 'static,
    {
        let runtime = Self::singleton();
        
        runtime.bind().local_set.spawn_local(future)
    }

    /// A wrapper function for the [`tokio::spawn_blocking`] function.
    /// 
    /// Use only for blocking operations, like sync HTTP requests,
    /// non-async fs operations, etc.
    /// 
    /// Since the function is running on a separate thread - `Sync`
    /// requirement cannot be relaxed, and `godot_rust` bindings
    /// [limitations][https://github.com/godot-rust/gdext/issues/18] apply
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        Self::singleton().bind().runtime.spawn_blocking(func)
    }

    /// Performs an evaluation of currently registered tasks/futures
    /// during idle time on the main thread.
    /// 
    /// This is the main mechanism of driving futures.
    fn tick(&mut self) {
        // Runs tokio runtime...
        self.runtime.block_on(
            // ...with the current-thread local_set...
            self.local_set.run_until(
                async {
                    // ...with a no-op payload. This allows stored futures to
                    // make progress without blocking the thread
                    tokio::task::spawn_local(async {}).await
                }
            )
        ).unwrap();
    }
}
