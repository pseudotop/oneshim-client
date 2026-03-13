//! Port for discovering and loading skill definitions.

use crate::error::CoreError;
use crate::models::skill::{Skill, SkillMeta};

/// Discovers SKILL.md files and parses their frontmatter/body.
pub trait SkillLoader: Send + Sync {
    /// List all available skill metadata (name + description only).
    /// Used for progressive disclosure in system prompts.
    fn list_skills(&self) -> Vec<SkillMeta>;

    /// Load a skill by name, including its full body content.
    fn get_skill(&self, name: &str) -> Result<Skill, CoreError>;

    /// Reload skills from disk (e.g., after config change).
    fn reload(&self) -> Result<(), CoreError>;
}
