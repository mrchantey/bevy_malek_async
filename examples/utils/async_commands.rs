use super::*;
use bevy::utils::prelude::DebugName;
use bevy_ecs::error::{ErrorContext, Result};
use bevy_ecs::system::SystemParam;

#[derive(SystemParam)]
pub struct AsyncCommands {
    world: AsyncWorld,
}
impl AsyncCommands {
    pub fn run<S, M>(&self, system: S)
    where
        S: AsyncSystem<M>,
    {
        let world = self.world.clone();
        let _handle = std::thread::Builder::new()
            .name("async-task".into())
            .spawn(move || {
                bevy::tasks::block_on(async move {
                    if let Err(err) = system.run(world.clone()).await {
                        world
                            .handle_error(
                                err,
                                ErrorContext::Command {
                                    name: DebugName::type_name::<S>(),
                                },
                            )
                            .await;
                    }
                });
            })
            .expect("spawn ureq task thread");
    }
}
