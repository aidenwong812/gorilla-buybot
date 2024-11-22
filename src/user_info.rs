use serde::{ Serialize, Deserialize };

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

