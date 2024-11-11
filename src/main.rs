use std::{sync::Arc, time, env};
// use chrono::{NaiveTime, Timelike};
use teloxide::types::{ChatId, InputFile, InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup, ForceReply};
use teloxide::{ prelude::*, utils::command::BotCommands };
use tokio::signal;
use reqwest::Client;
// use dotenv::dotenv;
use log::error;


pub mod setting_opts;
pub mod regex;
pub mod tx_info;
pub mod token_overview;
pub mod token_transfer;

use setting_opts::*;
use regex::*;
use tx_info::*;
use token_overview::*;
use token_transfer::*;



/// The default file path for the file where the setting options will be saved
const DEFAULT_SETTING_OPT_FILE_PATH: &str = "settingopts.json";
/// The name of the environment variable where the path of the setting_opt_file_path can be specified
const SETTING_OPT_ENV: &str = "TELOXIDE_SETTINGOPTFILE";

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description="Setup the language", parse_with="split")]
    Settings { bot_username: String },
    #[command(description="Show the start message", parse_with="split")]
    Start {availability: String},
}


#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting DailyBible Bot...");

    let setting_opts_wrapper: SettingOptsWrapper = SettingOptsWrapper::new();

    // Check whether we can load the latest user_states from a file
    let setting_opt_file = env::var(SETTING_OPT_ENV).unwrap_or(DEFAULT_SETTING_OPT_FILE_PATH.to_string());
    match setting_opts_wrapper.load_states_from_file(&setting_opt_file).await {
        Ok(_) => log::info!("Previous setting options successfully loaded."),
        Err(error) => log::warn!("Could not load previous setting options: {}", error.to_string()),
    }

    let bot: Bot = Bot::from_env();

    let bot_commands = Command::bot_commands();
    if bot.set_my_commands(bot_commands).await.is_err() {
        log::warn!("Could not set up the commands.");
    }

    let message_handler = Update::filter_message()
                .branch(
                    dptree::filter(|msg: Message| {
                        // Check if this message is a reply to a bot's message with ForceReply
                        msg.reply_to_message()
                            .and_then(|reply| reply.from())
                            .map_or(false, |user| user.is_bot)
                    })
                    .endpoint(answer_replyed_message)
                )
                .filter_command::<Command>()
                .endpoint(answer);

    let callback_handler = Update::filter_callback_query()
            .endpoint(answer_button);

    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    // let bot_arc = Arc::new(bot.clone());
    let setting_opts_wrapper_arc = Arc::new(setting_opts_wrapper);

    // let bot_arc_thread = bot_arc.clone();
    // let setting_opts_wrapper_arc_thread = setting_opts_wrapper_arc.clone();
    // tokio::spawn(async move { run_timer_thread_loop(bot_arc_thread.clone(), setting_opts_wrapper_arc_thread.clone()).await } );

    let setting_opts_wrapper_arc_thread = setting_opts_wrapper_arc.clone();
    tokio::spawn(async move { run_save_setting_opt_loop(setting_opts_wrapper_arc_thread.clone()).await } );

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![setting_opts_wrapper_arc.clone()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let chat_type = match msg.chat.kind {
        teloxide::types::ChatKind::Private { .. } => {
            "a private chat".to_string()
        }
        teloxide::types::ChatKind::Public(ref public_chat) => {
            match public_chat.kind {
                teloxide::types::PublicChatKind::Group { .. } => "a group".to_string(),
                teloxide::types::PublicChatKind::Supergroup { .. } => "a supergroup".to_string(),
                teloxide::types::PublicChatKind::Channel { .. } => "a channel".to_string(),
            }
        }
        // _ => {
        //     bot.send_message(msg.chat.id, "Could not determine chat type.")
        //         .await?;
        //     return Ok(());
        // }
    };
    let _ = match cmd {
        Command::Settings{bot_username} => settings_command(bot, msg, bot_username, chat_type).await,
        Command::Start{availability} => start_command(bot, msg, availability).await,
    };  
    Ok(())
}


