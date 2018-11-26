
use actix::prelude::*;

use actix_broker::{BrokerIssue, BrokerSubscribe};
use actix_web::{
    App, AsyncResponder, Error as ActixError, FromRequest,
    dev::JsonConfig, error, http, http::header::DispositionType, http::NormalizePath, middleware,
    HttpMessage, HttpRequest, HttpResponse, fs, fs::{NamedFile, StaticFileConfig, StaticFiles},
    Json, Path, Query, ResponseError, Responder, State, ws,
};

use actix_web::middleware::cors::Cors;

use dotenv::{dotenv};
use env_logger::{Builder, Target};

use futures::{future::{Future, result}, stream::once};
use futures::future;

use json;
use json::JsonValue;

use log::LevelFilter;

use model::{api, connection, connection::DatabaseExecutor};

use serde;
use serde_derive;
use serde_json;

use std::error::Error;
use std::result::Result;
use std::result::Result::Ok;
use std::path::PathBuf;
use std::env;

use super::handlers;
use super::session::TableSession;
use super::state::AppState;

type AsyncResponse = Box<Future<Item=HttpResponse, Error=ActixError>>;


//TODO: implement for own Response Type
impl ResponseError for api::Error {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(serde_json::to_string(&json!({ "error": self.description().to_string() })).unwrap())
    }
}

fn get_www_path() -> String {
    dotenv().expect("could not parse dotenv file");
    let www_path = env::var("WWW_PATH").expect("WWW_PATH must be set");
    www_path
}

fn http_response<M: Message<Result = Result<serde_json::Value, api::Error>>>
    (state: &AppState, msg: M) -> AsyncResponse
where
    M: Send + 'static,
    M::Result: Send,
    DatabaseExecutor: Handler<M>,
{
    let conn = &state.connect(0);
    conn
        .send(msg)
        .from_err()
        .and_then(|res| {
            let unwrapped_result = res?;
            println!("final result: {:?}", &unwrapped_result);
            let ok_result = serde_json::to_string(&unwrapped_result)?;
            Ok(
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(ok_result)
            )
        })
        .responder()
}


fn websocket_response<M: Message<Result = Result<serde_json::Value, api::Error>>>(
    ctx: &mut ws::WebsocketContext<TableSession, AppState>,
    action_name: &str,
    msg: M
)
where
    M: Send + 'static,
    M::Result: Send,
    DatabaseExecutor: Handler<M>,
{
    ctx.state()
        .connect(0)
        .send(msg)
        .wait()
        .or_else(|err| Err(api::Error::TooManyConnections))
        .and_then(|res| {
            let unwrapped_result = res?;
            println!("final result: {:?}", &unwrapped_result);
            let response = json!({
                "action": action_name,
                "data": unwrapped_result
            });
            let ok_result = serde_json::to_string(&response)
                .or_else(|err| Err(api::Error::SerializationError))?;

            ctx.text(ok_result);
            Ok(())
        })
        .or_else(|err| {
            println!("encountered error: {:?}", &err);
            Err(err)
        });
}


