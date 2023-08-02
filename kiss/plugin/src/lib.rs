mod routes;

use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    App, Error,
};

pub fn register<T>(app: App<T>) -> App<T>
where
    T: ServiceFactory<ServiceRequest, Error = Error, Config = (), InitError = ()>,
{
    app.service(crate::routes::r#box::power::single::post)
}
