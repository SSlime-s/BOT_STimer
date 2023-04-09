mod handler;
mod timer;

use std::{collections::HashMap, sync::Arc, time::SystemTime};

use timer::Timer;
use tokio::sync::{mpsc, Mutex};
use traq_ws_bot::builder;

type Timers = Arc<Mutex<HashMap<String, TimerState>>>;

#[derive(Debug, Clone)]
pub struct Resource {
    token: String,
    tx: mpsc::Sender<Operation>,
    timers: Timers,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimerState {
    /// (user_name, end_time)
    Idle(String, SystemTime),
    Removed,
}

#[derive(Debug, Clone)]
pub struct Message {
    message: String,
    time: SystemTime,
    message_uuid: String,
    channel_id: String,
    user_id: String,
}
impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.message_uuid == other.message_uuid
    }
}
impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Eq for Message {}
impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}

#[derive(Debug, Clone)]
pub enum Operation {
    Add(Message),
    Remove {
        remove_message_uuid: String,
        trigger_message_uuid: String,
        trigger_user_name: String,
    },
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::init();

    let token = std::env::var("BOT_ACCESS_TOKEN").expect("BOT_ACCESS_TOKEN is not set");

    let (tx, rx) = mpsc::channel(400);

    let timers = Arc::new(Mutex::new(HashMap::new()));

    let bot = builder(&token)
        .insert_resource(Arc::new(Resource {
            tx: tx.clone(),
            token: token.clone(),
            timers: timers.clone(),
        }))
        .on_message_created_with_resource(handler::on_message)
        .on_direct_message_created_with_resource(handler::on_direct_message)
        .build();

    let bot_process = bot.start();

    let mut timer = Timer::new(token.clone(), rx, timers);
    let timer_process = timer.run();

    tokio::select! {
        _ = bot_process => {}
        _ = timer_process => {}
    }
}
