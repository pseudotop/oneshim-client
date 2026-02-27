use async_trait::async_trait;

use crate::error::CoreError;

#[async_trait]
pub trait InputDriver: Send + Sync {
    async fn mouse_move(&self, x: i32, y: i32) -> Result<(), CoreError>;

    async fn mouse_click(&self, button: &str, x: i32, y: i32) -> Result<(), CoreError>;

    async fn type_text(&self, text: &str) -> Result<(), CoreError>;

    async fn key_press(&self, key: &str) -> Result<(), CoreError>;

    async fn key_release(&self, key: &str) -> Result<(), CoreError>;

    async fn hotkey(&self, keys: &[String]) -> Result<(), CoreError>;

    fn platform(&self) -> &str;
}