async fn answer_button(bot: Bot, callback: CallbackQuery, setting_opts_wrapper: Arc<SettingOptsWrapper>)  -> ResponseResult<()> {
    match callback.data {
        Some(callback_string) => {
            match callback_string.as_str() {
                "token_address" => { let _ = message_by_callback(bot, callback.from.id.into(), "token_address".to_string()).await; },
                "min_buy_amount" => { let _ = message_by_callback(bot, callback.from.id.into(), "min_buy_amount".to_string()).await; },
                "buy_step" => { let _ = message_by_callback(bot, callback.from.id.into(), "buy_step".to_string()).await; },
                "emoji" => { let _ = message_by_callback(bot, callback.from.id.into(), "emoji".to_string()).await; },
                "media_toggle" => { let _ = media_toggle(bot, callback.from.id.into(), setting_opts_wrapper).await; },
                "add_media" => { let _ = add_media_type(bot, callback.from.id.into()).await; },
                "tg_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "tg_link".to_string()).await; },
                "website_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "website_link".to_string()).await; },
                "twitter_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "twitter_link".to_string()).await; },
                "confirm" => { let _ = confirm_style_change(bot, callback.from.id.into(), setting_opts_wrapper).await; },
                "delete_token" => { let _ = delete_and_back_to_new_token(bot, callback.from.id.into(), setting_opts_wrapper).await; },
                "photo" => { let _ = add_media(bot, callback.from.id.into(), setting_opts_wrapper, "photo".to_string()).await; },
                "video" => { let _ = add_media(bot, callback.from.id.into(), setting_opts_wrapper, "video".to_string()).await; },
                _ => { log::warn!("Received callback {} which isn't implemented.", callback_string); }
            }
        }
        None => {}
    };
    Ok(())
}

