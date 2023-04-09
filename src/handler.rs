use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use regex::Regex;
use traq_ws_bot::{
    events::{
        common,
        payload::{DirectMessageCreated, MessageCreated},
    },
    openapi::{
        self,
        models::{
            PostBotActionJoinRequest, PostBotActionLeaveRequest, PostMessageRequest,
            PostMessageStampRequest,
        },
    },
    utils::{create_configuration, is_mentioned_message},
};

use crate::{Message, Operation, Resource, TimerState};

#[derive(Debug, Clone)]
pub enum Parsed {
    Add(String, SystemTime),
    Remove(String),
    /// isAll
    List(bool),
    Join,
    Leave,
}

const DEFAULT_MESSAGE: &str = "時間になりました :blob_bongo:";
/// NOTE: **not** equal user id
const SELF_ID: &str = "c3967e92-e752-48e3-9b3d-1eb5b4e19341";
const SELF_USER_ID: &str = "d352688f-a656-4444-8c5f-caa517e9ea1b";

/// like !{\"type\":\"user\",\"raw\":\"@BOT_STimer\",\"id\":\"d352688f-a656-4444-8c5f-caa517e9ea1b\"}
const MENTION_REGEX: &str =
    r#"!\{"type":"user","raw":"(?:[^\\"]|\\.)+","id":"d352688f-a656-4444-8c5f-caa517e9ea1b"\}"#;

const SPECIAL_MESSAGE_REGEX: &str = r#"!\{"type":"(user|channel|group)","raw":"(?P<raw>(?:[^\\"]|\\.)+)","id":"(?:[^\\"]|\\.)+"\}"#;

const COMMAND_NOT_FOUND_MESSAGE: &str = "コマンドが見つかりません :eyes_komatta:";

