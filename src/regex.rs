use regex::Regex;

pub fn is_token_address(text: &str) -> bool {
    Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap().is_match(text)
}
pub fn is_tg_link(text: &str) -> bool {
    Regex::new(r"^https://t\.me/[a-zA-Z0-9_]+$").unwrap().is_match(text)
}
pub fn is_website_link(text: &str) -> bool {
    Regex::new(r"^https://[a-zA-Z0-9_.]+$").unwrap().is_match(text)
}
pub fn is_twitter_link(text: &str) -> bool {
    Regex::new(r"^https://x\.com/[a-zA-Z0-9_]+$").unwrap().is_match(text)
}
pub fn is_emoji(text: &str) -> bool {
    Regex::new(r"^[\p{Emoji}]$").unwrap().is_match(text)
}
