use std::{sync::Arc, time, env};
use teloxide::types::{ChatId, InputFile, InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup, ForceReply};
use teloxide::{ prelude::*, utils::command::BotCommands };
use tokio::signal;
use tokio::sync::RwLock;
use reqwest::Client;
use log::error;
use mysql::*;
use mysql::prelude::*;


pub mod setting_opts;
pub mod regex;
pub mod tx_info;
pub mod token_overview;
pub mod token_transfer;
pub mod user_info;

use setting_opts::*;
use regex::*;
use tx_info::*;
use token_overview::*;
use token_transfer::*;
use user_info::*;

// Add this function to establish database connection
fn get_conn_pool() -> Pool {
    let url = "mysql://root:@localhost:3306/gorilla_buy_bot";    
    Pool::new(url).unwrap()
}

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
    log::info!("Starting Gorilla Buy...");

    let bot: Bot = Bot::from_env();
    let bot_arc = Arc::new(bot.clone());

    let bot_commands = Command::bot_commands();
    if bot.set_my_commands(bot_commands).await.is_err() {
        log::warn!("Could not set up the commands.");
    }

    let setting_opts_arc = Arc::new(RwLock::new(SettingOpts::default()));
    // println!("initial setting_opts_arc: {:?}", setting_opts_arc.read().await);

    // Initialize database connection
    let pool = get_conn_pool();
    
    // Create tables if they don't exist
    init_database(&pool).expect("Failed to initialize database");
    
    let callback_handler = Update::filter_callback_query()
    .endpoint(answer_button);
    
    let message_handler = Update::filter_message()
    .branch(
        dptree::filter(|msg: Message| {
            // Check if this message is a reply to a bot's message with ForceReply
            msg.reply_to_message()
            .and_then(|reply| reply.from())
            .map_or(false, |user| user.is_bot)
        })
        .branch(dptree::endpoint(answer_replyed_message))
    )
    .filter_command::<Command>()
    .endpoint(answer_command);

    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![setting_opts_arc.clone()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    
    
}

async fn answer_command(bot: Bot, msg: Message, cmd: Command, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
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
        Command::Settings{bot_username} => settings_command(bot, msg, bot_username, chat_type, setting_opts_arc).await,
        Command::Start{availability} => start_command(bot, msg, availability).await,
    };  
    Ok(())
}

async fn settings_command(bot: Bot, msg: Message, bot_username: String, chat_type: String, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
    match chat_type.as_str() {
        "a private chat" => {
            let _ = bot.send_message(msg.chat.id, format!("/settings command is not supported in this chat type."));
        }
        "a group" | "a supergroup" => {
            if let Some(user) = msg.from() {
                // Initialize database connection
                let pool = get_conn_pool();
                // Create UserInfo struct from user data
                let user_info = UserInfo {
                    user_id: user.id.to_string(),
                    username: user.username.clone(),
                    first_name: Some(user.first_name.clone()),
                    last_name: Some(user.last_name.clone().unwrap_or_default()),
                };
                
                // Save user info to database
                match save_user_info(&pool, user_info).await {
                    Ok(_) => log::info!("User info saved successfully for user_id: {}", user.id),
                    Err(e) => log::error!("Failed to save user info: {}", e),
                }

                //Update setting_opts_arc with user_id and group_chat_id
                setting_opts_arc.write().await.user_id = user.id.to_string();
                setting_opts_arc.write().await.group_chat_id = msg.chat.id.0;

                let sender_name = user.username.clone()
                    .unwrap_or_else(|| user.first_name.clone());
                
                let _ = start_settings(bot, msg.chat.id, sender_name, bot_username, setting_opts_arc.clone()).await;
            } else {
                log::warn!("No user information found in message");
                let _ = bot.send_message(msg.chat.id, "Could not process user information").await;
            }
        }
        _ => {
            let _ = bot.send_message(msg.chat.id, format!("This bot helps you to read Apechain token buy information. Type /help for more information")).await;
        }
    }
    Ok(())
}

