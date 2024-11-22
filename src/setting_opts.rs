use serde::{ Serialize, Deserialize };
use tokio::sync::RwLock;

use std::{
    error::Error, 
    sync::Arc
};
use std::ops::Deref;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingOpts {
    pub user_id: String,
    pub group_chat_id: i64,
    pub token_address: String,
    pub min_buy_amount: f64,
    pub buy_step: i32,
    pub emoji: String,
    pub media_toggle: bool,
    pub media_type: String,
    pub media_file_id: Option<String>,
    pub tg_link: String,
    pub twitter_link: String,
    pub website_link: String,
}

impl Default for SettingOpts {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            group_chat_id: 0,
            token_address: String::new(),
            min_buy_amount: 0.0,
            buy_step: 30,
            emoji: "💎".to_string(),
            media_toggle: true,
            media_type: String::new(),
            media_file_id: Some(String::new()),
            tg_link: String::new(),
            twitter_link: String::new(),
            website_link: String::new(),
        }
    }
}
