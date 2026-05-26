use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{FromRequestParts, Query, Request, State, WebSocketUpgrade};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use subtle::ConstantTimeEq;
use tokio::sync::broadcast;

use crate::http::RouterState;
use crate::keystrokes;
use crate::profiles;
use crate::state::{AppState, StateEvent};
use crate::types::{ALL_PROFILES, Command, ServerMessage, VERSION};

pub async fn ws_handler(
    Query(params): Query<HashMap<String, String>>,
    State(rs): State<RouterState>,
    request: Request,
) -> impl IntoResponse {
    // Regular browser GET (no Upgrade header) — serve the SPA entry point.
    // axum 0.8 dropped the `Option<WebSocketUpgrade>` extractor special-case,
    // so we sniff the Upgrade header manually before extracting.
    let is_ws_upgrade = request
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.eq_ignore_ascii_case("websocket"));

    if !is_ws_upgrade {
        return match crate::http::get_index_html() {
            Some(bytes) => {
                ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }

    let provided = params.get("t").map(String::as_str).unwrap_or("");
    let token = rs.app.token().await;

    if !bool::from(provided.as_bytes().ct_eq(token.as_bytes())) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let (mut parts, _body) = request.into_parts();
    let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws,
        Err(rej) => return rej.into_response(),
    };

    ws.on_upgrade(move |socket| handle_socket(socket, rs.app))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let bindings = profiles::action_bindings();
    let hello = ServerMessage::Hello {
        version: VERSION,
        profiles: ALL_PROFILES,
        bindings: &bindings,
    };
    if send_msg(&mut socket, &hello).await.is_err() {
        return;
    }

    let active = state.is_active().await;
    if send_msg(&mut socket, &ServerMessage::State { active })
        .await
        .is_err()
    {
        return;
    }

    let mut rx = state.subscribe();

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text)))
                        if handle_command(text.as_str(), &mut socket, &state).await.is_err() =>
                    {
                        break;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(StateEvent::ActiveChanged(active)) => {
                        if send_msg(&mut socket, &ServerMessage::State { active }).await.is_err() {
                            break;
                        }
                    }
                    Ok(StateEvent::PairingUrlRefreshed) => {} // tray-only concern
                    Err(broadcast::error::RecvError::Lagged(_)) => {} // continue
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_command(text: &str, socket: &mut WebSocket, state: &AppState) -> Result<(), ()> {
    let cmd: Command = match serde_json::from_str(text) {
        Ok(c) => c,
        Err(e) => {
            let msg = ServerMessage::Error {
                message: format!("invalid command: {e}"),
            };
            return send_msg(socket, &msg).await;
        }
    };

    // Mouse and text input commands bypass the is_active gate — they are always intentional
    let requires_active = matches!(
        cmd,
        Command::Key { .. } | Command::Combo { .. } | Command::Action { .. }
    );

    if requires_active && !state.is_active().await {
        return send_msg(
            socket,
            &ServerMessage::Ack {
                suppressed: Some(true),
            },
        )
        .await;
    }

    let result = tokio::task::spawn_blocking(move || dispatch(cmd)).await;

    match result {
        Ok(Ok(())) => send_msg(socket, &ServerMessage::Ack { suppressed: None }).await,
        Ok(Err(e)) => send_msg(socket, &ServerMessage::Error { message: e }).await,
        Err(_) => {
            send_msg(
                socket,
                &ServerMessage::Error {
                    message: "internal error".into(),
                },
            )
            .await
        }
    }
}

fn dispatch(cmd: Command) -> Result<(), String> {
    match cmd {
        Command::Key { key, mods } => keystrokes::tap(key, &mods),
        Command::Combo { keys } => keystrokes::combo(&keys),
        Command::Action { name, profile } => {
            let recipe = profiles::resolve_action(profile, name)
                .ok_or_else(|| format!("no mapping for action {name:?} in profile {profile:?}"))?;
            if let Some(combo) = &recipe.combo {
                keystrokes::combo(combo)
            } else if let Some(key) = recipe.key {
                keystrokes::tap(key, &recipe.mods)
            } else {
                Err("empty recipe".into())
            }
        }
        Command::MouseMove { dx, dy } => keystrokes::mouse_move(dx, dy),
        Command::MouseClick { button } => keystrokes::mouse_click(button),
        Command::MouseButton { button, action } => keystrokes::mouse_button(button, action),
        Command::MouseScroll { dx, dy } => keystrokes::mouse_scroll(dx, dy),
        Command::TypeText { text } => keystrokes::type_text(&text),
    }
}

async fn send_msg(socket: &mut WebSocket, msg: &ServerMessage<'_>) -> Result<(), ()> {
    let text = serde_json::to_string(msg).map_err(|_| ())?;
    socket.send(Message::Text(text.into())).await.map_err(|_| ())
}
