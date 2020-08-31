// use fern::colors::{Color, ColoredLevelConfig};
use actix_web::{web, App, HttpServer, middleware::Logger};
use hotuniapp_server::route::update::update_config;
use chrono::Local;

fn setup_logger() -> Result<(), fern::InitError> {
    // let colors = ColoredLevelConfig::new().debug(Color::Magenta);
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}:{}] [{}] {}",
                // colors.color(record.level()),
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.target(),
                record.line().unwrap_or(0),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .chain(fern::log_file("output.log")?)
        .apply()?;
    Ok(())
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "info");
    // let env = env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "trace");
    // env_logger::builder().format(|buf, record| {
    //     writeln!(
    //         buf,
    //         "{} {} [{}:{}] {}",
    //         Local::now().format("%Y-%m-%d %H:%M:%S"),
    //         record.level(),
    //         record.module_path().unwrap_or("<unnamed>"),
    //         record.line().unwrap_or(0),
    //         &record.args(),
    //     )
    // }).init();
    setup_logger().unwrap();
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            // .wrap(Logger::new("%a %{User-Agent}i"))
            .service(web::scope("/update").configure(update_config))
    })
        .bind("0.0.0.0:9699")?
        .run()
        .await
}