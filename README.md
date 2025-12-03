# bevy_malek_async

Runtime‑agnostic async ECS access for Bevy. Run async work on any executor (Tokio, Bevy task pools, or another runtime) and safely hop into Bevy’s world to read or mutate ECS state using normal `SystemParam`s.


## Features

- Runtime‑agnostic: use with Tokio, Bevy task pools, or any async executor
- Familiar ECS access: acquire `Res`, `ResMut`, `Query`, and other `SystemParam`s
- Persistent state across calls: reuse an `EcsTask<P>` to preserve `Local`, `Changed`, etc.


## How It Works

Add `AsyncEcsPlugin` to your app. It installs lightweight systems across many schedules. When those systems run, they wake pending async tasks, temporarily expose world access, run your closure with the requested `SystemParam`s, then apply commands/state and close access again.

Your async code creates an `EcsTask<P>` from a `WorldId` and calls `run_system(schedule, |params| { ... }).await`. The closure runs during the next access window for the chosen `schedule`.

Registered schedules include: `PreStartup`, `Startup`, `PostStartup`, `PreUpdate`, `Update`, `PostUpdate`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate`, `First`, `Last`, `FixedFirst`, `FixedLast`.

Important Note: The systems that we run are actually *more powerful* than normal bevy systems. You can use mutable state from outside the closure inside of it unlike normal bevy systems. 

## Install

In your `Cargo.toml`:

```toml
[dependencies]
bevy = "0.17"
bevy_malek_async = "0.2"

[dev-dependencies]
# If you want to use the Tokio + HTTP example below
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
```


## Quick Start (Tokio)

This example spawns a Tokio runtime on its own thread and mutates a Bevy resource from that async task using an `EcsTask`.

```rust
use bevy::prelude::*;
use bevy_ecs::world::WorldId;
use bevy_malek_async::{AsyncEcsPlugin, CreateEcsTask};

#[derive(Default, Resource)]
struct MyResource(String);

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .init_resource::<MyResource>()
        .add_plugins(AsyncEcsPlugin)
        .add_systems(Startup, spawn_tokio_task)
        .add_systems(Update, show_value)
        .run();
}

fn show_value(mut last: Local<String>, res: Res<MyResource>) {
    if last.as_str() != res.0.as_str() {
        *last = res.0.clone();
        println!("MyResource = {}", res.0);
    }
}

fn spawn_tokio_task(world_id: WorldId) {
    // Run an isolated Tokio runtime on its own thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime");

        rt.block_on(async move {
            // ... do any async work here (I/O, compute, etc.) ...

            // Hop into Bevy's world during the next access window
            world_id
                .ecs_task::<ResMut<MyResource>>()
                .run_system(Update, |mut my| {
                    my.0 = "hello from tokio".to_string();
                })
                .await;
        });
    });
}
```

Or run the included example:

```bash
cargo run --example tokio_example
```


## Other Runtimes (Bevy task pools, async-std, smol, …)

`EcsTask::run_system` is just an async method. You can `.await` it on any executor. For example, Bevy’s IO pool:

```rust
use bevy::prelude::*;
use bevy::tasks::IoTaskPool; // or AsyncComputeTaskPool/ComputeTaskPool
use bevy_malek_async::CreateEcsTask;

#[derive(Resource, Default)]
struct MyResource(String);

fn spawn_with_bevy_pool(world_id: WorldId) {
    IoTaskPool::get().spawn(async move {
        // background work...
        world_id
            .ecs_task::<ResMut<MyResource>>()
            .run_system(Update, |mut my| {
                my.0.push_str(" + task pool");
            })
            .await;
    });
}
```


## API Overview

- `AsyncEcsPlugin`
  - Installs the wake/apply systems across common schedules (see list above).

- `WorldId::ecs_task::<P>() -> EcsTask<P>` (via `CreateEcsTask`)
  - Creates a reusable task token keyed to a specific `WorldId` and `SystemParam` set `P`.
  - Reuse the same `EcsTask<P>` to preserve `Local`, `Changed`, and similar state across calls.

- `EcsTask<P>::run_system(schedule, |P| -> Out) -> impl Future<Output = Out>`
  - Schedules your closure to run during the next access window for `schedule` (e.g. `Update`).
  - `P` can be any `SystemParam` or tuple (e.g. `Res<T>`, `ResMut<T>`, `Query<...>`, etc.).
  - Returns the closure’s output to your async context.


## Notes and Limitations

- Access happens during short windows inside Bevy schedules and does not run in parallel with other systems.

