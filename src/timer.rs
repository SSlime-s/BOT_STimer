use std::{cmp::Reverse, collections::BinaryHeap};

use tokio::sync::mpsc;
use traq_ws_bot::{
    openapi::{
        self,
        models::{self, PostMessageStampRequest},
    },
    utils::create_configuration,
};

use crate::{Message, Operation, TimerState, Timers};

const THUMBS_UP_ID: &str = "269095e6-c71c-4887-afb0-e42b5e2ac73b";
const KAN_ID: &str = "68c4cc50-487d-44a1-ade3-0808023037b8";
const GIT_WORKFLOW_CANCEL_ID: &str = "13248e15-240f-4d8c-8c7a-47e84e773702";
#[allow(dead_code)]
const GIT_WORKFLOW_FAIL_ID: &str = "b3c6a7c7-aeb8-4f45-aee8-380c245089db";

#[derive(Debug)]
pub struct Timer {
    token: String,
    rx: mpsc::Receiver<Operation>,
    messages: BinaryHeap<Reverse<Message>>,
    /// message_id を key, state を value に持つ
    timer_states: Timers,
}
impl Timer {
    pub fn new(token: String, rx: mpsc::Receiver<Operation>, timers: Timers) -> Self {
        Self {
            token,
            rx,
            messages: BinaryHeap::new(),
            timer_states: timers,
        }
    }

    pub async fn run(&mut self) {
        loop {
            let now = std::time::SystemTime::now();
            let next_time = self.messages.peek().map(|m| m.0.time);
            if let Some(next_time) = next_time {
                // 指定時間が来ている場合は即座に通知する
                if next_time <= now {
                    if let Some(message) = self.consume_top_message().await {
                        self.notify(message).await
                    }
                    continue;
                }

                // そうでない場合は、指定時間まで待機 OR 新たなタイマーが追加されるまで待機
                let duration = next_time.duration_since(now).unwrap();
                tokio::select! {
                    _ = tokio::time::sleep(duration) => {
                        log::debug!("Timer expired: {:?}", next_time);
                        if let Some(message) = self.consume_top_message().await {
                            self.notify(message).await
                        }
                    }
                    operation = self.rx.recv() => {
                        log::debug!("Received operation: {:?}", operation);
                        if let Some(operation) = operation {
                            self.operation(operation).await;
                        }
                    }
                }
            } else {
                // タイマーがない場合は、新たなタイマーが追加されるまで待機
                if let Some(operation) = self.rx.recv().await {
                    self.operation(operation).await;
                }
            }
        }
    }

    async fn consume_top_message(&mut self) -> Option<Message> {
        if let Some(message) = self.messages.pop() {
            let message = message.0;
            let timer_state = {
                self.timer_states
                    .lock()
                    .await
                    .get(&message.message_uuid)
                    .copied()
            };
            if let Some(state) = timer_state {
                match state {
                    TimerState::Idle => {
                        log::debug!("Timer is idle: {:?}", message);
                        self.timer_states.lock().await.remove(&message.message_uuid);
                        Some(message)
                    }
                    TimerState::Removed => {
                        log::debug!("Timer already removed: {:?}", message);
                        self.timer_states.lock().await.remove(&message.message_uuid);
                        None
                    }
                }
            } else {
                log::debug!("Timer not found: {:?}", message);
                None
            }
        } else {
            None
        }
    }

    async fn notify(&self, message: Message) {
        log::debug!("Notify: {:?}", message);
        let configuration = create_configuration(&self.token);
        let res = openapi::apis::message_api::post_message(
            &configuration,
            &message.channel_id,
            Some(models::PostMessageRequest {
                content: format!("@{} {}", message.user_id, message.message),
                embed: Some(true),
            }),
        )
        .await;
        if let Err(e) = res {
            log::error!("Failed to post message: {:?}", e);
        }

        let res = openapi::apis::stamp_api::remove_message_stamp(
            &configuration,
            &message.message_uuid,
            THUMBS_UP_ID,
        );
        if let Err(e) = res.await {
            log::error!("Failed to remove stamp: {:?}", e);
        }
        let res = openapi::apis::stamp_api::add_message_stamp(
            &configuration,
            &message.message_uuid,
            KAN_ID,
            Some(PostMessageStampRequest { count: 1 }),
        );
        if let Err(e) = res.await {
            log::error!("Failed to add stamp: {:?}", e);
        }
    }

    async fn operation(&mut self, operation: Operation) {
        match operation {
            Operation::Add(message) => {
                self.timer_states
                    .lock()
                    .await
                    .insert(message.message_uuid.clone(), TimerState::Idle);
                self.messages.push(Reverse(message.clone()));
                let configuration = create_configuration(&self.token);
                let res = openapi::apis::stamp_api::add_message_stamp(
                    &configuration,
                    &message.message_uuid,
                    THUMBS_UP_ID,
                    Some(PostMessageStampRequest { count: 1 }),
                );
                if let Err(e) = res.await {
                    log::error!("Failed to add stamp: {:?}", e);
                }
            }
            Operation::Remove(message_uuid) => {
                let mut is_removed = false;
                self.timer_states
                    .lock()
                    .await
                    .entry(message_uuid.clone())
                    .and_modify(|e| {
                        is_removed = *e == TimerState::Removed;
                        *e = TimerState::Removed;
                    });
                if is_removed {
                    return;
                }
                let configuration = create_configuration(&self.token);
                let res = openapi::apis::stamp_api::remove_message_stamp(
                    &configuration,
                    &message_uuid,
                    THUMBS_UP_ID,
                );
                if let Err(e) = res.await {
                    log::error!("Failed to remove stamp: {:?}", e);
                }
                let res = openapi::apis::stamp_api::add_message_stamp(
                    &configuration,
                    &message_uuid,
                    GIT_WORKFLOW_CANCEL_ID,
                    Some(PostMessageStampRequest { count: 1 }),
                );
                if let Err(e) = res.await {
                    log::error!("Failed to add stamp: {:?}", e);
                }
            }
        }
    }
}
