use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use regex::Regex;
use traq_ws_bot::{
    events::{
        common,
        payload::{DirectMessageCreated, MessageCreated},
    },
    openapi::{self, models::PostMessageRequest},
    utils::{create_configuration, is_mentioned_message},
};

use crate::{Message, Operation, Resource, TimerState};

#[derive(Debug, Clone)]
pub enum Parsed {
    Add(String, SystemTime),
    Remove(String),
    List,
}

const DEFAULT_MESSAGE: &str = "時間になりました :blob_bongo:";
const SELF_USER_ID: &str = "d352688f-a656-4444-8c5f-caa517e9ea1b";

/// like !{\"type\":\"user\",\"raw\":\"@BOT_STimer\",\"id\":\"d352688f-a656-4444-8c5f-caa517e9ea1b\"}
const MENTION_REGEX: &str =
    r#"!\{"type":"user","raw":"(?:[^\\"]|\\.)+","id":"d352688f-a656-4444-8c5f-caa517e9ea1b"\}"#;

#[allow(clippy::redundant_allocation)]
async fn message_like_handler(message: common::Message, resource: Arc<Arc<Resource>>) {
    log::debug!("Received message: {:?}", message);
    if message.user.bot {
        return;
    }

    let (content, has_mention) = if is_mentioned_message(&message, SELF_USER_ID) {
        let content = Regex::new(MENTION_REGEX)
            .unwrap()
            .replace_all(&message.text, "")
            .to_string();
        (content, true)
    } else {
        (message.text, false)
    };
    let parsed = match parse(content, !has_mention) {
        Ok(parsed) => parsed,
        Err(e) => {
            let configuration = create_configuration(resource.token.clone());
            let res = openapi::apis::message_api::post_message(
                &configuration,
                &message.channel_id,
                Some(PostMessageRequest {
                    content: e,
                    embed: None,
                }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to post message: {:?}", e);
            }
            return;
        }
    };

    match parsed {
        Parsed::Add(notify_message, time) => {
            let message = Message {
                message: notify_message,
                time,
                message_uuid: message.id,
                channel_id: message.channel_id,
                user_id: message.user.name,
            };
            resource.tx.send(Operation::Add(message)).await.unwrap();
        }
        Parsed::Remove(message_uuid) => {
            resource
                .tx
                .send(Operation::Remove(message_uuid))
                .await
                .unwrap();
        }
        Parsed::List => {
            let timers = resource.timers.lock().await;
            let mut messages = timers
                .iter()
                .filter_map(|(uuid, state)| {
                    if *state == TimerState::Idle {
                        Some(uuid)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            messages.sort();
            todo!()
        }
    }
}

#[allow(clippy::redundant_allocation)]
pub async fn on_message(payload: MessageCreated, resource: Arc<Arc<Resource>>) {
    message_like_handler(payload.message, resource).await;
}
#[allow(clippy::redundant_allocation)]
pub async fn on_direct_message(payload: DirectMessageCreated, resource: Arc<Arc<Resource>>) {
    message_like_handler(payload.message, resource).await;
}

const ADD_COMMAND: [&str; 5] = ["+", "add", "a", "set", "s"];
const REMOVE_COMMAND: [&str; 5] = ["-", "remove", "r", "delete", "d"];
const LIST_COMMAND: [&str; 2] = ["list", "l"];

/// like https://q.trap.jp/messages/6bb86c45-65d5-458f-83c0-57116d81eca1
const MESSAGE_REGEX: &str = r#"(?:https?)?://q\.trap\.jp/messages/(?P<uuid>[0-9a-f-]+)"#;

fn parse(content: String, need_timer_prefix: bool) -> Result<Parsed, String> {
    let content = content.trim();
    let splitted = content.split_whitespace().collect::<Vec<_>>();

    let (content, splitted) = if need_timer_prefix {
        if splitted.get(0) != Some(&"timer") {
            return Err(String::new());
        }

        (
            content.trim_start_matches("timer").trim(),
            splitted[1..].to_vec(),
        )
    } else if splitted.get(0) == Some(&"timer") {
        (
            content.trim_start_matches("timer").trim(),
            splitted[1..].to_vec(),
        )
    } else {
        (content, splitted)
    };

    for command in ADD_COMMAND.iter() {
        if splitted[0] != *command {
            continue;
        }

        if splitted.len() < 2 {
            return Err("時間を指定してください".to_string());
        }

        let duration = parse_duration(splitted[1].to_string())?;
        let now = std::time::SystemTime::now();
        let time = now + duration;

        let message = content
            .trim_start_matches(command)
            .trim()
            .trim_start_matches(splitted[1])
            .trim()
            .to_string();

        return Ok(Parsed::Add(
            if message.is_empty() {
                DEFAULT_MESSAGE.to_string()
            } else {
                message
            },
            time,
        ));
    }

    for command in REMOVE_COMMAND.iter() {
        if splitted[0] != *command {
            continue;
        }

        if splitted.len() < 2 {
            return Err("メッセージのURLを指定してください".to_string());
        }

        let content = content.trim_start_matches(command).trim().to_string();

        let matches = Regex::new(MESSAGE_REGEX)
            .unwrap()
            .captures_iter(&content)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err("メッセージのURLを指定してください".to_string());
        }
        if matches.len() > 1 {
            return Err("メッセージのURLは1つだけ指定してください".to_string());
        }

        let uuid = matches[0]["uuid"].to_string();

        return Ok(Parsed::Remove(uuid));
    }

    for command in LIST_COMMAND.iter() {
        if splitted[0] != *command {
            continue;
        }

        return Ok(Parsed::List);
    }

    Err("コマンドが見つかりません :eyes_komatta:".to_string())
}

/// like 1w2d3h4m5s
fn parse_duration(duration: String) -> Result<Duration, String> {
    struct DurationBuilder {
        weeks: Option<u64>,
        days: Option<u64>,
        hours: Option<u64>,
        minutes: Option<u64>,
        seconds: Option<u64>,
    }

    impl DurationBuilder {
        fn new() -> Self {
            Self {
                weeks: None,
                days: None,
                hours: None,
                minutes: None,
                seconds: None,
            }
        }

        fn build(self) -> Duration {
            Duration::new(
                self.seconds.unwrap_or(0)
                    + self.minutes.unwrap_or(0) * 60
                    + self.hours.unwrap_or(0) * 60 * 60
                    + self.days.unwrap_or(0) * 60 * 60 * 24
                    + self.weeks.unwrap_or(0) * 60 * 60 * 24 * 7,
                0,
            )
        }
    }

    let mut builder = DurationBuilder::new();

    let duration_regex = Regex::new(r"(?P<value>\d+)(?P<unit>[wdhms])").unwrap();
    for duration in duration_regex.captures_iter(&duration) {
        let value = duration["value"].parse::<u64>().unwrap();
        match &duration["unit"] {
            "w" => {
                if builder.weeks.is_some() {
                    return Err("週は1つだけ指定してください".to_string());
                }
                builder.weeks = Some(value);
            }
            "d" => {
                if builder.days.is_some() {
                    return Err("日は1つだけ指定してください".to_string());
                }
                builder.days = Some(value);
            }
            "h" => {
                if builder.hours.is_some() {
                    return Err("時間は1つだけ指定してください".to_string());
                }
                builder.hours = Some(value);
            }
            "m" => {
                if builder.minutes.is_some() {
                    return Err("分は1つだけ指定してください".to_string());
                }
                builder.minutes = Some(value);
            }
            "s" => {
                if builder.seconds.is_some() {
                    return Err("秒は1つだけ指定してください".to_string());
                }
                builder.seconds = Some(value);
            }
            _ => unreachable!(),
        }
    }

    Ok(builder.build())
}