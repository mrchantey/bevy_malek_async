//! Demonstrates fetching a web page with **ureq** (blocking HTTP) offloaded
//! via `blocking::unblock`, driven by `futures_lite::future::block_on` on a
//! dedicated thread – no Tokio needed.
//!
//! The `AsyncWorld` API is identical to the one used in `tokio_demo`.

use bevy::prelude::*;
use bevy_app::prelude::App;
use bevy_malek_async::AsyncEcsPlugin;
use std::time::Duration;

mod utils;
use utils::{AsyncCommands, AsyncWorld};

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(AsyncEcsPlugin)
        .add_systems(Startup, spawn_web_request)
        .add_observer(print_response)
        .run();
}

#[derive(Event)]
struct Response(String);

// if we implemented IntoSystem for async systems this step
// would not be nessecary.
fn spawn_web_request(commands: AsyncCommands) {
    commands.run(fetch_example_com);
}

async fn fetch_example_com(world: AsyncWorld) -> Result {
    let body = send_request("http://example.com").await?;
    world.trigger(Response(body)).await;
    Ok(())
}
fn print_response(response: On<Response>, mut exit: MessageWriter<AppExit>) {
    println!("{}", response.0);
    exit.write(AppExit::Success);
}

async fn send_request(url: &str) -> Result<String> {
    let url = url.to_string();
    blocking::unblock(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();

        let resp = agent.get(&url).call()?;

        let body = resp.into_string()?;
        Ok(body)
    })
    .await
}
