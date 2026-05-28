use std::{env, error::Error};

use color_eyre::owo_colors::OwoColorize;
use tracing_subscriber::layer::SubscriberExt;
use engine::*;
use tracing::{self, level_filters::LevelFilter};
use tracing_subscriber::{prelude::*, fmt};



#[cfg(any(feature = "memory_profiling", debug_assertions))]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System>  =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

fn main() -> Result<(), Box<dyn Error>> {


    let subscriber = tracing_subscriber::registry();

    #[cfg(debug_assertions)]
    let subscriber = subscriber.with(tracing_tracy::TracyLayer::default());
    
    let fmt_layer = fmt::Layer::default()
            .with_file(true)
            .with_ansi(true)
            .with_level(true)
            .with_line_number(true)
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true);
    #[cfg(not(debug_assertions))]
    let fmt_layer = fmt_layer
        .with_filter(tracing_subscriber::filter::LevelFilter::from_level(tracing::Level::ERROR));
    let subscriber = subscriber.with(fmt_layer);
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("setup tracy layer");

    color_eyre::install()?;
    Engine::new("test_game", 1)?
        .import::<modules::core::CoreModule>()?
        .import::<modules::events::EventsModule>()?
        .import::<modules::window::WindowModule>()?
        .import::<modules::renderer::RendererModule>()?
        .run()?;
    Ok(())
}