async fn answer_replyed_message(bot: Bot, msg: Message, setting_opts_wrapper: Arc<SettingOptsWrapper>) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let reply_text = msg.reply_to_message().and_then(|reply| reply.text());

    if msg.photo().is_some() {
        if reply_text == Some("photo") {
            if let Some(latest_photo) = msg.photo().iter().last() {
                let mut selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
                selected_setting_opt.media_file_id = Some(latest_photo[0].file.id.clone());
                
                // Update the settings
                setting_opts_wrapper.set_selected_setting_opt(selected_setting_opt.clone()).await;
                setting_opts_wrapper.update_setting_opt(selected_setting_opt.clone()).await;
               
                setting_option(bot.clone(), chat_id, "🎉 Photo saved. Now you can adjust other settings:".to_string(), selected_setting_opt.token_address, setting_opts_wrapper).await?;
                return Ok(());
            }
        }
    } else if msg.video().is_some() {
        if reply_text == Some("video") {
            if let Some(latest_video) = msg.video().iter().last() {
                let mut selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
                selected_setting_opt.media_file_id = Some(latest_video.file.id.clone());
                
                // Update the settings
                setting_opts_wrapper.set_selected_setting_opt(selected_setting_opt.clone()).await;
                setting_opts_wrapper.update_setting_opt(selected_setting_opt.clone()).await;

                setting_option(bot.clone(), chat_id, "🎉 Video saved. Now you can adjust the other settings:".to_string(), selected_setting_opt.token_address, setting_opts_wrapper).await?;
                return Ok(());
            }
        }
    }  else if let Some(text) = msg.text() {
        if text.starts_with("0x") {
            if text.len() == 42 {
                let setting_opt = setting_opts_wrapper.find_setting_opt(text.to_string()).await;
                setting_opts_wrapper.set_selected_setting_opt(setting_opt.clone()).await;
                let existing_opt = setting_opts_wrapper.find_setting_opt(text.to_string()).await;
                setting_opts_wrapper.update_setting_opt(existing_opt).await;
                

                bot.send_message(
                    chat_id,
                    format!("✅ Token address saved: {}", text)
                ).await?;

                setting_option(bot.clone(), chat_id, "🎉 Token address saved. Now you can adjust the other settings:".to_string(), text.to_string(), setting_opts_wrapper).await?;
            } else{
                bot.send_message(
                    chat_id,
                    format!("❌ Token address is not valid.")
                ).await?;
            }
        } 
        else if let Some(reply_text) = reply_text {
            let mut selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
            let mut head_text = "";
            match reply_text {
                "token_address" => {
                    if is_token_address(text) {
                        selected_setting_opt.token_address = text.to_string();
                        head_text = "🎉 Token address saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Token address is not valid. Please try again.";
                    }
                },
                "min_buy_amount" => {
                    if let Ok(amount) = text.parse::<f64>() {
                        selected_setting_opt.min_buy_amount = amount;
                        head_text = "🎉 Min buy amount saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Min buy amount is not valid. Please try again.";
                    }
                },
                "buy_step" => {
                    if let Ok(step) = text.parse::<i32>() {
                        selected_setting_opt.buy_step = step;
                        head_text = "🎉 Buy step saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Buy step is not valid. Please try again.";
                    }
                },
                "emoji" => {
                    if is_emoji(text) {
                        selected_setting_opt.emoji = text.to_string();
                        head_text = "🎉 Emoji saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Emoji is not valid. Please try again.";
                    }
                },
                "tg_link" => {
                    if is_tg_link(text) {
                        selected_setting_opt.tg_link = text.to_string();
                        head_text = "🎉 Tg link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Tg link is not valid. Please try again.";
                    }
                },
                "website_link" => {
                    if is_website_link(text) {
                        selected_setting_opt.website_link = text.to_string();
                        head_text = "🎉 Website link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Website link is not valid. Please try again.";
                    }
                },
                "twitter_link" => {
                    if is_twitter_link(text) {
                        selected_setting_opt.twitter_link = text.to_string();
                        head_text = "🎉 Twitter link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Twitter link is not valid. Please try again.";
                    }
                },
                
                _ => log::warn!("Unhandled reply type: {}", reply_text)
            }
            // Clone before updating
            setting_opts_wrapper.set_selected_setting_opt(selected_setting_opt.clone()).await;
            setting_opts_wrapper.update_setting_opt(selected_setting_opt.clone()).await;
            // Now we can use selected_setting_opt for the JSON message
            // bot.send_message(
            //     chat_id,
            //     format!("The following data about setting option is saved on the server: \n\
            //     \n\
            //     ```\
            //     {}\
            //     ```\
            //     ", serde_json::to_string_pretty(&selected_setting_opt).unwrap()
            //     )
            // )
            // .parse_mode(MarkdownV2)
            // .await?;
           
            setting_option(bot.clone(), chat_id, head_text.to_string(), selected_setting_opt.token_address, setting_opts_wrapper).await?;
        }
    } 
    // else if msg.document().is_some() {
    // } else if msg.sticker().is_some() {
    // } else {
    // }
    
    Ok(())
}

async fn settings_command(bot: Bot, msg: Message, bot_username: String, chat_type: String) -> ResponseResult<()> {
    match chat_type.as_str() {
        "a private chat" => {
            let _ = bot.send_message(msg.chat.id, format!("/settings command is not supported in this chat type."));
        }
        "a group" | "a supergroup" => {
            let sender_name = msg.from()
                                    .and_then(|user| user.username.clone())
                                    .unwrap_or_else(|| {
                                        msg.from()
                                            .map(|user| user.first_name.clone())
                                            .unwrap_or_else(|| "Unknown User".to_string())
                                    });
            let _ = settings(bot, msg.chat.id, sender_name, bot_username).await;
        }
        _ => {
            let _ = bot.send_message(msg.chat.id, format!("This bot helps you to read Apechain token buy information. Type /help for more information")).await;
        }
    }
    Ok(())
}

