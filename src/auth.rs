// Authentication module
pub fn authenticate(username: &str, password: &str) -> bool {
    // Basic auth logic with validation
    if username.is_empty() || password.is_empty() {
        return false;
    }
    username.len() >= 3 && password.len() >= 8
}

pub fn hash_password(password: &str) -> String {
    format!("hashed_{}", password)
}

pub struct Session {
    pub user: String,
    pub token: String,
}