const WAVE_ID: &str = "54e37bdc-7f8d-4fe9-aaf8-6173b97d0607";

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
    let parsed = match parse(content, has_mention) {
        Ok(parsed) => parsed,
        Err(Some(e)) => {
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
        Err(None) => {
            if has_mention {
                let configuration = create_configuration(resource.token.clone());
                let res = openapi::apis::message_api::post_message(
                    &configuration,
                    &message.channel_id,
                    Some(PostMessageRequest {
                        content: COMMAND_NOT_FOUND_MESSAGE.to_string(),
                        embed: None,
                    }),
                )
                .await;
                if let Err(e) = res {
                    log::error!("Failed to post message: {:?}", e);
                }
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
                .send(Operation::Remove(
                    message_uuid,
                    message.id,
                    message.user.name,
                ))
                .await
                .unwrap();
        }
        Parsed::List(is_all) => {
            let timers = resource.timers.lock().await;
            let mut messages = timers
                .iter()
                .filter_map(|(uuid, state)| {
                    if let TimerState::Idle(user_name, end_time) = state {
                        Some((uuid, user_name, end_time))
                    } else {
                        None
                    }
                })
                .filter(|(_, user_name, _)| {
                    if is_all {
                        true
                    } else {
                        *user_name == &message.user.name
                    }
                })
                .collect::<Vec<_>>();
            messages.sort();
            let table_label = format!("{}|終了予定|url|", if is_all { "|設定者" } else { "" });
            let table_separator = format!("{}|---|---|", if is_all { "|---" } else { "" });
            let tables = messages
                .iter()
                .map(|(uuid, user_name, &end_time)| {
                    let url = message_url(uuid, true);
                    let time: DateTime<Local> = end_time.into();
                    format!(
                        "{}|{}|{}|",
                        if is_all {
                            format!("| :@{}: ", user_name)
                        } else {
                            "".to_string()
                        },
                        time.format("%Y-%m-%d %H:%M:%S"),
                        url
                    )
                })
                .collect::<Vec<_>>();
            let content = if tables.is_empty() {
                "現在設定されているタイマーはありません :melting_face:".to_owned()
            } else {
                format!(
                    "{}\n{}\n{}",
                    table_label,
                    table_separator,
                    tables.join("\n")
                )
            };
            let configuration = create_configuration(resource.token.clone());
            let res = openapi::apis::message_api::post_message(
                &configuration,
                &message.channel_id,
                Some(PostMessageRequest {
                    content,
                    embed: None,
                }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to post message: {:?}", e);
            }
        }
        Parsed::Join => {
            let configuration = create_configuration(resource.token.clone());
            let channel_id_uuid = uuid::Uuid::parse_str(&message.channel_id);
            if let Err(e) = channel_id_uuid {
                log::error!("Failed to parse channel id: {:?}", e);
                return;
            }
            let res = openapi::apis::bot_api::let_bot_join_channel(
                &configuration,
                SELF_ID,
                Some(PostBotActionJoinRequest {
                    channel_id: channel_id_uuid.unwrap(),
                }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to join channel: {:?}", e);
            }

            let res = openapi::apis::message_api::post_message(
                &configuration,
                &message.channel_id,
                Some(PostMessageRequest {
                    content: "参加しました :blob_pyon:".to_string(),
                    embed: None,
                }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to post message: {:?}", e);
            }
        }
        Parsed::Leave => {
            let configuration = create_configuration(resource.token.clone());
            let channel_id_uuid = uuid::Uuid::parse_str(&message.channel_id);
            if let Err(e) = channel_id_uuid {
                log::error!("Failed to parse channel id: {:?}", e);
                return;
            }
            let res = openapi::apis::bot_api::let_bot_leave_channel(
                &configuration,
                SELF_ID,
                Some(PostBotActionLeaveRequest {
                    channel_id: channel_id_uuid.unwrap(),
                }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to leave channel: {:?}", e);
            }

            let res = openapi::apis::stamp_api::add_message_stamp(
                &configuration,
                &message.id,
                WAVE_ID,
                Some(PostMessageStampRequest { count: 0 }),
            )
            .await;
            if let Err(e) = res {
                log::error!("Failed to post message: {:?}", e);
            }
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
const LIST_COMMAND: [&str; 3] = ["list", "l", "ls"];

/// like https://q.trap.jp/messages/6bb86c45-65d5-458f-83c0-57116d81eca1
const MESSAGE_REGEX: &str = r#"(?:https?:)?//q\.trap\.jp/messages/(?P<uuid>[0-9a-f-]+)"#;

fn parse(content: String, is_mentioned: bool) -> Result<Parsed, Option<String>> {
    let content = content.trim();
    let splitted = content.split_whitespace().collect::<Vec<_>>();

    if is_mentioned {
        if splitted.first() == Some(&"join") {
            return Ok(Parsed::Join);
        }
        if splitted.first() == Some(&"leave") {
            return Ok(Parsed::Leave);
        }
    }

    let (content, splitted) = if !is_mentioned {
        if splitted.first() != Some(&"timer") {
            return Err(None);
        }

        (
            content.trim_start_matches("timer").trim(),
            splitted[1..].to_vec(),
        )
    } else if splitted.first() == Some(&"timer") {
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
            return Err(Some("時間を指定してください".to_string()));
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
        let message = Regex::new(SPECIAL_MESSAGE_REGEX)
            .unwrap()
            .replace_all(&message, "${raw}")
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
            return Err(Some("メッセージのURLを指定してください".to_string()));
        }

        let content = content.trim_start_matches(command).trim().to_string();

        let matches = Regex::new(MESSAGE_REGEX)
            .unwrap()
            .captures_iter(&content)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(Some("メッセージのURLを指定してください".to_string()));
        }
        if matches.len() > 1 {
            return Err(Some("メッセージのURLは1つだけ指定してください".to_string()));
        }

        let uuid = matches[0]["uuid"].to_string();

        return Ok(Parsed::Remove(uuid));
    }

    for command in LIST_COMMAND.iter() {
        if splitted[0] != *command {
            continue;
        }

        if content.contains("-a") {
            return Ok(Parsed::List(true));
        }

        return Ok(Parsed::List(false));
    }

    Err(Some(COMMAND_NOT_FOUND_MESSAGE.to_string()))
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

fn message_url(message_uuid: &str, short: bool) -> String {
    format!(
        "{}//q.trap.jp/messages/{}",
        if short { "" } else { "https:" },
        message_uuid
    )
}