async fn start_settings(bot: Bot, chat_id: ChatId, username: String, bot_username: String, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
    // setting_opts_arc.write().await.group_chat_id = chat_id.0;

    let bot_name = std::env::var("BOT_USERNAME").unwrap_or_default();
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::url(
            "Configure Settings",
            if bot_username.is_empty() {
                format!("https://t.me/{}?start=available", bot_name).parse().unwrap()
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

async fn start_command(bot: Bot, msg: Message, availability: String) -> ResponseResult<()> {
    match availability.as_str() {
        "available" => {
            let _ = start(bot, msg.chat.id).await;
        }
        _ => {}
    }
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

async fn answer_button(bot: Bot, callback: CallbackQuery, setting_opts_arc: Arc<RwLock<SettingOpts>>)  -> ResponseResult<()> {
    match callback.data {
        Some(callback_string) => {
            // println!("callback query:  {}", callback_string);
            match callback_string.as_str() {
                "token_address" => { let _ = message_by_callback(bot, callback.from.id.into(), "token_address".to_string()).await; },
                "min_buy_amount" => { let _ = message_by_callback(bot, callback.from.id.into(), "min_buy_amount".to_string()).await; },
                "buy_step" => { let _ = message_by_callback(bot, callback.from.id.into(), "buy_step".to_string()).await; },
                "emoji" => { let _ = message_by_callback(bot, callback.from.id.into(), "emoji".to_string()).await; },
                "media_toggle" => { let _ = media_toggle(bot, callback.from.id.into(), setting_opts_arc).await; },
                "add_media" => { let _ = select_media_type(bot, callback.from.id.into()).await; },
                "tg_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "tg_link".to_string()).await; },
                "website_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "website_link".to_string()).await; },
                "twitter_link" => { let _ = message_by_callback(bot, callback.from.id.into(), "twitter_link".to_string()).await; },
                // "confirm" => { let _ = confirm_style_change(bot, callback.from.id.into(), setting_opts_arc).await; },
                "delete_token" => { let _ = delete_and_back_to_new_token(bot, callback.from.id.into(), setting_opts_arc).await; },
                "photo" => { let _ = add_media(bot, callback.from.id.into(), setting_opts_arc, "photo".to_string()).await; },
                "video" => { let _ = add_media(bot, callback.from.id.into(), setting_opts_arc, "video".to_string()).await; },
                _ => { log::warn!("Received callback {} which isn't implemented.", callback_string); }
            }
        }
        None => {}
    };
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

async fn media_toggle(bot: Bot, chat_id: ChatId, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
    let pool = get_conn_pool();
    let toogle_value = !(setting_opts_arc.read().await.media_toggle);
    setting_opts_arc.write().await.media_toggle = toogle_value;
    save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;
    
    setting_option(bot.clone(), chat_id, "🎉 Media toggle option is saved. Now you can adjust the other settings:".to_string(), setting_opts_arc.read().await.clone()).await?;
    Ok(())
}

async fn select_media_type(bot: Bot, chat_id: ChatId) -> ResponseResult<()> {
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

async fn add_media(bot: Bot, chat_id: ChatId, setting_opts_arc: Arc<RwLock<SettingOpts>>, callback_string: String) -> ResponseResult<()> {
    let pool = get_conn_pool();
    setting_opts_arc.write().await.media_type = callback_string.to_string();
    save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;
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


async fn answer_replyed_message(bot: Bot, msg: Message, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
    let pool = get_conn_pool();
    let user_id = msg.from().unwrap().id.to_string();
    let chat_id = msg.chat.id;
    let reply_text = msg.reply_to_message().and_then(|reply| reply.text());

    if msg.photo().is_some() {
        if reply_text == Some("photo") {
            if let Some(latest_photo) = msg.photo().iter().last() {
                setting_opts_arc.write().await.media_file_id = Some(latest_photo[0].file.id.clone());
                
                // Update the settings
                save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;
               
                setting_option(bot.clone(), chat_id, "🎉 Photo saved. Now you can adjust other settings:".to_string(), setting_opts_arc.read().await.clone()).await?;
                return Ok(());
            }
        } else {
            // let selected_setting_opt = setting_opts_wrapper.get_selected_setting_opt().await;
            setting_option(bot.clone(), chat_id, "❌ Invalid photo style. Please again".to_string(), setting_opts_arc.read().await.clone()).await?;
            return Ok(());
        }
    } else if msg.video().is_some() {
        if reply_text == Some("video") {
            if let Some(latest_video) = msg.video().iter().last() {
                setting_opts_arc.write().await.media_file_id = Some(latest_video.file.id.clone());
                
                // Update the settings
                save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;

                setting_option(bot.clone(), chat_id, "🎉 Video saved. Now you can adjust the other settings:".to_string(), setting_opts_arc.read().await.clone()).await?;
                return Ok(());
            }
        } else {
            setting_option(bot.clone(), chat_id, "❌ Invalid video style. Please again".to_string(), setting_opts_arc.read().await.clone()).await?;
            return Ok(());
        }
    }  else if let Some(text) = msg.text() {
        if text.starts_with("0x") {
            if text.len() == 42 {
                if reply_text == Some("token_address") {
                    let mut existing_settings = get_setting_opt(&pool, user_id.clone(), text.to_string()).await.unwrap();
                    let group_chat_id = setting_opts_arc.read().await.group_chat_id;
                    existing_settings.group_chat_id = group_chat_id;
                    // println!("Found existing settings: {:?}", existing_settings.clone());
                    *setting_opts_arc.write().await = existing_settings.clone();
                    save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;
                    
                    setting_option(bot.clone(), chat_id, "🎉 Token address saved. Now you can adjust the other settings:".to_string(), setting_opts_arc.read().await.clone()).await?;
                    let _ = confirm_style_change(bot.clone(), chat_id, setting_opts_arc.read().await.clone()).await;

                      
                } else{
                    bot.send_message(
                        chat_id,
                        format!("❌ Token address is not valid. Try again")
                    ).await?;
                    message_by_callback(bot.clone(), chat_id, "token_address".to_string()).await?;
                }
            }
        } 
        else if let Some(reply_text) = reply_text {
            let mut head_text = "";
            match reply_text {
                // "token_address" => {
                //     if is_token_address(text) {
                //         selected_setting_opt.token_address = text.to_string();
                //         head_text = "🎉 Token address saved. Now you can adjust the other settings:";
                //     } else {
                //         head_text = "❌ Token address is not valid. Please try again.";
                //     }
                // },
                "min_buy_amount" => {
                    if let Ok(amount) = text.parse::<f64>() {
                        setting_opts_arc.write().await.min_buy_amount = amount;
                        head_text = "🎉 Min buy amount saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Min buy amount is not valid. Please try again.";
                    }
                },
                "buy_step" => {
                    if let Ok(step) = text.parse::<i32>() {
                        setting_opts_arc.write().await.buy_step = step;
                        head_text = "🎉 Buy step saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Buy step is not valid. Please try again.";
                    }
                },
                "emoji" => {
                    if is_emoji(text) {
                        setting_opts_arc.write().await.emoji = text.to_string();
                        head_text = "🎉 Emoji saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Emoji is not valid. Please try again.";
                    }
                },
                "tg_link" => {
                    if is_tg_link(text) {
                        setting_opts_arc.write().await.tg_link = text.to_string();
                        head_text = "🎉 Tg link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Tg link is not valid. Please try again.";
                    }
                },
                "website_link" => {
                    if is_website_link(text) {
                        setting_opts_arc.write().await.website_link = text.to_string();
                        head_text = "🎉 Website link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Website link is not valid. Please try again.";
                    }
                },
                "twitter_link" => {
                    if is_twitter_link(text) {
                        setting_opts_arc.write().await.twitter_link = text.to_string();
                        head_text = "🎉 Twitter link saved. Now you can adjust the other settings:";
                    } else {
                        head_text = "❌ Twitter link is not valid. Please try again.";
                    }
                },
                
                _ => log::warn!("Unhandled reply type: {}", reply_text)
            }

            let _ = save_setting_opts_db(&pool, setting_opts_arc.read().await.clone()).await;
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
           
            setting_option(bot.clone(), chat_id, head_text.to_string(), setting_opts_arc.read().await.clone()).await?;
        }
    } 
    // else if msg.document().is_some() {
    // } else if msg.sticker().is_some() {
    // } else {
    // }
    
    Ok(())
}

async fn setting_option(bot: Bot, chat_id: ChatId, head_text: String,  setting_opts: SettingOpts) -> ResponseResult<()> {
    // let group_chat_id = setting_opts.group_chat_id;
    // let bot_name = std::env::var("BOT_USERNAME").unwrap_or_default();

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(format!("Change minBuy: {}", setting_opts.min_buy_amount), "min_buy_amount")],
        vec![InlineKeyboardButton::callback(format!("Change step: {}", setting_opts.buy_step), "buy_step")],
        vec![InlineKeyboardButton::callback(format!("Change Emoji: {}", setting_opts.emoji), "emoji")],
        vec![InlineKeyboardButton::callback(format!("Enable/Disable media: {}", setting_opts.media_toggle), "media_toggle")],
        vec![InlineKeyboardButton::callback(format!("Add Media: {}", setting_opts.media_file_id.clone().unwrap_or("".to_string())), "add_media")],
        vec![InlineKeyboardButton::callback(format!("Change Tg Link: {}", setting_opts.tg_link), "tg_link")],
        vec![InlineKeyboardButton::callback(format!("Change Twitter Link: {}", setting_opts.twitter_link), "twitter_link")],
        vec![InlineKeyboardButton::callback(format!("Change Website Link: {}", setting_opts.website_link), "website_link")],
        vec![InlineKeyboardButton::callback("Delete Token", "delete_token")],
        // vec![
        //     // InlineKeyboardButton::callback("Confirm", "confirm"),
        //     InlineKeyboardButton::url(
        //         "Go back to group",
        //         format!("https://t.me/c/{}", group_chat_id).parse().unwrap()
        //     )
        // ]
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

async fn confirm_style_change(bot: Bot, chat_id: ChatId, setting_opts: SettingOpts) -> ResponseResult<()> {
    let group_chat_id = setting_opts.group_chat_id;
    bot.send_message(ChatId(group_chat_id), "Catching new buy transactions...").await?;

    let request_client = Client::new();
    let debank_api_key = std::env::var("DEBANK_API_KEY").unwrap();
    

    let interval = tokio::time::interval(std::time::Duration::from_secs(5));
    tokio::spawn(async move {
        let mut interval = interval;
        let mut flag_transaction_hash = String::new();
        loop {
            interval.tick().await;
            let token_adr = &setting_opts.token_address;
            if !token_adr.is_empty() && !setting_opts.user_id.is_empty() {
                match get_token_transfers(request_client.clone(), &token_adr).await {
                    Ok(token_transfer) => {
                        if let Some(first_transfer) = token_transfer.items.first() {
                            let transaction_hash = first_transfer.tx_hash.clone();
                            let current_transaction_to_name = first_transfer.to.name.clone().unwrap_or_default();
                            if flag_transaction_hash != transaction_hash && !current_transaction_to_name.is_empty() {
                                flag_transaction_hash = transaction_hash;
                                
                                //get setting options
                                let website_link = &setting_opts.website_link;
                                let tg_link = &setting_opts.tg_link;
                                let twitter_link = &setting_opts.twitter_link;
                                let emoji = &setting_opts.emoji;
                                let min_buy_amount = &setting_opts.min_buy_amount;
                                let buy_step = &setting_opts.buy_step;
                                let media_toggle = &setting_opts.media_toggle;
                                let media_file_id = &setting_opts.media_file_id;
                                let media_type = &setting_opts.media_type;
                                
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
                            
                                let emoji_count = (tx_value / *buy_step as f64) as i32;
                                let emoji_string = emoji.repeat((emoji_count + 1) as usize);

                                if tx_value * token_price > *min_buy_amount {
                                    let text = format!(
                                        "{11}\n\n\
                                        💲 Spent: ${1} (${7}) APE\n\
                                        💰 Got: {5} ${2}\n\
                                        ✅ Dex: <a href=\"https://ape.express/explore/{0}?\">Ape_Express</a> | \
                                        🔖 <a href=\"https://t.me/Apechain_Trending_Bot\">Book Trending</a> - \
                                        <a href=\"https://t.me/ApechainAds_Bot\">ADS</a>\n\
                                        🏷️ Price: ${6}\n\
                                        📊 Marketcap: ${4}\n\n\
                                        <a href=\"https://apescan.io/tx/{3}\">TX</a> | \
                                        <a href=\"https://dexscreener.com/apechain/{0}\">Chart</a> | \
                                        <a href=\"{8}\">TG</a> | \
                                        <a href=\"{9}\">X</a> | \
                                        <a href=\"{10}\">Website</a>",
                                        token_address, tx_value_usd,  token_symbol, flag_transaction_hash, mcap, tx_value_output, token_price_output, tx_total_usd, tg_link, twitter_link, website_link, emoji_string
                                    );
                            
                    
                                    // bot.send_message(chat_id, text)
                                    //     .parse_mode(teloxide::types::ParseMode::Html)
                                    //         .await.unwrap();
                                    if *media_toggle && media_file_id.clone().is_some() {
                                        if media_type == "photo" {
                                            bot.send_photo(ChatId(group_chat_id), InputFile::file_id(media_file_id.clone().unwrap()))
                                                .caption(text)
                                                .parse_mode(teloxide::types::ParseMode::Html)
                                                .await.unwrap();
                                        } else if media_type == "video" {
                                            bot.send_video(ChatId(group_chat_id), InputFile::file_id(media_file_id.clone().unwrap()))
                                                .caption(text)
                                                .parse_mode(teloxide::types::ParseMode::Html)
                                                .await.unwrap();
                                        }
                                    } else {
                                        bot.send_message(ChatId(group_chat_id), text)
                                            .parse_mode(teloxide::types::ParseMode::Html)
                                            .await.unwrap();
                                    }
                                }
                            }
                        }
                        else {
                            bot.send_message(ChatId(group_chat_id), "Not found any new transfer")
                                .await.unwrap();
                        }

                    }
                    Err(e) => {
                        error!("Error fetching token overview: {}", e);
                        // bot.send_message(ChatId(group_chat_id), "Invalid token address or Failed API request. Please try again!")
                        // bot.send_message(ChatId(group_chat_id), e.to_string())
                        //     .await.unwrap();
                        continue;
                    }
                };
            } else {
                break;
            }
        }
    });
    Ok(())
}

async fn delete_and_back_to_new_token(bot: Bot, chat_id: ChatId, setting_opts_arc: Arc<RwLock<SettingOpts>>) -> ResponseResult<()> {
    let pool = get_conn_pool();
    let is_deleted = delete_setting_opt_from_db(&pool, &setting_opts_arc.read().await.token_address, setting_opts_arc.read().await.user_id.clone()).await.unwrap();
    if is_deleted {
        bot.send_message(
            chat_id,
            format!("The token {} is deleted. Please return to group chat.", setting_opts_arc.read().await.token_address)
        )
        .await?;
    } else {
        bot.send_message(
            chat_id,
            format!("The token {} is not found.", setting_opts_arc.read().await.token_address)
        )
        .await?;
    }

    Ok(())
}


async fn get_token_transfers(client: Client, token_address: &str) -> Result<TokenTransfer, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://apechain.calderaexplorer.xyz/api/v2/tokens/{}/transfers",
        token_address
    );

    let response = client
        .get(&url)
        .send()
        .await?;

    let text = response.text().await?;
    
    match serde_json::from_str::<TokenTransfer>(&text) {
        Ok(transfer) => Ok(transfer),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(Box::new(e))
        }
    }
}

async fn get_tx_info(client: Client, tx_hash: &str) -> Result<TxInfo, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://apechain.calderaexplorer.xyz/api/v2/transactions/{}", tx_hash);
    let response = client.get(&url).send().await?;
    let text = response.text().await?;
    match serde_json::from_str::<TxInfo>(&text) {
        Ok(tx_info) => Ok(tx_info),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(Box::new(e))
        }
    }
}   

async fn get_token_overview(client: Client, api_key: &str, token_address: &str) -> Result<TokenOverview, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://pro-openapi.debank.com/v1/token?chain_id=ape&id={}", token_address);
    let response = client.get(&url).header("Accesskey", api_key).send().await?;
    let text = response.text().await?;
    match serde_json::from_str::<TokenOverview>(&text) {
        Ok(token_overview) => Ok(token_overview),
        Err(e) => {
            error!("Deserialization error: {}", e);
            Err(Box::new(e))
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

fn init_database(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = pool.get_conn()?;
    
    // Create user_info table first
    conn.query_drop(r"
        CREATE TABLE IF NOT EXISTS user_info (
            id BIGINT AUTO_INCREMENT PRIMARY KEY,
            user_id BIGINT NOT NULL UNIQUE,
            username VARCHAR(255),
            first_name VARCHAR(255),
            last_name VARCHAR(255),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci
    ")?;

    // Then create setting_opts table with foreign key
    conn.query_drop(r"
        CREATE TABLE IF NOT EXISTS setting_opts (
            id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            group_chat_id BIGINT NOT NULL,
            token_address VARCHAR(42) NOT NULL,
            min_buy_amount DOUBLE NOT NULL,
            buy_step INT NOT NULL,
            emoji VARCHAR(10) NOT NULL,
            media_toggle BOOLEAN NOT NULL,
            media_file_id VARCHAR(255),
            media_type VARCHAR(10),
            tg_link VARCHAR(255),
            website_link VARCHAR(255),
            twitter_link VARCHAR(255),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE KEY unique_id (id)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci
    ")?;

    Ok(())
}

async fn save_user_info(pool: &Pool, user: UserInfo) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = pool.get_conn()?;
    
    let params = params! {
        "user_id" => user.user_id,
        "username" => &user.username,
        "first_name" => &user.first_name,
        "last_name" => &user.last_name,
    };
    // println!("Params: {:?}", params);
    conn.exec_drop(
        r"INSERT INTO user_info 
          (user_id, username, first_name, last_name)
          VALUES (:user_id, :username, :first_name, :last_name)
          ON DUPLICATE KEY UPDATE
          username = :username,
          first_name = :first_name,
          last_name = :last_name",
        params
    )?;

    Ok(())
}

async fn save_setting_opts_db(pool: &Pool, opt: SettingOpts) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = pool.get_conn()?;
    
    let params = params! {
        "id" => format!("{}/{}", &opt.user_id, &opt.token_address),
        "user_id" => &opt.user_id,
        "group_chat_id" => opt.group_chat_id,
        "token_address" => &opt.token_address,
        "min_buy_amount" => opt.min_buy_amount,
        "buy_step" => opt.buy_step,
        "emoji" => &opt.emoji,
        "media_toggle" => opt.media_toggle,
        "media_file_id" => &opt.media_file_id,
        "media_type" => &opt.media_type,
        "tg_link" => &opt.tg_link,
        "website_link" => &opt.website_link,
        "twitter_link" => &opt.twitter_link
    };

    match conn.exec_drop(
        r"INSERT INTO setting_opts 
          (id, user_id, group_chat_id, token_address, min_buy_amount, buy_step, emoji, 
           media_toggle, media_file_id, media_type, tg_link, website_link, twitter_link)
          VALUES 
          (:id, :user_id, :group_chat_id, :token_address, :min_buy_amount, :buy_step, :emoji,
           :media_toggle, :media_file_id, :media_type, :tg_link, :website_link, :twitter_link)
          ON DUPLICATE KEY UPDATE
          user_id = :user_id,
          group_chat_id = :group_chat_id,
          token_address = :token_address,
          min_buy_amount = :min_buy_amount,
          buy_step = :buy_step,
          emoji = :emoji,
          media_toggle = :media_toggle, 
          media_file_id = :media_file_id,
          media_type = :media_type,
          tg_link = :tg_link,
          website_link = :website_link,
          twitter_link = :twitter_link",
        params
    ) {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Database error: {}", e);
            Err(Box::new(e))
        }
    }?;

    Ok(())
}



async fn get_setting_opt(pool: &Pool, userid: String, token_adr: String) -> Result<SettingOpts, Box<dyn std::error::Error>> {
    let mut conn = pool.get_conn()?;
    let result = conn.exec_first(
            r"SELECT 
                CAST(user_id AS CHAR) as user_id,
                group_chat_id,
                CAST(token_address AS CHAR) as token_address,
                min_buy_amount,
                buy_step,
                CAST(emoji AS CHAR) as emoji,
                media_toggle,
                NULLIF(CAST(media_file_id AS CHAR), '') as media_file_id,
                CAST(media_type AS CHAR) as media_type,
                CAST(tg_link AS CHAR) as tg_link,
                CAST(website_link AS CHAR) as website_link,
                CAST(twitter_link AS CHAR) as twitter_link
              FROM setting_opts 
              WHERE token_address = ? AND user_id = ?
              LIMIT 1",
            (token_adr.clone(), userid.clone()),
        )?;

    if let Some((user_id, group_chat_id, token_address, min_buy_amount, buy_step, emoji,
        media_toggle, media_file_id, media_type, tg_link, website_link, twitter_link)) = result {
        Ok(SettingOpts {
            user_id,
            group_chat_id,
            token_address,
            min_buy_amount,
            buy_step,
            emoji,
            media_toggle,
            media_file_id,
            media_type,
            tg_link,
            website_link,
            twitter_link,
        })
    } else {
        Ok(SettingOpts {
            user_id: userid.clone(),
            group_chat_id: 0,
            token_address: token_adr.to_string(),
            min_buy_amount: 0.0,
            buy_step: 30,
            emoji: "💎".to_string(),    
            media_toggle: true,
            media_type: String::new(),
            media_file_id: Some(String::new()),
            tg_link: String::new(),
            twitter_link: String::new(),
            website_link: String::new(),
        })
    }
}

async fn delete_setting_opt_from_db(pool: &Pool, token_address: &str, user_id: String) -> Result<bool, Box<dyn std::error::Error>> {
    let mut conn = pool.get_conn()?;
    
    let result = conn.exec_drop(
        r"DELETE FROM setting_opts 
          WHERE token_address = ? AND user_id = ?",
        (token_address, user_id),
    )?;

    // Check if any row was affected
    let affected_rows = conn.affected_rows();
    Ok(affected_rows > 0)
}