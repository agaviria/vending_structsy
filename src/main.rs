use axum::{
    extract::{rejection::JsonRejection, FromRequest, Path, Request, State},
    http::{HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{error_span, field};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use structsy::{derive::Persistent, Structsy, StructsyError, StructsyTx};

#[derive(Debug)]
enum AppError {
    // The request body contained invalid JSON
    JsonRejection(JsonRejection),
    StructsyError(StructsyError), // Database error
    IOError(std::io::Error),
}

impl From<StructsyError> for AppError {
    fn from(e: structsy::StructsyError) -> Self {
        AppError::StructsyError(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::IOError(e)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone)]
pub struct AppStateT {
    pub connection: Structsy,
}

pub type AppState = Arc<AppStateT>;

// Create our own JSON extractor by wrapping `axum::Json`. This makes it easy to override the
// rejection and provide our own which formats errors to match our application.
//
// `axum::Json` responds with plain text if the input is invalid.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
struct AppJson<T>(T);

impl<T> IntoResponse for AppJson<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct ErrorResponse {
            message: String,
        }

        let (status, message) = match self {
            AppError::JsonRejection(rejection) => {
                tracing::error!("bad user input -> {:?}", rejection.body_text());
                (rejection.status(), rejection.body_text())
            }
            AppError::StructsyError(err) => {
                tracing::error!("DB error -> {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
            AppError::IOError(err) => {
                tracing::error!("I/O error -> {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "something went wrong.  Try agin later!".to_owned(),
                )
            }
        };

        (status, AppJson(ErrorResponse { message })).into_response()
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        Self::JsonRejection(rejection)
    }
}

#[derive(Serialize, Deserialize, Persistent)]
struct Coffee {
    brand: String,
    size: u32,
    time: String,
}

#[derive(Serialize, Deserialize)]
struct CoffeeItem {
    id: String,
    coffee: Coffee,
}

#[derive(Serialize, Deserialize)]
struct CoffeeList {
    coffees: Vec<CoffeeItem>,
}

#[derive(Serialize, Deserialize, Persistent)]
struct Beer {
    brand: String,
    size: u32,
    time: String,
}

#[derive(Serialize, Deserialize)]
struct BeerItem {
    id: String,
    beer: Beer,
}

#[derive(Serialize, Deserialize)]
struct BeerList {
    beers: Vec<BeerItem>,
}

async fn drink_coffee(
    State(state): State<AppState>,
    AppJson(coffee): AppJson<Coffee>,
) -> Result<(), AppError> {
    state.connection.define::<Coffee>()?;
    let mut tx = state.connection.begin()?;
    tx.insert(&coffee)?;
    tx.commit()?;
    Ok(())
}

async fn list_coffees(State(state): State<AppState>) -> Result<AppJson<CoffeeList>, AppError> {
    let mut coffees = Vec::new();
    for (id, coffee) in state.connection.scan::<Coffee>()? {
        coffees.push(CoffeeItem {
            id: id.to_string(),
            coffee,
        });
    }
    Ok(AppJson(CoffeeList { coffees }))
}

async fn update_coffee(
    Path(id): Path<String>,
    State(state): State<AppState>,
    AppJson(coffee): AppJson<Coffee>,
) -> Result<(), AppError> {
    let p_id: structsy::Ref<Coffee> = id.parse()?;
    let mut tx = state.connection.begin()?;
    tx.update(&p_id, &coffee)?;
    tx.commit()?;
    Ok(())
}

async fn delete_coffee(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<(), AppError> {
    let p_id: structsy::Ref<Coffee> = id.parse()?;
    let mut tx = state.connection.begin()?;
    tx.delete(&p_id)?;
    tx.commit()?;
    Ok(())
}

async fn drink_beer(
    State(state): State<AppState>,
    AppJson(beer): AppJson<Beer>,
) -> Result<(), AppError> {
    state.connection.define::<Beer>()?;
    let mut tx = state.connection.begin()?;
    tx.insert(&beer)?;
    tx.commit()?;
    Ok(())
}

async fn list_beers(State(state): State<AppState>) -> Result<AppJson<BeerList>, AppError> {
    let mut beers = Vec::new();
    for (id, beer) in state.connection.scan::<Beer>()? {
        beers.push(BeerItem {
            id: id.to_string(),
            beer,
        });
    }
    Ok(AppJson(BeerList { beers }))
}

async fn update_beer(
    Path(id): Path<String>,
    State(state): State<AppState>,
    AppJson(beer): AppJson<Beer>,
) -> Result<(), AppError> {
    let p_id: structsy::Ref<Beer> = id.parse()?;
    let mut tx = state.connection.begin()?;
    tx.update(&p_id, &beer)?;
    tx.commit()?;
    Ok(())
}
async fn delete_beer(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<(), AppError> {
    let p_id: structsy::Ref<Beer> = id.parse()?;
    let mut tx = state.connection.begin()?;
    tx.delete(&p_id)?;
    tx.commit()?;
    Ok(())
}

pub async fn create_router(state: AppState) {
    let coffee_routes = Router::new()
        .route("/create", post(drink_coffee))
        .with_state(state.clone())
        .route("/list", get(list_coffees))
        .with_state(state.clone())
        .route("/update/:id", post(update_coffee))
        .with_state(state.clone())
        .route("/delete/:id", delete(delete_coffee))
        .with_state(state.clone());

    let beer_routes = Router::new()
        .route("/create", post(drink_beer))
        .with_state(state.clone())
        .route("/list", get(list_beers))
        .with_state(state.clone())
        .route("/update/:id", post(update_beer))
        .with_state(state.clone())
        .route("/delete/:id", delete(delete_beer))
        .with_state(state.clone());

    let mut app = Router::new()
        .with_state(state.clone())
        .nest("/coffee", coffee_routes)
        .nest("/beer", beer_routes);

    let x_request_id = HeaderName::from_static("x-request-id");

    // Basic access logging
    app = app.layer(
        TraceLayer::new_for_http().make_span_with(move |req: &Request<_>| {
            const REQUEST_ID: &str = "request_id";

            let method = req.method();
            let uri = req.uri();
            let request_id = req
                .headers()
                .get(&x_request_id)
                .and_then(|id| id.to_str().ok());

            let span = error_span!("request", %method, %uri, { REQUEST_ID } = field::Empty);

            if let Some(request_id) = request_id {
                span.record(REQUEST_ID, field::display(request_id));
            }

            span
        }),
    );

    let x_request_id = HeaderName::from_static("x-request-id");

    // propagate `x-request-id` headers from request to response
    app = app.layer(PropagateRequestIdLayer::new(x_request_id.clone()));

    app = app.layer(SetRequestIdLayer::new(
        x_request_id.clone(),
        MakeRequestUuid::default(),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_error_handling=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let connection = Structsy::open(Structsy::config("./track.db").create(true)).unwrap();
    let state = AppState::new(AppStateT { connection });

    let app = create_router(state).await;
    tracing::info!("Listening on port: 3000");
    app
}
