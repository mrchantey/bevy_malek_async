# bevy_malek_async

Runtime‑agnostic async ECS access for Bevy. This crate lets you run async work on any executor (Tokio, Bevy's task pools, or another runtime), and then safely hop into Bevy's world to read or mutate ECS state using normal `SystemParam`s.

This is an experimental, stop‑gap crate — a simplified version of functionality I'd like to eventually ( potentially ) upstream into Bevy, where it could run in parallel with other systems. Today, world access is serialized into short "access windows" inside Bevy's schedule. See Limitations for details.


## Features

- Runtime‑agnostic: use with Tokio, Bevy task pools, or any async executor
- Familiar ECS access: acquire `Res`, `ResMut`, `Query`, and other `SystemParam`s
- Minimal plumbing: add one plugin and `await` a single helper future


## How It Works

`AsyncPlugin` installs a small system in multiple schedule stages (Pre/Startup/Update/Post). Each time that system runs, it temporarily exposes the ECS world to pending async tasks, wakes them, lets their closures run to completion, applies their system state, then closes access again. This guarantees that world access does not overlap with other systems.

Your async code calls `async_access(world_id, |params| { /* ECS work */ }).await;`. The closure runs during the next access window, with `params` being whatever `SystemParam` (or tuple of them) you requested.


## Install

In your `Cargo.toml`:

```toml
[dependencies]
bevy = "0.17"
bevy_malek_async = "0.1"

# If you want to use the Tokio example below
tokio = { version = "1", features = ["full"] }
```


## Quick Start (Tokio)

Below is a minimal example that spawns a Tokio task on its own thread and then mutates a Bevy resource from that task using `async_access`.

```rust
use bevy::prelude::*;
use bevy_malek_async::{async_access, AsyncPlugin, WorldIdRes};

#[derive(Resource, Default)]
struct MyResource(String);

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .init_resource::<MyResource>()
        .add_plugins(AsyncPlugin)
        .add_systems(Startup, spawn_tokio_task)
        .add_systems(Update, show_value)
        .run();
}

fn show_value(res: Res<MyResource>) {
    // Replace with `info!` if you use `LogPlugin`.
    println!("MyResource = {}", res.0);
}

fn spawn_tokio_task(world_id: Res<WorldIdRes>) {
    let world_id = world_id.0;

    // Run an isolated Tokio runtime on its own thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime");

        rt.block_on(async move {
            // ... do any async work here (I/O, compute, etc.) ...

            // Hop into Bevy's world during the next access window
            async_access::<ResMut<MyResource>, _, _>(world_id, |mut my| {
                my.0 = "hello from tokio".to_string();
            })
            .await;
        });
    });
}
```

Run the included example from this repo instead:

```bash
cargo run --example tokio_example
```


## Other Runtimes (Bevy task pools, async-std, smol, …)

`async_access` is just a `Future`. You can `await` it on any executor:

- Bevy task pools (names vary by Bevy version):

  ```rust
  use bevy::prelude::*;
  use bevy::tasks::IoTaskPool; // or AsyncComputeTaskPool/ComputeTaskPool
  use bevy_malek_async::{async_access, WorldIdRes};

  #[derive(Resource, Default)]
  struct MyResource(String);

  fn spawn_with_bevy_pool(world_id: Res<WorldIdRes>) {
    let world_id = world_id.0;
    IoTaskPool::get().spawn(async move {
        // background work...
        async_access::<ResMut<MyResource>, _, _>(world_id, |mut my| {
            my.0.push_str(" + task pool");
        })
        .await;
    });
  }
  ```

- Any other runtime: spawn a task as usual and `await async_access` the same way.


## API Overview

- `AsyncPlugin`
  - Registers access windows in `PreStartup`, `Startup`, `PostStartup`, `PreUpdate`, `Update`, and `PostUpdate` stages.
  - Also initializes `WorldIdRes` and an internal counter resource.

- `async_access<P, F, Out>(world_id, f) -> impl Future<Output = Out>`
  - `P: SystemParam` (tuples work too). Examples: `Res<T>`, `ResMut<T>`, `Query<...>`, `Commands`, etc.
  - `f: FnOnce(P::Item<'_, '_>) -> Out` runs during the next access window.
  - Returns `Out` to your async context; often `()` is fine.

- `WorldIdRes`
  - A resource containing the current `WorldId`. You can also obtain it with `World::id()` inside a regular system.

Example of multiple params in one access:

```rust
use bevy::prelude::*;
use bevy_malek_async::async_access;

async fn update_stuff(world_id: WorldId) {
    async_access::<(Res<MyConfig>, Query<&mut Transform>), _, _>(world_id, |(cfg, mut q)| {
        for mut t in &mut q {
            t.translation.x += cfg.speed;
        }
    })
    .await;
}
```


## Limitations

- Not parallel with other systems: ECS access from async tasks is serialized into short access windows and does not run in parallel with Bevy systems.
- Keep closures short: do only the minimal world reads/writes inside `async_access`. Perform heavy work before/after, off the world unless you're willing to eat the cost on *both* your async pool and your bevy world.
- Experimental/unsafe internals: relies on carefully‑scoped unsafe world access. Expect sharp edges; not production‑ready.
- Per‑world: you must pass the correct `WorldId` to `async_access`.


## Motivation and Future Work

The goal is to offer a simple bridge between external async work and Bevy ECS without committing to a specific runtime. Long‑term, the intention is a first‑class Bevy solution that can safely interleave with system parallelism and scheduling.

