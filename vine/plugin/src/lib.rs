mod routes;

use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    App, Error,
};

pub fn register<T>(app: App<T>) -> App<T>
where
    T: ServiceFactory<ServiceRequest, Error = Error, Config = (), InitError = ()>,
{
    app.service(crate::routes::desktop::batch::post_exec_broadcast)
        .service(crate::routes::desktop::single::post_exec)
        .service(crate::routes::user::get)
}
