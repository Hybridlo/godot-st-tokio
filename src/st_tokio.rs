use std::sync::{LazyLock, Mutex};

use godot::{classes::Engine, prelude::*};
use tokio::{
    runtime::{self, Runtime},
    task::{JoinHandle, LocalSet},
};

static TOKIO_BUILDER: LazyLock<Mutex<runtime::Builder>> = LazyLock::new(|| {
    let mut builder = runtime::Builder::new_current_thread();
    builder.enable_all();

    Mutex::new(builder)
});

/// Holds the actual tokio [`Runtime`] that drives the spawned futures,
/// and a [`LocalSet`] which holds said futures to run on the current/main thread
#[derive(GodotClass)]
#[class(base=Object)]
pub struct TokioRuntime {
    base: Base<Object>,
    runtime: Runtime,
    local_set: LocalSet,
}

#[godot_api]
impl IObject for TokioRuntime {
    fn init(base: Base<Object>) -> Self {
        let mut builder_guard = TOKIO_BUILDER
            .lock()
            .expect("Failed getting TOKIO_BUILDER static during TokioRuntime::init");

        // Get the builder in the static by replacing it with the one in the Mutex
        let mut new_builder = runtime::Builder::new_current_thread();
        new_builder.enable_all();
        let mut builder = std::mem::replace(&mut *builder_guard, new_builder);

        Self {
            base,
            runtime: builder.build().unwrap(),
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
    /// happens if idle time is starved, but if it is, I imagine you have bigger
    /// problems anyway
    fn init_and_start() -> Option<Gd<Self>> {
        let async_runtime = Self::new_alloc();
        Engine::singleton()
            .get_main_loop()?
            .cast::<SceneTree>()
            .signals()
            .process_frame()
            .connect_other(&async_runtime, TokioRuntime::tick);

        Some(async_runtime)
    }

    /// Replaces a runtime builder that will be used to build a Tokio runtime.
    /// Builder is not clone-able, so if you unregister the runtime, you need
    /// to re-set the builder before spawning any futures, if you need custom
    /// configuration.
    ///
    /// If this function isn't called - a runtime is initialized with
    /// `Builder::new_current_thread().enable_all()` on first spawn of a future.
    ///
    /// # Warning
    ///
    /// It is possible to pass a multithreaded builder here, but LocalSet is
    /// the storage of the futures, so they still will run on one thread.
    pub fn set_builder(tokio_builder: runtime::Builder) {
        let mut builder_guard = TOKIO_BUILDER
            .lock()
            .expect("Failed getting TOKIO_BUILDER static during set_builder");

        *builder_guard = tokio_builder;
    }

    /// Get an active singleton, create one if it doesn't exist.
    ///
    /// #### WARNING! Cannot be used during level_init
    ///
    /// Expects for a main loop to be initialized, which means
    /// it's usually initialized on the first call to [`spawn`][Self::spawn]
    /// or [`spawn_signal`][Self::spawn_signal]
    pub fn singleton() -> Option<Gd<TokioRuntime>> {

        let singleton_option = match Engine::singleton().has_singleton(Self::SINGLETON) {
            true => Engine::singleton().get_singleton(Self::SINGLETON),
            false => None
        };

        match singleton_option {
            Some(singleton) => Some(singleton.cast::<Self>()),
            None => {
                let singleton = TokioRuntime::init_and_start()?;
                Engine::singleton().register_singleton(TokioRuntime::SINGLETON, &singleton);

                Some(singleton)
            }
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

            let callable =
                Callable::from_fn("TokioRuntime::spawn_signal::callable", move |_args| {
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
    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
    {
        let runtime = Self::singleton()
            .expect("TokioRuntime singleton did not exist while trying to spawn a future");

        runtime.bind().local_set.spawn_local(future)
    }

    /// A spawn function that runs a future on a runtime directly.
    /// Might be useful, if you are deciding to use a multithreaded runtime,
    /// and know what you are doing. I won't describe all dangers of using
    /// this.
    pub fn spawn_rt<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let runtime = Self::singleton()
            .expect("TokioRuntime singleton did not exist while trying to spawn a future");

        runtime.bind().runtime.spawn(future)
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
        Self::singleton()
            .expect("TokioRuntime singleton did not exist while trying to spawn a blocking thread")
            .bind()
            .runtime
            .spawn_blocking(func)
    }

    /// Performs an evaluation of currently registered tasks/futures
    /// during idle time on the main thread.
    ///
    /// This is the main mechanism of driving futures.
    fn tick(&mut self) {
        // Runs tokio runtime...
        self.runtime
            .block_on(
                // ...with the current-thread local_set...
                self.local_set.run_until(async {
                    // ...with a no-op payload. This allows stored futures to
                    // make progress without blocking the thread
                    tokio::task::spawn_local(async {}).await
                }),
            )
            .unwrap();
    }

    pub fn unregister_singleton() {
        let mut engine = Engine::singleton();

        // Here is where we free our async runtime singleton from memory.
        if engine.has_singleton(TokioRuntime::SINGLETON) {
            if let Some(async_singleton) = engine.get_singleton(TokioRuntime::SINGLETON) {
                engine.unregister_singleton(TokioRuntime::SINGLETON);
                async_singleton.free();
            }
        }
    }
}
