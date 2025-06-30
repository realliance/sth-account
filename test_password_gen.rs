// Test program to generate password hash for tests
use sth_account::auth::Backend;

#[tokio::main]
async fn main() {
    let hash = Backend::hash_password("password123").await.unwrap();
    println!("Password hash for 'password123': {}", hash);
}