async fn start_command(bot: Bot, msg: Message, availability: String) -> ResponseResult<()> {
    match availability.as_str() {
        "available" => {
            let _ = start(bot, msg.chat.id).await;
        }
        _ => {}
    }
    Ok(())
}

async fn settings(bot: Bot, chat_id: ChatId, username: String, bot_username: String) -> ResponseResult<()> {
    let bot_name = std::env::var("BOT_USERNAME").unwrap_or_default();
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::url(
            "Configure Settings",
            if bot_username.is_empty() {
                format!("https://t.me/{}?start=available",bot_name).parse().unwrap()
            } else {
                format!("https://t.me/{}?start=available", bot_username).parse().unwrap()
            }
        )]
    ]);

    bot.send_message(
        chat_id,
        format!("@{}, to configure settings, please click the button below and then start a private chat with me if you haven't already.", username)
    )
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

async fn start(bot: Bot, chat_id: ChatId) -> ResponseResult<()> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("Enter Token Address", "token_address")],
    ]);
       
    // First message with keyboard
    bot.send_message(
        chat_id, 
        "Please click the button below to enter a token address"
    )
    .reply_markup(keyboard)
    .await?;

    Ok(())

    
}

async fn setting_option(bot: Bot, chat_id: ChatId, head_text: String, token_address: String, setting_opts_wrapper: Arc<SettingOptsWrapper>) -> ResponseResult<()> {
    let selected_setting_opt = setting_opts_wrapper.find_setting_opt(token_address).await;

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(format!("Change minBuy: {}", selected_setting_opt.min_buy_amount), "min_buy_amount")],
        vec![InlineKeyboardButton::callback(format!("Change step: {}", selected_setting_opt.buy_step), "buy_step")],
        vec![InlineKeyboardButton::callback(format!("Change Emoji: {}", selected_setting_opt.emoji), "emoji")],
        vec![InlineKeyboardButton::callback(format!("Enable/Disable media: {}", selected_setting_opt.media_toggle), "media_toggle")],
        vec![InlineKeyboardButton::callback(format!("Add Media: {}", selected_setting_opt.media_file_id.clone().unwrap_or("".to_string())), "add_media")],
        vec![InlineKeyboardButton::callback(format!("Change Tg Link: {}", selected_setting_opt.tg_link), "tg_link")],
        vec![InlineKeyboardButton::callback(format!("Change Twitter Link: {}", selected_setting_opt.twitter_link), "twitter_link")],
        vec![InlineKeyboardButton::callback(format!("Change Website Link: {}", selected_setting_opt.website_link), "website_link")],
        vec![InlineKeyboardButton::callback("Delete Token", "delete_token")],
        vec![InlineKeyboardButton::callback("Yes", "confirm")]
    ]);
       
    // First message with keyboard
    bot.send_message(
        chat_id, 
        format!("{}", head_text)
    )
    .reply_markup(keyboard)
    // .reply_markup(ReplyMarkup::ForceReply(
    //     ForceReply::new()
    // ))
    .await?;

    Ok(())
}



async fn message_by_callback(bot: Bot, chat_id: ChatId, callback_string: String) -> ResponseResult<()> {
    bot.send_message(
        chat_id,
        format!("{}", callback_string)
    )
    .reply_markup(ReplyMarkup::ForceReply(
        ForceReply::new()
            // .input_field_placeholder(Some("0x...".to_string()))
    ))
    .await?;

    Ok(())
}

