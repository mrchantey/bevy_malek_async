#![allow(unused)]
use bevy_app::Update;
use bevy_ecs::error::{BevyError, ErrorContext, Result};
use bevy_ecs::event::Event;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::{Commands, ResMut, SystemParam};
use bevy_ecs::world::{World, WorldId};
use bevy_malek_async::{CreateEcsTask, async_exclusive_access};

pub trait AsyncSystem<M>: 'static + Send {
    fn run(self, world: AsyncWorld) -> impl Future<Output = Result>;
}
impl<T, F> AsyncSystem<F> for T
where
    T: 'static + Send + FnOnce(AsyncWorld) -> F,
    F: Future<Output = Result>,
{
    fn run(self, world: AsyncWorld) -> impl Future<Output = Result> {
        (self)(world)
    }
}

/// Ergonomic wrapper around [`WorldId`] for async ECS access.
///
/// Clone this freely – it's just a [`WorldId`] under the hood.
#[derive(Clone, Copy, SystemParam)]
pub struct AsyncWorld {
    world_id: WorldId,
}

impl AsyncWorld {
    pub fn new(world_id: WorldId) -> Self {
        Self { world_id }
    }

    /// Schedule a one-shot system to run on the given schedule label,
    /// returning the system's output once it has been executed.
    pub async fn run<P, Func, Out>(&self, schedule: impl ScheduleLabel, system: Func) -> Out
    where
        P: SystemParam + 'static,
        for<'w, 's> Func: FnOnce(P::Item<'w, 's>) -> Out,
    {
        self.world_id
            .ecs_task::<P>()
            .run_system(schedule, system)
            .await
    }

    /// Schedule an exclusive system (one taking `&mut World`) to run on the
    /// given schedule label, returning its output once executed.
    pub async fn run_exclusive<Func, Out>(&self, schedule: impl ScheduleLabel, system: Func) -> Out
    where
        Func: FnOnce(&mut World) -> Out,
    {
        async_exclusive_access(self.world_id, schedule, system).await
    }

    #[allow(unused)]
    pub async fn with_resource<R: Resource, Out>(
        &self,
        func: impl FnOnce(ResMut<R>) -> Out,
    ) -> Out {
        self.run::<ResMut<R>, _, _>(Update, func).await
    }

    pub async fn trigger<E: Event>(&self, event: E)
    where
        for<'a> E::Trigger<'a>: Default,
    {
        self.run::<Commands, _, _>(Update, move |mut commands| {
            commands.trigger(event);
        })
        .await;
    }

    pub async fn handle_error(&self, err: impl Into<BevyError>, cx: ErrorContext) {
        self.run_exclusive(Update, move |world: &mut World| {
            world.default_error_handler()(err.into(), cx);
        })
        .await;
    }
}

impl From<WorldId> for AsyncWorld {
    fn from(world_id: WorldId) -> Self {
        Self::new(world_id)
    }
}
