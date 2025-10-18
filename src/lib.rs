use bevy_app::{App, Plugin, PostStartup, PostUpdate, PreStartup, PreUpdate, Startup, Update};
use bevy_ecs::{
    prelude::{FromWorld, Resource},
    system::{SystemParam, SystemState},
    world::{World, WorldId, unsafe_world_cell::UnsafeWorldCell},
};
use bevy_platform::collections::HashMap;
use crossbeam::sync::WaitGroup;
use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll, Waker},
};

static ASYNC_ECS_WORLD_ACCESS: OnceLock<Mutex<Option<UnsafeWorldCell>>> = OnceLock::new();
static ASYNC_ECS_WAKER_LIST: OnceLock<Mutex<HashMap<WorldId, Vec<Waker>>>> = OnceLock::new();

pub async fn async_access<P, Func, Out>(world_id: WorldId, ecs_access: Func) -> Out
where
    P: SystemParam + 'static,
    for<'w, 's> Func: FnOnce(P::Item<'w, 's>) -> Out,
{
    SystemParamThing::<P, Func, Out>(PhantomData::<P>, PhantomData, Some(ecs_access), world_id)
        .await
}

struct SystemParamThing<'a, 'b, P: SystemParam + 'static, Func, Out>(
    PhantomData<P>,
    PhantomData<(Out, &'a (), &'b ())>,
    Option<Func>,
    WorldId,
);

impl<'a, 'b, P: SystemParam + 'static, Func, Out> Unpin for SystemParamThing<'a, 'b, P, Func, Out> {}

impl<'a, 'b, P, Func, Out> Future for SystemParamThing<'a, 'b, P, Func, Out>
where
    P: SystemParam + 'static,
    for<'w, 's> Func: FnOnce(P::Item<'w, 's>) -> Out,
{
    type Output = Out;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(async_ecs_world_access) = ASYNC_ECS_WORLD_ACCESS.get()
            && let Some(wc) = async_ecs_world_access.lock().unwrap().as_mut()
        {
            let out;
            let world_id;
            unsafe {
                world_id = wc.world().id();
                // SAFETY: This is safe because we have a mutex around our world cell, so only one thing can have access to it at a time.
                let mut system_state: SystemState<P> = SystemState::new(wc.world_mut());
                {
                    // Obtain params and immediately consume them with the closure,
                    // ensuring the borrow ends before `apply`.
                    let state = system_state.get_unchecked(wc.clone());
                    out = self.as_mut().2.take().unwrap()(state);
                }
                system_state.apply(wc.world_mut());
                wc.get_resource_mut::<AsyncEcsCounter>()
                    .unwrap()
                    .0
                    .lock()
                    .unwrap()
                    .pop();
            }
            Poll::Ready(out)
        } else {
            let mut hashmap = ASYNC_ECS_WAKER_LIST
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap();
            if !hashmap.contains_key(&self.3) {
                hashmap.insert(self.3.clone(), Vec::new());
            }
            hashmap.get_mut(&self.3).unwrap().push(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn run_async_ecs_accesses(world: &mut World) {
    let world_id = world.id();
    unsafe {
        ASYNC_ECS_WORLD_ACCESS
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap()
            // SAFETY: This mem transmute is safe only because we drop it after, and our ASYNC_ECS_WORLD_ACCESS is private, and we don't clone it
            // where we do use it, so the lifetime doesn't get propagated anywhere.
            .replace(std::mem::transmute(world.as_unsafe_world_cell()));
    }
    let mut num_wakers = 0;
    if let Some(wakers) = ASYNC_ECS_WAKER_LIST
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .remove(&world_id)
    {
        num_wakers = wakers.len();
        let wg = WaitGroup::new();
        {
            let mut tickets = world
                .get_resource::<AsyncEcsCounter>()
                .unwrap()
                .0
                .lock()
                .unwrap();
            tickets.clear();
            for _ in 0..num_wakers {
                tickets.push(wg.clone());
            }
        }
        for waker in wakers {
            waker.wake();
        }
        if num_wakers > 0 {
            wg.wait();
        }
    }
    ASYNC_ECS_WORLD_ACCESS
        .get()
        .unwrap()
        .lock()
        .unwrap()
        .take()
        .unwrap();
}

pub struct AsyncPlugin;

impl Plugin for AsyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldIdRes>()
            .init_resource::<AsyncEcsCounter>()
            .add_systems(PreStartup, run_async_ecs_accesses)
            .add_systems(Startup, run_async_ecs_accesses)
            .add_systems(PostStartup, run_async_ecs_accesses)
            .add_systems(PreUpdate, run_async_ecs_accesses)
            .add_systems(Update, run_async_ecs_accesses)
            .add_systems(PostUpdate, run_async_ecs_accesses);
    }
}

#[derive(Resource)]
pub struct WorldIdRes(pub WorldId);
impl FromWorld for WorldIdRes {
    fn from_world(world: &mut World) -> Self {
        Self(world.id())
    }
}

#[derive(Resource)]
pub struct AsyncEcsCounter(pub Arc<Mutex<Vec<WaitGroup>>>);
impl Default for AsyncEcsCounter {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
}