async fn media_toggle(bot: Bot, chat_id: ChatId, setting_opts_wrapper: Arc<SettingOptsWrapper>) -> ResponseResult<()> {
    let mut selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
    selected_setting_opt.media_toggle = !selected_setting_opt.media_toggle;
    setting_opts_wrapper.set_selected_setting_opt(selected_setting_opt.clone()).await;
    setting_opts_wrapper.update_setting_opt(selected_setting_opt.clone()).await;
    
    setting_option(bot.clone(), chat_id, "🎉 Media toggle option is saved. Now you can adjust the other settings:".to_string(), selected_setting_opt.token_address, setting_opts_wrapper).await?;
    Ok(())
}
async fn add_media_type(bot: Bot, chat_id: ChatId) -> ResponseResult<()> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("Photo", "photo")],
        vec![InlineKeyboardButton::callback("Video", "video")],
    ]);
       
    // First message with keyboard
    bot.send_message(
        chat_id, 
        "Please choose the type of media."
    )
    .reply_markup(keyboard)
    .await?;

    Ok(())
}
async fn add_media(bot: Bot, chat_id: ChatId, setting_opts_wrapper: Arc<SettingOptsWrapper>, callback_string: String) -> ResponseResult<()> {
    let mut selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
    selected_setting_opt.media_type = callback_string.to_string();
    setting_opts_wrapper.set_selected_setting_opt(selected_setting_opt.clone()).await;
    setting_opts_wrapper.update_setting_opt(selected_setting_opt.clone()).await;
    bot.send_message(
        chat_id,
        format!("{}", callback_string)
    )
    .reply_markup(ReplyMarkup::ForceReply(
        ForceReply::new()
            // .input_field_placeholder(Some("0x...".to_string()))
    ))
    .await?;

    Ok(())
}

