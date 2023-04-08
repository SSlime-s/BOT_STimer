use std::{cmp::Reverse, collections::BinaryHeap};

use tokio::sync::mpsc;
use traq_ws_bot::{
    openapi::{self, models},
    utils::create_configuration,
};

use crate::{Message, Operation, TimerState, Timers};

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
                        if let Some(message) = self.consume_top_message().await {
                            self.notify(message).await
                        }
                    }
                    operation = self.rx.recv() => {
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
            if let Some(state) = self.timer_states.lock().await.get(&message.message_uuid) {
                match state {
                    TimerState::Idle => {
                        self.timer_states.lock().await.remove(&message.message_uuid);
                        Some(message)
                    }
                    TimerState::Removed => {
                        self.timer_states.lock().await.remove(&message.message_uuid);
                        None
                    }
                }
            } else {
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
                content: message.message,
                embed: Some(true),
            }),
        )
        .await;
        if let Err(e) = res {
            log::error!("Failed to post message: {:?}", e);
        }
    }

    async fn operation(&mut self, operation: Operation) {
        match operation {
            Operation::Add(message) => {
                self.timer_states
                    .lock()
                    .await
                    .insert(message.message_uuid.clone(), TimerState::Idle);
                self.messages.push(Reverse(message));
            }
            Operation::Remove(message_uuid) => {
                self.timer_states
                    .lock()
                    .await
                    .entry(message_uuid)
                    .and_modify(|e| {
                        *e = TimerState::Removed;
                    });
            }
        }
    }
}
