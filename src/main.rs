mod four_transports;
mod web_api_plane;
use std::{env, sync::Arc};

use axum::{
    Router,
    extract::{
        Form, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use maud::{DOCTYPE, Markup, html};
use sea_orm::{Database, DatabaseConnection};
use serde::Deserialize;
use tokio::sync::{RwLock, broadcast};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: Option<DatabaseConnection>,
    items: Arc<RwLock<Vec<Item>>>,
    events: broadcast::Sender<String>,
    supabase_url: Option<String>,
}

#[derive(Clone)]
struct Item {
    id: Uuid,
    title: String,
    detail: String,
}

#[derive(Deserialize)]
struct NewItem {
    title: String,
    detail: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let db = match env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(Database::connect(url).await?),
        _ => None,
    };
    let (events, _) = broadcast::channel(256);
    let state = AppState {
        db,
        items: Arc::new(RwLock::new(seed_items())),
        events,
        supabase_url: env::var("SUPABASE_URL").ok(),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(health))
            .route("/v1/data-plane/capabilities", axum::routing::get(|| async { axum::Json(crate::web_api_plane::capabilities()) }))
        .route(
            "/partials/reservations",
            get(items_partial).post(create_item),
        )
        .route("/ws", get(ws_upgrade))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = env::var("PORT").unwrap_or_else(|_| "8081".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index(State(state): State<AppState>) -> Html<String> {
    let items = state.items.read().await.clone();
    Html(layout(items_markup(&items)).into_string())
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    axum::Json(
        serde_json::json!({"status":"ok","database":state.db.is_some(),"supabase":state.supabase_url.is_some()}),
    )
}

async fn items_partial(State(state): State<AppState>) -> Html<String> {
    Html(items_markup(&state.items.read().await).into_string())
}

async fn create_item(State(state): State<AppState>, Form(input): Form<NewItem>) -> Html<String> {
    let item = Item {
        id: Uuid::new_v4(),
        title: input.title,
        detail: input.detail,
    };
    state.items.write().await.push(item.clone());
    let _ = state.events.send(format!("created:{}", item.id));
    Html(items_markup(&state.items.read().await).into_string())
}

fn layout(content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Hacker House Medellín" }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style { "body{font-family:system-ui;max-width:900px;margin:3rem auto;padding:0 1rem} form{display:grid;gap:.75rem} .card{border:1px solid #ddd;border-radius:12px;padding:1rem;margin:.75rem 0}" }
            }
            body {
                header { h1 { "Hacker House Medellín" } p { "Operations and community software for an entrepreneur-focused coliving and coworking house in Medellín, Colombia." } }
                form hx-post="/partials/reservations" hx-target="#items" hx-swap="innerHTML" {
                    input type="text" name="title" placeholder="Title" required;
                    textarea name="detail" placeholder="Details" required {}
                    button type="submit" { "Create Reservation" }
                }
                section id="items" { (content) }
                script { (maud::PreEscaped("const proto=location.protocol==='https:'?'wss':'ws';const ws=new WebSocket(proto+'://'+location.host+'/ws');ws.onmessage=()=>htmx.ajax('GET','/partials/reservations',{target:'#items'});")) }
            }
        }
    }
}

fn items_markup(items: &[Item]) -> Markup {
    html! { @for item in items { article class="card" data-id=(item.id) { h2 { (item.title) } p { (item.detail) } } } }
}

fn seed_items() -> Vec<Item> {
    vec![Item {
        id: Uuid::new_v4(),
        title: "Foundation ready".into(),
        detail: "Maud + Axum + SeaORM + Supabase configuration + HTMX + WebSockets".into(),
    }]
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket(socket, state))
}
async fn websocket(socket: WebSocket, state: AppState) {
    let (mut tx, mut rx) = socket.split();
    let mut events = state.events.subscribe();
    loop {
        tokio::select! {
            message = rx.next() => match message { Some(Ok(Message::Close(_))) | None => break, _ => {} },
            event = events.recv() => match event { Ok(event) => if tx.send(Message::Text(event.into())).await.is_err() { break; }, Err(_) => break },
        }
    }
}