async fn confirm_style_change(bot: Bot, chat_id: ChatId, setting_opts_wrapper: Arc<SettingOptsWrapper>) -> ResponseResult<()> {
    bot.send_message(chat_id, "Catching new buy transactions...").await?;

    let request_client = Client::new();
    let debank_api_key = std::env::var("DEBANK_API_KEY").unwrap();
    let token_adr = setting_opts_wrapper.get_selected_setting_opt().await.token_address;
    let website_link = setting_opts_wrapper.get_selected_setting_opt().await.website_link;
    let tg_link = setting_opts_wrapper.get_selected_setting_opt().await.tg_link;
    let twitter_link = setting_opts_wrapper.get_selected_setting_opt().await.twitter_link;
    let emoji = setting_opts_wrapper.get_selected_setting_opt().await.emoji;
    let min_buy_amount = setting_opts_wrapper.get_selected_setting_opt().await.min_buy_amount;
    let buy_step = setting_opts_wrapper.get_selected_setting_opt().await.buy_step;
    let media_toggle = setting_opts_wrapper.get_selected_setting_opt().await.media_toggle;
    let media_file_id = setting_opts_wrapper.get_selected_setting_opt().await.media_file_id;
    let media_type = setting_opts_wrapper.get_selected_setting_opt().await.media_type;

    let interval = tokio::time::interval(std::time::Duration::from_secs(5));
    tokio::spawn(async move {
        let mut interval = interval;
        let mut flag_transaction_hash = String::new();
        loop {
            interval.tick().await;                   
            match get_token_transfers(request_client.clone(), &token_adr).await {
                Ok(token_transfer) => {
                    if let Some(first_transfer) = token_transfer.items.first() {
                        let transaction_hash = first_transfer.tx_hash.clone();
                        let current_transaction_to_name = first_transfer.to.name.clone().unwrap_or_default();
                        if flag_transaction_hash != transaction_hash && !current_transaction_to_name.is_empty() {
                            flag_transaction_hash = transaction_hash;
                            
                            //get token overview
                            let token_overview = get_token_overview(request_client.clone(), &debank_api_key, &token_adr).await.unwrap();
                            let token_price = token_overview.price;
                            let token_price_output = num_floating_point(&token_price, 5);

                            //make message
                            let token_address = &token_transfer.items[0].token.address;
                            let token_name = &token_transfer.items[0].token.name;
                            let token_symbol = &token_transfer.items[0].token.symbol;
                            let token_decimals = &token_transfer.items[0].token.decimals.parse().unwrap_or(0.0);
                            let token_tx_decimal = &token_transfer.items[0].total.decimals.parse().unwrap_or(0.0);
                            let token_tx_value = &token_transfer.items[0].total.value.parse().unwrap_or(0.0) / 10_f64.powi(*token_tx_decimal as i32);
                            let token_total_supply = &token_transfer.items[0].token.total_supply.parse().unwrap_or(0.0);
                            let total_supply = *token_total_supply / 10_f64.powi(*token_decimals as i32);

                            //get transaction info
                            let tx_info = get_tx_info(request_client.clone(), &flag_transaction_hash).await.unwrap();
                            // let tx_fee = controll_big_float(tx_info.fee.value.parse().unwrap_or(0.0) / 10_f64.powi(*token_decimals as i32));
                            let tx_value = token_tx_value - tx_info.fee.value.parse().unwrap_or(0.0) / 10_f64.powi(*token_decimals as i32);
                            let tx_value_output = num_floating_point(&tx_value, 5);
                            let tx_value_usd = controll_big_float(tx_value * token_price);
                            let tx_total_usd = controll_big_float(token_tx_value * token_price);
                            // let tx_value = controll_big_float(tx_info.value.parse().unwrap_or(0.0) / 10_f64.powi(*token_decimals as i32));
                            
                            let mcap = controll_big_float(total_supply * token_price);
                            
                            let emoji_count = (tx_value / buy_step as f64) as i32;
                            let emoji_string = emoji.repeat((emoji_count + 1) as usize);

                            if tx_value * token_price > min_buy_amount {
                                let text = format!(
                                    "{12}\n\n\
                                    💲 Spent: ${1} (${8}) {2}\n\
                                    💰 Got: {6} ${3}\n\
                                    ✅ Dex: <a href=\"https://ape.express/explore/{0}?\">Ape_Express</a> | \
                                    🔖 <a href=\"https://t.me/ApechainTrending_Bot\">Book Trending</a> - \
                                    <a href=\"https://t.me/ApechainADSBot\">DexScreener</a>\n\
                                    🏷️ Price: ${7}\n\
                                    📊 Marketcap: ${5}\n\n\
                                    <a href=\"https://apechain.calderaexplorer.xyz/tx/{4}\">TX</a> | \
                                    <a href=\"https://dexscreener.com/apechain/{0}\">Chart</a> | \
                                    <a href=\"{9}\">TG</a> | \
                                    <a href=\"{10}\">X</a> | \
                                    <a href=\"{11}\">Website</a>",
                                    token_address, tx_value_usd, token_name, token_symbol, flag_transaction_hash, mcap, tx_value_output, token_price_output, tx_total_usd, tg_link, twitter_link, website_link, emoji_string
                                );
                            
                    
                                // bot.send_message(chat_id, text)
                                //     .parse_mode(teloxide::types::ParseMode::Html)
                                //         .await.unwrap();
                                if media_toggle && media_file_id.clone().is_some() {
                                    if media_type == "photo" {
                                        bot.send_photo(chat_id, InputFile::file_id(media_file_id.clone().unwrap()))
                                            .caption(text)
                                            .parse_mode(teloxide::types::ParseMode::Html)
                                            .await.unwrap();
                                    } else if media_type == "video" {
                                        bot.send_video(chat_id, InputFile::file_id(media_file_id.clone().unwrap()))
                                            .caption(text)
                                            .parse_mode(teloxide::types::ParseMode::Html)
                                            .await.unwrap();
                                    }
                                } else {
                                    bot.send_message(chat_id, text)
                                        .parse_mode(teloxide::types::ParseMode::Html)
                                        .await.unwrap();
                                }
                            }
                        }
                    }
                    else {
                        bot.send_message(chat_id, "Not found any new transfer")
                            .await.unwrap();
                    }

                }
                Err(e) => {
                    error!("Error fetching token overview: {}", e);
                    // bot.send_message(chat_id, "Invalid token address or Failed API request. Please try again!")
                    bot.send_message(chat_id, e.to_string())
                        .await.unwrap();
                    return;
                }
            };
            
        }
    });
    Ok(())
}
async fn delete_and_back_to_new_token(bot: Bot, chat_id: ChatId, setting_opts_wrapper: Arc<SettingOptsWrapper>) -> ResponseResult<()> {
    let selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
    let is_deleted = setting_opts_wrapper.delete_setting_opt(selected_setting_opt.clone()).await;
    if is_deleted {
        bot.send_message(
            chat_id,
            format!("The token {} is deleted. Please return to group chat.", selected_setting_opt.token_address)
        )
        .await?;
    } else {
        bot.send_message(
            chat_id,
            format!("The token {} is not found.", selected_setting_opt.token_address)
        )
        .await?;
    }
    
    Ok(())
}



