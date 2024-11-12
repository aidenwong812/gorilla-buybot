use serde::{ Serialize, Deserialize };
use tokio::sync::RwLock;

use std::{
    error::Error, 
    // path::Path,
    sync::Arc
    // collections::HashMap
};
use std::ops::Deref;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettingOpts {
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

// impl SettingOpts {
//     // Add a new constructor
//     pub fn new() -> Self {
//         SettingOpts {
//             token_address: String::new(),
//             min_buy_amount: 0.0,
//             buy_step: 30,
//             emoji: "💎".to_string(),    
//             media_toggle: true,
//             media_type: vec!["image".to_string(), "video".to_string()],
//             tg_link: String::new(),
//             twitter_link: String::new(),
//             website_link: String::new(),
//         }
//     }
// }

pub type SettingOptsVector = Arc<RwLock<Vec<SettingOpts>>>;

#[derive(Debug)]
pub struct SettingOptsWrapper {
    pub group_chat_id: RwLock<i64>,
    pub selected_setting_opt: RwLock<SettingOpts>,
    pub setting_opts: SettingOptsVector,
}

impl Clone for SettingOptsWrapper {
    fn clone(&self) -> Self {
        // Create a new RwLock with the value from the existing one
        
        
        Self {
            group_chat_id: RwLock::new(0),
            selected_setting_opt: RwLock::new(SettingOpts::default()),
            setting_opts: self.setting_opts.clone(),  // Arc can be cloned
        }
    }
}

impl SettingOptsWrapper {
    pub fn new() -> Self {
        Self {
            group_chat_id: RwLock::new(0),
            selected_setting_opt: RwLock::new(SettingOpts::default()),
            setting_opts: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    // pub fn new_setting_opt() -> SettingOpts {
    //     SettingOpts::new()
    // }

    pub async fn setting_opt_exists(&self, token_address: String) -> bool {
        for u in self.setting_opts.read().await.iter() {
            if u.token_address == token_address {
                return true;
            }
        }
        false
    }

    
    /// Returns a `UserState` by a given `ChatId`. This function is save, that means, if no UserSate for a
    /// given ChatId is saved, the default UserState will be returned.
    /// 
    /// # Params
    /// - `chat_id` A `ChatId`
    /// # Returns
    /// The saved `UserState` if one is saved, or the default `UserState` if no one is found.
    pub async fn find_setting_opt(&self, token_adr: String) -> SettingOpts {
        let default_setting_opts = SettingOpts {
                token_address: token_adr.clone(),
                min_buy_amount: 0.0,
                buy_step: 30,
                emoji: "💎".to_string(),    
                media_toggle: true,
                media_type: String::new(),
                media_file_id: None,
                tg_link: String::new(),
                twitter_link: String::new(),
                website_link: String::new(),
        };

        for u in self.setting_opts.read().await.iter() {
            if u.token_address == token_adr {
                return u.clone();
            }
        }
        default_setting_opts
    }

    
    pub async fn update_setting_opt(&self, setting_opt: SettingOpts) -> bool {
        for u in self.setting_opts.write().await.iter_mut() {
            if u.token_address == setting_opt.token_address {
                *u = setting_opt.clone();
                
                // End the function if a UserState already exists which has been updated
                return true;
            }
        };
        
        // If there has been no user_state saved, the function will get here and add a new UserState element
        self.setting_opts.write().await.push(setting_opt);
        
        false
    }
    
    pub async fn delete_setting_opt(&self, setting_opt: SettingOpts) -> bool {
        let mut setting_opts = self.setting_opts.write().await;
        let initial_len = setting_opts.len();
        setting_opts.retain(|opt| opt.token_address != setting_opt.token_address);
        setting_opts.len() < initial_len
    }
    
    pub async fn write_states_to_file(&self, file_path: &str) -> Result<(), Box<dyn Error>> {
        match serde_json::to_string_pretty(self.setting_opts.read().await.deref()) {
            Ok(json_string) => {
                tokio::fs::write(file_path, json_string).await?;
                Ok(())
            },
            Err(e) => Err(Box::new(e)),
        }
    }

    pub async fn load_states_from_file(&self, file_path: &str) -> Result<(), Box<dyn Error>> {
        match tokio::fs::read_to_string(file_path).await {
            Ok(file_string) => {
                match serde_json::from_str(&file_string) {
                    Ok(object) => {
                        let mut setting_opts: Vec<SettingOpts> = object;
                        let mut setting_opt_lock = self.setting_opts.write().await;
                        setting_opt_lock.clear();
                        setting_opt_lock.append(&mut setting_opts);
                        Ok(())
                    },
                    Err(error) => Err(Box::new(error))
                }
            },
            Err(error) => Err(Box::new(error))
        }
        
    }

    pub async fn set_selected_setting_opt(&self, opt: SettingOpts) {
        let mut selected = self.selected_setting_opt.write().await;
        *selected = opt;
    }

    pub async fn get_selected_setting_opt(&self) -> SettingOpts {
        let selected = self.selected_setting_opt.read().await;
        selected.clone()
    }
    
    pub async fn set_group_chat_id(&self, id: i64) {
        let mut guard = self.group_chat_id.write().await;
        *guard = id;
    }

    pub async fn get_group_chat_id(&self) -> i64 {
        *self.group_chat_id.read().await
    }

}