// getting
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetTables {
    #[serde(default)]
    pub detailed: bool,
    #[serde(default)]
    pub show_deleted: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetTable {
    #[serde(default)]
    pub detailed: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetTableDataQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    #[serde(default)]
    pub format: api::TableDataFormat,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InsertTableDataQuery {
    #[serde(default)]
    pub format: api::TableDataFormat,
}



fn get_tables((state, query): (State<AppState>, Query<GetTables>)) -> AsyncResponse {
    println!("received message: {:?}", &query);
    http_response(&state,handlers::GetTables {
        detailed: query.detailed,
        show_deleted: query.show_deleted,
    })
}



fn post_tables((state, json): (State<AppState>, Json<api::PostTable>)) -> AsyncResponse {
    println!("received message: {:?}", &json);
    http_response(&state,handlers::CreateTable {
        reqdata: json.into_inner()
    })
}

fn get_table((state, path, query): (State<AppState>, Path<String>, Query<GetTable>)) -> AsyncResponse {
    println!("received message: {:?}", &query);
    http_response(&state,handlers::GetTable {
        name: path.to_string(),
        detailed: query.detailed,
    })
}

fn put_table((state, path, json): (State<AppState>, Path<String>, Json<u32>)) -> AsyncResponse {

    result(Ok(
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(serde_json::to_string(&json!({ "error": "method not implemented" })).unwrap())
    )).responder()
}

fn delete_table((state, path): (State<AppState>, Path<String>)) -> AsyncResponse {

    result(Ok(
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(serde_json::to_string(&json!({ "error": "method not implemented" })).unwrap())
    )).responder()
}

fn get_table_data((state, path, query): (State<AppState>, Path<String>, Query<GetTableDataQuery>)) -> AsyncResponse {
    println!("received message: {:?}", &query);
    http_response(&state,handlers::GetTableData {
        name: path.to_string(),
        start: query.start,
        end: query.end,
        format: query.format,
    })
}

fn post_table_data((state, path, json, query): (State<AppState>, Path<String>, Json<api::TableData>, Query<InsertTableDataQuery>)) -> AsyncResponse {
    println!("received message: {:?}", &json);
    //TODO: on duplicate - update (default), ignore, fail
    //TODO: implement
    http_response(&state,handlers::InsertTableData {
        name: path.to_string(),
        data: json.into_inner(),
        format: query.format,
    })
}

fn put_table_data((state, path, json): (State<AppState>, Path<(String, String)>, Json<u32>)) -> AsyncResponse {
    //TODO: implement
    result(Ok(
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(serde_json::to_string(&json!({ "error": "method not implemented" })).unwrap())
    )).responder()
}

fn delete_table_data((state, path): (State<AppState>, Path<(String, String)>)) -> AsyncResponse {
    //TODO: implement
    result(Ok(
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(serde_json::to_string(&json!({ "error": "method not implemented" })).unwrap())
    )).responder()
}


impl TableSession {

    fn handle_action(&self, ctx: &mut <Self as Actor>::Context, table_session_request: api::TableSessionRequest) {
        match table_session_request {
            api::TableSessionRequest::GetTable => {
                websocket_response(ctx, "getTable", handlers::GetTable {
                    name: self.table_name.to_string(),
                    detailed: false,
                })
            },
            api::TableSessionRequest::GetTableData { begin, end } => {
                websocket_response(ctx, "getTableData", handlers::GetTableData {
                    name: self.table_name.to_string(),
                    start: begin,
                    end: end,
                    format: api::FLAT_TABLE_DATA_FORMAT,
                })
            },
            api::TableSessionRequest::Create { data } => {
                websocket_response(ctx, "create", handlers::InsertTableData { //TODO: this is upsert
                    name: self.table_name.to_string(),
                    data: data.into_table_data(),
                    format: api::FLAT_TABLE_DATA_FORMAT,
                })
            },
            api::TableSessionRequest::Update { data } => {
                websocket_response(ctx, "update", handlers::InsertTableData { //TODO: this is upsert
                    name: self.table_name.to_string(),
                    data: data.into_table_data(),
                    format: api::FLAT_TABLE_DATA_FORMAT,
                })
            },
            api::TableSessionRequest::Delete { data } => {
                //TODO: implement me
                websocket_response(ctx, "delete",handlers::GetTable {
                    name: self.table_name.to_string(),
                    detailed: false,
                })
            },
        };

    }

}


impl StreamHandler<ws::Message, ws::ProtocolError> for TableSession {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        match msg {
            ws::Message::Text(text) => {
                // parse json
                serde_json::from_str(&text)
                    .or_else::<serde_json::error::Error, _>(|err| {
                        println!("Error occured while parsing websocket request: {:?}", err);
                        ctx.stop();
                        //TODO: send error message
                        Err(err)
                    })
                    .and_then(|table_session_request: api::TableSessionRequest| {
                        self.handle_action(ctx, table_session_request);
                        Ok(())
                    });
            },
            ws::Message::Close(_) => {
                ctx.stop();
            },
            _ => {}
        }
    }
}

fn websocket_table_listener(req: &HttpRequest<AppState>) -> Result<HttpResponse, ActixError> {
    let path = Path::<String>::extract(req);
    let table_name = path?.to_string();

    ws::start(req, TableSession::new(table_name))
}


fn index(state: State<AppState>) -> Result<NamedFile, ActixError> {
    let www_path = get_www_path();
    let mut path = PathBuf::from(www_path);
    path.push("index.html");
    Ok(NamedFile::open(path)?)
}

fn config(cfg: &mut JsonConfig<AppState>) -> () {
    cfg.limit(4096)
        .error_handler(|err, req| {
            println!("error: {:?}", err);
            let response =  HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(serde_json::to_string(&json!({ "error": err.to_string() }))
                    .unwrap_or_default());
            error::InternalError::from_response(
                err, response).into()
        });
}



pub fn serve() {

    dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let connection = vec![
        connection::create(&database_url),
        //TODO: a connection for each user
    ];


    actix_web::server::new(move || {
        let state = AppState::new(connection.clone(), "Kakapo");
        let www_path = get_www_path();

        App::with_state(state)
            .middleware(middleware::Logger::default())
            .configure(|app| Cors::for_app(app)
                //.allowed_origin("http://localhost:3000") //TODO: this doesn't work in the current version of cors middleware https://github.com/actix/actix-web/issues/603
                //.allowed_origin("http://localhost:8080")
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                .allowed_header(http::header::CONTENT_TYPE)
                .max_age(3600)
                .resource("/api/manage/table", |r| {
                    r.method(http::Method::GET).with(get_tables);
                    r.method(http::Method::POST).with_config(post_tables, |((_, cfg),)| config(cfg));
                })
                .resource("/api/manage/table/{table_name}", |r| {
                    r.method(http::Method::GET).with(get_table);
                    r.method(http::Method::PUT).with(put_table);
                    r.method(http::Method::DELETE).with(delete_table);
                })
                .resource("/api/table/{table_name}", |r| {
                    r.method(http::Method::GET).with(get_table_data);
                    r.method(http::Method::POST).with_config(post_table_data, |((_, _, cfg, _),)| config(cfg));
                })
                .resource("/api/table/{table_name}/{id}", |r| {
                    r.method(http::Method::PUT).with(put_table_data);
                    r.method(http::Method::DELETE).with(delete_table_data);
                })
                .resource("/api/listen/table/{table_name}", |r| {
                    r.method(http::Method::GET).f(websocket_table_listener)
                })
                .register())
            .resource("/", |r| {
                r.method(http::Method::GET).with(index)
            })
            .default_resource(|r| r.h(NormalizePath::default()))
            .handler(
                "/",
                fs::StaticFiles::new(PathBuf::from(www_path))
                    .unwrap()
                    .show_files_listing())
        })
        .workers(num_cpus::get())
        .keep_alive(None)
        .bind("127.0.0.1:8080")
        .unwrap()
        .shutdown_timeout(1)
        .start();

    println!("Started http server: 127.0.0.1:8080");
}