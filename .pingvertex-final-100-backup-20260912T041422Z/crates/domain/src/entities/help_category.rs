use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::HelpCategoryId;

/// Data required to create a help category.
#[derive(Debug, Clone)]
pub struct CreateHelpCategory {
    pub id: HelpCategoryId,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub sort_order: i32,
}

/// A Help Center category grouping related articles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelpCategory {
    pub id: HelpCategoryId,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl HelpCategory {
    pub const MAX_NAME_LENGTH: usize = 100;
    pub const MAX_SLUG_LENGTH: usize = 150;
    pub const MAX_DESCRIPTION_LENGTH: usize = 500;

    /// Creates a new help category with validation.
    pub fn create(input: CreateHelpCategory) -> Result<Self, DomainError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyHelpCategoryName);
        }
        if name.chars().count() > Self::MAX_NAME_LENGTH {
            return Err(DomainError::HelpCategoryNameTooLong);
        }

        let slug = input.slug.trim();
        if slug.is_empty() {
            return Err(DomainError::InvalidHelpCategorySlug);
        }
        if slug.chars().count() > Self::MAX_SLUG_LENGTH {
            return Err(DomainError::InvalidHelpCategorySlug);
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::InvalidHelpCategorySlug);
        }

        let description = input.description.trim();
        if description.chars().count() > Self::MAX_DESCRIPTION_LENGTH {
            return Err(DomainError::InvalidHelpCategorySlug);
        }

        let now = Utc::now();

        Ok(Self {
            id: input.id,
            name: name.to_owned(),
            slug: slug.to_owned(),
            description: description.to_owned(),
            sort_order: input.sort_order,
            created_at: now,
            updated_at: now,
        })
    }
}
