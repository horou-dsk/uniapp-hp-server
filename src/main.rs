use actix_web::{web, App, HttpServer, middleware::Logger};
use hotuniapp_server::route::update::update_config;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
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