async fn run_save_setting_opt_loop(setting_opts_wrapper_arc: Arc<SettingOptsWrapper>) {
    let control_c_pressed = tokio::spawn(
        async {
            let _ = signal::ctrl_c().await;
            log::info!("Shutdown the setting option saver timer");
        }
    );
    
    loop {
        let selected_setting_opts_wrapper_arc = setting_opts_wrapper_arc.clone();
        tokio::spawn(
            async move {
                handle_save_current_setting_opts(selected_setting_opts_wrapper_arc).await;
            }
        );
        
        tokio::time::sleep(time::Duration::from_secs(30)).await;
        if control_c_pressed.is_finished() {
            handle_save_current_setting_opts(setting_opts_wrapper_arc.clone()).await;               
            break;
        }
    }
}

async fn handle_save_current_setting_opts(setting_opts_wrapper_arc: Arc<SettingOptsWrapper>) {
    let setting_opt_file = env::var(SETTING_OPT_ENV).unwrap_or(DEFAULT_SETTING_OPT_FILE_PATH.to_string());
    
    match setting_opts_wrapper_arc.write_states_to_file(&setting_opt_file).await {
        Ok(_) => log::info!("Saved setting options to {}", setting_opt_file),
        Err(error) => log::warn!("Could not save setting option file: {}", error.to_string())
    }
}

async fn get_token_transfers(client: Client, token_address: &str) -> Result<TokenTransfer, serde_json::Error> {
    let url = format!(
        "https://apechain.calderaexplorer.xyz/api/v2/tokens/{}/transfers",
        token_address
    );

    let response = client
        .get(&url)
        .send()
        .await
        .unwrap();

    let text = response.text().await.unwrap();
    
    // Try to parse and log any error details
    match serde_json::from_str::<TokenTransfer>(&text) {
        Ok(transfer) => Ok(transfer),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(e)
        }
    }
}

async fn get_tx_info(client: Client, tx_hash: &str) -> Result<TxInfo, serde_json::Error> {
    let url = format!("https://apechain.calderaexplorer.xyz/api/v2/transactions/{}", tx_hash);
    let response = client.get(&url).send().await.unwrap();
    let text = response.text().await.unwrap();
    match serde_json::from_str::<TxInfo>(&text) {
        Ok(tx_info) => Ok(tx_info),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(e)
        }
    }
}   

async fn get_token_overview(client: Client, api_key: &str, token_address: &str) -> Result<TokenOverview, serde_json::Error> {
    let url = format!("https://pro-openapi.debank.com/v1/token?chain_id=ape&id={}", token_address);
    let response = client.get(&url).header("Accesskey", api_key).send().await.unwrap();
    let text = response.text().await.unwrap();
    match serde_json::from_str::<TokenOverview>(&text) {
        Ok(token_overview) => Ok(token_overview),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(e)
        }
    }
}

fn num_floating_point(num: &f64, length: i32) -> f64 {
    ((num * 10_f64.powi(length as i32)).round()) / 10_f64.powi(length as i32)
}

fn controll_big_float(num: f64) -> String {
    if num > 1_000_000.0 {
        format!("{:.1}M", num / 1_000_000.0)
    } else if num > 1_000.0 {
        format!("{:.2}K", num / 1000.0)
    } else {
        format!("{:.3}", num)
    }
}
