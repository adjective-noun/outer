//! WebSocket server handler

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::crdt::{Participant, ParticipantKind, ParticipantStatus, RoomEvent};
use crate::delegation::{Capability, WorkItemStatus};
use crate::delegation::work_item::WorkPriority;
use crate::delegation::capability::CapabilitySet;
use crate::error;
use crate::models::{BlockStatus, BlockType};
use crate::opencode::{OpenCodeClient, SendMessageRequest, StreamEvent};
use crate::AppState;

/// WebSocket handler
pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Connection state for tracking subscriptions and delegation
struct ConnectionState {
    /// Map of journal_id -> participant_id for this connection (CRDT presence)
    subscriptions: std::collections::HashMap<Uuid, Uuid>,
    /// The registered participant ID for delegation (per journal)
    /// Map of journal_id -> registered_participant_id
    delegation_registrations: std::collections::HashMap<Uuid, Uuid>,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            subscriptions: std::collections::HashMap::new(),
            delegation_registrations: std::collections::HashMap::new(),
        }
    }
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));

    // Get OpenCode URL from environment
    let opencode_url =
        std::env::var("OPENCODE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let opencode = OpenCodeClient::new(opencode_url);

    // Connection state
    let conn_state = Arc::new(Mutex::new(ConnectionState::new()));

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
        };

        // Parse client message
        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let error = ServerMessage::Error {
                    message: format!("Invalid message: {}", e),
                };
                let mut sender = sender.lock().await;
                if let Err(e) = sender
                    .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                    .await
                {
                    tracing::error!("Failed to send error: {}", e);
                }
                continue;
            }
        };

        // Handle message
        match client_msg {
            ClientMessage::Submit {
                journal_id,
                content,
                session_id,
            } => {
                let mut sender_guard = sender.lock().await;
                if let Err(e) = handle_submit(
                    &mut sender_guard,
                    &state,
                    &opencode,
                    journal_id,
                    content,
                    session_id,
                )
                .await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender_guard
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::CreateJournal { title } => {
                match state.store.create_journal(title).await {
                    Ok(journal) => {
                        let msg = ServerMessage::JournalCreated {
                            journal_id: journal.id,
                            title: journal.title,
                        };
                        let mut sender = sender.lock().await;
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send journal created: {}", e);
                        }
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send error: {}", e);
                        }
                    }
                }
            }
            ClientMessage::GetJournal { journal_id } => {
                match state.store.get_journal(journal_id).await {
                    Ok(journal) => {
                        let blocks = state
                            .store
                            .get_blocks_for_journal(journal_id)
                            .await
                            .unwrap_or_default();
                        let msg = ServerMessage::Journal { journal, blocks };
                        let mut sender = sender.lock().await;
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send journal: {}", e);
                        }
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send error: {}", e);
                        }
                    }
                }
            }
            ClientMessage::ListJournals => match state.store.list_journals().await {
                Ok(journals) => {
                    let msg = ServerMessage::Journals { journals };
                    let mut sender = sender.lock().await;
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send journals: {}", e);
                    }
                }
                Err(e) => {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    let mut sender = sender.lock().await;
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            },
            ClientMessage::Fork {
                block_id,
                session_id,
            } => {
                let mut sender_guard = sender.lock().await;
                if let Err(e) =
                    handle_fork(&mut sender_guard, &state, &opencode, block_id, session_id).await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender_guard
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::Rerun {
                block_id,
                session_id,
            } => {
                let mut sender_guard = sender.lock().await;
                if let Err(e) =
                    handle_rerun(&mut sender_guard, &state, &opencode, block_id, session_id).await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender_guard
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::Cancel { block_id } => {
                let mut sender_guard = sender.lock().await;
                if let Err(e) = handle_cancel(&mut sender_guard, &state, block_id).await {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender_guard
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::Subscribe {
                journal_id,
                name,
                kind,
            } => {
                handle_subscribe(
                    Arc::clone(&sender),
                    &state,
                    Arc::clone(&conn_state),
                    journal_id,
                    name,
                    kind,
                )
                .await;
            }
            ClientMessage::Unsubscribe { journal_id } => {
                handle_unsubscribe(
                    Arc::clone(&sender),
                    &state,
                    Arc::clone(&conn_state),
                    journal_id,
                )
                .await;
            }
            ClientMessage::Cursor {
                journal_id,
                block_id,
                offset,
            } => {
                let conn = conn_state.lock().await;
                if let Some(&participant_id) = conn.subscriptions.get(&journal_id) {
                    if let Some(room) = state.room_manager.get(journal_id).await {
                        room.update_cursor(participant_id, block_id, offset).await;
                    }
                }
            }
            ClientMessage::GetPresence { journal_id } => {
                if let Some(room) = state.room_manager.get(journal_id).await {
                    let participants = room.participants().await;
                    let msg = ServerMessage::Presence {
                        journal_id,
                        participants,
                    };
                    let mut sender = sender.lock().await;
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send presence: {}", e);
                    }
                } else {
                    let msg = ServerMessage::Presence {
                        journal_id,
                        participants: vec![],
                    };
                    let mut sender = sender.lock().await;
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                        .await;
                }
            }
            ClientMessage::CrdtUpdate { journal_id, update } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.subscriptions.get(&journal_id).copied();
                drop(conn);

                if let Some(room) = state.room_manager.get(journal_id).await {
                    match base64_decode(&update) {
                        Ok(update_bytes) => {
                            if let Err(e) = room.apply_update(participant_id, &update_bytes).await {
                                tracing::error!("Failed to apply CRDT update: {:?}", e);
                            }
                        }
                        Err(e) => {
                            let error = ServerMessage::Error {
                                message: format!("Invalid base64 update: {}", e),
                            };
                            let mut sender = sender.lock().await;
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                                .await;
                        }
                    }
                }
            }
            ClientMessage::SyncRequest {
                journal_id,
                state_vector,
            } => {
                if let Some(room) = state.room_manager.get(journal_id).await {
                    let state_data = match state_vector {
                        Some(sv) => {
                            match base64_decode(&sv) {
                                Ok(sv_bytes) => room.doc().encode_diff(&sv_bytes).ok(),
                                Err(_) => Some(room.get_sync_state()),
                            }
                        }
                        None => Some(room.get_sync_state()),
                    };

                    if let Some(data) = state_data {
                        let msg = ServerMessage::SyncState {
                            journal_id,
                            state: base64_encode(&data),
                        };
                        let mut sender = sender.lock().await;
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send sync state: {}", e);
                        }
                    }
                }
            }
            // --- Delegation handlers ---
            ClientMessage::RegisterParticipant {
                journal_id,
                name,
                kind,
                capabilities,
            } => {
                let participant_kind = kind
                    .as_deref()
                    .and_then(|k| k.parse().ok())
                    .unwrap_or(ParticipantKind::User);

                let participant = Participant::new(&name, participant_kind);

                let registered = if let Some(caps) = capabilities {
                    let cap_set: CapabilitySet = caps
                        .iter()
                        .filter_map(|s| s.parse::<Capability>().ok())
                        .collect::<Vec<_>>()
                        .into();
                    state
                        .delegation_manager
                        .register_participant_with_capabilities(participant, cap_set)
                        .await
                } else {
                    state.delegation_manager.register_participant(participant).await
                };

                // Store registration
                {
                    let mut conn = conn_state.lock().await;
                    conn.delegation_registrations.insert(journal_id, registered.id());
                }

                let msg = ServerMessage::ParticipantRegistered {
                    participant_id: registered.id(),
                    name: registered.name().to_string(),
                    kind: registered.kind().as_str().to_string(),
                    capabilities: registered.capabilities.to_vec().iter().map(|c| c.as_str().to_string()).collect(),
                };
                let mut sender = sender.lock().await;
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await;
            }
            ClientMessage::Delegate {
                journal_id,
                description,
                assignee_id,
                block_id: _,
                priority,
                requires_approval,
                approver_id,
            } => {
                let conn = conn_state.lock().await;
                let delegator_id = match conn.delegation_registrations.get(&journal_id) {
                    Some(&id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };
                drop(conn);

                let priority = priority
                    .as_deref()
                    .and_then(|p| p.parse::<WorkPriority>().ok());

                match state
                    .delegation_manager
                    .delegate(
                        journal_id,
                        description,
                        delegator_id,
                        assignee_id,
                        priority,
                        requires_approval,
                        approver_id,
                    )
                    .await
                {
                    Ok(work_item) => {
                        let msg = ServerMessage::WorkDelegated { work_item };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::AcceptWork { work_item_id } => {
                let conn = conn_state.lock().await;
                // Find the participant ID (from any journal registration)
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.accept_work(work_item_id, participant_id).await {
                    Ok(_) => {
                        let msg = ServerMessage::WorkAccepted {
                            work_item_id,
                            assignee_id: participant_id,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::DeclineWork { work_item_id } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.decline_work(work_item_id, participant_id).await {
                    Ok(_) => {
                        let msg = ServerMessage::WorkDeclined {
                            work_item_id,
                            assignee_id: participant_id,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::SubmitWork { work_item_id, result } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.submit_work(work_item_id, participant_id, result).await {
                    Ok(work_item) => {
                        if work_item.status == WorkItemStatus::AwaitingApproval {
                            // Get the approval request
                            let approvals = state.delegation_manager.get_approval_queue(work_item.get_approver_id()).await;
                            if let Some(approval) = approvals.iter().find(|a| a.work_item_id == work_item_id) {
                                let msg = ServerMessage::ApprovalRequested {
                                    approval: approval.clone(),
                                    work_item: work_item.clone(),
                                };
                                let mut sender = sender.lock().await;
                                let _ = sender
                                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                                    .await;
                            }
                        } else {
                            let msg = ServerMessage::WorkApproved {
                                work_item_id,
                                approver_id: work_item.delegator_id,
                                feedback: None,
                            };
                            let mut sender = sender.lock().await;
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                                .await;
                        }
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::ApproveWork { approval_id, feedback } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.approve(approval_id, participant_id, feedback.clone()).await {
                    Ok((_, work_item)) => {
                        let msg = ServerMessage::WorkApproved {
                            work_item_id: work_item.id,
                            approver_id: participant_id,
                            feedback,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::RejectWork { approval_id, feedback } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.reject(approval_id, participant_id, &feedback).await {
                    Ok((_, work_item)) => {
                        let msg = ServerMessage::WorkRejected {
                            work_item_id: work_item.id,
                            approver_id: participant_id,
                            feedback,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::CancelWork { work_item_id } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.cancel_work(work_item_id, participant_id).await {
                    Ok(_) => {
                        let msg = ServerMessage::WorkCancelled {
                            work_item_id,
                            cancelled_by: participant_id,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::ClaimWork { work_item_id } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.claim_work(work_item_id, participant_id).await {
                    Ok(_) => {
                        let msg = ServerMessage::WorkClaimed {
                            work_item_id,
                            claimed_by: participant_id,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::GetWorkQueue => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let items = if let Some(id) = participant_id {
                    state.delegation_manager.get_work_queue(id).await
                } else {
                    vec![]
                };

                let msg = ServerMessage::WorkQueue { items };
                let mut sender = sender.lock().await;
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await;
            }
            ClientMessage::GetApprovalQueue => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let items = if let Some(id) = participant_id {
                    state.delegation_manager.get_approval_queue(id).await
                } else {
                    vec![]
                };

                let msg = ServerMessage::ApprovalQueue { items };
                let mut sender = sender.lock().await;
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await;
            }
            ClientMessage::SetAcceptingWork { accepting } => {
                let conn = conn_state.lock().await;
                let participant_id = conn.delegation_registrations.values().next().copied();
                drop(conn);

                let participant_id = match participant_id {
                    Some(id) => id,
                    None => {
                        let error = ServerMessage::Error {
                            message: "Not registered with delegation system".to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match state.delegation_manager.set_accepting_work(participant_id, accepting).await {
                    Ok(_) => {
                        let msg = ServerMessage::AcceptingWorkChanged {
                            participant_id,
                            accepting,
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        let mut sender = sender.lock().await;
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await;
                    }
                }
            }
            ClientMessage::GetParticipants { journal_id: _ } => {
                let participants = state.delegation_manager.list_available_participants().await;
                let msg = ServerMessage::AvailableParticipants { participants };
                let mut sender = sender.lock().await;
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await;
            }
        }
    }

    // Cleanup: Leave all subscribed rooms and unregister from delegation
    let conn = conn_state.lock().await;
    for (journal_id, participant_id) in conn.subscriptions.iter() {
        if let Some(room) = state.room_manager.get(*journal_id).await {
            room.leave(*participant_id).await;
        }
    }
    // Unregister from delegation system
    for (_, participant_id) in conn.delegation_registrations.iter() {
        state.delegation_manager.unregister_participant(*participant_id).await;
    }
}

/// Handle subscription to a journal
async fn handle_subscribe(
    sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
    state: &Arc<AppState>,
    conn_state: Arc<Mutex<ConnectionState>>,
    journal_id: Uuid,
    name: String,
    kind: Option<String>,
) {
    let participant_kind = kind
        .as_deref()
        .and_then(|k| k.parse().ok())
        .unwrap_or(ParticipantKind::User);

    let room = state.room_manager.get_or_create(journal_id).await;
    let participant = room.join(name, participant_kind).await;
    let participant_id = participant.id;

    // Store subscription
    {
        let mut conn = conn_state.lock().await;
        conn.subscriptions.insert(journal_id, participant_id);
    }

    // Get current participants
    let participants = room.participants().await;

    // Send subscribed confirmation
    let msg = ServerMessage::Subscribed {
        journal_id,
        participant: participant.clone(),
        participants,
    };
    {
        let mut sender_guard = sender.lock().await;
        if let Err(e) = sender_guard
            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
            .await
        {
            tracing::error!("Failed to send subscribed: {}", e);
        }
    }

    // Spawn task to forward room events to this client
    let mut room_rx = room.subscribe();
    let sender_clone = Arc::clone(&sender);

    tokio::spawn(async move {
        while let Ok(event) = room_rx.recv().await {
            let server_msg = match event {
                RoomEvent::ParticipantJoined(p) => {
                    // Don't send our own join event
                    if p.id == participant_id {
                        continue;
                    }
                    Some(ServerMessage::ParticipantJoined {
                        journal_id,
                        participant: p,
                    })
                }
                RoomEvent::ParticipantLeft { participant_id: pid } => {
                    Some(ServerMessage::ParticipantLeft {
                        journal_id,
                        participant_id: pid,
                    })
                }
                RoomEvent::CursorMoved {
                    participant_id: pid,
                    block_id,
                    offset,
                } => {
                    // Don't echo our own cursor moves
                    if pid == participant_id {
                        continue;
                    }
                    Some(ServerMessage::CursorMoved {
                        journal_id,
                        participant_id: pid,
                        block_id,
                        offset,
                    })
                }
                RoomEvent::StatusChanged {
                    participant_id: pid,
                    status,
                } => Some(ServerMessage::ParticipantStatusChanged {
                    journal_id,
                    participant_id: pid,
                    status,
                }),
                RoomEvent::CrdtUpdate { source, update } => {
                    // Don't echo our own updates
                    if source == Some(participant_id) {
                        continue;
                    }
                    Some(ServerMessage::CrdtUpdate {
                        journal_id,
                        source,
                        update: base64_encode(&update),
                    })
                }
                RoomEvent::SyncState { state } => Some(ServerMessage::SyncState {
                    journal_id,
                    state: base64_encode(&state),
                }),
            };

            if let Some(msg) = server_msg {
                let mut sender_guard = sender_clone.lock().await;
                if sender_guard
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .is_err()
                {
                    // Connection closed
                    break;
                }
            }
        }
    });
}

/// Handle unsubscription from a journal
async fn handle_unsubscribe(
    sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
    state: &Arc<AppState>,
    conn_state: Arc<Mutex<ConnectionState>>,
    journal_id: Uuid,
) {
    let participant_id = {
        let mut conn = conn_state.lock().await;
        conn.subscriptions.remove(&journal_id)
    };

    if let Some(pid) = participant_id {
        if let Some(room) = state.room_manager.get(journal_id).await {
            room.leave(pid).await;
        }
    }

    let msg = ServerMessage::Unsubscribed { journal_id };
    let mut sender = sender.lock().await;
    let _ = sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await;
}

/// Base64 encode helper
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Base64 decode helper
fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Result<u8, String> {
        if c == b'=' {
            return Ok(0);
        }
        ALPHABET
            .iter()
            .position(|&x| x == c)
            .map(|p| p as u8)
            .ok_or_else(|| format!("Invalid base64 character: {}", c as char))
    }

    let data = data.trim();
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let bytes: Vec<u8> = data.bytes().filter(|&b| b != b'\n' && b != b'\r').collect();
    if bytes.len() % 4 != 0 {
        return Err("Invalid base64 length".to_string());
    }

    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);

    for chunk in bytes.chunks(4) {
        let b0 = decode_char(chunk[0])?;
        let b1 = decode_char(chunk[1])?;
        let b2 = decode_char(chunk[2])?;
        let b3 = decode_char(chunk[3])?;

        result.push((b0 << 2) | (b1 >> 4));

        if chunk[2] != b'=' {
            result.push((b1 << 4) | (b2 >> 2));
        }

        if chunk[3] != b'=' {
            result.push((b2 << 6) | b3);
        }
    }

    Ok(result)
}

async fn handle_submit(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    journal_id: Uuid,
    content: String,
    session_id: Option<String>,
) -> error::Result<()> {
    // Create user block
    let user_block = state
        .store
        .create_block(journal_id, BlockType::User, &content)
        .await?;

    // Send block created
    let msg = ServerMessage::BlockCreated {
        block: user_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Create assistant block (pending)
    let assistant_block = state
        .store
        .create_block(journal_id, BlockType::Assistant, "")
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Update block to streaming
    state
        .store
        .update_block_status(assistant_block.id, BlockStatus::Streaming)
        .await?;

    let msg = ServerMessage::BlockStatusChanged {
        block_id: assistant_block.id,
        status: BlockStatus::Streaming,
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Stream response from OpenCode
    let mut stream = opencode
        .send_message(&session_id, SendMessageRequest { content })
        .await?;

    let mut full_content = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Content(content_event)) => {
                full_content.push_str(&content_event.text);

                // Send streaming update
                let msg = ServerMessage::BlockContentDelta {
                    block_id: assistant_block.id,
                    delta: content_event.text,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Done) => {
                // Update block to complete
                state
                    .store
                    .update_block_content(assistant_block.id, &full_content)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Complete)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Complete,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Error(error_event)) => {
                // Update block to error
                state
                    .store
                    .update_block_content(assistant_block.id, &error_event.message)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Error,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Unknown { .. }) => {
                // Ignore unknown events
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                // Update block to error
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_fork(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    block_id: Uuid,
    session_id: Option<String>,
) -> error::Result<()> {
    // Fork creates a new user block with the same content, branching from the original
    let forked_block = state.store.fork_block(block_id).await?;

    // Send block forked notification
    let msg = ServerMessage::BlockForked {
        original_block_id: block_id,
        new_block: forked_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Now execute the fork by sending to OpenCode (reusing submit logic)
    // Create assistant block for the response
    let assistant_block = state
        .store
        .create_block_with_lineage(
            forked_block.journal_id,
            BlockType::Assistant,
            "",
            Some(forked_block.id),
            None,
        )
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Stream response from OpenCode
    stream_response(sender, state, opencode, &session_id, assistant_block, &forked_block.content)
        .await
}

async fn handle_rerun(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    block_id: Uuid,
    session_id: Option<String>,
) -> error::Result<()> {
    // Rerun creates a new execution of the same prompt
    let rerun_block = state.store.rerun_block(block_id).await?;

    // Send block created notification
    let msg = ServerMessage::BlockCreated {
        block: rerun_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Create assistant block for the response
    let assistant_block = state
        .store
        .create_block_with_lineage(
            rerun_block.journal_id,
            BlockType::Assistant,
            "",
            Some(rerun_block.id),
            None,
        )
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Stream response from OpenCode
    stream_response(sender, state, opencode, &session_id, assistant_block, &rerun_block.content)
        .await
}

async fn handle_cancel(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    block_id: Uuid,
) -> error::Result<()> {
    // Update block status to error (cancelled)
    state
        .store
        .update_block_status(block_id, BlockStatus::Error)
        .await?;

    let msg = ServerMessage::BlockCancelled { block_id };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    Ok(())
}

/// Helper function to stream response from OpenCode
async fn stream_response(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    session_id: &str,
    assistant_block: crate::models::Block,
    content: &str,
) -> error::Result<()> {
    // Update block to streaming
    state
        .store
        .update_block_status(assistant_block.id, BlockStatus::Streaming)
        .await?;

    let msg = ServerMessage::BlockStatusChanged {
        block_id: assistant_block.id,
        status: BlockStatus::Streaming,
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Stream response from OpenCode
    let mut stream = opencode
        .send_message(session_id, SendMessageRequest {
            content: content.to_string(),
        })
        .await?;

    let mut full_content = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Content(content_event)) => {
                full_content.push_str(&content_event.text);

                let msg = ServerMessage::BlockContentDelta {
                    block_id: assistant_block.id,
                    delta: content_event.text,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Done) => {
                state
                    .store
                    .update_block_content(assistant_block.id, &full_content)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Complete)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Complete,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Error(error_event)) => {
                state
                    .store
                    .update_block_content(assistant_block.id, &error_event.message)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Error,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Unknown { .. }) => {
                // Ignore unknown events
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;
            }
        }
    }

    Ok(())
}

/// Messages from client to server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Submit a prompt
    Submit {
        journal_id: Uuid,
        content: String,
        session_id: Option<String>,
    },
    /// Create a new journal
    CreateJournal { title: Option<String> },
    /// Get a journal with its blocks
    GetJournal { journal_id: Uuid },
    /// List all journals
    ListJournals,
    /// Fork a block (create new session from a branch point)
    Fork {
        block_id: Uuid,
        session_id: Option<String>,
    },
    /// Re-run a block (same prompt, new execution)
    Rerun {
        block_id: Uuid,
        session_id: Option<String>,
    },
    /// Cancel a streaming block
    Cancel { block_id: Uuid },
    /// Subscribe to a journal for real-time updates
    Subscribe {
        journal_id: Uuid,
        name: String,
        #[serde(default)]
        kind: Option<String>,
    },
    /// Unsubscribe from a journal
    Unsubscribe { journal_id: Uuid },
    /// Update cursor position
    Cursor {
        journal_id: Uuid,
        block_id: Option<Uuid>,
        offset: Option<u32>,
    },
    /// Request presence information for a journal
    GetPresence { journal_id: Uuid },
    /// Apply a CRDT update
    CrdtUpdate {
        journal_id: Uuid,
        /// Base64-encoded update data
        update: String,
    },
    /// Request sync state for a journal
    SyncRequest {
        journal_id: Uuid,
        /// Base64-encoded state vector (optional, for diff sync)
        state_vector: Option<String>,
    },
    // --- Delegation messages ---
    /// Register as a participant with the delegation system
    RegisterParticipant {
        journal_id: Uuid,
        name: String,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        capabilities: Option<Vec<String>>,
    },
    /// Delegate work to another participant
    Delegate {
        journal_id: Uuid,
        description: String,
        assignee_id: Uuid,
        #[serde(default)]
        block_id: Option<Uuid>,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        requires_approval: bool,
        #[serde(default)]
        approver_id: Option<Uuid>,
    },
    /// Accept delegated work
    AcceptWork { work_item_id: Uuid },
    /// Decline delegated work
    DeclineWork { work_item_id: Uuid },
    /// Submit completed work (optionally for approval)
    SubmitWork {
        work_item_id: Uuid,
        result: String,
    },
    /// Approve completed work
    ApproveWork {
        approval_id: Uuid,
        #[serde(default)]
        feedback: Option<String>,
    },
    /// Reject completed work
    RejectWork {
        approval_id: Uuid,
        feedback: String,
    },
    /// Cancel delegated work (by delegator)
    CancelWork { work_item_id: Uuid },
    /// Claim unassigned work
    ClaimWork { work_item_id: Uuid },
    /// Get participant's work queue
    GetWorkQueue,
    /// Get participant's pending approvals
    GetApprovalQueue,
    /// Set whether accepting work
    SetAcceptingWork { accepting: bool },
    /// Get list of available participants for delegation
    GetParticipants { journal_id: Uuid },
}

/// Messages from server to client
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Journal was created
    JournalCreated { journal_id: Uuid, title: String },
    /// Journal with blocks
    Journal {
        journal: crate::models::Journal,
        blocks: Vec<crate::models::Block>,
    },
    /// List of journals
    Journals {
        journals: Vec<crate::models::Journal>,
    },
    /// Block was created
    BlockCreated { block: crate::models::Block },
    /// Block content delta (streaming)
    BlockContentDelta { block_id: Uuid, delta: String },
    /// Block status changed
    BlockStatusChanged {
        block_id: Uuid,
        status: BlockStatus,
    },
    /// Block was forked
    BlockForked {
        original_block_id: Uuid,
        new_block: crate::models::Block,
    },
    /// Block was cancelled
    BlockCancelled { block_id: Uuid },
    /// Error occurred
    Error { message: String },
    /// Successfully subscribed to a journal
    Subscribed {
        journal_id: Uuid,
        participant: Participant,
        /// Current participants in the room
        participants: Vec<Participant>,
    },
    /// Unsubscribed from a journal
    Unsubscribed { journal_id: Uuid },
    /// A participant joined the journal
    ParticipantJoined {
        journal_id: Uuid,
        participant: Participant,
    },
    /// A participant left the journal
    ParticipantLeft {
        journal_id: Uuid,
        participant_id: Uuid,
    },
    /// A participant's cursor moved
    CursorMoved {
        journal_id: Uuid,
        participant_id: Uuid,
        block_id: Option<Uuid>,
        offset: Option<u32>,
    },
    /// A participant's status changed
    ParticipantStatusChanged {
        journal_id: Uuid,
        participant_id: Uuid,
        status: ParticipantStatus,
    },
    /// Presence information for a journal
    Presence {
        journal_id: Uuid,
        participants: Vec<Participant>,
    },
    /// CRDT update to apply
    CrdtUpdate {
        journal_id: Uuid,
        source: Option<Uuid>,
        /// Base64-encoded update data
        update: String,
    },
    /// Full sync state
    SyncState {
        journal_id: Uuid,
        /// Base64-encoded state data
        state: String,
    },
    // --- Delegation messages ---
    /// Participant was registered with delegation system
    ParticipantRegistered {
        participant_id: Uuid,
        name: String,
        kind: String,
        capabilities: Vec<String>,
    },
    /// Work was delegated
    WorkDelegated {
        work_item: crate::delegation::WorkItem,
    },
    /// Work was accepted
    WorkAccepted {
        work_item_id: Uuid,
        assignee_id: Uuid,
    },
    /// Work was declined
    WorkDeclined {
        work_item_id: Uuid,
        assignee_id: Uuid,
    },
    /// Approval was requested
    ApprovalRequested {
        approval: crate::delegation::ApprovalRequest,
        work_item: crate::delegation::WorkItem,
    },
    /// Work was approved
    WorkApproved {
        work_item_id: Uuid,
        approver_id: Uuid,
        feedback: Option<String>,
    },
    /// Work was rejected
    WorkRejected {
        work_item_id: Uuid,
        approver_id: Uuid,
        feedback: String,
    },
    /// Work was cancelled
    WorkCancelled {
        work_item_id: Uuid,
        cancelled_by: Uuid,
    },
    /// Work was claimed
    WorkClaimed {
        work_item_id: Uuid,
        claimed_by: Uuid,
    },
    /// Work queue response
    WorkQueue {
        items: Vec<crate::delegation::WorkItem>,
    },
    /// Approval queue response
    ApprovalQueue {
        items: Vec<crate::delegation::ApprovalRequest>,
    },
    /// Available participants response
    AvailableParticipants {
        participants: Vec<crate::delegation::RegisteredParticipant>,
    },
    /// Accepting work status changed
    AcceptingWorkChanged {
        participant_id: Uuid,
        accepting: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_client_message_submit_deserialization() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "submit", "journal_id": "{}", "content": "Hello", "session_id": "sess_123"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Submit {
                journal_id: jid,
                content,
                session_id,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(content, "Hello");
                assert_eq!(session_id, Some("sess_123".to_string()));
            }
            _ => panic!("Expected Submit message"),
        }
    }

    #[test]
    fn test_client_message_submit_no_session() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "submit", "journal_id": "{}", "content": "Test"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Submit { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Submit message"),
        }
    }

    #[test]
    fn test_client_message_create_journal() {
        let json = r#"{"type": "create_journal", "title": "My Journal"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::CreateJournal { title } => {
                assert_eq!(title, Some("My Journal".to_string()));
            }
            _ => panic!("Expected CreateJournal message"),
        }
    }

    #[test]
    fn test_client_message_create_journal_no_title() {
        let json = r#"{"type": "create_journal"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::CreateJournal { title } => {
                assert_eq!(title, None);
            }
            _ => panic!("Expected CreateJournal message"),
        }
    }

    #[test]
    fn test_client_message_get_journal() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "get_journal", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::GetJournal { journal_id: jid } => {
                assert_eq!(jid, journal_id);
            }
            _ => panic!("Expected GetJournal message"),
        }
    }

    #[test]
    fn test_client_message_list_journals() {
        let json = r#"{"type": "list_journals"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::ListJournals));
    }

    #[test]
    fn test_client_message_invalid_type() {
        let json = r#"{"type": "invalid_type"}"#;
        let result: Result<ClientMessage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_message_journal_created() {
        let journal_id = Uuid::new_v4();
        let msg = ServerMessage::JournalCreated {
            journal_id,
            title: "Test".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journal_created"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_server_message_journal() {
        let journal = crate::models::Journal {
            id: Uuid::nil(),
            title: "Test Journal".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::Journal {
            journal,
            blocks: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journal"));
        assert!(json.contains("Test Journal"));
    }

    #[test]
    fn test_server_message_journals() {
        let msg = ServerMessage::Journals { journals: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journals"));
    }

    #[test]
    fn test_server_message_block_created() {
        let block = crate::models::Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Hello".to_string(),
            status: BlockStatus::Complete,
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::BlockCreated { block };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_created"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_server_message_block_content_delta() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockContentDelta {
            block_id,
            delta: "new content".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_content_delta"));
        assert!(json.contains("new content"));
    }

    #[test]
    fn test_server_message_block_status_changed() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockStatusChanged {
            block_id,
            status: BlockStatus::Streaming,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_status_changed"));
        assert!(json.contains("streaming"));
    }

    #[test]
    fn test_server_message_error() {
        let msg = ServerMessage::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Something went wrong"));
    }

    #[test]
    fn test_server_message_debug() {
        let msg = ServerMessage::Error {
            message: "test".to_string(),
        };
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Error"));
    }

    #[test]
    fn test_client_message_debug() {
        let msg = ClientMessage::ListJournals;
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("ListJournals"));
    }

    #[test]
    fn test_all_block_statuses_in_server_message() {
        let block_id = Uuid::new_v4();

        for status in [
            BlockStatus::Pending,
            BlockStatus::Streaming,
            BlockStatus::Complete,
            BlockStatus::Error,
        ] {
            let msg = ServerMessage::BlockStatusChanged { block_id, status };
            let json = serde_json::to_string(&msg).unwrap();
            assert!(json.contains(status.as_str()));
        }
    }

    #[test]
    fn test_client_message_fork() {
        let block_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "fork", "block_id": "{}", "session_id": "sess_123"}}"#,
            block_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Fork {
                block_id: bid,
                session_id,
            } => {
                assert_eq!(bid, block_id);
                assert_eq!(session_id, Some("sess_123".to_string()));
            }
            _ => panic!("Expected Fork message"),
        }
    }

    #[test]
    fn test_client_message_fork_no_session() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "fork", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Fork { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Fork message"),
        }
    }

    #[test]
    fn test_client_message_rerun() {
        let block_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "rerun", "block_id": "{}", "session_id": "sess_456"}}"#,
            block_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Rerun {
                block_id: bid,
                session_id,
            } => {
                assert_eq!(bid, block_id);
                assert_eq!(session_id, Some("sess_456".to_string()));
            }
            _ => panic!("Expected Rerun message"),
        }
    }

    #[test]
    fn test_client_message_rerun_no_session() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "rerun", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Rerun { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Rerun message"),
        }
    }

    #[test]
    fn test_client_message_cancel() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "cancel", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Cancel { block_id: bid } => {
                assert_eq!(bid, block_id);
            }
            _ => panic!("Expected Cancel message"),
        }
    }

    #[test]
    fn test_server_message_block_forked() {
        let original_block_id = Uuid::new_v4();
        let block = crate::models::Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Forked content".to_string(),
            status: BlockStatus::Pending,
            parent_id: Some(original_block_id),
            forked_from_id: Some(original_block_id),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::BlockForked {
            original_block_id,
            new_block: block,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_forked"));
        assert!(json.contains("original_block_id"));
        assert!(json.contains("new_block"));
        assert!(json.contains("Forked content"));
    }

    #[test]
    fn test_server_message_block_cancelled() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockCancelled { block_id };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_cancelled"));
        assert!(json.contains(&block_id.to_string()));
    }

    // New CRDT/Presence message tests

    #[test]
    fn test_client_message_subscribe() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "subscribe", "journal_id": "{}", "name": "Alice", "kind": "user"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Subscribe {
                journal_id: jid,
                name,
                kind,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(name, "Alice");
                assert_eq!(kind, Some("user".to_string()));
            }
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[test]
    fn test_client_message_subscribe_no_kind() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "subscribe", "journal_id": "{}", "name": "Bob"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Subscribe { kind, .. } => {
                assert_eq!(kind, None);
            }
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[test]
    fn test_client_message_unsubscribe() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "unsubscribe", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Unsubscribe { journal_id: jid } => {
                assert_eq!(jid, journal_id);
            }
            _ => panic!("Expected Unsubscribe message"),
        }
    }

    #[test]
    fn test_client_message_cursor() {
        let journal_id = Uuid::new_v4();
        let block_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "cursor", "journal_id": "{}", "block_id": "{}", "offset": 42}}"#,
            journal_id, block_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Cursor {
                journal_id: jid,
                block_id: bid,
                offset,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(bid, Some(block_id));
                assert_eq!(offset, Some(42));
            }
            _ => panic!("Expected Cursor message"),
        }
    }

    #[test]
    fn test_client_message_cursor_null_position() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "cursor", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Cursor {
                block_id, offset, ..
            } => {
                assert_eq!(block_id, None);
                assert_eq!(offset, None);
            }
            _ => panic!("Expected Cursor message"),
        }
    }

    #[test]
    fn test_client_message_get_presence() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "get_presence", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::GetPresence { journal_id: jid } => {
                assert_eq!(jid, journal_id);
            }
            _ => panic!("Expected GetPresence message"),
        }
    }

    #[test]
    fn test_client_message_crdt_update() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "crdt_update", "journal_id": "{}", "update": "SGVsbG8="}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::CrdtUpdate {
                journal_id: jid,
                update,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(update, "SGVsbG8=");
            }
            _ => panic!("Expected CrdtUpdate message"),
        }
    }

    #[test]
    fn test_client_message_sync_request() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "sync_request", "journal_id": "{}", "state_vector": "AQAA"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::SyncRequest {
                journal_id: jid,
                state_vector,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(state_vector, Some("AQAA".to_string()));
            }
            _ => panic!("Expected SyncRequest message"),
        }
    }

    #[test]
    fn test_client_message_sync_request_no_sv() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "sync_request", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::SyncRequest { state_vector, .. } => {
                assert_eq!(state_vector, None);
            }
            _ => panic!("Expected SyncRequest message"),
        }
    }

    #[test]
    fn test_server_message_subscribed() {
        let journal_id = Uuid::new_v4();
        let participant = Participant::new("Alice", ParticipantKind::User);
        let msg = ServerMessage::Subscribed {
            journal_id,
            participant: participant.clone(),
            participants: vec![participant],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribed"));
        assert!(json.contains("Alice"));
    }

    #[test]
    fn test_server_message_unsubscribed() {
        let journal_id = Uuid::new_v4();
        let msg = ServerMessage::Unsubscribed { journal_id };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("unsubscribed"));
    }

    #[test]
    fn test_server_message_participant_joined() {
        let journal_id = Uuid::new_v4();
        let participant = Participant::new("Bob", ParticipantKind::Agent);
        let msg = ServerMessage::ParticipantJoined {
            journal_id,
            participant,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("participant_joined"));
        assert!(json.contains("Bob"));
        assert!(json.contains("agent"));
    }

    #[test]
    fn test_server_message_participant_left() {
        let journal_id = Uuid::new_v4();
        let participant_id = Uuid::new_v4();
        let msg = ServerMessage::ParticipantLeft {
            journal_id,
            participant_id,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("participant_left"));
        assert!(json.contains(&participant_id.to_string()));
    }

    #[test]
    fn test_server_message_cursor_moved() {
        let journal_id = Uuid::new_v4();
        let participant_id = Uuid::new_v4();
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::CursorMoved {
            journal_id,
            participant_id,
            block_id: Some(block_id),
            offset: Some(100),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("cursor_moved"));
        assert!(json.contains(&block_id.to_string()));
    }

    #[test]
    fn test_server_message_participant_status_changed() {
        let journal_id = Uuid::new_v4();
        let participant_id = Uuid::new_v4();
        let msg = ServerMessage::ParticipantStatusChanged {
            journal_id,
            participant_id,
            status: ParticipantStatus::Idle,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("participant_status_changed"));
        assert!(json.contains("idle"));
    }

    #[test]
    fn test_server_message_presence() {
        let journal_id = Uuid::new_v4();
        let msg = ServerMessage::Presence {
            journal_id,
            participants: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("presence"));
        assert!(json.contains("participants"));
    }

    #[test]
    fn test_server_message_crdt_update() {
        let journal_id = Uuid::new_v4();
        let source = Uuid::new_v4();
        let msg = ServerMessage::CrdtUpdate {
            journal_id,
            source: Some(source),
            update: "SGVsbG8=".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("crdt_update"));
        assert!(json.contains("SGVsbG8="));
    }

    #[test]
    fn test_server_message_sync_state() {
        let journal_id = Uuid::new_v4();
        let msg = ServerMessage::SyncState {
            journal_id,
            state: "AQAAAQ==".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("sync_state"));
        assert!(json.contains("AQAAAQ=="));
    }

    #[test]
    fn test_base64_encode_decode_roundtrip() {
        let original = b"Hello, CRDT World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base64_encode_empty() {
        let encoded = base64_encode(&[]);
        assert_eq!(encoded, "");
    }

    #[test]
    fn test_base64_decode_empty() {
        let decoded = base64_decode("").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_decode_invalid() {
        let result = base64_decode("!!invalid!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_encode_various_lengths() {
        // Test padding cases
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
        assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
    }
}
