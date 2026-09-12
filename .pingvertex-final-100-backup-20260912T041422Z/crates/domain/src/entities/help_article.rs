use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::HelpArticleStatus;
use crate::errors::DomainError;
use crate::shared::ids::{HelpArticleId, HelpCategoryId};

/// Data required to create a help article.
#[derive(Debug, Clone)]
pub struct CreateHelpArticle {
    pub id: HelpArticleId,
    pub category_id: HelpCategoryId,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub status: HelpArticleStatus,
}

/// A single Help Center article.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelpArticle {
    pub id: HelpArticleId,
    pub category_id: HelpCategoryId,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub status: HelpArticleStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl HelpArticle {
    pub const MAX_TITLE_LENGTH: usize = 200;
    pub const MAX_SLUG_LENGTH: usize = 200;
    pub const MAX_CONTENT_LENGTH: usize = 100_000;

    /// Creates a new help article with validation.
    pub fn create(input: CreateHelpArticle) -> Result<Self, DomainError> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DomainError::EmptyHelpArticleTitle);
        }
        if title.chars().count() > Self::MAX_TITLE_LENGTH {
            return Err(DomainError::HelpArticleTitleTooLong);
        }

        let slug = input.slug.trim();
        if slug.is_empty() {
            return Err(DomainError::InvalidHelpArticleSlug);
        }
        if slug.chars().count() > Self::MAX_SLUG_LENGTH {
            return Err(DomainError::InvalidHelpArticleSlug);
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::InvalidHelpArticleSlug);
        }

        let content = input.content.trim();
        if content.is_empty() {
            return Err(DomainError::EmptyHelpArticleContent);
        }
        if content.chars().count() > Self::MAX_CONTENT_LENGTH {
            return Err(DomainError::HelpArticleContentTooLong);
        }

        let now = Utc::now();

        Ok(Self {
            id: input.id,
            category_id: input.category_id,
            title: title.to_owned(),
            slug: slug.to_owned(),
            content: content.to_owned(),
            status: input.status,
            created_at: now,
            updated_at: now,
        })
    }
}
